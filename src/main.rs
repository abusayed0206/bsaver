//! বাংলা ঘড়ি স্ক্রীনসেভার (Bangla Clock Screensaver)
//!
//! A beautiful digital clock screensaver for Windows featuring Bangla font support.
//!
//! # Features
//! - Digital clock with Bangla custom font (Ekush)
//! - Black background with white text
//! - Optional date and day display
//! - Bangla or English numerals and names
//! - Configurable via JSON settings file
//!
//! # Usage
//! - `/s` or no arguments: Run screensaver
//! - `/p <hwnd>`: Preview mode in settings dialog
//! - `/c`: Show configuration dialog
//!
//! # Configuration
//! Settings are stored in `%APPDATA%\abusayed\bsaver\config.json`

// Hide console window on Windows
#![windows_subsystem = "windows"]

mod bangla_date;
mod clock;
mod config;
mod renderer;
mod screensaver;
mod settings;

use screensaver::ScreensaverMode;

fn main() {
    let mode = ScreensaverMode::from_args();

    // Errors are silently ignored — stderr is not available with
    // #![windows_subsystem = "windows"] and a screensaver has no
    // reasonable way to report errors to the user.
    let _ = match mode {
        ScreensaverMode::Screensaver => screensaver::run_screensaver(),
        ScreensaverMode::Preview(hwnd) => screensaver::run_preview(hwnd),
        ScreensaverMode::Configure => settings::show_config_dialog(),
    };
}
