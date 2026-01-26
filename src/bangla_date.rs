//! Bangla calendar conversion and formatting
//!
//! Implements conversion from Gregorian to Bangla calendar dates,
//! including seasons, months, and proper formatting.
//!
//! Important: Bangla date is calculated based on the timezone of the calendar region:
//! - Bangladesh calendar uses GMT+6 (Bangladesh Standard Time)
//! - India calendar uses GMT+5:30 (Indian Standard Time)

use crate::config::CalendarRegion;
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, Utc};

/// Bangla month names
const BANGLA_MONTHS: [&str; 12] = [
    "বৈশাখ",   // Boishakh - April 14 - May 14
    "জ্যৈষ্ঠ",  // Jyoishtho - May 15 - June 14
    "আষাঢ়",    // Asharh - June 15 - July 15
    "শ্রাবণ",   // Shrabon - July 16 - August 15
    "ভাদ্র",    // Bhadro - August 16 - September 15
    "আশ্বিন",  // Ashshin - September 16 - October 15
    "কার্তিক",  // Kartik - October 16 - November 14
    "অগ্রহায়ণ", // Ogrohayon - November 15 - December 14
    "পৌষ",    // Poush - December 15 - January 13
    "মাঘ",     // Magh - January 14 - February 12
    "ফাল্গুন",   // Falgun - February 13 - March 13/14
    "চৈত্র",   // Choitro - March 14/15 - April 13
];

/// Bangla season names
const BANGLA_SEASONS: [&str; 6] = [
    "গ্রীষ্মকাল", // Summer (Boishakh, Jyoishtho)
    "বর্ষাকাল",   // Monsoon (Asharh, Shrabon)
    "শরৎকাল",   // Autumn (Bhadro, Ashshin)
    "হেমন্তকাল", // Late Autumn (Kartik, Ogrohayon)
    "শীতকাল",   // Winter (Poush, Magh)
    "বসন্তকাল",  // Spring (Falgun, Choitro)
];

/// Standard month lengths (revised Bangladesh calendar 2019)
/// First 5 months: 31 days, Last 7 months: 30 days
/// Choitro has 31 days in leap year
const MONTH_LENGTHS: [u8; 12] = [31, 31, 31, 31, 31, 30, 30, 30, 30, 30, 30, 30];

/// Bangla digits
const BANGLA_DIGITS: [char; 10] = ['০', '১', '২', '৩', '৪', '৫', '৬', '৭', '৮', '৯'];

/// Bangla ordinal suffixes for dates
fn get_bangla_ordinal_suffix(day: u8) -> &'static str {
    match day {
        1 => "লা",
        2 | 3 => "রা",
        4 => "ঠা",
        5 | 6 => "ই",
        7 => "ই",
        8 | 9 | 10 => "ই",
        11 | 12 => "ই",
        _ => "ই",
    }
}

/// Represents a Bangla calendar date
#[derive(Debug, Clone)]
pub struct BanglaDate {
    pub year: i32,
    pub month: usize, // 0-11 index
    pub day: u8,
}

impl BanglaDate {
    /// Convert system local time to Bangla date
    ///
    /// IMPORTANT: This converts the local time to the appropriate timezone first:
    /// - Bangladesh calendar: Converts to GMT+6 (Bangladesh Standard Time)
    /// - India calendar: Converts to GMT+5:30 (Indian Standard Time)
    ///
    /// This ensures users worldwide see the correct Bangla date based on the
    /// actual date/time in Bangladesh or India, not their local system time.
    pub fn from_local_with_region(dt: DateTime<Local>, region: CalendarRegion) -> Self {
        // Convert local time to UTC, then to the region's timezone
        let utc_time = dt.with_timezone(&Utc);
        let region_offset = FixedOffset::east_opt(region.timezone_offset_seconds()).unwrap();
        let region_time = utc_time.with_timezone(&region_offset);

        // Now calculate Bangla date based on the region's local date
        Self::from_date_with_region(region_time.date_naive(), region)
    }

    /// Convert a UTC DateTime to Bangla date for the specified region
    #[allow(dead_code)]
    pub fn from_utc_with_region(dt: DateTime<Utc>, region: CalendarRegion) -> Self {
        let region_offset = FixedOffset::east_opt(region.timezone_offset_seconds()).unwrap();
        let region_time = dt.with_timezone(&region_offset);
        Self::from_date_with_region(region_time.date_naive(), region)
    }

    /// Convert a NaiveDate (already in the correct timezone) to Bangla date
    pub fn from_date_with_region(current_date: NaiveDate, region: CalendarRegion) -> Self {
        let g_year = current_date.year();
        let pohela_day = region.pohela_boishakh_day();

        // Pohela Boishakh date varies by region
        let pohela_boishakh = NaiveDate::from_ymd_opt(g_year, 4, pohela_day).unwrap();

        let is_before_pohela = current_date < pohela_boishakh;
        let bangla_year = if is_before_pohela {
            g_year - 594
        } else {
            g_year - 593
        };

        // Reference year for calculation
        let ref_year = if is_before_pohela { g_year - 1 } else { g_year };
        let ref_pohela = NaiveDate::from_ymd_opt(ref_year, 4, pohela_day).unwrap();

        // Days since Pohela Boishakh
        let day_diff = (current_date - ref_pohela).num_days() as i32;

        // Handle negative day difference (shouldn't happen with correct logic)
        if day_diff < 0 {
            return BanglaDate {
                year: bangla_year,
                month: 0,
                day: 1,
            };
        }

        // Get month lengths for this Bangla year
        let month_lengths = Self::get_month_lengths(ref_year + 1);

        let mut remaining_days = day_diff;
        let mut month_index = 0;

        // Find the month
        while month_index < 12 && remaining_days >= month_lengths[month_index] as i32 {
            remaining_days -= month_lengths[month_index] as i32;
            month_index += 1;
        }

        // Handle overflow to next year
        if month_index >= 12 {
            month_index = 0;
            remaining_days = 0;
        }

        BanglaDate {
            year: bangla_year,
            month: month_index,
            day: (remaining_days + 1) as u8,
        }
    }

    /// Get month lengths accounting for leap year
    fn get_month_lengths(gregorian_year: i32) -> [u8; 12] {
        let mut lengths = MONTH_LENGTHS;
        // Choitro (index 11) has 31 days if the Gregorian year is a leap year
        if Self::is_leap_year(gregorian_year) {
            lengths[11] = 31;
        }
        lengths
    }

    /// Check if a year is a leap year
    fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
    }

    /// Get the month name
    pub fn get_month_name(&self) -> &'static str {
        BANGLA_MONTHS[self.month]
    }

    /// Get the season based on month
    pub fn get_season(&self) -> &'static str {
        BANGLA_SEASONS[self.month / 2]
    }

    /// Format the date in Bangla (e.g., "১৩ই মাঘ, ১৪৩২ বঙ্গাব্দ")
    pub fn format_bangla(&self, use_bangla_numerals: bool) -> String {
        let day_str = if use_bangla_numerals {
            to_bangla_digits(&self.day.to_string())
        } else {
            self.day.to_string()
        };

        let suffix = get_bangla_ordinal_suffix(self.day);

        let year_str = if use_bangla_numerals {
            to_bangla_digits(&self.year.to_string())
        } else {
            self.year.to_string()
        };

        format!(
            "{}{} {}, {} বঙ্গাব্দ",
            day_str,
            suffix,
            self.get_month_name(),
            year_str
        )
    }
}

/// Convert a string to Bangla digits
pub fn to_bangla_digits(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if let Some(digit) = c.to_digit(10) {
                BANGLA_DIGITS[digit as usize]
            } else {
                c
            }
        })
        .collect()
}

/// Get the Bangla weekday name
pub fn get_bangla_weekday(dt: DateTime<Local>) -> &'static str {
    const BANGLA_WEEKDAYS: [&str; 7] = [
        "রবিবার",    // Sunday
        "সোমবার",    // Monday
        "মঙ্গলবার",   // Tuesday
        "বুধবার",     // Wednesday
        "বৃহস্পতিবার", // Thursday
        "শুক্রবার",    // Friday
        "শনিবার",    // Saturday
    ];

    let weekday = dt.weekday().num_days_from_sunday() as usize;
    BANGLA_WEEKDAYS[weekday]
}

/// English month names
const ENGLISH_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Bangla English month names
const BANGLA_ENGLISH_MONTHS: [&str; 12] = [
    "জানুয়ারি",
    "ফেব্রুয়ারি",
    "মার্চ",
    "এপ্রিল",
    "মে",
    "জুন",
    "জুলাই",
    "আগস্ট",
    "সেপ্টেম্বর",
    "অক্টোবর",
    "নভেম্বর",
    "ডিসেম্বর",
];

/// Format Gregorian date
pub fn format_gregorian_date(
    dt: DateTime<Local>,
    use_bangla_names: bool,
    use_bangla_numerals: bool,
) -> String {
    let day = dt.day();
    let month = dt.month() as usize - 1;
    let year = dt.year();

    let day_str = if use_bangla_numerals {
        to_bangla_digits(&day.to_string())
    } else {
        day.to_string()
    };

    let month_name = if use_bangla_names {
        BANGLA_ENGLISH_MONTHS[month]
    } else {
        ENGLISH_MONTHS[month]
    };

    let year_str = if use_bangla_numerals {
        to_bangla_digits(&year.to_string())
    } else {
        year.to_string()
    };

    let suffix = if use_bangla_names {
        " ঈসায়ী"
    } else {
        ""
    };

    format!("{} {} {}{}", day_str, month_name, year_str, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Helper to create a timezone offset
    fn tz(hours: i32, minutes: i32) -> FixedOffset {
        FixedOffset::east_opt(hours * 3600 + minutes * 60).unwrap()
    }

    /// Common timezone offsets
    fn tz_usa_est() -> FixedOffset {
        tz(-5, 0)
    } // EST (New York)
    fn tz_usa_pst() -> FixedOffset {
        tz(-8, 0)
    } // PST (Los Angeles)
    fn tz_uk() -> FixedOffset {
        tz(0, 0)
    } // GMT (London)
    fn tz_india() -> FixedOffset {
        tz(5, 30)
    } // IST (India)
    fn tz_bangladesh() -> FixedOffset {
        tz(6, 0)
    } // BST (Bangladesh)
    fn tz_china() -> FixedOffset {
        tz(8, 0)
    } // CST (China)
    fn tz_russia_msk() -> FixedOffset {
        tz(3, 0)
    } // MSK (Moscow)
    fn tz_canada_est() -> FixedOffset {
        tz(-5, 0)
    } // EST (Toronto)

    #[test]
    fn test_pohela_boishakh_bangladesh() {
        // April 14, 2026 in Bangladesh = 1 Boishakh 1433
        let date = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        let bangla = BanglaDate::from_date_with_region(date, CalendarRegion::Bangladesh);

        assert_eq!(bangla.year, 1433);
        assert_eq!(bangla.month, 0);
        assert_eq!(bangla.day, 1);
        assert_eq!(bangla.get_month_name(), "বৈশাখ");
    }

    #[test]
    fn test_pohela_boishakh_india() {
        // April 15, 2026 in India = 1 Boishakh 1433
        let date = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let bangla = BanglaDate::from_date_with_region(date, CalendarRegion::India);

        assert_eq!(bangla.year, 1433);
        assert_eq!(bangla.month, 0);
        assert_eq!(bangla.day, 1);
        assert_eq!(bangla.get_month_name(), "বৈশাখ");
    }

    #[test]
    fn test_digit_conversion() {
        assert_eq!(to_bangla_digits("2026"), "২০২৬");
        assert_eq!(to_bangla_digits("12:30:45"), "১২:৩০:৪৫");
    }

    /// Test: January 26, 2026 at 8:00 AM in different timezones
    /// This simulates what users in different countries would see
    #[test]
    fn test_timezone_conversion_morning() {
        // Scenario: It's January 26, 2026, 8:00 AM local time in each country
        // What Bangla date would they see?

        println!("\n========================================");
        println!("Bangla Date Display Test - Morning Time");
        println!("Local Time: 8:00 AM on January 26, 2026");
        println!("========================================\n");

        // Create 8:00 AM local time for each timezone
        let test_cases = vec![
            ("Bangladesh (Dhaka)", tz_bangladesh(), "GMT+6"),
            ("India (Kolkata)", tz_india(), "GMT+5:30"),
            ("China (Beijing)", tz_china(), "GMT+8"),
            ("Russia (Moscow)", tz_russia_msk(), "GMT+3"),
            ("UK (London)", tz_uk(), "GMT+0"),
            ("USA (New York)", tz_usa_est(), "GMT-5"),
            ("USA (Los Angeles)", tz_usa_pst(), "GMT-8"),
            ("Canada (Toronto)", tz_canada_est(), "GMT-5"),
        ];

        for (country, local_tz, tz_name) in test_cases {
            // Create 8:00 AM local time
            let local_time = local_tz.with_ymd_and_hms(2026, 1, 26, 8, 0, 0).unwrap();
            let utc_time = local_time.with_timezone(&Utc);

            // Calculate what time it is in Bangladesh and India
            let bd_time = utc_time.with_timezone(&tz_bangladesh());
            let in_time = utc_time.with_timezone(&tz_india());

            // Get Bangla dates for both calendars
            let bd_bangla = BanglaDate::from_utc_with_region(utc_time, CalendarRegion::Bangladesh);
            let in_bangla = BanglaDate::from_utc_with_region(utc_time, CalendarRegion::India);

            println!("📍 {} ({})", country, tz_name);
            println!("   Local: {} 8:00 AM", local_time.format("%Y-%m-%d"));
            println!("   UTC:   {}", utc_time.format("%Y-%m-%d %H:%M"));
            println!("   → Bangladesh Time: {}", bd_time.format("%Y-%m-%d %H:%M"));
            println!("   → India Time:      {}", in_time.format("%Y-%m-%d %H:%M"));
            println!(
                "   📅 Bangladesh Calendar: {} {} {}",
                bd_bangla.day,
                bd_bangla.get_month_name(),
                bd_bangla.year
            );
            println!(
                "   📅 India Calendar:      {} {} {}",
                in_bangla.day,
                in_bangla.get_month_name(),
                in_bangla.year
            );
            println!();
        }
    }

    /// Test: Edge case - When it's late night in Bangladesh but still daytime elsewhere
    #[test]
    fn test_date_boundary_crossing() {
        println!("\n========================================");
        println!("Date Boundary Test - Late Night in BD");
        println!("========================================\n");

        // Scenario: It's 11:00 PM on Jan 25, 2026 in New York (EST)
        // That's 10:00 AM on Jan 26, 2026 in Bangladesh!
        let ny_time = tz_usa_est()
            .with_ymd_and_hms(2026, 1, 25, 23, 0, 0)
            .unwrap();
        let utc_time = ny_time.with_timezone(&Utc);
        let bd_time = utc_time.with_timezone(&tz_bangladesh());

        let bd_bangla = BanglaDate::from_utc_with_region(utc_time, CalendarRegion::Bangladesh);

        println!(
            "🗽 New York: {} (Jan 25, 11:00 PM)",
            ny_time.format("%Y-%m-%d %H:%M")
        );
        println!("🌍 UTC:      {}", utc_time.format("%Y-%m-%d %H:%M"));
        println!(
            "🇧🇩 Bangladesh: {} (Next day!)",
            bd_time.format("%Y-%m-%d %H:%M")
        );
        println!(
            "📅 Bangla Date: {}ই {} {}",
            bd_bangla.day,
            bd_bangla.get_month_name(),
            bd_bangla.year
        );

        // The user in New York at 11 PM on Jan 25 should see
        // Bangladesh date of Jan 26 (13 Magh), not Jan 25 (12 Magh)!
        // Magh starts Jan 14, so Jan 26 = day 13
        assert_eq!(bd_time.day(), 26);
        assert_eq!(bd_bangla.day, 13); // 13 Magh
    }

    /// Test: Verify January 26, 2026 = 13 Magh for Bangladesh calendar
    /// Calculation: Magh starts Jan 14, so Jan 26 = day 13
    #[test]
    fn test_january_26_2026_bangladesh() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 26).unwrap();
        let bangla = BanglaDate::from_date_with_region(date, CalendarRegion::Bangladesh);

        // Magh starts Jan 14 (after Poush ends Jan 13)
        // Jan 26 = Jan 14 + 12 days = 13 Magh
        assert_eq!(bangla.day, 13);
        assert_eq!(bangla.month, 9); // Magh is index 9
        assert_eq!(bangla.get_month_name(), "মাঘ");
        assert_eq!(bangla.year, 1432);

        println!(
            "✓ January 26, 2026 (Bangladesh) = {}ই {} {} বঙ্গাব্দ",
            bangla.day,
            bangla.get_month_name(),
            bangla.year
        );
    }

    /// Test: Verify January 26, 2026 = 12 Magh for India calendar
    /// India's Pohela Boishakh is April 15, so dates shift by 1 day
    #[test]
    fn test_january_26_2026_india() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 26).unwrap();
        let bangla = BanglaDate::from_date_with_region(date, CalendarRegion::India);

        // India's Magh starts Jan 15 (1 day later than Bangladesh)
        // Jan 26 = Jan 15 + 11 days = 12 Magh
        assert_eq!(bangla.day, 12);
        assert_eq!(bangla.month, 9); // Magh is index 9
        assert_eq!(bangla.get_month_name(), "মাঘ");
        assert_eq!(bangla.year, 1432);

        println!(
            "✓ January 26, 2026 (India) = {}ই {} {} বঙ্গাব্দ",
            bangla.day,
            bangla.get_month_name(),
            bangla.year
        );
    }

    /// Comprehensive test showing all regions at a specific UTC time
    #[test]
    fn test_world_clock_utc_midnight() {
        println!("\n========================================");
        println!("World Clock - UTC Midnight Jan 26, 2026");
        println!("========================================\n");

        // UTC midnight on January 26, 2026
        let utc_midnight = Utc.with_ymd_and_hms(2026, 1, 26, 0, 0, 0).unwrap();

        let regions = vec![
            (
                "Bangladesh (Dhaka)",
                tz_bangladesh(),
                CalendarRegion::Bangladesh,
            ),
            ("India (Kolkata)", tz_india(), CalendarRegion::Bangladesh), // Using BD calendar
            ("China (Beijing)", tz_china(), CalendarRegion::Bangladesh),
            (
                "Russia (Moscow)",
                tz_russia_msk(),
                CalendarRegion::Bangladesh,
            ),
            ("UK (London)", tz_uk(), CalendarRegion::Bangladesh),
            ("USA (New York)", tz_usa_est(), CalendarRegion::Bangladesh),
            (
                "Canada (Toronto)",
                tz_canada_est(),
                CalendarRegion::Bangladesh,
            ),
        ];

        println!("UTC Time: {}", utc_midnight.format("%Y-%m-%d %H:%M:%S"));
        println!("All showing BANGLADESH calendar (Pohela Boishakh = Apr 14)\n");

        for (country, local_tz, calendar) in regions {
            let local_time = utc_midnight.with_timezone(&local_tz);
            let bangla = BanglaDate::from_utc_with_region(utc_midnight, calendar);

            println!("📍 {}", country);
            println!("   Local Time: {}", local_time.format("%Y-%m-%d %H:%M"));
            println!("   System Date: {}", local_time.format("%B %d, %Y"));
            println!(
                "   📅 Bangla: {}ই {}, {} বঙ্গাব্দ",
                bangla.day,
                bangla.get_month_name(),
                bangla.year
            );
            println!();
        }
    }
}
