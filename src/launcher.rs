//! BanglaSaver Launcher - UI for managing the screensaver
//!
//! Provides a Win32 window with buttons to:
//! - Set bsaver as the system screensaver (HKCU registry)
//! - Remove the screensaver registration
//! - Open Windows screensaver settings
//! - Preview the screensaver
//!
//! All operations are per-user (HKCU) - no admin required.
//!
//! # Safety
//! This module uses Win32 APIs extensively. All unsafe operations are wrapped
//! in explicit `unsafe {}` blocks per Rust 2024 edition requirements.

#![windows_subsystem = "windows"]

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;
use windows::{
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::System::Registry::*,
    Win32::UI::Input::KeyboardAndMouse::EnableWindow,
    Win32::UI::Shell::ShellExecuteW,
    Win32::UI::WindowsAndMessaging::*,
    core::*,
};

// Button IDs
const IDC_BTN_SET: i32 = 1001;
const IDC_BTN_REMOVE: i32 = 1002;
const IDC_BTN_SETTINGS: i32 = 1003;
const IDC_BTN_PREVIEW: i32 = 1004;
const IDC_BTN_BSETTINGS: i32 = 1005;
const IDC_STATUS: i32 = 1006;

/// COLORREF helper: 0x00BBGGRR
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | (r as u32))
}

// Thread-local UI state — Win32 windows are single-threaded, so thread_local!
// is the correct and safe replacement for `static mut` (denied in Rust 2024).
thread_local! {
    static STATUS_COLOR: Cell<COLORREF> = const { Cell::new(COLORREF(0)) };
    static H_STATUS: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static H_BTN_SET: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static H_BTN_REMOVE: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    static H_BTN_PREVIEW: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
}

/// GDI resources stored behind a Mutex for interior mutability.
/// Only ever accessed from the UI thread — the Mutex is for safe Rust semantics.
struct GdiResources {
    brush_bg: HBRUSH,
    font_title: HFONT,
    font_subtitle: HFONT,
    font_normal: HFONT,
    font_btn: HFONT,
    font_footer: HFONT,
}

impl Default for GdiResources {
    fn default() -> Self {
        Self {
            brush_bg: HBRUSH(std::ptr::null_mut()),
            font_title: HFONT(std::ptr::null_mut()),
            font_subtitle: HFONT(std::ptr::null_mut()),
            font_normal: HFONT(std::ptr::null_mut()),
            font_btn: HFONT(std::ptr::null_mut()),
            font_footer: HFONT(std::ptr::null_mut()),
        }
    }
}

// Safety: GDI handles are only ever accessed from the main (UI) thread.
unsafe impl Send for GdiResources {}

static GDI: LazyLock<Mutex<GdiResources>> =
    LazyLock::new(|| Mutex::new(GdiResources::default()));

/// Create a font with the given height, weight, and face name.
///
/// # Safety
/// Caller must ensure this is called in a valid GDI context.
unsafe fn make_font(height: i32, weight: i32, face: PCWSTR) -> HFONT {
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(0),
            CLEARTYPE_QUALITY,
            0,
            face,
        )
    }
}

/// Send WM_SETFONT to a control.
///
/// # Safety
/// `hwnd` must be a valid window handle and `font` a valid HFONT.
unsafe fn set_control_font(hwnd: HWND, font: HFONT) {
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

/// Get the path to bsaver.exe alongside this executable.
fn get_scr_path() -> PathBuf {
    let mut exe_path = std::env::current_exe().unwrap_or_default();
    exe_path.pop();
    exe_path.push("bsaver.exe");
    exe_path
}

/// Read current screensaver path from HKCU registry.
fn get_current_screensaver() -> String {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Control Panel\\Desktop");
        let res = RegOpenKeyExW(HKEY_CURRENT_USER, subkey, Some(0), KEY_READ, &mut hkey);
        if res.is_err() {
            return String::new();
        }

        let mut buffer = [0u16; 260];
        let mut size = (buffer.len() * 2) as u32;
        let value_name = w!("SCRNSAVE.EXE");
        let res = RegQueryValueExW(
            hkey,
            value_name,
            None,
            None,
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);

        if res.is_err() {
            return String::new();
        }

        let len = (size as usize / 2).saturating_sub(1);
        String::from_utf16_lossy(&buffer[..len])
    }
}

/// Check if our screensaver is the currently active one.
fn is_our_screensaver_set() -> bool {
    let current = get_current_screensaver().to_lowercase();
    let ours = get_scr_path()
        .to_string_lossy()
        .to_lowercase()
        .to_string();
    !current.is_empty() && current == ours
}

/// Update the status label and button states.
///
/// # Safety
/// All stored HWNDs must be valid (set during WM_CREATE).
unsafe fn update_status(hwnd: HWND) {
    let scr_path = get_scr_path();
    let scr_exists = scr_path.exists();

    let h_status = H_STATUS.get();
    let h_btn_set = H_BTN_SET.get();
    let h_btn_remove = H_BTN_REMOVE.get();
    let h_btn_preview = H_BTN_PREVIEW.get();

    unsafe {
        if !scr_exists {
            let _ = SetWindowTextW(h_status, w!("\u{26A0}  স্ক্রিনসেভার ফাইল পাওয়া যায়নি"));
            STATUS_COLOR.set(rgb(230, 170, 50));
            let _ = EnableWindow(h_btn_set, false);
            let _ = EnableWindow(h_btn_preview, false);
        } else if is_our_screensaver_set() {
            let _ = SetWindowTextW(h_status, w!("\u{2714}  বাংলাসেভার সক্রিয় আছে"));
            STATUS_COLOR.set(rgb(60, 179, 113));
            let _ = EnableWindow(h_btn_set, false);
            let _ = EnableWindow(h_btn_remove, true);
            let _ = EnableWindow(h_btn_preview, true);
        } else {
            let current = get_current_screensaver();
            if current.is_empty() {
                let _ = SetWindowTextW(
                    h_status,
                    w!("\u{2022}  কোনো স্ক্রিনসেভার সেট করা নেই"),
                );
                STATUS_COLOR.set(rgb(140, 148, 160));
            } else {
                let _ = SetWindowTextW(
                    h_status,
                    w!("\u{2022}  অন্য একটি স্ক্রিনসেভার সক্রিয় আছে"),
                );
                STATUS_COLOR.set(rgb(230, 170, 50));
            }
            let _ = EnableWindow(h_btn_set, true);
            let _ = EnableWindow(h_btn_remove, false);
            let _ = EnableWindow(h_btn_preview, true);
        }
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// Set bsaver as the screensaver via HKCU registry.
///
/// # Safety
/// `hwnd` must be a valid window handle.
unsafe fn set_screensaver(hwnd: HWND) {
    let scr_path = get_scr_path();
    let scr_path_str = scr_path.to_string_lossy().to_string();
    let wide: Vec<u16> = scr_path_str
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Control Panel\\Desktop");
        let res = RegOpenKeyExW(HKEY_CURRENT_USER, subkey, Some(0), KEY_SET_VALUE, &mut hkey);
        if res.is_err() {
            let _ = MessageBoxW(
                Some(hwnd),
                w!("রেজিস্ট্রি খোলা যায়নি।"),
                w!("সমস্যা"),
                MB_ICONERROR,
            );
            return;
        }

        let value_name = w!("SCRNSAVE.EXE");
        let data_bytes =
            std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
        let res = RegSetValueExW(hkey, value_name, Some(0), REG_SZ, Some(data_bytes));
        let _ = RegCloseKey(hkey);

        if res.is_err() {
            let _ = MessageBoxW(
                Some(hwnd),
                w!("রেজিস্ট্রি লেখা যায়নি।"),
                w!("সমস্যা"),
                MB_ICONERROR,
            );
            return;
        }

        // Enable screensaver in Windows
        let _ = SystemParametersInfoW(SPI_SETSCREENSAVEACTIVE, 1, None, SPIF_SENDCHANGE);

        update_status(hwnd);
        let _ = MessageBoxW(
            Some(hwnd),
            w!("বাংলাসেভার স্ক্রিনসেভার হিসেবে সেট হয়েছে!"),
            w!("সফল"),
            MB_ICONINFORMATION,
        );
    }
}

/// Remove the screensaver from registry.
///
/// # Safety
/// `hwnd` must be a valid window handle.
unsafe fn remove_screensaver(hwnd: HWND) {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Control Panel\\Desktop");
        let res = RegOpenKeyExW(HKEY_CURRENT_USER, subkey, Some(0), KEY_SET_VALUE, &mut hkey);
        if res.is_err() {
            let _ = MessageBoxW(
                Some(hwnd),
                w!("রেজিস্ট্রি খোলা যায়নি।"),
                w!("সমস্যা"),
                MB_ICONERROR,
            );
            return;
        }

        let _ = RegDeleteValueW(hkey, w!("SCRNSAVE.EXE"));
        let _ = RegCloseKey(hkey);

        // Disable screensaver in Windows
        let _ = SystemParametersInfoW(SPI_SETSCREENSAVEACTIVE, 0, None, SPIF_SENDCHANGE);

        update_status(hwnd);
        let _ = MessageBoxW(
            Some(hwnd),
            w!("স্ক্রিনসেভার সরিয়ে দেওয়া হয়েছে।"),
            w!("সরানো হয়েছে"),
            MB_ICONINFORMATION,
        );
    }
}

/// Open Windows screensaver settings control panel.
unsafe fn open_screensaver_settings() {
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            w!("control.exe"),
            w!("desk.cpl,,@screensaver"),
            None,
            SW_SHOW,
        );
    }
}

/// Launch bsaver in screensaver mode for preview.
unsafe fn preview_screensaver() {
    let scr_path = get_scr_path();
    let wide_path: Vec<u16> = scr_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let path_pcwstr = PCWSTR(wide_path.as_ptr());
        let _ = ShellExecuteW(None, w!("open"), path_pcwstr, w!("/s"), None, SW_SHOW);
    }
}

/// Launch bsaver in configure mode.
unsafe fn open_bsaver_settings() {
    let scr_path = get_scr_path();
    let wide_path: Vec<u16> = scr_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let path_pcwstr = PCWSTR(wide_path.as_ptr());
        let _ = ShellExecuteW(None, w!("open"), path_pcwstr, w!("/c"), None, SW_SHOW);
    }
}

/// Helper to convert &str to wide string buffer for DrawTextW.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Create a Win32 button control.
///
/// # Safety
/// `hwnd` must be a valid parent window handle.
unsafe fn create_button(
    hwnd: HWND,
    text: PCWSTR,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
) -> HWND {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            text,
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0000_0000),
            x,
            y,
            w,
            h,
            Some(hwnd),
            Some(HMENU(id as *mut _)),
            Some(GetModuleHandleW(None).unwrap().into()),
            None,
        )
        .unwrap_or_default()
    }
}

/// Draw the Bangladesh flag at the given position.
/// Green field with red circle offset slightly left of center.
///
/// # Safety
/// `hdc` must be a valid device context.
unsafe fn draw_flag(hdc: HDC, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        // Green background
        let green_brush = CreateSolidBrush(rgb(0, 106, 78));
        let flag_rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        FillRect(hdc, &flag_rc, green_brush);

        // Red circle (offset slightly left of center, radius ~1/5 of flag length)
        let circle_r = h * 2 / 5;
        let circle_cx = x + (w * 9 / 20); // slightly left of center per spec
        let circle_cy = y + h / 2;
        let red_brush = CreateSolidBrush(rgb(244, 42, 65));
        let null_pen = GetStockObject(NULL_PEN);
        let old_pen = SelectObject(hdc, null_pen);
        let old_brush = SelectObject(hdc, HGDIOBJ(red_brush.0));
        let _ = Ellipse(
            hdc,
            circle_cx - circle_r,
            circle_cy - circle_r,
            circle_cx + circle_r,
            circle_cy + circle_r,
        );
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ(green_brush.0));
        let _ = DeleteObject(HGDIOBJ(red_brush.0));
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            // Initialize GDI resources
            {
                let mut gdi = GDI.lock().unwrap();
                unsafe {
                    gdi.brush_bg = CreateSolidBrush(rgb(22, 27, 34));
                    gdi.font_title = make_font(36, FW_BOLD.0 as i32, w!("Segoe UI"));
                    gdi.font_subtitle = make_font(18, FW_NORMAL.0 as i32, w!("Segoe UI"));
                    gdi.font_normal = make_font(17, FW_NORMAL.0 as i32, w!("Segoe UI"));
                    gdi.font_btn = make_font(19, FW_SEMIBOLD.0 as i32, w!("Segoe UI"));
                    gdi.font_footer = make_font(13, FW_NORMAL.0 as i32, w!("Segoe UI"));
                }
            }
            let gdi = GDI.lock().unwrap();

            let mut client_rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut client_rect) };
            let client_w = client_rect.right;

            let btn_w = 340;
            let btn_h = 44;
            let left_margin = (client_w - btn_w) / 2;
            let mut y = 180;
            let gap = 12;

            unsafe {
                // Status label (SS_CENTER = 0x0001)
                let h_status = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0001),
                    left_margin - 20,
                    y,
                    btn_w + 40,
                    28,
                    Some(hwnd),
                    Some(HMENU(IDC_STATUS as *mut _)),
                    Some(GetModuleHandleW(None).unwrap().into()),
                    None,
                )
                .unwrap_or_default();
                set_control_font(h_status, gdi.font_normal);
                H_STATUS.set(h_status);
                y += 46;

                // Set Screensaver
                let h_btn_set = create_button(
                    hwnd,
                    w!("\u{25B6}  স্ক্রিনসেভার সেট করুন"),
                    left_margin,
                    y,
                    btn_w,
                    btn_h,
                    IDC_BTN_SET,
                );
                set_control_font(h_btn_set, gdi.font_btn);
                H_BTN_SET.set(h_btn_set);
                y += btn_h + gap;

                // Remove Screensaver
                let h_btn_remove = create_button(
                    hwnd,
                    w!("\u{2716}  স্ক্রিনসেভার সরান"),
                    left_margin,
                    y,
                    btn_w,
                    btn_h,
                    IDC_BTN_REMOVE,
                );
                set_control_font(h_btn_remove, gdi.font_btn);
                H_BTN_REMOVE.set(h_btn_remove);
                y += btn_h + gap;

                // Screensaver Settings (Windows control panel)
                let h_btn_settings = create_button(
                    hwnd,
                    w!("\u{2699}  স্ক্রিনসেভার সেটিংস"),
                    left_margin,
                    y,
                    btn_w,
                    btn_h,
                    IDC_BTN_SETTINGS,
                );
                set_control_font(h_btn_settings, gdi.font_btn);
                y += btn_h + gap;

                // Preview
                let h_btn_preview = create_button(
                    hwnd,
                    w!("\u{25B7}  প্রিভিউ দেখুন"),
                    left_margin,
                    y,
                    btn_w,
                    btn_h,
                    IDC_BTN_PREVIEW,
                );
                set_control_font(h_btn_preview, gdi.font_btn);
                H_BTN_PREVIEW.set(h_btn_preview);
                y += btn_h + gap;

                // Bsaver clock configuration
                let h_btn_bsettings = create_button(
                    hwnd,
                    w!("\u{2699}  ঘড়ি কনফিগারেশন"),
                    left_margin,
                    y,
                    btn_w,
                    btn_h,
                    IDC_BTN_BSETTINGS,
                );
                set_control_font(h_btn_bsettings, gdi.font_btn);
            }

            drop(gdi); // release lock before update_status
            unsafe { update_status(hwnd) };
            LRESULT(0)
        }

        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            unsafe {
                match id {
                    IDC_BTN_SET => set_screensaver(hwnd),
                    IDC_BTN_REMOVE => remove_screensaver(hwnd),
                    IDC_BTN_SETTINGS => open_screensaver_settings(),
                    IDC_BTN_PREVIEW => preview_screensaver(),
                    IDC_BTN_BSETTINGS => open_bsaver_settings(),
                    _ => {}
                }
            }
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as *mut _);
            let mut rc = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut rc) };
            let gdi = GDI.lock().unwrap();
            unsafe { FillRect(hdc, &rc, gdi.brush_bg) };
            LRESULT(1)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

            let mut client_rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut client_rect) };
            let client_w = client_rect.right;

            unsafe {
                SetBkMode(hdc, TRANSPARENT);

                // Draw Bangladesh flag (3:5 aspect ratio)
                let flag_h = 30;
                let flag_w = flag_h * 5 / 3;
                let flag_x = (client_w - flag_w) / 2;
                let flag_y = 22;
                draw_flag(hdc, flag_x, flag_y, flag_w, flag_h);

                let gdi = GDI.lock().unwrap();

                // Title: বাংলাসেভার
                let mut rc_title = RECT {
                    left: 0,
                    top: 60,
                    right: client_w,
                    bottom: 110,
                };
                SetTextColor(hdc, rgb(0, 106, 78));
                SelectObject(hdc, HGDIOBJ(gdi.font_title.0));
                DrawTextW(
                    hdc,
                    &mut to_wide("বাংলাসেভার"),
                    &mut rc_title,
                    DT_CENTER | DT_SINGLELINE,
                );

                // Subtitle: স্ক্রিনসেভার ম্যানেজার
                let mut rc_sub = RECT {
                    left: 0,
                    top: 110,
                    right: client_w,
                    bottom: 140,
                };
                SetTextColor(hdc, rgb(140, 148, 160));
                SelectObject(hdc, HGDIOBJ(gdi.font_subtitle.0));
                DrawTextW(
                    hdc,
                    &mut to_wide("স্ক্রিনসেভার ম্যানেজার"),
                    &mut rc_sub,
                    DT_CENTER | DT_SINGLELINE,
                );

                // Separator line
                let pen = CreatePen(PS_SOLID, 1, rgb(55, 62, 72));
                SelectObject(hdc, HGDIOBJ(pen.0));
                let _ = MoveToEx(hdc, 50, 155, None);
                let _ = LineTo(hdc, client_w - 50, 155);
                let _ = DeleteObject(HGDIOBJ(pen.0));

                // Footer
                let mut rc_footer = RECT {
                    left: 0,
                    top: client_rect.bottom - 32,
                    right: client_w,
                    bottom: client_rect.bottom - 8,
                };
                SetTextColor(hdc, rgb(100, 110, 125));
                SelectObject(hdc, HGDIOBJ(gdi.font_footer.0));
                DrawTextW(
                    hdc,
                    &mut to_wide("মাতৃভূমি অথবা মৃত্যু"),
                    &mut rc_footer,
                    DT_CENTER | DT_SINGLELINE,
                );

                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }

        WM_CTLCOLORSTATIC => {
            let hdc_static = HDC(wparam.0 as *mut _);
            let h_ctrl = HWND(lparam.0 as *mut _);
            unsafe {
                SetBkMode(hdc_static, TRANSPARENT);
                if h_ctrl == H_STATUS.get() {
                    SetTextColor(hdc_static, STATUS_COLOR.get());
                } else {
                    SetTextColor(hdc_static, rgb(220, 225, 232));
                }
            }
            let gdi = GDI.lock().unwrap();
            LRESULT(gdi.brush_bg.0 as isize)
        }

        WM_DESTROY => {
            let mut gdi = GDI.lock().unwrap();
            let fonts = [
                gdi.font_title,
                gdi.font_subtitle,
                gdi.font_normal,
                gdi.font_btn,
                gdi.font_footer,
            ];
            unsafe {
                for font in fonts {
                    if !font.is_invalid() {
                        let _ = DeleteObject(HGDIOBJ(font.0));
                    }
                }
                if !gdi.brush_bg.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(gdi.brush_bg.0));
                }
            }
            // Zero out to prevent double-free on any subsequent access
            *gdi = GdiResources::default();
            drop(gdi);
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn run_launcher() -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("BanglaSaverLauncher");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: window_class,
            ..Default::default()
        };

        RegisterClassW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let win_w = 480;
        let win_h = 580;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            window_class,
            w!("বাংলাসেভার"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            (screen_w - win_w) / 2,
            (screen_h - win_h) / 2,
            win_w,
            win_h,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

fn main() {
    if let Err(e) = run_launcher() {
        eprintln!("Error: {:?}", e);
    }
}
