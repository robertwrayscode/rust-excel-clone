//! Excel date serial numbers (1900 date system, including the 1900 leap-year bug).

/// Days from 1970-01-01 to civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Civil date from days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// Serial 25569 == 1970-01-01. Serials >= 61 are offset by the fake 1900-02-29.
const UNIX_EPOCH_SERIAL: i64 = 25569;

/// Convert a (year, month, day) to an Excel serial. Month and day may overflow
/// (e.g. month 13), matching DATE().
pub fn date_to_serial(year: i64, month: i64, day: i64) -> Option<f64> {
    let mut y = year;
    if (0..1900).contains(&y) {
        y += 1900;
    }
    // Normalize month
    let m0 = month - 1;
    let y = y + m0.div_euclid(12);
    let m = (m0.rem_euclid(12) + 1) as u32;
    if !(1900..=9999).contains(&y) {
        return None;
    }
    let days = days_from_civil(y, m, 1) + (day - 1);
    let serial = days + UNIX_EPOCH_SERIAL;
    // Excel's fake leap day: serials before March 1 1900 are one less.
    let serial = if serial < 61 { serial - 1 } else { serial };
    if serial < 0 || serial > 2_958_465 {
        return None;
    }
    // Special case 1900-02-29 itself which Excel accepts as serial 60
    if y == 1900 && m == 2 && day == 29 {
        return Some(60.0);
    }
    Some(serial as f64)
}

/// Convert serial (integer part) to (year, month, day).
pub fn serial_to_date(serial: f64) -> Option<(i64, u32, u32)> {
    if !(0.0..2_958_466.0).contains(&serial) {
        return None;
    }
    let s = serial.floor() as i64;
    if s == 0 {
        return Some((1900, 1, 0));
    }
    if s == 60 {
        return Some((1900, 2, 29));
    }
    let adj = if s < 60 { s + 1 } else { s };
    Some(civil_from_days(adj - UNIX_EPOCH_SERIAL))
}

/// Day of week: 0 = Sunday .. 6 = Saturday.
pub fn weekday(serial: f64) -> u32 {
    let s = serial.floor() as i64;
    // Serial 1 (1900-01-01) is reported as Sunday in Excel (due to the bug).
    (((s - 1) % 7 + 7) % 7) as u32
}

pub fn time_to_fraction(h: f64, m: f64, s: f64) -> f64 {
    (h * 3600.0 + m * 60.0 + s) / 86400.0
}

/// Split the fractional part of a serial into h, m, s (seconds rounded).
pub fn serial_to_hms(serial: f64) -> (u32, u32, u32) {
    let frac = serial - serial.floor();
    let mut secs = (frac * 86400.0).round() as i64;
    if secs >= 86400 {
        secs = 0;
    }
    ((secs / 3600) as u32, ((secs % 3600) / 60) as u32, (secs % 60) as u32)
}

pub fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

pub const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];
pub const DAY_NAMES: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn month_from_name(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    if l.len() < 3 {
        return None;
    }
    MONTH_NAMES.iter().position(|m| m.to_ascii_lowercase().starts_with(&l)).map(|i| i as u32 + 1)
}

/// Current local date/time as an Excel serial.
pub fn now_serial() -> f64 {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    let d = date_to_serial(now.year() as i64, now.month() as i64, now.day() as i64).unwrap_or(0.0);
    d + time_to_fraction(now.hour() as f64, now.minute() as f64, now.second() as f64)
}

/// Parse a time like `13:45`, `1:45 PM`, `13:45:30`, `1:45:30.5 pm`.
pub fn parse_time_text(s: &str) -> Option<f64> {
    let t = s.trim().to_ascii_uppercase();
    let (body, ampm) = if let Some(b) = t.strip_suffix("AM") {
        (b.trim().to_string(), Some(false))
    } else if let Some(b) = t.strip_suffix("PM") {
        (b.trim().to_string(), Some(true))
    } else if let Some(b) = t.strip_suffix('A') {
        (b.trim().to_string(), Some(false))
    } else if let Some(b) = t.strip_suffix('P') {
        (b.trim().to_string(), Some(true))
    } else {
        (t.clone(), None)
    };
    let parts: Vec<&str> = body.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    if parts.len() == 1 && ampm.is_none() {
        return None;
    }
    let h: f64 = parts[0].trim().parse::<u32>().ok()? as f64;
    let m: f64 = if parts.len() > 1 { parts[1].trim().parse::<u32>().ok()? as f64 } else { 0.0 };
    let sec: f64 = if parts.len() > 2 { parts[2].trim().parse::<f64>().ok()? } else { 0.0 };
    if m >= 60.0 || sec >= 60.0 {
        return None;
    }
    let h = match ampm {
        Some(pm) => {
            if h < 1.0 || h > 12.0 {
                return None;
            }
            let base = if h == 12.0 { 0.0 } else { h };
            if pm {
                base + 12.0
            } else {
                base
            }
        }
        None => h,
    };
    Some(time_to_fraction(h, m, sec))
}

/// Parse a date-only string like `2024-03-05`, `3/5/2024`, `5-Mar-2024`, `March 5, 2024`, `Mar 2024`.
pub fn parse_date_text(s: &str) -> Option<f64> {
    let t = s.trim();
    // ISO yyyy-mm-dd or yyyy/mm/dd
    let seps = ['-', '/', '.'];
    for sep in seps {
        let parts: Vec<&str> = t.split(sep).map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let nums: Vec<Option<i64>> = parts.iter().map(|p| p.parse::<i64>().ok()).collect();
            if let (Some(a), Some(b), Some(c)) = (nums[0], nums[1], nums[2]) {
                if parts[0].len() == 4 {
                    // y-m-d
                    if (1..=12).contains(&b) && c >= 1 && c <= days_in_month(a, b as u32) as i64 {
                        return date_to_serial(a, b, c);
                    }
                    return None;
                }
                // m/d/y (US)
                let y = if parts[2].len() <= 2 { if c < 30 { 2000 + c } else { 1900 + c } } else { c };
                if (1..=12).contains(&a) && b >= 1 && b <= days_in_month(y, a as u32) as i64 {
                    return date_to_serial(y, a, b);
                }
                return None;
            }
            // d-mmm-yyyy
            if let (Some(d), Some(m), Some(y)) = (nums[0], month_from_name(parts[1]), nums[2]) {
                let y = if parts[2].len() <= 2 { if y < 30 { 2000 + y } else { 1900 + y } } else { y };
                if d >= 1 && d <= days_in_month(y, m) as i64 {
                    return date_to_serial(y, m as i64, d);
                }
            }
            return None;
        }
        if parts.len() == 2 && sep != '.' {
            // m/d (current year) or mmm-yy / d-mmm
            let nums: Vec<Option<i64>> = parts.iter().map(|p| p.parse::<i64>().ok()).collect();
            use chrono::Datelike;
            let cy = chrono::Local::now().year() as i64;
            if let (Some(a), Some(b)) = (nums[0], nums[1]) {
                if (1..=12).contains(&a) && b >= 1 && b <= days_in_month(cy, a as u32) as i64 {
                    return date_to_serial(cy, a, b);
                }
                return None;
            }
            if let (Some(d), Some(m)) = (nums[0], month_from_name(parts[1])) {
                return date_to_serial(cy, m as i64, d);
            }
            if let (Some(m), Some(y)) = (month_from_name(parts[0]), nums[1]) {
                let y = if parts[1].len() <= 2 { if y < 30 { 2000 + y } else { 1900 + y } } else { y };
                return date_to_serial(y, m as i64, 1);
            }
            return None;
        }
    }
    // "March 5, 2024" / "5 March 2024"
    let words: Vec<&str> = t.split(|c: char| c == ' ' || c == ',').filter(|w| !w.is_empty()).collect();
    if words.len() == 3 {
        if let (Some(m), Ok(d), Ok(y)) = (month_from_name(words[0]), words[1].parse::<i64>(), words[2].parse::<i64>()) {
            return date_to_serial(y, m as i64, d);
        }
        if let (Ok(d), Some(m), Ok(y)) = (words[0].parse::<i64>(), month_from_name(words[1]), words[2].parse::<i64>()) {
            return date_to_serial(y, m as i64, d);
        }
    }
    None
}

/// Parse a date, time, or date+time string to a serial.
pub fn parse_datetime_text(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // Must contain something date/time-like to avoid misparsing random text
    if !t.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    if let Some(d) = parse_date_text(t) {
        return Some(d);
    }
    if let Some(tm) = parse_time_text(t) {
        return Some(tm);
    }
    // date + time, split at last space(s)
    let normalized = t.replacen('T', " ", 1);
    let parts: Vec<&str> = normalized.splitn(2, ' ').collect();
    if parts.len() == 2 {
        if let (Some(d), Some(tm)) = (parse_date_text(parts[0]), parse_time_text(parts[1])) {
            return Some(d + tm);
        }
    }
    // "March 5, 2024 10:00"
    if let Some(idx) = t.rfind(' ') {
        let (a, b) = t.split_at(idx);
        if let (Some(d), Some(tm)) = (parse_date_text(a), parse_time_text(b)) {
            return Some(d + tm);
        }
        // time with AM/PM "1:30 PM" handled above; date + "1:30 PM"
        if let Some(idx2) = a.trim_end().rfind(' ') {
            let (a2, b2) = t.split_at(idx2);
            if let (Some(d), Some(tm)) = (parse_date_text(a2), parse_time_text(b2)) {
                return Some(d + tm);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serials() {
        assert_eq!(date_to_serial(1900, 1, 1), Some(1.0));
        assert_eq!(date_to_serial(1900, 2, 28), Some(59.0));
        assert_eq!(date_to_serial(1900, 3, 1), Some(61.0));
        assert_eq!(date_to_serial(2024, 1, 1), Some(45292.0));
        assert_eq!(serial_to_date(45292.0), Some((2024, 1, 1)));
        assert_eq!(serial_to_date(1.0), Some((1900, 1, 1)));
        assert_eq!(serial_to_date(61.0), Some((1900, 3, 1)));
        assert_eq!(date_to_serial(2024, 13, 1), date_to_serial(2025, 1, 1));
        assert_eq!(date_to_serial(2024, 3, 0), date_to_serial(2024, 2, 29));
        // 2024-01-01 is a Monday
        assert_eq!(weekday(45292.0), 1);
    }
    #[test]
    fn parsing() {
        assert_eq!(parse_datetime_text("2024-01-01"), Some(45292.0));
        assert_eq!(parse_datetime_text("1/1/2024"), Some(45292.0));
        assert_eq!(parse_datetime_text("1-Jan-2024"), Some(45292.0));
        assert_eq!(parse_datetime_text("January 1, 2024"), Some(45292.0));
        assert_eq!(parse_datetime_text("12:00"), Some(0.5));
        assert_eq!(parse_datetime_text("6:00 PM"), Some(0.75));
        assert_eq!(parse_datetime_text("2024-01-01 12:00"), Some(45292.5));
        assert_eq!(parse_datetime_text("hello"), None);
    }
}
