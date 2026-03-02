//! Windows screensaver integration using windows-rs
//!
//! This module handles all Windows-specific screensaver functionality including
//! window creation, message handling, and rendering.
//!
//! Memory optimizations:
//! - Thread-local frame buffer (reused, never shrinks)
//! - Font system loads only embedded font (no system fonts)

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use windows::{
    Win32::Foundation::*, Win32::Graphics::Gdi::*, Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*, core::*,
};

use crate::clock::{
    get_combined_date_string, get_day_string, get_season_string, get_time_period_string,
    get_time_string,
};
use crate::config::Config;
use crate::renderer::{Renderer, font_ratios};

/// Global flag to control screensaver running state
static RUNNING: AtomicBool = AtomicBool::new(true);

/// Global renderer instance (thread-safe, initialized once)
static RENDERER: OnceLock<Mutex<Renderer>> = OnceLock::new();

// Thread-local buffer for rendering to avoid per-frame allocations
thread_local! {
    static RENDER_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Screensaver operating mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreensaverMode {
    /// Normal screensaver mode (/s)
    Screensaver,
    /// Preview mode in settings dialog (/p <hwnd>)
    Preview(isize),
    /// Configuration dialog (/c)
    Configure,
}

impl ScreensaverMode {
    /// Parse command line arguments to determine operating mode
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        if args.len() < 2 {
            return ScreensaverMode::Screensaver;
        }

        // Windows screensaver arguments can be /s, /S, -s, /c:hwnd, /p:hwnd, etc.
        let arg = args[1].to_lowercase();

        // Handle both /c and /c:hwnd formats
        if arg.starts_with("/c") || arg.starts_with("-c") {
            ScreensaverMode::Configure
        } else if arg.starts_with("/p") || arg.starts_with("-p") {
            // Extract hwnd from /p:hwnd or /p hwnd
            let hwnd = if arg.contains(':') {
                arg.split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0)
            } else if arg.len() > 2 {
                arg[2..].trim().parse().unwrap_or(0)
            } else if args.len() > 2 {
                args[2].trim().parse().unwrap_or(0)
            } else {
                0
            };
            ScreensaverMode::Preview(hwnd)
        } else {
            // Default to Screensaver mode for /s, -s, or any other argument
            ScreensaverMode::Screensaver
        }
    }
}

/// Initialize the global renderer with configuration
fn init_renderer() {
    let config = Config::load();
    let _ = RENDERER.set(Mutex::new(Renderer::new(config)));
}

/// Run the screensaver in fullscreen mode
pub fn run_screensaver() -> Result<()> {
    init_renderer();

    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("BsaverScreensaver");

        let wc = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance.into(),
            lpszClassName: window_class,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(screensaver_wndproc),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            window_class,
            w!("Bangla Screensaver"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            screen_width,
            screen_height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        ShowCursor(false);
        SetTimer(Some(hwnd), 1, 100, None);

        run_message_loop();

        ShowCursor(true);
        Ok(())
    }
}

/// Run the screensaver in preview mode (embedded in settings dialog)
pub fn run_preview(parent_hwnd: isize) -> Result<()> {
    init_renderer();

    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("BsaverPreview");
        let parent = HWND(parent_hwnd as *mut _);

        let wc = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance.into(),
            lpszClassName: window_class,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(preview_wndproc),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let mut rect = RECT::default();
        GetClientRect(parent, &mut rect)?;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("Preview"),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            rect.right,
            rect.bottom,
            Some(parent),
            None,
            Some(instance.into()),
            None,
        )?;

        SetTimer(Some(hwnd), 1, 1000, None);

        run_message_loop();

        Ok(())
    }
}

/// Run the Windows message loop
fn run_message_loop() {
    unsafe {
        let mut message = MSG::default();
        while RUNNING.load(Ordering::Relaxed) && GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// Draw the clock display using provided HDC (from BeginPaint)
fn draw_clock(hdc: HDC, hwnd: HWND) {
    unsafe {
        if hdc.is_invalid() {
            return;
        }

        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return;
        }

        let width = rect.right as u32;
        let height = rect.bottom as u32;

        if width == 0 || height == 0 {
            return;
        }

        // Create memory DC for double buffering
        let mem_dc = CreateCompatibleDC(Some(hdc));
        if mem_dc.is_invalid() {
            return;
        }

        let bitmap = CreateCompatibleBitmap(hdc, width as i32, height as i32);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem_dc);
            return;
        }

        let old_bitmap = SelectObject(mem_dc, bitmap.into());

        // Fill with black background
        let brush = CreateSolidBrush(COLORREF(0));
        FillRect(mem_dc, &rect, brush);
        let _ = DeleteObject(brush.into());

        // Render clock content
        render_clock_content(mem_dc, width, height);

        // Copy to screen
        let _ = BitBlt(
            hdc,
            0,
            0,
            width as i32,
            height as i32,
            Some(mem_dc),
            0,
            0,
            SRCCOPY,
        );

        // Cleanup GDI resources
        SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem_dc);
    }
}

/// Render clock content to a device context
fn render_clock_content(dc: HDC, width: u32, height: u32) {
    let Some(renderer_mutex) = RENDERER.get() else {
        return;
    };

    let Ok(mut renderer) = renderer_mutex.lock() else {
        return;
    };

    // Clone config quickly to minimize lock time
    let config = renderer.config.clone();

    // Calculate font sizes using named constants
    let num_chars = if config.show_seconds { 8.0 } else { 5.0 };
    let target_width = width as f32 / 3.0;

    let base_font_size = (target_width / (num_chars * font_ratios::CHAR_WIDTH_RATIO))
        .min(height as f32 * font_ratios::MAX_HEIGHT_RATIO);
    let time_font_size = (base_font_size * config.font_size.multiplier()).max(24.0);
    let period_font_size = (time_font_size * font_ratios::PERIOD_RATIO).max(16.0);
    let day_font_size = (time_font_size * font_ratios::DAY_RATIO).max(14.0);
    let date_font_size = (time_font_size * font_ratios::DATE_RATIO).max(12.0);

    // Use thread-local buffer to avoid per-frame allocation
    let stride = width * 4;
    let buffer_size = (stride * height) as usize;

    RENDER_BUFFER.with(|buf| {
        let mut buffer = buf.borrow_mut();

        // Resize only if needed (grows but never shrinks)
        if buffer.len() < buffer_size {
            buffer.resize(buffer_size, 0);
        }

        // Fill with background color
        let bg = config.background_color;
        let pixel = [bg[2], bg[1], bg[0], 255u8]; // BGRA
        for chunk in buffer[..buffer_size].chunks_exact_mut(4) {
            chunk.copy_from_slice(&pixel);
        }

        // Calculate total content height for vertical centering
        let time_height = time_font_size * 1.3;
        let mut total_height = time_height;

        if config.show_time_period {
            total_height += period_font_size * 1.2;
        }
        if config.show_day || config.show_season {
            total_height += day_font_size * 1.4;
        }
        if config.show_english_date || config.show_bangla_date {
            total_height += date_font_size * 1.5;
        }

        let mut y_offset = ((height as f32 - total_height) / 2.0).max(0.0) as u32;

        // 1. Render time period (সকাল/দুপুর/বিকাল/রাত)
        if config.show_time_period {
            let period_str = get_time_period_string(&config);
            renderer.render_text_centered(
                &period_str,
                period_font_size,
                width,
                y_offset,
                &mut buffer,
                stride,
            );
            y_offset += (period_font_size * 1.2) as u32;
        }

        // 2. Render time using fixed-width grid
        let time_str = get_time_string(&config);
        renderer.render_time_fixed_grid(
            &time_str,
            time_font_size,
            width,
            y_offset,
            &mut buffer,
            stride,
        );
        y_offset += time_height as u32;

        // 3. Render day of week (সোমবার) or day + season (সোমবার, শীতকাল)
        if config.show_day || config.show_season {
            let day_str = if config.show_day {
                get_day_string(&config)
            } else {
                get_season_string(&config)
            };
            renderer.render_text_centered(
                &day_str,
                day_font_size,
                width,
                y_offset,
                &mut buffer,
                stride,
            );
            y_offset += (day_font_size * 1.4) as u32;
        }

        // 4. Render date (English | Bangla or just one)
        if config.show_english_date || config.show_bangla_date {
            let date_str = get_combined_date_string(&config);
            if !date_str.is_empty() {
                renderer.render_text_centered(
                    &date_str,
                    date_font_size,
                    width,
                    y_offset,
                    &mut buffer,
                    stride,
                );
            }
        }

        // Create bitmap info for DIB
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: buffer_size as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        unsafe {
            SetDIBitsToDevice(
                dc,
                0,
                0,
                width,
                height,
                0,
                0,
                0,
                height,
                buffer.as_ptr() as *const _,
                &bmi,
                DIB_RGB_COLORS,
            );
        }
    });
}

/// Window procedure for main screensaver window
extern "system" fn screensaver_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_CREATE => LRESULT(0),

            WM_TIMER => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }

            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                draw_clock(hdc, hwnd);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }

            WM_ERASEBKGND => LRESULT(1),

            WM_KEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                RUNNING.store(false, Ordering::Relaxed);
                PostQuitMessage(0);
                LRESULT(0)
            }

            WM_MOUSEMOVE => {
                static FIRST_MOUSE_POS: OnceLock<(i32, i32)> = OnceLock::new();
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                let first = FIRST_MOUSE_POS.get_or_init(|| (x, y));
                let dx = (x - first.0).abs();
                let dy = (y - first.1).abs();

                if dx > 10 || dy > 10 {
                    RUNNING.store(false, Ordering::Relaxed);
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }

            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), 1);
                PostQuitMessage(0);
                LRESULT(0)
            }

            WM_SETCURSOR => {
                SetCursor(None);
                LRESULT(1)
            }

            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

/// Window procedure for preview window
extern "system" fn preview_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_CREATE => LRESULT(0),

            WM_TIMER => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }

            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                draw_clock(hdc, hwnd);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }

            WM_ERASEBKGND => LRESULT(1),

            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), 1);
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
