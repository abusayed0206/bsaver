//! Clock time formatting

use crate::bangla_date::{BanglaDate, format_gregorian_date, get_bangla_weekday, to_bangla_digits};
use crate::config::Config;
use chrono::{Datelike, Local, Timelike};

/// Bangla time period names
const BANGLA_TIME_PERIODS: [&str; 6] = ["ভোর", "সকাল", "দুপুর", "বিকাল", "সন্ধ্যা", "রাত"];
const ENGLISH_TIME_PERIODS: [&str; 6] =
    ["Dawn", "Morning", "Noon", "Afternoon", "Evening", "Night"];

/// Get time period index based on hour
fn get_time_period_index(hour: u32) -> usize {
    match hour {
        4..=5 => 0,   // ভোর (Dawn)
        6..=11 => 1,  // সকাল (Morning)
        12..=14 => 2, // দুপুর (Noon)
        15..=17 => 3, // বিকাল (Afternoon)
        18..=19 => 4, // সন্ধ্যা (Evening)
        _ => 5,       // রাত (Night)
    }
}

/// Get the time period string (সকাল/দুপুর/বিকাল/রাত)
pub fn get_time_period_string(config: &Config) -> String {
    let now = Local::now();
    let hour = now.hour();
    let period_idx = get_time_period_index(hour);

    if config.use_bangla_names {
        BANGLA_TIME_PERIODS[period_idx].to_string()
    } else {
        ENGLISH_TIME_PERIODS[period_idx].to_string()
    }
}

/// Get the current time formatted according to config
pub fn get_time_string(config: &Config) -> String {
    let now = Local::now();
    let mut hour = now.hour();
    let minute = now.minute();
    let second = now.second();

    // Convert to 12-hour format if enabled
    if config.use_12_hour {
        hour = if hour == 0 {
            12
        } else if hour > 12 {
            hour - 12
        } else {
            hour
        };
    }

    let time_str = if config.show_seconds {
        format!("{:02}:{:02}:{:02}", hour, minute, second)
    } else {
        format!("{:02}:{:02}", hour, minute)
    };

    if config.use_bangla_numerals {
        to_bangla_digits(&time_str)
    } else {
        time_str
    }
}

/// Get the current day of week formatted according to config
/// If show_season is true, combines day and season on the same line
pub fn get_day_string(config: &Config) -> String {
    let now = Local::now();

    let day_name = if config.use_bangla_names {
        get_bangla_weekday(now).to_string()
    } else {
        const ENGLISH_DAYS: [&str; 7] = [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ];
        let weekday = now.weekday().num_days_from_sunday() as usize;
        ENGLISH_DAYS[weekday].to_string()
    };

    if config.show_season {
        // Combine day and season: "সোমবার, শীতকাল"
        let season = get_season_string(config);
        format!("{}, {}", day_name, season)
    } else {
        // Just day with comma (for consistency with date below)
        day_name
    }
}

/// Get the combined date string (English | Bangla or just one)
pub fn get_combined_date_string(config: &Config) -> String {
    let now = Local::now();
    let bangla_date = BanglaDate::from_local_with_region(now, config.calendar_region);

    let english_part = if config.show_english_date {
        Some(format_gregorian_date(
            now,
            config.use_bangla_names,
            config.use_bangla_numerals,
        ))
    } else {
        None
    };

    let bangla_part = if config.show_bangla_date {
        Some(bangla_date.format_bangla(config.use_bangla_numerals))
    } else {
        None
    };

    match (english_part, bangla_part) {
        (Some(eng), Some(ban)) => format!("{}  |  {}", eng, ban),
        (Some(eng), None) => eng,
        (None, Some(ban)) => ban,
        (None, None) => String::new(),
    }
}

/// Get the season string
pub fn get_season_string(config: &Config) -> String {
    let now = Local::now();
    let bangla_date = BanglaDate::from_local_with_region(now, config.calendar_region);

    if config.use_bangla_names {
        bangla_date.get_season().to_string()
    } else {
        // English season names
        const ENGLISH_SEASONS: [&str; 6] = [
            "Summer",
            "Monsoon",
            "Autumn",
            "Late Autumn",
            "Winter",
            "Spring",
        ];
        ENGLISH_SEASONS[bangla_date.month / 2].to_string()
    }
}
