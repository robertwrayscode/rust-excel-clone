//! Date & time functions.

use super::{num, num_or, text, Registry};
use crate::ast::Expr;
use crate::datetime::{
    date_to_serial, days_in_month, is_leap_year, now_serial, parse_datetime_text, serial_to_date, serial_to_hms,
    weekday,
};
use crate::eval::Ctx;
use crate::tryv;
use crate::value::{ErrorKind, Value};

const CAT: &str = "Date & Time";

/// Largest valid serial (9999-12-31).
const MAX_SERIAL: f64 = 2_958_465.0;

pub fn register(r: &mut Registry) {
    r.scalar("DATE", 3, 3, date, CAT, "DATE(year, month, day)", "Returns the serial number of a particular date.");
    r.scalar("DATEDIF", 3, 3, datedif, CAT, "DATEDIF(start_date, end_date, unit)", "Calculates the number of days, months, or years between two dates.");
    r.scalar("DATEVALUE", 1, 1, datevalue, CAT, "DATEVALUE(date_text)", "Converts a date in the form of text to a serial number.");
    r.scalar("DAY", 1, 1, day, CAT, "DAY(serial_number)", "Converts a serial number to a day of the month.");
    r.scalar("DAYS", 2, 2, days, CAT, "DAYS(end_date, start_date)", "Returns the number of days between two dates.");
    r.scalar("DAYS360", 2, 3, days360, CAT, "DAYS360(start_date, end_date, [method])", "Calculates the number of days between two dates based on a 360-day year.");
    r.scalar("EDATE", 2, 2, edate, CAT, "EDATE(start_date, months)", "Returns the serial number of the date that is the indicated number of months before or after the start date.");
    r.scalar("EOMONTH", 2, 2, eomonth, CAT, "EOMONTH(start_date, months)", "Returns the serial number of the last day of the month before or after a specified number of months.");
    r.scalar("HOUR", 1, 1, hour, CAT, "HOUR(serial_number)", "Converts a serial number to an hour.");
    r.scalar("ISOWEEKNUM", 1, 1, isoweeknum, CAT, "ISOWEEKNUM(date)", "Returns the number of the ISO week number of the year for a given date.");
    r.scalar("MINUTE", 1, 1, minute, CAT, "MINUTE(serial_number)", "Converts a serial number to a minute.");
    r.scalar("MONTH", 1, 1, month, CAT, "MONTH(serial_number)", "Converts a serial number to a month.");
    r.raw("NETWORKDAYS", 2, 3, networkdays, CAT, "NETWORKDAYS(start_date, end_date, [holidays])", "Returns the number of whole workdays between two dates.");
    r.raw("NETWORKDAYS.INTL", 2, 4, networkdays_intl, CAT, "NETWORKDAYS.INTL(start_date, end_date, [weekend], [holidays])", "Returns the number of whole workdays between two dates using parameters to indicate which and how many days are weekend days.");
    r.scalar("NOW", 0, 0, |_| Value::Number(now_serial()), CAT, "NOW()", "Returns the serial number of the current date and time.");
    r.scalar("SECOND", 1, 1, second, CAT, "SECOND(serial_number)", "Converts a serial number to a second.");
    r.scalar("TIME", 3, 3, time, CAT, "TIME(hour, minute, second)", "Returns the serial number of a particular time.");
    r.scalar("TIMEVALUE", 1, 1, timevalue, CAT, "TIMEVALUE(time_text)", "Converts a time in the form of text to a serial number.");
    r.scalar("TODAY", 0, 0, |_| Value::Number(now_serial().floor()), CAT, "TODAY()", "Returns the serial number of today's date.");
    r.scalar("WEEKDAY", 1, 2, weekday_fn, CAT, "WEEKDAY(serial_number, [return_type])", "Converts a serial number to a day of the week.");
    r.scalar("WEEKNUM", 1, 2, weeknum, CAT, "WEEKNUM(serial_number, [return_type])", "Converts a serial number to a number representing where the week falls numerically with a year.");
    r.raw("WORKDAY", 2, 3, workday, CAT, "WORKDAY(start_date, days, [holidays])", "Returns the serial number of the date before or after a specified number of workdays.");
    r.raw("WORKDAY.INTL", 2, 4, workday_intl, CAT, "WORKDAY.INTL(start_date, days, [weekend], [holidays])", "Returns the serial number of the date before or after a specified number of workdays using parameters to indicate which and how many days are weekend days.");
    r.scalar("YEAR", 1, 1, year, CAT, "YEAR(serial_number)", "Converts a serial number to a year.");
    r.scalar("YEARFRAC", 2, 3, yearfrac, CAT, "YEARFRAC(start_date, end_date, [basis])", "Returns the year fraction representing the number of whole days between start_date and end_date.");
}

// ----- helpers ---------------------------------------------------------------

/// A date argument: coerced to a number, must be a valid serial; truncated.
fn date_arg(args: &[Value], i: usize) -> Result<f64, ErrorKind> {
    let n = num(args, i)?;
    if !(0.0..MAX_SERIAL + 1.0).contains(&n) {
        return Err(ErrorKind::Num);
    }
    Ok(n.floor())
}

/// A serial (date or time) argument, keeping the fraction.
fn serial_arg(args: &[Value], i: usize) -> Result<f64, ErrorKind> {
    let n = num(args, i)?;
    if !(0.0..MAX_SERIAL + 1.0).contains(&n) {
        return Err(ErrorKind::Num);
    }
    Ok(n)
}

fn ymd(serial: f64) -> Result<(i64, u32, u32), ErrorKind> {
    serial_to_date(serial).ok_or(ErrorKind::Num)
}

fn serial(y: i64, m: i64, d: i64) -> Result<f64, ErrorKind> {
    date_to_serial(y, m, d).ok_or(ErrorKind::Num)
}

fn date_value(r: Result<f64, ErrorKind>) -> Value {
    match r {
        Ok(n) if (0.0..=MAX_SERIAL).contains(&n) => Value::Number(n),
        Ok(_) => Value::Error(ErrorKind::Num),
        Err(e) => Value::Error(e),
    }
}

// ----- basic date parts ---------------------------------------------------------

fn date(args: &[Value]) -> Value {
    let y = tryv!(num(args, 0)).trunc();
    let m = tryv!(num(args, 1)).trunc();
    let d = tryv!(num(args, 2)).trunc();
    if !(0.0..10000.0).contains(&y) || m.abs() > 1e6 || d.abs() > 1e8 {
        return Value::Error(ErrorKind::Num);
    }
    date_value(serial(y as i64, m as i64, d as i64))
}

fn year(args: &[Value]) -> Value {
    let s = tryv!(serial_arg(args, 0));
    Value::Number(tryv!(ymd(s)).0 as f64)
}

fn month(args: &[Value]) -> Value {
    let s = tryv!(serial_arg(args, 0));
    Value::Number(tryv!(ymd(s)).1 as f64)
}

fn day(args: &[Value]) -> Value {
    let s = tryv!(serial_arg(args, 0));
    Value::Number(tryv!(ymd(s)).2 as f64)
}

fn hms(args: &[Value]) -> Result<(u32, u32, u32), ErrorKind> {
    let s = serial_arg(args, 0)?;
    Ok(serial_to_hms(s))
}

fn hour(args: &[Value]) -> Value {
    Value::Number(tryv!(hms(args)).0 as f64)
}

fn minute(args: &[Value]) -> Value {
    Value::Number(tryv!(hms(args)).1 as f64)
}

fn second(args: &[Value]) -> Value {
    Value::Number(tryv!(hms(args)).2 as f64)
}

fn time(args: &[Value]) -> Value {
    let h = tryv!(num(args, 0)).trunc();
    let m = tryv!(num(args, 1)).trunc();
    let s = tryv!(num(args, 2)).trunc();
    if h > 32767.0 || m > 32767.0 || s > 32767.0 {
        return Value::Error(ErrorKind::Num);
    }
    let total = h * 3600.0 + m * 60.0 + s;
    if total < 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    Value::Number((total % 86400.0) / 86400.0)
}

fn datevalue(args: &[Value]) -> Value {
    let s = match &args[0] {
        Value::Text(s) => s.clone(),
        Value::Error(e) => return Value::Error(*e),
        _ => return Value::Error(ErrorKind::Value),
    };
    match parse_datetime_text(&s) {
        Some(n) if n >= 0.0 => Value::Number(n.floor()),
        _ => Value::Error(ErrorKind::Value),
    }
}

fn timevalue(args: &[Value]) -> Value {
    let s = match &args[0] {
        Value::Text(s) => s.clone(),
        Value::Error(e) => return Value::Error(*e),
        _ => return Value::Error(ErrorKind::Value),
    };
    match parse_datetime_text(&s) {
        Some(n) if n >= 0.0 => {
            let f = n - n.floor();
            // Round to the nearest second to avoid float noise.
            Value::Number((f * 86400.0).round() / 86400.0)
        }
        _ => Value::Error(ErrorKind::Value),
    }
}

fn days(args: &[Value]) -> Value {
    let e = tryv!(date_arg(args, 0));
    let s = tryv!(date_arg(args, 1));
    Value::Number(e - s)
}

fn edate_impl(args: &[Value], eom: bool) -> Value {
    let s = tryv!(date_arg(args, 0));
    let m = tryv!(num(args, 1)).trunc();
    let (y, mo, d) = tryv!(ymd(s));
    let total = y * 12 + (mo as i64 - 1) + m as i64;
    let ny = total.div_euclid(12);
    let nm = (total.rem_euclid(12) + 1) as u32;
    if !(1900..=9999).contains(&ny) {
        return Value::Error(ErrorKind::Num);
    }
    let dim = days_in_month(ny, nm);
    let nd = if eom { dim } else { d.min(dim) };
    date_value(serial(ny, nm as i64, nd as i64))
}

fn edate(args: &[Value]) -> Value {
    edate_impl(args, false)
}

fn eomonth(args: &[Value]) -> Value {
    edate_impl(args, true)
}

fn datedif(args: &[Value]) -> Value {
    let s = tryv!(date_arg(args, 0));
    let e = tryv!(date_arg(args, 1));
    let unit = tryv!(text(args, 2)).to_ascii_uppercase();
    if s > e {
        return Value::Error(ErrorKind::Num);
    }
    let (y1, m1, d1) = tryv!(ymd(s));
    let (y2, m2, d2) = tryv!(ymd(e));
    let months = (y2 - y1) * 12 + m2 as i64 - m1 as i64 - if d2 < d1 { 1 } else { 0 };
    let r = match unit.as_str() {
        "Y" => {
            let mut y = y2 - y1;
            if (m2, d2) < (m1, d1) {
                y -= 1;
            }
            y as f64
        }
        "M" => months as f64,
        "D" => e - s,
        "YM" => months.rem_euclid(12) as f64,
        "MD" => {
            if d2 >= d1 {
                (d2 - d1) as f64
            } else {
                // Excel: start day carried into the month before the end month.
                let anchor = tryv!(serial(y2, m2 as i64 - 1, d1 as i64));
                e - anchor
            }
        }
        "YD" => {
            let mut a = tryv!(serial(y2, m1 as i64, d1 as i64));
            if a > e {
                a = tryv!(serial(y2 - 1, m1 as i64, d1 as i64));
            }
            e - a
        }
        _ => return Value::Error(ErrorKind::Num),
    };
    Value::Number(r)
}

fn is_last_of_feb(y: i64, m: u32, d: u32) -> bool {
    m == 2 && d == days_in_month(y, 2)
}

fn days360(args: &[Value]) -> Value {
    let s = tryv!(date_arg(args, 0));
    let e = tryv!(date_arg(args, 1));
    let european = match args.get(2) {
        None | Some(Value::Empty) => false,
        Some(v) => tryv!(v.to_bool()),
    };
    let (y1, m1, d1) = tryv!(ymd(s));
    let (y2, m2, d2) = tryv!(ymd(e));
    let (mut d1, mut d2) = (d1 as i64, d2 as i64);
    if european {
        if d1 == 31 {
            d1 = 30;
        }
        if d2 == 31 {
            d2 = 30;
        }
    } else {
        if d1 == 31 || is_last_of_feb(y1, m1, d1 as u32) {
            d1 = 30;
        }
        if d2 == 31 && d1 >= 30 {
            d2 = 30;
        }
    }
    Value::Number(((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (d2 - d1)) as f64)
}

fn yearfrac(args: &[Value]) -> Value {
    let mut s = tryv!(date_arg(args, 0));
    let mut e = tryv!(date_arg(args, 1));
    let basis = tryv!(num_or(args, 2, 0.0)).trunc();
    if !(0.0..=4.0).contains(&basis) {
        return Value::Error(ErrorKind::Num);
    }
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    let (y1, m1, d1) = tryv!(ymd(s));
    let (y2, m2, d2) = tryv!(ymd(e));
    let r = match basis as i64 {
        0 => {
            let (mut a, mut b) = (d1 as i64, d2 as i64);
            let lf1 = is_last_of_feb(y1, m1, d1);
            let lf2 = is_last_of_feb(y2, m2, d2);
            if a == 31 && b == 31 {
                a = 30;
                b = 30;
            } else if a == 31 {
                a = 30;
            } else if a == 30 && b == 31 {
                b = 30;
            } else if lf1 && lf2 {
                a = 30;
                b = 30;
            } else if lf1 {
                a = 30;
            }
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (b - a)) as f64 / 360.0
        }
        1 => {
            if s == e {
                0.0
            } else {
                let within_year =
                    y1 == y2 || (y2 == y1 + 1 && (m1 > m2 || (m1 == m2 && d1 >= d2)));
                if within_year {
                    let leap = (y1 == y2 && is_leap_year(y1))
                        || feb29_between(s, e, y1, y2)
                        || (m2 == 2 && d2 == 29);
                    let year_len = if leap {
                        366.0
                    } else {
                        365.0
                    };
                    (e - s) / year_len
                } else {
                    let years = (y2 - y1 + 1) as f64;
                    let a = tryv!(serial(y1, 1, 1));
                    let b = tryv!(serial(y2 + 1, 1, 1));
                    (e - s) / ((b - a) / years)
                }
            }
        }
        2 => (e - s) / 360.0,
        3 => (e - s) / 365.0,
        _ => {
            let a = (d1 as i64).min(30);
            let b = (d2 as i64).min(30);
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (b - a)) as f64 / 360.0
        }
    };
    Value::number(r)
}

/// Does a Feb 29 fall within [s, e]?
fn feb29_between(s: f64, e: f64, y1: i64, y2: i64) -> bool {
    (y1..=y2).any(|y| {
        is_leap_year(y)
            && date_to_serial(y, 2, 29).map(|f| f >= s && f <= e).unwrap_or(false)
    })
}

// ----- weekdays and week numbers ---------------------------------------------

fn weekday_fn(args: &[Value]) -> Value {
    let s = tryv!(date_arg(args, 0));
    let t = tryv!(num_or(args, 1, 1.0)).trunc() as i64;
    let w = weekday(s) as i64; // 0 = Sunday
    let r = match t {
        1 => w + 1,
        2 => (w + 6) % 7 + 1,
        3 => (w + 6) % 7,
        11..=17 => {
            // 11 = Monday first ... 17 = Sunday first
            let first = (t - 10) % 7; // weekday index (0=Sun) of day 1
            (w - first).rem_euclid(7) + 1
        }
        _ => return Value::Error(ErrorKind::Num),
    };
    Value::Number(r as f64)
}

fn iso_week(s: f64) -> Result<i64, ErrorKind> {
    let w = weekday(s) as i64; // 0 = Sunday
    let mon_based = (w + 6) % 7; // 0 = Monday
    let thursday = s - mon_based as f64 + 3.0;
    let (ty, _, _) = ymd(thursday)?;
    let jan1 = serial(ty, 1, 1)?;
    Ok(((thursday - jan1) / 7.0).floor() as i64 + 1)
}

fn isoweeknum(args: &[Value]) -> Value {
    let s = tryv!(date_arg(args, 0));
    Value::Number(tryv!(iso_week(s)) as f64)
}

fn weeknum(args: &[Value]) -> Value {
    let s = tryv!(date_arg(args, 0));
    let t = tryv!(num_or(args, 1, 1.0)).trunc() as i64;
    let first = match t {
        1 | 17 => 0,
        2 | 11 => 1,
        12..=16 => t - 10,
        21 => return Value::Number(tryv!(iso_week(s)) as f64),
        _ => return Value::Error(ErrorKind::Num),
    };
    let (y, _, _) = tryv!(ymd(s));
    let jan1 = tryv!(serial(y, 1, 1));
    let offset = (weekday(jan1) as i64 - first).rem_euclid(7);
    Value::Number((((s - jan1) as i64 + offset) / 7 + 1) as f64)
}

// ----- workdays ----------------------------------------------------------------

/// Weekend mask indexed by weekday (0 = Sunday).
fn weekend_mask(v: &Value) -> Result<[bool; 7], ErrorKind> {
    let mut mask = [false; 7];
    match v {
        Value::Empty => {
            mask[0] = true;
            mask[6] = true;
        }
        Value::Text(s) => {
            let b: Vec<char> = s.chars().collect();
            if b.len() != 7 || !b.iter().all(|c| *c == '0' || *c == '1') {
                return Err(ErrorKind::Value);
            }
            for (i, c) in b.iter().enumerate() {
                // string starts on Monday
                mask[(i + 1) % 7] = *c == '1';
            }
            if mask.iter().all(|x| *x) {
                return Err(ErrorKind::Value);
            }
        }
        Value::Error(e) => return Err(*e),
        v => {
            let n = v.to_number()?.trunc() as i64;
            match n {
                1..=7 => {
                    // Monday-based indices (n+4)%7 and (n+5)%7
                    for mb in [(n + 4) % 7, (n + 5) % 7] {
                        mask[((mb + 1) % 7) as usize] = true;
                    }
                }
                11..=17 => {
                    let mb = (n - 12).rem_euclid(7);
                    mask[((mb + 1) % 7) as usize] = true;
                }
                _ => return Err(ErrorKind::Num),
            }
        }
    }
    Ok(mask)
}

/// Collect holiday serials from an argument (range, array or scalar).
fn holidays(ctx: &mut Ctx, e: Option<&Expr>) -> Result<Vec<f64>, ErrorKind> {
    let Some(e) = e else { return Ok(vec![]) };
    if matches!(e, Expr::Missing) {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    ctx.for_each_value(std::slice::from_ref(e), &mut |v, _| {
        match v {
            Value::Empty => {}
            Value::Error(e) => return Err(*e),
            v => {
                let n = v.to_number()?;
                if !(0.0..MAX_SERIAL + 1.0).contains(&n) {
                    return Err(ErrorKind::Num);
                }
                out.push(n.floor());
            }
        }
        Ok(())
    })?;
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out.dedup();
    Ok(out)
}

fn date_expr(ctx: &mut Ctx, e: &Expr) -> Result<f64, ErrorKind> {
    let n = ctx.eval_number(e)?;
    if !(0.0..MAX_SERIAL + 1.0).contains(&n) {
        return Err(ErrorKind::Num);
    }
    Ok(n.floor())
}

fn weekend_arg(ctx: &mut Ctx, e: Option<&Expr>) -> Result<[bool; 7], ErrorKind> {
    match e {
        None | Some(Expr::Missing) => weekend_mask(&Value::Empty),
        Some(e) => {
            let v = ctx.eval_scalar(e);
            weekend_mask(&v)
        }
    }
}

fn is_workday(d: f64, mask: &[bool; 7], hol: &[f64]) -> bool {
    !mask[weekday(d) as usize] && hol.binary_search_by(|h| h.partial_cmp(&d).unwrap()).is_err()
}

fn networkdays_impl(ctx: &mut Ctx, args: &[Expr], weekend: Option<&Expr>, hol: Option<&Expr>) -> Value {
    let s = tryv!(date_expr(ctx, &args[0]));
    let e = tryv!(date_expr(ctx, &args[1]));
    let mask = tryv!(weekend_arg(ctx, weekend));
    let hol = tryv!(holidays(ctx, hol));
    let (a, b, sign) = if s <= e { (s, e, 1.0) } else { (e, s, -1.0) };
    // Count whole weeks quickly, then the remainder.
    let total = (b - a) as i64 + 1;
    let per_week = mask.iter().filter(|x| !**x).count() as i64;
    let weeks = total / 7;
    let mut count = weeks * per_week;
    let mut d = a + (weeks * 7) as f64;
    while d <= b {
        if !mask[weekday(d) as usize] {
            count += 1;
        }
        d += 1.0;
    }
    for h in &hol {
        if *h >= a && *h <= b && !mask[weekday(*h) as usize] {
            count -= 1;
        }
    }
    Value::Number(sign * count as f64)
}

fn networkdays(ctx: &mut Ctx, args: &[Expr]) -> Value {
    networkdays_impl(ctx, args, None, args.get(2))
}

fn networkdays_intl(ctx: &mut Ctx, args: &[Expr]) -> Value {
    networkdays_impl(ctx, args, args.get(2), args.get(3))
}

fn workday_impl(ctx: &mut Ctx, args: &[Expr], weekend: Option<&Expr>, hol: Option<&Expr>) -> Value {
    let s = tryv!(date_expr(ctx, &args[0]));
    let n = tryv!(ctx.eval_number(&args[1])).trunc();
    let mask = tryv!(weekend_arg(ctx, weekend));
    let hol = tryv!(holidays(ctx, hol));
    if n.abs() > 1e7 {
        return Value::Error(ErrorKind::Num);
    }
    let step = if n >= 0.0 { 1.0 } else { -1.0 };
    let mut left = n.abs() as i64;
    let mut d = s;
    while left > 0 {
        d += step;
        if !(1.0..=MAX_SERIAL).contains(&d) {
            return Value::Error(ErrorKind::Num);
        }
        if is_workday(d, &mask, &hol) {
            left -= 1;
        }
    }
    Value::Number(d)
}

fn workday(ctx: &mut Ctx, args: &[Expr]) -> Value {
    workday_impl(ctx, args, None, args.get(2))
}

fn workday_intl(ctx: &mut Ctx, args: &[Expr]) -> Value {
    workday_impl(ctx, args, args.get(2), args.get(3))
}
