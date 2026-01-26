//! Configuration management for the screensaver

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Font size options for the clock
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum FontSize {
    /// Small font size
    Small,
    /// Regular font size (default)
    #[default]
    Regular,
    /// Larger font size
    Larger,
    /// Extra large font size
    ExtraLarge,
}

impl FontSize {
    /// Get the font size multiplier
    pub fn multiplier(&self) -> f32 {
        match self {
            FontSize::Small => 0.6,
            FontSize::Regular => 1.0,
            FontSize::Larger => 1.4,
            FontSize::ExtraLarge => 1.8,
        }
    }

    /// Get display name in English
    pub fn display_name_en(&self) -> &'static str {
        match self {
            FontSize::Small => "Small",
            FontSize::Regular => "Regular",
            FontSize::Larger => "Larger",
            FontSize::ExtraLarge => "Extra Large",
        }
    }

    /// Cycle to next size
    pub fn next(&self) -> Self {
        match self {
            FontSize::Small => FontSize::Regular,
            FontSize::Regular => FontSize::Larger,
            FontSize::Larger => FontSize::ExtraLarge,
            FontSize::ExtraLarge => FontSize::Small,
        }
    }
}

/// Calendar region for Bangla date calculation
/// Bangladesh uses April 14 as Pohela Boishakh (reformed 1987)
/// India uses April 15 as Pohela Boishakh (traditional)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum CalendarRegion {
    /// Bangladesh calendar (April 14 = Pohela Boishakh)
    #[default]
    Bangladesh,
    /// India/West Bengal calendar (April 15 = Pohela Boishakh)
    India,
}

impl CalendarRegion {
    /// Get Pohela Boishakh day (April day number)
    pub fn pohela_boishakh_day(&self) -> u32 {
        match self {
            CalendarRegion::Bangladesh => 14,
            CalendarRegion::India => 15,
        }
    }

    /// Get timezone offset in seconds from UTC
    /// Bangladesh: GMT+6 (21600 seconds)
    /// India: GMT+5:30 (19800 seconds)
    pub fn timezone_offset_seconds(&self) -> i32 {
        match self {
            CalendarRegion::Bangladesh => 6 * 3600,      // +6:00
            CalendarRegion::India => 5 * 3600 + 30 * 60, // +5:30
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            CalendarRegion::Bangladesh => "Bangladesh (14 Apr)",
            CalendarRegion::India => "India (15 Apr)",
        }
    }

    /// Toggle between regions
    pub fn toggle(&self) -> Self {
        match self {
            CalendarRegion::Bangladesh => CalendarRegion::India,
            CalendarRegion::India => CalendarRegion::Bangladesh,
        }
    }
}

/// Screensaver configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Show seconds in the clock
    pub show_seconds: bool,
    /// Show English/Gregorian date
    pub show_english_date: bool,
    /// Show Bangla calendar date
    pub show_bangla_date: bool,
    /// Show day of week
    pub show_day: bool,
    /// Show time period (সকাল/দুপুর/বিকাল/রাত)
    pub show_time_period: bool,
    /// Show season (ঋতু)
    pub show_season: bool,
    /// Use Bangla numerals (০১২৩৪৫৬৭৮৯)
    pub use_bangla_numerals: bool,
    /// Use Bangla day/month names
    pub use_bangla_names: bool,
    /// Use 12-hour format
    pub use_12_hour: bool,
    /// Calendar region (Bangladesh or India)
    pub calendar_region: CalendarRegion,
    /// Font size option
    pub font_size: FontSize,
    /// Text color (RGB)
    pub text_color: [u8; 3],
    /// Background color (RGB)
    pub background_color: [u8; 3],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_seconds: true,
            show_english_date: true,
            show_bangla_date: true,
            show_day: true,
            show_time_period: true,
            show_season: true,
            use_bangla_numerals: true,
            use_bangla_names: true,
            use_12_hour: true,
            calendar_region: CalendarRegion::Bangladesh,
            font_size: FontSize::Regular,
            text_color: [255, 255, 255], // White
            background_color: [0, 0, 0], // Black
        }
    }
}

impl Config {
    /// Get the configuration file path
    pub fn config_path() -> Option<PathBuf> {
        if let Some(proj_dirs) = directories::ProjectDirs::from("dev", "abusayed", "bsaver") {
            let config_dir = proj_dirs.config_dir();
            Some(config_dir.join("config.json"))
        } else {
            None
        }
    }

    /// Load configuration from file, or create default if not exists
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str(&content) {
                        return config;
                    }
                }
            }
        }
        let config = Self::default();
        config.save(); // Save default config
        config
    }

    /// Save configuration to file
    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(content) = serde_json::to_string_pretty(self) {
                let _ = fs::write(&path, content);
            }
        }
    }
}
