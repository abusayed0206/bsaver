//! BanglaSaver Launcher - UI for managing the screensaver
//!
//! Provides a Win32 window with buttons to:
//! - Install bsaver as a system screensaver (copies to System32, needs elevation)
//! - Remove the screensaver (deletes from System32, needs elevation)
//! - Open Windows screensaver settings
//! - Preview the screensaver
//!
//! The `.scr` file must live in `%WINDIR%\System32` for the Windows
//! Screensaver Settings dialog to enumerate it. Copying there requires
//! admin privileges, so the launcher requests UAC elevation via
//! `ShellExecuteW` with the `"runas"` verb.
//!
//! Registry writes use `reg.exe` (a system binary outside the MSIX
//! container) so that `HKCU\Control Panel\Desktop` modifications are
//! visible to the Windows screensaver service and not virtualized.
//!
//! # Safety
//! This module uses Win32 APIs extensively. All unsafe operations are wrapped
//! in explicit `unsafe {}` blocks per Rust 2024 edition requirements.

#![windows_subsystem = "windows"]

use std::cell::Cell;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;
use std::sync::Mutex;
use windows::{
    Win32::Foundation::*, Win32::Graphics::Gdi::*, Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Controls::DRAWITEMSTRUCT, Win32::UI::Input::KeyboardAndMouse::EnableWindow,
    Win32::UI::Shell::ShellExecuteW, Win32::UI::WindowsAndMessaging::*, core::*,
};

// Button IDs
const IDC_BTN_SET: i32 = 1001;
const IDC_BTN_REMOVE: i32 = 1002;
const IDC_BTN_SETTINGS: i32 = 1003;
const IDC_BTN_PREVIEW: i32 = 1004;
const IDC_BTN_BSETTINGS: i32 = 1005;
const IDC_STATUS: i32 = 1006;
const IDC_BTN_GITHUB: i32 = 1007;

/// Embedded Ekush font data — the same Bangla font used by the screensaver.
const EKUSH_FONT: &[u8] = include_bytes!("../font/Ekush-Regular.ttf");

/// Prevent child `reg.exe` process from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x08000000;

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

static GDI: LazyLock<Mutex<GdiResources>> = LazyLock::new(|| Mutex::new(GdiResources::default()));

/// Window background color (#161B22).
const BG_COLOR: COLORREF = rgb(22, 27, 34);

/// Colors for an owner-drawn button.
struct ButtonColors {
    bg: COLORREF,
    bg_pressed: COLORREF,
    bg_disabled: COLORREF,
    text: COLORREF,
    text_disabled: COLORREF,
    border: COLORREF,
}

/// Return the color scheme for a given button control ID.
fn get_button_colors(ctl_id: u32) -> ButtonColors {
    match ctl_id as i32 {
        IDC_BTN_SET => ButtonColors {
            bg: rgb(35, 134, 54), // green primary
            bg_pressed: rgb(27, 107, 43),
            bg_disabled: rgb(28, 56, 36),
            text: rgb(255, 255, 255),
            text_disabled: rgb(100, 140, 110),
            border: rgb(46, 160, 67),
        },
        IDC_BTN_REMOVE => ButtonColors {
            bg: rgb(55, 30, 34), // muted red card
            bg_pressed: rgb(45, 24, 28),
            bg_disabled: rgb(35, 28, 30),
            text: rgb(248, 81, 73),
            text_disabled: rgb(100, 60, 58),
            border: rgb(75, 40, 44),
        },
        IDC_BTN_PREVIEW => ButtonColors {
            bg: rgb(26, 46, 68), // blue card
            bg_pressed: rgb(20, 38, 56),
            bg_disabled: rgb(24, 32, 40),
            text: rgb(88, 166, 255),
            text_disabled: rgb(60, 80, 100),
            border: rgb(36, 60, 88),
        },
        IDC_BTN_BSETTINGS => ButtonColors {
            bg: rgb(45, 26, 68), // purple card
            bg_pressed: rgb(37, 21, 56),
            bg_disabled: rgb(32, 26, 40),
            text: rgb(188, 140, 255),
            text_disabled: rgb(80, 60, 100),
            border: rgb(60, 36, 88),
        },
        IDC_BTN_GITHUB => ButtonColors {
            bg: rgb(33, 38, 45), // dark neutral card
            bg_pressed: rgb(27, 31, 36),
            bg_disabled: rgb(28, 32, 39),
            text: rgb(139, 148, 158),
            text_disabled: rgb(72, 79, 88),
            border: rgb(48, 54, 61),
        },
        _ => ButtonColors {
            // neutral (Settings)
            bg: rgb(33, 38, 45),
            bg_pressed: rgb(27, 31, 36),
            bg_disabled: rgb(28, 32, 39),
            text: rgb(201, 209, 217),
            text_disabled: rgb(72, 79, 88),
            border: rgb(48, 54, 61),
        },
    }
}

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

/// Get the path to bsaver.exe alongside this executable (source for install).
fn get_scr_path() -> PathBuf {
    let mut exe_path = std::env::current_exe().unwrap_or_default();
    exe_path.pop();
    exe_path.push("bsaver.exe");
    exe_path
}

/// Get the destination path in System32 where the `.scr` must be installed
/// for Windows Screensaver Settings to enumerate it.
fn get_system32_scr_path() -> PathBuf {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    PathBuf::from(windir)
        .join("System32")
        .join("BanglaSaver.scr")
}

/// Read current screensaver path from HKCU registry.
///
/// Uses `reg.exe` (a system binary outside the MSIX container) so that we
/// always read the **real** registry, not the per-app virtualized hive.
fn get_current_screensaver() -> String {
    let Ok(output) = Command::new("reg")
        .args(["query", r"HKCU\Control Panel\Desktop", "/v", "SCRNSAVE.EXE"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return String::new();
    };

    if !output.status.success() {
        return String::new();
    }

    // Parse: "    SCRNSAVE.EXE    REG_SZ    C:\path\to\file.scr"
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find("REG_SZ") {
            let value = trimmed[pos + "REG_SZ".len()..].trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    String::new()
}

/// Check if our screensaver is the currently active one.
/// Compares the registry value against the System32 installation path.
fn is_our_screensaver_set() -> bool {
    let current = get_current_screensaver().to_lowercase();
    let system32_path = get_system32_scr_path().to_string_lossy().to_lowercase();
    !current.is_empty() && current == system32_path
}

/// Update the status label and button states.
///
/// # Safety
/// All stored HWNDs must be valid (set during WM_CREATE).
unsafe fn update_status(hwnd: HWND) {
    let src_exists = get_scr_path().exists();
    let installed = get_system32_scr_path().exists();
    let active = is_our_screensaver_set();

    let h_status = H_STATUS.get();
    let h_btn_set = H_BTN_SET.get();
    let h_btn_remove = H_BTN_REMOVE.get();
    let h_btn_preview = H_BTN_PREVIEW.get();

    unsafe {
        if !src_exists {
            let _ = SetWindowTextW(h_status, w!("\u{26A0}  স্ক্রিনসেভার ফাইল পাওয়া যায়নি"));
            STATUS_COLOR.set(rgb(230, 170, 50));
            let _ = EnableWindow(h_btn_set, false);
            let _ = EnableWindow(h_btn_remove, installed);
            let _ = EnableWindow(h_btn_preview, false);
        } else if installed && active {
            let _ = SetWindowTextW(h_status, w!("\u{2714}  বাংলাসেভার সক্রিয় আছে"));
            STATUS_COLOR.set(rgb(60, 179, 113));
            let _ = EnableWindow(h_btn_set, false);
            let _ = EnableWindow(h_btn_remove, true);
            let _ = EnableWindow(h_btn_preview, true);
        } else if installed {
            let _ = SetWindowTextW(h_status, w!("\u{2022}  ইনস্টল আছে, তবে সক্রিয় নয়"));
            STATUS_COLOR.set(rgb(230, 170, 50));
            let _ = EnableWindow(h_btn_set, true);
            let _ = EnableWindow(h_btn_remove, true);
            let _ = EnableWindow(h_btn_preview, true);
        } else {
            let current = get_current_screensaver();
            if current.is_empty() {
                let _ = SetWindowTextW(h_status, w!("\u{2022}  কোনো স্ক্রিনসেভার সেট করা নেই"));
                STATUS_COLOR.set(rgb(140, 148, 160));
            } else {
                let _ = SetWindowTextW(h_status, w!("\u{2022}  অন্য একটি স্ক্রিনসেভার সক্রিয় আছে"));
                STATUS_COLOR.set(rgb(230, 170, 50));
            }
            let _ = EnableWindow(h_btn_set, true);
            let _ = EnableWindow(h_btn_remove, false);
            let _ = EnableWindow(h_btn_preview, true);
        }
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// Install the screensaver to System32 and activate it.
///
/// 1. Requests UAC elevation via `ShellExecuteW("runas")`
/// 2. Copies `bsaver.exe` → `%WINDIR%\System32\BanglaSaver.scr`
/// 3. Sets `HKCU\Control Panel\Desktop\SCRNSAVE.EXE` via `reg.exe`
/// 4. Enables the screensaver (`ScreenSaveActive = 1`)
///
/// The copy to System32 is required because the Windows Screensaver
/// Settings dialog only enumerates `.scr` files from that directory.
/// Registry writes use `reg.exe` (not direct Win32 API) to bypass
/// MSIX registry virtualization.
///
/// # Safety
/// `hwnd` must be a valid window handle.
unsafe fn set_screensaver(hwnd: HWND) {
    let src = get_scr_path();
    if !src.exists() {
        unsafe {
            let _ = MessageBoxW(
                Some(hwnd),
                w!("bsaver.exe পাওয়া যায়নি।"),
                w!("সমস্যা"),
                MB_ICONERROR,
            );
        }
        return;
    }

    let dst = get_system32_scr_path();
    let dst_str = dst.to_string_lossy();

    // Build an elevated cmd.exe command that:
    //   1. Copies the binary to System32 as a .scr
    //   2. Sets SCRNSAVE.EXE registry value to the System32 path
    //   3. Enables the screensaver
    //   4. Sets a default timeout (300 seconds = 5 minutes) so the
    //      screensaver actually activates after idle — without this,
    //      Windows may never trigger it.
    let args = format!(
        r#"/c copy /Y "{}" "{}" && reg add "HKCU\Control Panel\Desktop" /v SCRNSAVE.EXE /t REG_SZ /d "{}" /f && reg add "HKCU\Control Panel\Desktop" /v ScreenSaveActive /t REG_SZ /d 1 /f && reg add "HKCU\Control Panel\Desktop" /v ScreenSaveTimeOut /t REG_SZ /d 300 /f"#,
        src.display(),
        dst_str,
        dst_str
    );
    let wide_args: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        // Request elevation — shows the UAC consent dialog
        let ret = ShellExecuteW(
            Some(hwnd),
            w!("runas"),
            w!("cmd.exe"),
            PCWSTR(wide_args.as_ptr()),
            None,
            SW_HIDE,
        );

        // ShellExecuteW returns > 32 on success
        if (ret.0 as usize) <= 32 {
            // User cancelled UAC or an error occurred
            return;
        }

        // Poll for System32 copy to appear (cmd.exe runs async)
        for _ in 0..25 {
            if dst.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        update_status(hwnd);

        if dst.exists() {
            // Notify Windows that screensaver settings changed.
            // Without this, the system may not pick up the registry
            // changes until the next logon.
            let _ = SystemParametersInfoW(
                SPI_SETSCREENSAVEACTIVE,
                1, // TRUE — enable
                None,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0x01 | 0x02), // SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
            );
            let _ = MessageBoxW(
                Some(hwnd),
                w!("বাংলাসেভার স্ক্রিনসেভার হিসেবে সেট হয়েছে!"),
                w!("সফল"),
                MB_ICONINFORMATION,
            );
        } else {
            let _ = MessageBoxW(
                Some(hwnd),
                w!("ইনস্টল করা যায়নি। অনুগ্রহ করে আবার চেষ্টা করুন।"),
                w!("সমস্যা"),
                MB_ICONERROR,
            );
        }
    }
}

/// Remove the screensaver from System32 and deactivate it.
///
/// 1. Requests UAC elevation
/// 2. Deletes `BanglaSaver.scr` from System32
/// 3. Removes `SCRNSAVE.EXE` registry value
/// 4. Disables the screensaver (`ScreenSaveActive = 0`)
///
/// # Safety
/// `hwnd` must be a valid window handle.
unsafe fn remove_screensaver(hwnd: HWND) {
    let dst = get_system32_scr_path();
    let dst_str = dst.to_string_lossy();

    // Elevated command: delete .scr + clean registry
    let args = format!(
        r#"/c del /F "{}" && reg delete "HKCU\Control Panel\Desktop" /v SCRNSAVE.EXE /f && reg add "HKCU\Control Panel\Desktop" /v ScreenSaveActive /t REG_SZ /d 0 /f"#,
        dst_str
    );
    let wide_args: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let ret = ShellExecuteW(
            Some(hwnd),
            w!("runas"),
            w!("cmd.exe"),
            PCWSTR(wide_args.as_ptr()),
            None,
            SW_HIDE,
        );

        if (ret.0 as usize) <= 32 {
            return;
        }

        // Wait for deletion to complete
        for _ in 0..25 {
            if !dst.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        update_status(hwnd);

        if !dst.exists() {
            // Notify Windows that the screensaver has been deactivated.
            let _ = SystemParametersInfoW(
                SPI_SETSCREENSAVEACTIVE,
                0, // FALSE — disable
                None,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0x01 | 0x02), // SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
            );
            let _ = MessageBoxW(
                Some(hwnd),
                w!("স্ক্রিনসেভার সরিয়ে দেওয়া হয়েছে।"),
                w!("সরানো হয়েছে"),
                MB_ICONINFORMATION,
            );
        } else {
            let _ = MessageBoxW(
                Some(hwnd),
                w!("সরানো যায়নি। অনুগ্রহ করে আবার চেষ্টা করুন।"),
                w!("সমস্যা"),
                MB_ICONERROR,
            );
        }
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

/// Open the GitHub repository page in the default browser.
unsafe fn open_github() {
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            w!("https://github.com/abusayed0206/bsaver"),
            None,
            None,
            SW_SHOW,
        );
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
unsafe fn create_button(hwnd: HWND, text: PCWSTR, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    // BS_OWNERDRAW = 0x0B — we paint the button ourselves in WM_DRAWITEM
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            text,
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0000_000B),
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
                    gdi.brush_bg = CreateSolidBrush(BG_COLOR);
                    gdi.font_title = make_font(38, FW_BOLD.0 as i32, w!("Ekush"));
                    gdi.font_subtitle = make_font(18, FW_NORMAL.0 as i32, w!("Ekush"));
                    gdi.font_normal = make_font(17, FW_NORMAL.0 as i32, w!("Ekush"));
                    gdi.font_btn = make_font(18, FW_SEMIBOLD.0 as i32, w!("Ekush"));
                    gdi.font_footer = make_font(13, FW_NORMAL.0 as i32, w!("Ekush"));
                }
            }
            let gdi = GDI.lock().unwrap();

            let mut client_rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut client_rect) };
            let client_w = client_rect.right;

            // ── Grid layout constants ──
            let margin = 40;
            let content_w = client_w - margin * 2;
            let col_gap = 14;
            let col_w = (content_w - col_gap) / 2;
            let row_gap = 12;
            let grid_btn_h = 58;
            let x_left = margin;
            let x_right = margin + col_w + col_gap;

            unsafe {
                // ── Status label (SS_CENTER = 0x0001) ──
                let h_status = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0001),
                    margin,
                    148,
                    content_w,
                    28,
                    Some(hwnd),
                    Some(HMENU(IDC_STATUS as *mut _)),
                    Some(GetModuleHandleW(None).unwrap().into()),
                    None,
                )
                .unwrap_or_default();
                set_control_font(h_status, gdi.font_normal);
                H_STATUS.set(h_status);

                // ── Row 1: Set · Remove ──
                let y1 = 186;
                let h_btn_set = create_button(
                    hwnd,
                    w!("সেট করুন"),
                    x_left,
                    y1,
                    col_w,
                    grid_btn_h,
                    IDC_BTN_SET,
                );
                H_BTN_SET.set(h_btn_set);

                let h_btn_remove = create_button(
                    hwnd,
                    w!("সরান"),
                    x_right,
                    y1,
                    col_w,
                    grid_btn_h,
                    IDC_BTN_REMOVE,
                );
                H_BTN_REMOVE.set(h_btn_remove);

                // ── Row 2: Preview · Clock Config ──
                let y2 = y1 + grid_btn_h + row_gap;
                let h_btn_preview = create_button(
                    hwnd,
                    w!("প্রিভিউ দেখুন"),
                    x_left,
                    y2,
                    col_w,
                    grid_btn_h,
                    IDC_BTN_PREVIEW,
                );
                H_BTN_PREVIEW.set(h_btn_preview);

                let _h_btn_bsettings = create_button(
                    hwnd,
                    w!("ঘড়ি সেটিংস"),
                    x_right,
                    y2,
                    col_w,
                    grid_btn_h,
                    IDC_BTN_BSETTINGS,
                );

                // ── Row 3: Settings · GitHub ──
                let y3 = y2 + grid_btn_h + row_gap;
                let _h_btn_settings = create_button(
                    hwnd,
                    w!("স্ক্রিনসেভার সেটিংস"),
                    x_left,
                    y3,
                    col_w,
                    grid_btn_h,
                    IDC_BTN_SETTINGS,
                );

                let _h_btn_github = create_button(
                    hwnd,
                    w!("গিটহাব"),
                    x_right,
                    y3,
                    col_w,
                    grid_btn_h,
                    IDC_BTN_GITHUB,
                );
            }

            drop(gdi); // release lock before update_status
            unsafe { update_status(hwnd) };
            LRESULT(0)
        }

        WM_DRAWITEM => {
            // Owner-drawn button painting with rounded rectangles & accent colors
            let dis = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            let id = dis.CtlID;
            let hdc = dis.hDC;
            let rc = dis.rcItem;

            // ODS_SELECTED = 0x0001, ODS_DISABLED = 0x0004
            let is_pressed = (dis.itemState.0 & 0x0001) != 0;
            let is_disabled = (dis.itemState.0 & 0x0004) != 0;

            let colors = get_button_colors(id);
            let bg = if is_disabled {
                colors.bg_disabled
            } else if is_pressed {
                colors.bg_pressed
            } else {
                colors.bg
            };
            let txt = if is_disabled {
                colors.text_disabled
            } else {
                colors.text
            };

            unsafe {
                // Clear the entire rect with parent bg so rounded corners look clean
                let parent_brush = CreateSolidBrush(BG_COLOR);
                FillRect(hdc, &rc, parent_brush);
                let _ = DeleteObject(HGDIOBJ(parent_brush.0));

                // Rounded rectangle button face
                let btn_brush = CreateSolidBrush(bg);
                let border_pen = CreatePen(PS_SOLID, 1, colors.border);
                let old_brush = SelectObject(hdc, HGDIOBJ(btn_brush.0));
                let old_pen = SelectObject(hdc, HGDIOBJ(border_pen.0));
                let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 14, 14);
                SelectObject(hdc, old_brush);
                SelectObject(hdc, old_pen);
                let _ = DeleteObject(HGDIOBJ(btn_brush.0));
                let _ = DeleteObject(HGDIOBJ(border_pen.0));

                // Text
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, txt);
                let gdi = GDI.lock().unwrap();
                SelectObject(hdc, HGDIOBJ(gdi.font_btn.0));

                let mut text_buf = [0u16; 256];
                let len = GetWindowTextW(dis.hwndItem, &mut text_buf);
                let mut text_slice = text_buf[..len as usize].to_vec();

                let mut text_rc = RECT {
                    left: rc.left + 12,
                    top: rc.top,
                    right: rc.right - 12,
                    bottom: rc.bottom,
                };
                DrawTextW(
                    hdc,
                    &mut text_slice,
                    &mut text_rc,
                    DT_CENTER | DT_SINGLELINE | DT_VCENTER,
                );

                // Subtle focus indicator (thin inner border)
                if (dis.itemState.0 & 0x0010) != 0 {
                    // ODS_FOCUS
                    let focus_pen = CreatePen(PS_DOT, 1, colors.text);
                    let ofp = SelectObject(hdc, HGDIOBJ(focus_pen.0));
                    let null_brush = GetStockObject(NULL_BRUSH);
                    let ofb = SelectObject(hdc, null_brush);
                    let _ = RoundRect(
                        hdc,
                        rc.left + 3,
                        rc.top + 3,
                        rc.right - 3,
                        rc.bottom - 3,
                        10,
                        10,
                    );
                    SelectObject(hdc, ofp);
                    SelectObject(hdc, ofb);
                    let _ = DeleteObject(HGDIOBJ(focus_pen.0));
                }
            }
            LRESULT(1)
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
                    IDC_BTN_GITHUB => open_github(),
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

                // Footer
                let mut rc_footer = RECT {
                    left: 0,
                    top: client_rect.bottom - 30,
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

        // Register the embedded Ekush font with GDI so CreateFontW can find it.
        // AddFontMemResourceEx makes the font available for this process only —
        // no system-wide install, no cleanup needed (removed on process exit).
        let num_fonts: u32 = 0;
        let _font_handle = AddFontMemResourceEx(
            EKUSH_FONT.as_ptr() as *const _,
            EKUSH_FONT.len() as u32,
            None,
            &num_fonts,
        );

        // Load the embedded icon (resource ID 1, set by build.rs / winresource)
        #[allow(clippy::manual_dangling_ptr)]
        let icon_handle = LoadImageW(
            Some(instance.into()),
            PCWSTR(1 as *const u16), // MAKEINTRESOURCE(1)
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        );
        let hicon = if let Ok(h) = icon_handle {
            HICON(h.0)
        } else {
            LoadIconW(None, IDI_APPLICATION)?
        };

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: window_class,
            hIcon: hicon,
            ..Default::default()
        };

        RegisterClassW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        // Compute exact outer window size from desired client area.
        // Layout: flag(22+30) title(60-110) subtitle(110-140) status(148-176)
        // grid: y1=186, 3 rows×58h + 2 gaps×12 → bottom = 186+58×3+12×2 = 384
        // footer: 12px gap + 22px text + 8px padding → 384+42 = 426
        let client_w = 500;
        let client_h = 426;
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rc_size = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        let _ = AdjustWindowRectEx(&mut rc_size, style, false, WINDOW_EX_STYLE(0));
        let win_w = rc_size.right - rc_size.left;
        let win_h = rc_size.bottom - rc_size.top;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            window_class,
            w!("বাংলাসেভার"),
            style,
            (screen_w - win_w) / 2,
            (screen_h - win_h) / 2,
            win_w,
            win_h,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        // Set both large and small icons on the window
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(1)),
            Some(LPARAM(hicon.0 as isize)),
        ); // ICON_BIG
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(0)),
            Some(LPARAM(hicon.0 as isize)),
        ); // ICON_SMALL

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
    // Errors are silently ignored — stderr is unavailable with
    // #![windows_subsystem = "windows"].
    let _ = run_launcher();
}
