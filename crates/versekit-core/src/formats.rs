// VerseKit — companion for Star Citizen.
// Copyright (C) 2026 Xharig
//
// SPDX-License-Identifier: GPL-3.0-only
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Numbers and dates written per language.
//!
//! The Python version decides this with `current() == 'de'` at a dozen places
//! (`pages.py`, `overview.py`, `stats_pages.py`, `report.py`, `Moment` in
//! `language.py`); here it is in one place.

use crate::language::{self, Lang};

/// `,` for German, `.` for English.
pub fn decimal_mark(lang: Lang) -> char {
    match lang {
        Lang::De => ',',
        Lang::En => '.',
    }
}

/// `'%.Nf' % value` with the decimal mark of `lang`.
pub fn fixed(lang: Lang, value: f64, places: usize) -> String {
    let text = language::py_fixed(value, places, false);
    match lang {
        Lang::De => text.replace('.', ","),
        Lang::En => text,
    }
}

/// Whole number with thousands separators: `1.234.567` German, `1,234,567`
/// English (`stats_pages.stats_count`).
pub fn count(lang: Lang, value: i64) -> String {
    let separator = match lang {
        Lang::De => '.',
        Lang::En => ',',
    };
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        out.push('-');
    }
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// An amount with its unit after it: as few decimals as needed (at most
/// three), thousands separated, e.g. `3,5 SCU`, `0,125 SCU`, `1.234 SCU`, `12 Stück`.
pub fn amount(lang: Lang, value: f64, unit: &str) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    let whole = rounded.trunc();
    let mut text = count(lang, whole as i64);
    if rounded < 0.0 && whole == 0.0 {
        text.insert(0, '-');
    }
    let fraction = format!("{:.3}", (rounded - whole).abs());
    let digits = fraction.trim_start_matches('0').trim_start_matches('.').trim_end_matches('0');
    if !digits.is_empty() {
        text.push(decimal_mark(lang));
        text.push_str(digits);
    }
    if unit.is_empty() { text } else { format!("{text} {unit}") }
}

/// Money in whole aUEC with thousands separators: `1.234.500 aUEC`.
pub fn money(lang: Lang, value: f64) -> String {
    format!("{} aUEC", count(lang, value.round() as i64))
}

/// A share in whole percent; above 0 but under 1 it says `< 1 %`.
pub fn percent(value: f64) -> String {
    if value > 0.0 && value < 1.0 { "< 1 %".to_string() } else { format!("{} %", value.round() as i64) }
}

/// A duration in its two largest units: `2 h 10 min`, `45 s`,
/// `3 d 4 h`; whole units drop their zero rest (`2 h`).
pub fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as i64;
    let (days, hours, minutes, secs) = (total / 86_400, total % 86_400 / 3600, total % 3600 / 60, total % 60);
    let pair = |big: i64, big_unit: &str, small: i64, small_unit: &str| {
        if small == 0 { format!("{big} {big_unit}") } else { format!("{big} {big_unit} {small} {small_unit}") }
    };
    if days > 0 {
        pair(days, "d", hours, "h")
    } else if hours > 0 {
        pair(hours, "h", minutes, "min")
    } else if minutes > 0 {
        pair(minutes, "min", secs, "s")
    } else {
        format!("{secs} s")
    }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

/// Date and time of day in the local time zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Seconds as `time.localtime()` takes them: rounded down.
pub fn floor_seconds(timestamp: f64) -> Option<i64> {
    let floor = timestamp.floor();
    (floor.is_finite() && floor.abs() < 1e15).then_some(floor as i64)
}

/// Seconds as `datetime.fromtimestamp()` sees them: the fraction is rounded to
/// whole microseconds (ties to even) first, which can carry into the next
/// second.
pub fn rounded_seconds(timestamp: f64) -> Option<i64> {
    if !timestamp.is_finite() || timestamp.abs() >= 1e15 {
        return None;
    }
    let whole = timestamp.trunc();
    let micros = ((timestamp - whole) * 1e6).round_ties_even();
    let mut seconds = whole as i64;
    let mut micros = micros as i64;
    if micros >= 1_000_000 {
        seconds += 1;
        micros -= 1_000_000;
    }
    if micros < 0 {
        seconds -= 1;
        micros += 1_000_000;
        if micros >= 1_000_000 {
            seconds += 1;
        }
    }
    Some(seconds)
}

/// Local time of a Unix time stamp, like the C library's `localtime`.
#[cfg(unix)]
pub fn local_time(seconds: i64) -> Option<LocalTime> {
    #[allow(clippy::useless_conversion, reason = "time_t has 32 bits on some platforms")]
    let time: libc::time_t = seconds.try_into().ok()?;
    // SAFETY: `tm` is plain data; all fields are written by `localtime_r`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers point to live values for the duration of the call.
    let result = unsafe { libc::localtime_r(&time, &mut tm) };
    if result.is_null() {
        return None;
    }
    Some(LocalTime {
        year: i64::from(tm.tm_year) + 1900,
        month: u32::try_from(tm.tm_mon + 1).ok()?,
        day: u32::try_from(tm.tm_mday).ok()?,
        hour: u32::try_from(tm.tm_hour).ok()?,
        minute: u32::try_from(tm.tm_min).ok()?,
        second: u32::try_from(tm.tm_sec).ok()?,
    })
}

/// Local time of a Unix time stamp, with the daylight saving rules of that
/// year (dynamic time zone information).
#[cfg(windows)]
pub fn local_time(seconds: i64) -> Option<LocalTime> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, FileTimeToSystemTime, GetDynamicTimeZoneInformation,
        SystemTimeToTzSpecificLocalTimeEx,
    };
    // 100-ns ticks since 1601-01-01.
    let ticks = u64::try_from(seconds.checked_add(11_644_473_600)?.checked_mul(10_000_000)?).ok()?;
    let file_time = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    // SAFETY: plain data structs, filled by the calls below; pointers are valid.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        let mut zone: DYNAMIC_TIME_ZONE_INFORMATION = std::mem::zeroed();
        if FileTimeToSystemTime(&file_time, &mut utc) == 0 {
            return None;
        }
        GetDynamicTimeZoneInformation(&mut zone);
        if SystemTimeToTzSpecificLocalTimeEx(&zone, &utc, &mut local) == 0 {
            return None;
        }
        Some(LocalTime {
            year: i64::from(local.wYear),
            month: u32::from(local.wMonth),
            day: u32::from(local.wDay),
            hour: u32::from(local.wHour),
            minute: u32::from(local.wMinute),
            second: u32::from(local.wSecond),
        })
    }
}

/// The setting: the clock is written with 24 hours (`18:30`), else with 12 and AM/PM (`6:30 PM`).
pub const CLOCK_24_SETTING: &str = "uhr_24h";

static CLOCK_24: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Switches the clock of every time of day shown in the program ([`shown`]); the default is 24 hours.
pub fn set_clock_24(on: bool) {
    CLOCK_24.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Is the clock written with 24 hours?
pub fn clock_24() -> bool {
    CLOCK_24.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reads the setting of the clock and switches it; true when it is the 24-hour clock.
pub fn apply_clock(paths: &crate::paths::Paths) -> bool {
    let on = paths.setting_bool(CLOCK_24_SETTING, true);
    set_clock_24(on);
    on
}

/// The format `format` for a time shown to the player: with the 12-hour clock `%H:%M:%S` and `%H:%M` become
/// `%l:%M:%S %p` and `%l:%M %p`. Formats of stored or exchanged stamps never go through here.
pub fn shown(format: &str) -> std::borrow::Cow<'_, str> {
    shown_with(format, clock_24())
}

/// [`shown`] for a clock given.
pub fn shown_with(format: &str, clock_24: bool) -> std::borrow::Cow<'_, str> {
    if clock_24 || !format.contains("%H:%M") {
        return std::borrow::Cow::Borrowed(format);
    }
    std::borrow::Cow::Owned(format.replace("%H:%M:%S", "%l:%M:%S %p").replace("%H:%M", "%l:%M %p"))
}

/// A full hour of the day for the player: `14:00`, or `2 PM` with the 12-hour clock.
pub fn hour_label(hour: u32) -> String {
    hour_label_with(hour, clock_24())
}

/// [`hour_label`] for a clock given.
pub fn hour_label_with(hour: u32, clock_24: bool) -> String {
    if clock_24 {
        return format!("{hour:02}:00");
    }
    format!("{} {}", if hour.is_multiple_of(12) { 12 } else { hour % 12 }, if hour % 24 < 12 { "AM" } else { "PM" })
}

/// [`strftime`] of a format shown to the player ([`shown`]).
pub fn strftime_shown(format: &str, time: &LocalTime) -> String {
    strftime(&shown(format), time)
}

/// `strftime` for the directives the texts use: `%d %m %Y %y %H %M %S %%`, and for the 12-hour clock `%l` (hour
/// 1 to 12, no zero in front) and `%p` (`AM`, `PM`). Anything else stays as it is.
pub fn strftime(format: &str, time: &LocalTime) -> String {
    let mut out = String::with_capacity(format.len() + 8);
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('d') => out.push_str(&format!("{:02}", time.day)),
            Some('m') => out.push_str(&format!("{:02}", time.month)),
            Some('Y') => out.push_str(&time.year.to_string()),
            Some('y') => out.push_str(&format!("{:02}", time.year.rem_euclid(100))),
            Some('H') => out.push_str(&format!("{:02}", time.hour)),
            Some('M') => out.push_str(&format!("{:02}", time.minute)),
            Some('S') => out.push_str(&format!("{:02}", time.second)),
            Some('l') => out.push_str(&(if time.hour.is_multiple_of(12) { 12 } else { time.hour % 12 }).to_string()),
            Some('p') => out.push_str(if time.hour < 12 { "AM" } else { "PM" }),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Python's `Moment`: the date format of `key` in `lang`, applied to
/// `time.localtime(timestamp)`. The year comes first in English.
///
/// Empty if the time stamp cannot be converted (Python raises there).
pub fn moment(lang: Lang, timestamp: f64, key: &str) -> String {
    let format = language::t_in(lang, key);
    floor_seconds(timestamp).and_then(local_time).map(|time| strftime_shown(&format, &time)).unwrap_or_default()
}

/// Date of the error report (`b_datum`), as `datetime.fromtimestamp(…)`.
pub fn report_date(lang: Lang, timestamp: f64) -> String {
    let format = language::t_in(lang, "b_datum");
    rounded_seconds(timestamp).and_then(local_time).map(|time| strftime(&format, &time)).unwrap_or_default()
}

/// Date with time of day for the overview cards (`overview.py`, last backup).
pub fn date_time(lang: Lang, timestamp: f64) -> String {
    let format = match lang {
        Lang::De => "%d.%m.%Y %H:%M",
        Lang::En => "%Y-%m-%d %H:%M",
    };
    floor_seconds(timestamp).and_then(local_time).map(|time| strftime_shown(format, &time)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One way to write amounts, money, shares and durations.
    #[test]
    fn amounts_money_shares_durations() {
        assert_eq!(amount(Lang::De, 3.5, "SCU"), "3,5 SCU");
        assert_eq!(amount(Lang::De, 0.125, "SCU"), "0,125 SCU");
        assert_eq!(amount(Lang::De, 3.0, "SCU"), "3 SCU");
        assert_eq!(amount(Lang::De, 1234.5, "SCU"), "1.234,5 SCU");
        assert_eq!(amount(Lang::En, 1234.5, "SCU"), "1,234.5 SCU");
        assert_eq!(amount(Lang::De, 0.0004, "SCU"), "0 SCU");
        assert_eq!(amount(Lang::De, -0.5, ""), "-0,5");
        assert_eq!(amount(Lang::De, 12.0, "Stück"), "12 Stück");
        assert_eq!(money(Lang::De, 1_234_500.4), "1.234.500 aUEC");
        assert_eq!(money(Lang::En, 41_200.0), "41,200 aUEC");
        assert_eq!(percent(48.4), "48 %");
        assert_eq!(percent(0.4), "< 1 %");
        assert_eq!(percent(0.0), "0 %");
        assert_eq!(duration(7_800.0), "2 h 10 min");
        assert_eq!(duration(7_200.0), "2 h");
        assert_eq!(duration(45.0), "45 s");
        assert_eq!(duration(185.0), "3 min 5 s");
        assert_eq!(duration(273_600.0), "3 d 4 h");
        assert_eq!(duration(-5.0), "0 s");
    }

    #[test]
    fn numbers_per_language() {
        assert_eq!(fixed(Lang::De, 2.345, 2), "2,35");
        assert_eq!(fixed(Lang::En, 2.345, 2), "2.35");
        assert_eq!(fixed(Lang::De, 1234.5, 0), "1234");
        assert_eq!(count(Lang::De, 1234567), "1.234.567");
        assert_eq!(count(Lang::En, -1234), "-1,234");
        assert_eq!(count(Lang::En, 999), "999");
        assert_eq!(count(Lang::De, i64::MIN), "-9.223.372.036.854.775.808");
        assert_eq!(decimal_mark(Lang::De), ',');
    }

    #[test]
    fn seconds_like_python() {
        assert_eq!(floor_seconds(59.9999999), Some(59));
        assert_eq!(rounded_seconds(59.9999999), Some(60));
        assert_eq!(rounded_seconds(59.9999994), Some(59));
        assert_eq!(rounded_seconds(-0.0000001), Some(0));
        assert_eq!(rounded_seconds(-0.5), Some(-1));
        assert_eq!(floor_seconds(f64::NAN), None);
    }

    #[test]
    fn formats_dates() {
        let time = LocalTime { year: 2026, month: 3, day: 9, hour: 7, minute: 5, second: 4 };
        assert_eq!(strftime("%d.%m.%Y, %H:%M", &time), "09.03.2026, 07:05");
        assert_eq!(strftime("%Y-%m-%d %y %S %% %q", &time), "2026-03-09 26 04 % %q");
        assert!(local_time(0).is_some());
    }

    #[test]
    fn the_12_hour_clock_writes_am_and_pm_without_a_zero_in_front() {
        let at = |hour| LocalTime { year: 2026, month: 3, day: 9, hour, minute: 5, second: 4 };
        let twelve = |format: &str, hour| strftime(&shown_with(format, false), &at(hour));
        let full = |format: &str, hour| strftime(&shown_with(format, true), &at(hour));
        assert_eq!(twelve("%H:%M", 0), "12:05 AM");
        assert_eq!(twelve("%H:%M", 6), "6:05 AM");
        assert_eq!(twelve("%H:%M", 12), "12:05 PM");
        assert_eq!(twelve("%H:%M", 18), "6:05 PM");
        assert_eq!(twelve("%H:%M", 23), "11:05 PM");
        assert_eq!(twelve("%d.%m. %H:%M:%S", 18), "09.03. 6:05:04 PM");
        assert_eq!(full("%H:%M", 18), "18:05");
        assert_eq!(full("%d.%m. %H:%M:%S", 7), "09.03. 07:05:04");
        // Without a time of day nothing changes, and a stamp that is stored is written as it is.
        assert_eq!(twelve("%d.%m.%Y", 18), "09.03.2026");
        assert_eq!(strftime("%Y-%m-%d %H:%M:%S", &at(18)), "2026-03-09 18:05:04");
        assert_eq!(hour_label_with(14, true), "14:00");
        assert_eq!(hour_label_with(0, true), "00:00");
        assert_eq!(hour_label_with(14, false), "2 PM");
        assert_eq!(hour_label_with(0, false), "12 AM");
        assert_eq!(hour_label_with(12, false), "12 PM");
    }

    #[test]
    fn the_clock_is_read_from_its_setting_and_is_24_hours_without_one() {
        let root = std::env::temp_dir().join(format!("versekit-uhr-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let home = root.to_string_lossy().into_owned();
        let paths = crate::paths::Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, home.as_str())],
            &home,
        ));
        assert!(paths.setting_bool(CLOCK_24_SETTING, true), "24 hours without a setting");
        assert!(apply_clock(&paths));
        assert!(clock_24());
        let _ = std::fs::remove_dir_all(&root);
    }
}
