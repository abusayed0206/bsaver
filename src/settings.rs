//! Settings dialog for the screensaver

use windows::{
    Win32::Foundation::*, Win32::Graphics::Gdi::*, Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*, core::*,
};

use crate::config::Config;

// Button style constants
const BS_PUSHBUTTON: u32 = 0x00000000;
const BS_LEFT: u32 = 0x00000100;

// Button IDs for settings dialog
const BTN_SECONDS: i32 = 101;
const BTN_ENGLISH_DATE: i32 = 102;
const BTN_BANGLA_DATE: i32 = 103;
const BTN_DAY: i32 = 104;
const BTN_TIME_PERIOD: i32 = 105;
const BTN_SEASON: i32 = 106;
const BTN_12_HOUR: i32 = 107;
const BTN_BANGLA_NUMS: i32 = 108;
const BTN_BANGLA_NAMES: i32 = 109;
const BTN_CALENDAR_REGION: i32 = 110;
const BTN_FONT_SIZE: i32 = 111;
const BTN_CLOSE: i32 = 112;

/// Show configuration dialog with toggle buttons
pub fn show_config_dialog() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = w!("BsaverSettings");

        let wc = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance.into(),
            lpszClassName: window_class,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(settings_wndproc),
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as *mut _),
            ..Default::default()
        };

        RegisterClassW(&wc);

        // Center window on screen - increased height for more options
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        let win_width = 360;
        let win_height = 560;
        let x = (screen_width - win_width) / 2;
        let y = (screen_height - win_height) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            window_class,
            w!("Bsaver Settings - বাংলা ঘড়ি"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            x,
            y,
            win_width,
            win_height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        // Run message loop for settings dialog
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

/// Settings window procedure
extern "system" fn settings_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_CREATE => {
                create_settings_controls(hwnd);
                LRESULT(0)
            }

            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                handle_settings_command(hwnd, id);
                LRESULT(0)
            }

            WM_CLOSE | WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

/// Create toggle buttons for settings
fn create_settings_controls(hwnd: HWND) {
    let config = Config::load();

    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();

        let y_start = 15;
        let y_gap = 36;
        let btn_width = 310;
        let btn_height = 28;
        let x = 15;

        // Time display options
        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start,
            btn_width,
            btn_height,
            BTN_SECONDS,
            "Seconds (সেকেন্ড)",
            config.show_seconds,
        );

        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap,
            btn_width,
            btn_height,
            BTN_TIME_PERIOD,
            "Time Period (সময়কাল)",
            config.show_time_period,
        );

        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 2,
            btn_width,
            btn_height,
            BTN_DAY,
            "Day (বার)",
            config.show_day,
        );

        // Date options
        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 3,
            btn_width,
            btn_height,
            BTN_ENGLISH_DATE,
            "English Date (ইংরেজি তারিখ)",
            config.show_english_date,
        );

        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 4,
            btn_width,
            btn_height,
            BTN_BANGLA_DATE,
            "Bangla Date (বাংলা তারিখ)",
            config.show_bangla_date,
        );

        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 5,
            btn_width,
            btn_height,
            BTN_SEASON,
            "Season (ঋতু)",
            config.show_season,
        );

        // Format options
        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 6,
            btn_width,
            btn_height,
            BTN_12_HOUR,
            "12-Hour Format (১২ ঘণ্টা)",
            config.use_12_hour,
        );

        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 7,
            btn_width,
            btn_height,
            BTN_BANGLA_NUMS,
            "Bangla Numbers (বাংলা সংখ্যা)",
            config.use_bangla_numerals,
        );

        create_toggle_button(
            hwnd,
            instance.into(),
            x,
            y_start + y_gap * 8,
            btn_width,
            btn_height,
            BTN_BANGLA_NAMES,
            "Bangla Names (বাংলা নাম)",
            config.use_bangla_names,
        );

        // Calendar region button (toggles between Bangladesh and India)
        let region_text = format!("Calendar: {} ▼", config.calendar_region.display_name());
        let region_wide: Vec<u16> = region_text
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let btn_style = WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32);
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(region_wide.as_ptr()),
            btn_style,
            x,
            y_start + y_gap * 9,
            btn_width,
            btn_height,
            Some(hwnd),
            Some(HMENU(BTN_CALENDAR_REGION as *mut _)),
            Some(instance.into()),
            None,
        );

        // Font size button (cycles through sizes)
        let font_text = format!("Font Size: {} ▼", config.font_size.display_name_en());
        let font_wide: Vec<u16> = font_text.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(font_wide.as_ptr()),
            btn_style,
            x,
            y_start + y_gap * 10,
            btn_width,
            btn_height,
            Some(hwnd),
            Some(HMENU(BTN_FONT_SIZE as *mut _)),
            Some(instance.into()),
            None,
        );

        // Close button
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Close"),
            btn_style,
            x + 105,
            y_start + y_gap * 11 + 15,
            100,
            35,
            Some(hwnd),
            Some(HMENU(BTN_CLOSE as *mut _)),
            Some(instance.into()),
            None,
        );
    }
}

/// Create a toggle button with ON/OFF indicator
fn create_toggle_button(
    hwnd: HWND,
    instance: HINSTANCE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: i32,
    label: &str,
    is_on: bool,
) -> Option<HWND> {
    let indicator = if is_on { "●" } else { "○" };
    let text = format!("{} {}", indicator, label);
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(text_wide.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32 | BS_LEFT as u32),
            x,
            y,
            width,
            height,
            Some(hwnd),
            Some(HMENU(id as *mut _)),
            Some(instance),
            None,
        )
        .ok()
    }
}

/// Handle button clicks in settings dialog
fn handle_settings_command(hwnd: HWND, id: i32) {
    let mut config = Config::load();

    match id {
        BTN_SECONDS => {
            config.show_seconds = !config.show_seconds;
            config.save();
            update_toggle_button(hwnd, BTN_SECONDS, "Seconds (সেকেন্ড)", config.show_seconds);
        }
        BTN_ENGLISH_DATE => {
            config.show_english_date = !config.show_english_date;
            config.save();
            update_toggle_button(
                hwnd,
                BTN_ENGLISH_DATE,
                "English Date (ইংরেজি তারিখ)",
                config.show_english_date,
            );
        }
        BTN_BANGLA_DATE => {
            config.show_bangla_date = !config.show_bangla_date;
            config.save();
            update_toggle_button(
                hwnd,
                BTN_BANGLA_DATE,
                "Bangla Date (বাংলা তারিখ)",
                config.show_bangla_date,
            );
        }
        BTN_DAY => {
            config.show_day = !config.show_day;
            config.save();
            update_toggle_button(hwnd, BTN_DAY, "Day (বার)", config.show_day);
        }
        BTN_TIME_PERIOD => {
            config.show_time_period = !config.show_time_period;
            config.save();
            update_toggle_button(
                hwnd,
                BTN_TIME_PERIOD,
                "Time Period (সময়কাল)",
                config.show_time_period,
            );
        }
        BTN_SEASON => {
            config.show_season = !config.show_season;
            config.save();
            update_toggle_button(hwnd, BTN_SEASON, "Season (ঋতু)", config.show_season);
        }
        BTN_12_HOUR => {
            config.use_12_hour = !config.use_12_hour;
            config.save();
            update_toggle_button(
                hwnd,
                BTN_12_HOUR,
                "12-Hour Format (১২ ঘণ্টা)",
                config.use_12_hour,
            );
        }
        BTN_BANGLA_NUMS => {
            config.use_bangla_numerals = !config.use_bangla_numerals;
            config.save();
            update_toggle_button(
                hwnd,
                BTN_BANGLA_NUMS,
                "Bangla Numbers (বাংলা সংখ্যা)",
                config.use_bangla_numerals,
            );
        }
        BTN_BANGLA_NAMES => {
            config.use_bangla_names = !config.use_bangla_names;
            config.save();
            update_toggle_button(
                hwnd,
                BTN_BANGLA_NAMES,
                "Bangla Names (বাংলা নাম)",
                config.use_bangla_names,
            );
        }
        BTN_CALENDAR_REGION => {
            config.calendar_region = config.calendar_region.toggle();
            config.save();
            let text = format!("Calendar: {} ▼", config.calendar_region.display_name());
            let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                if let Ok(btn) = GetDlgItem(Some(hwnd), BTN_CALENDAR_REGION) {
                    let _ = SetWindowTextW(btn, PCWSTR(text_wide.as_ptr()));
                }
            }
        }
        BTN_FONT_SIZE => {
            config.font_size = config.font_size.next();
            config.save();
            let text = format!("Font Size: {} ▼", config.font_size.display_name_en());
            let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                if let Ok(btn) = GetDlgItem(Some(hwnd), BTN_FONT_SIZE) {
                    let _ = SetWindowTextW(btn, PCWSTR(text_wide.as_ptr()));
                }
            }
        }
        BTN_CLOSE => unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        },
        _ => {}
    }
}

/// Update toggle button text with new state
fn update_toggle_button(hwnd: HWND, id: i32, label: &str, is_on: bool) {
    let indicator = if is_on { "●" } else { "○" };
    let text = format!("{} {}", indicator, label);
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        if let Ok(btn) = GetDlgItem(Some(hwnd), id) {
            let _ = SetWindowTextW(btn, PCWSTR(text_wide.as_ptr()));
        }
    }
}
