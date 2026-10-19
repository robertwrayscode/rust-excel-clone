//! Excel number format codes.
//!
//! Supports the full custom format language: up to four sections
//! (positive;negative;zero;text), conditions (`[>=100]`), colors (`[Red]`,
//! `[Color10]`), quoted literals, backslash escapes, `_x` spacing, `*x` fill,
//! `@`, digit placeholders `0 # ?`, thousands separators and comma scaling,
//! percent, scientific notation, fractions, dates/times (including elapsed
//! `[h]`), `General` and locale/currency tags such as `[$€-407]`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::datetime::{serial_to_date, weekday, DAY_NAMES, MONTH_NAMES};
use crate::style::Color;
use crate::value::{format_number_general, Value};

/// Formatted cell text plus an optional color from a `[Red]`-style section.
#[derive(Debug, Clone, PartialEq)]
pub struct Formatted {
    pub text: String,
    pub color: Option<Color>,
}

/// Common formats by friendly name (as in Excel's Number Format dropdown).
pub const BUILTIN_FORMATS: &[(&str, &str)] = &[
    ("General", "General"),
    ("Number", "0.00"),
    ("Number (thousands)", "#,##0.00"),
    ("Integer", "0"),
    ("Currency", "$#,##0.00"),
    ("Currency (red negatives)", "$#,##0.00;[Red]($#,##0.00)"),
    ("Accounting", "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)"),
    ("Short Date", "m/d/yyyy"),
    ("Long Date", "dddd, mmmm d, yyyy"),
    ("ISO Date", "yyyy-mm-dd"),
    ("Day-Month-Year", "d-mmm-yy"),
    ("Month-Year", "mmm-yy"),
    ("Time", "h:mm:ss AM/PM"),
    ("Short Time", "h:mm"),
    ("Date Time", "m/d/yyyy h:mm"),
    ("Elapsed Time", "[h]:mm:ss"),
    ("Percentage", "0.00%"),
    ("Percentage (integer)", "0%"),
    ("Fraction", "# ?/?"),
    ("Fraction (two digits)", "# ??/??"),
    ("Scientific", "0.00E+00"),
    ("Text", "@"),
];

/// Excel's built-in number format IDs (0-49) as format codes (en-US).
pub fn builtin_format_code(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "$#,##0_);($#,##0)",
        6 => "$#,##0_);[Red]($#,##0)",
        7 => "$#,##0.00_);($#,##0.00)",
        8 => "$#,##0.00_);[Red]($#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yyyy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        41 => r#"_(* #,##0_);_(* \(#,##0\);_(* "-"_);_(@_)"#,
        42 => r#"_("$"* #,##0_);_("$"* \(#,##0\);_("$"* "-"_);_(@_)"#,
        43 => r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#,
        44 => r#"_("$"* #,##0.00_);_("$"* \(#,##0.00\);_("$"* "-"??_);_(@_)"#,
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// Reverse of [`builtin_format_code`]: the built-in ID of a format code, if any.
pub fn builtin_format_id(code: &str) -> Option<u32> {
    (0..=49).find(|&i| builtin_format_code(i).map(|c| c.eq_ignore_ascii_case(code)).unwrap_or(false))
}

/// Excel's default 64-entry indexed color palette (indices 0..=63).
pub const INDEXED_COLORS: [u32; 64] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, // 0-7
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, // 8-15
    0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, // 16-23
    0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, // 24-31
    0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, // 32-39
    0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, // 40-47
    0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699, 0x969696, // 48-55
    0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333, // 56-63
];

/// Color for an Excel indexed color (64 = system foreground, 65 = system background).
pub fn indexed_color(i: u32) -> Option<Color> {
    match i {
        0..=63 => Some(Color(INDEXED_COLORS[i as usize])),
        64 => Some(Color::BLACK),
        65 => Some(Color::WHITE),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Lit(String),
    /// '0', '#' or '?'
    Digit(char),
    Point,
    Comma,
    Percent,
    /// Scientific notation; `plus` = always show sign; `ch` = 'E' or 'e'.
    Exp { plus: bool, ch: char },
    Slash,
    FixedDen(u32),
    At,
    General,
    Year(u8),
    Month(u8),
    Minute(u8),
    Day(u8),
    Hour(u8),
    Second(u8),
    SubSec(u8),
    AmPm { am: String, pm: String },
    /// Elapsed time: 'h', 'm' or 's'.
    Elapsed(char, u8),
}

impl Tok {
    fn is_datetime(&self) -> bool {
        matches!(
            self,
            Tok::Year(_)
                | Tok::Month(_)
                | Tok::Minute(_)
                | Tok::Day(_)
                | Tok::Hour(_)
                | Tok::Second(_)
                | Tok::SubSec(_)
                | Tok::AmPm { .. }
                | Tok::Elapsed(..)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CondOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq)]
struct Section {
    toks: Vec<Tok>,
    color: Option<Color>,
    cond: Option<(CondOp, f64)>,
    is_date: bool,
    is_text: bool,
}

impl Section {
    fn matches(&self, v: f64) -> bool {
        match self.cond {
            None => true,
            Some((op, x)) => match op {
                CondOp::Lt => v < x,
                CondOp::Le => v <= x,
                CondOp::Gt => v > x,
                CondOp::Ge => v >= x,
                CondOp::Eq => v == x,
                CondOp::Ne => v != x,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Parsed {
    sections: Vec<Section>,
}

fn split_sections(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.last_mut().unwrap().push(c);
                for d in chars.by_ref() {
                    out.last_mut().unwrap().push(d);
                    if d == '"' {
                        break;
                    }
                }
            }
            '\\' | '_' | '*' => {
                out.last_mut().unwrap().push(c);
                if let Some(d) = chars.next() {
                    out.last_mut().unwrap().push(d);
                }
            }
            '[' => {
                out.last_mut().unwrap().push(c);
                for d in chars.by_ref() {
                    out.last_mut().unwrap().push(d);
                    if d == ']' {
                        break;
                    }
                }
            }
            ';' => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out
}

fn named_color(s: &str) -> Option<Color> {
    let l = s.to_ascii_lowercase();
    Some(Color(match l.as_str() {
        "black" => 0x000000,
        "white" => 0xFFFFFF,
        "red" => 0xFF0000,
        "green" => 0x00FF00,
        "blue" => 0x0000FF,
        "yellow" => 0xFFFF00,
        "magenta" => 0xFF00FF,
        "cyan" => 0x00FFFF,
        _ => {
            let n: u32 = l.strip_prefix("color")?.trim().parse().ok()?;
            if (1..=56).contains(&n) {
                INDEXED_COLORS[(n + 7) as usize]
            } else {
                return None;
            }
        }
    }))
}

fn parse_cond(s: &str) -> Option<(CondOp, f64)> {
    let s = s.trim();
    let (op, rest) = if let Some(r) = s.strip_prefix("<=") {
        (CondOp::Le, r)
    } else if let Some(r) = s.strip_prefix(">=") {
        (CondOp::Ge, r)
    } else if let Some(r) = s.strip_prefix("<>") {
        (CondOp::Ne, r)
    } else if let Some(r) = s.strip_prefix('<') {
        (CondOp::Lt, r)
    } else if let Some(r) = s.strip_prefix('>') {
        (CondOp::Gt, r)
    } else if let Some(r) = s.strip_prefix('=') {
        (CondOp::Eq, r)
    } else {
        return None;
    };
    Some((op, rest.trim().parse().ok()?))
}

fn push_lit(toks: &mut Vec<Tok>, s: &str) {
    if let Some(Tok::Lit(prev)) = toks.last_mut() {
        prev.push_str(s);
    } else {
        toks.push(Tok::Lit(s.to_string()));
    }
}

fn parse_section(src: &str) -> Section {
    let chars: Vec<char> = src.chars().collect();
    let mut toks: Vec<Tok> = Vec::new();
    let mut color = None;
    let mut cond = None;
    let mut i = 0;
    let lower_at = |i: usize, pat: &str| -> bool {
        let p: Vec<char> = pat.chars().collect();
        i + p.len() <= chars.len() && chars[i..i + p.len()].iter().zip(&p).all(|(a, b)| a.to_ascii_lowercase() == *b)
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    s.push(chars[i]);
                    i += 1;
                }
                i += 1;
                push_lit(&mut toks, &s);
                continue;
            }
            '\\' => {
                if i + 1 < chars.len() {
                    push_lit(&mut toks, &chars[i + 1].to_string());
                }
                i += 2;
                continue;
            }
            '_' => {
                push_lit(&mut toks, " ");
                i += 2;
                continue;
            }
            '*' => {
                i += 2;
                continue;
            }
            '[' => {
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != ']' {
                    s.push(chars[i]);
                    i += 1;
                }
                i += 1;
                let l = s.to_ascii_lowercase();
                if let Some(col) = named_color(&s) {
                    color = Some(col);
                } else if l.starts_with(['<', '>', '=']) {
                    cond = parse_cond(&s);
                } else if let Some(rest) = s.strip_prefix('$') {
                    let sym = rest.split('-').next().unwrap_or("");
                    if !sym.is_empty() {
                        push_lit(&mut toks, sym);
                    }
                } else if !l.is_empty() && l.chars().all(|ch| ch == l.chars().next().unwrap()) {
                    let ch = l.chars().next().unwrap();
                    if matches!(ch, 'h' | 'm' | 's') {
                        toks.push(Tok::Elapsed(ch, l.len() as u8));
                    }
                }
                continue;
            }
            '0' | '#' | '?' => {
                toks.push(Tok::Digit(c));
            }
            '1'..='9' if matches!(toks.last(), Some(Tok::Slash)) => {
                let mut n = 0u32;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    n = n.saturating_mul(10).saturating_add(chars[i] as u32 - '0' as u32);
                    i += 1;
                }
                toks.push(Tok::FixedDen(n.max(1)));
                continue;
            }
            '.' => toks.push(Tok::Point),
            ',' => toks.push(Tok::Comma),
            '%' => toks.push(Tok::Percent),
            '/' => toks.push(Tok::Slash),
            '@' => toks.push(Tok::At),
            'E' | 'e' if i + 1 < chars.len() && (chars[i + 1] == '+' || chars[i + 1] == '-') => {
                toks.push(Tok::Exp { plus: chars[i + 1] == '+', ch: c });
                i += 2;
                continue;
            }
            _ if lower_at(i, "general") => {
                toks.push(Tok::General);
                i += 7;
                continue;
            }
            _ if lower_at(i, "am/pm") => {
                toks.push(Tok::AmPm { am: "AM".into(), pm: "PM".into() });
                i += 5;
                continue;
            }
            _ if lower_at(i, "a/p") => {
                let (am, pm) = (chars[i].to_string(), chars[i + 2].to_string());
                toks.push(Tok::AmPm { am, pm });
                i += 3;
                continue;
            }
            _ => {
                let l = c.to_ascii_lowercase();
                if matches!(l, 'y' | 'm' | 'd' | 'h' | 's') {
                    let mut n = 0;
                    while i < chars.len() && chars[i].to_ascii_lowercase() == l {
                        n += 1;
                        i += 1;
                    }
                    let n = n.min(255) as u8;
                    toks.push(match l {
                        'y' => Tok::Year(n),
                        'm' => Tok::Month(n),
                        'd' => Tok::Day(n),
                        'h' => Tok::Hour(n),
                        _ => Tok::Second(n),
                    });
                    continue;
                }
                push_lit(&mut toks, &c.to_string());
            }
        }
        i += 1;
    }
    let is_date = toks.iter().any(|t| t.is_datetime());
    if is_date {
        // Sub-second digits after seconds: ".0", ".00", ".000"
        let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
        let mut j = 0;
        while j < toks.len() {
            if toks[j] == Tok::Point && matches!(toks.get(j + 1), Some(Tok::Digit('0'))) {
                let mut n = 0u8;
                let mut k = j + 1;
                while matches!(toks.get(k), Some(Tok::Digit('0'))) {
                    n += 1;
                    k += 1;
                }
                out.push(Tok::SubSec(n.min(3)));
                j = k;
                continue;
            }
            out.push(match &toks[j] {
                Tok::Point => Tok::Lit(".".into()),
                Tok::Comma => Tok::Lit(",".into()),
                Tok::Percent => Tok::Lit("%".into()),
                Tok::Slash => Tok::Lit("/".into()),
                Tok::Digit(c) => Tok::Lit(c.to_string()),
                t => t.clone(),
            });
            j += 1;
        }
        toks = out;
        // Month vs minute disambiguation.
        let dt: Vec<usize> = (0..toks.len()).filter(|&k| toks[k].is_datetime() && !matches!(toks[k], Tok::AmPm { .. })).collect();
        for (pos, &k) in dt.iter().enumerate() {
            if let Tok::Month(n) = toks[k] {
                if n > 2 {
                    continue;
                }
                let prev_hour = pos > 0 && matches!(toks[dt[pos - 1]], Tok::Hour(_) | Tok::Elapsed('h', _));
                let next_sec = pos + 1 < dt.len() && matches!(toks[dt[pos + 1]], Tok::Second(_) | Tok::Elapsed('s', _));
                if prev_hour || next_sec {
                    toks[k] = Tok::Minute(n);
                }
            }
        }
    }
    let is_text = toks.contains(&Tok::At);
    Section { toks, color, cond, is_date, is_text }
}

fn parse_code(code: &str) -> Parsed {
    let sections = split_sections(code).iter().take(4).map(|s| parse_section(s)).collect();
    Parsed { sections }
}

thread_local! {
    static CACHE: RefCell<HashMap<String, Rc<Parsed>>> = RefCell::new(HashMap::new());
}

fn parsed(code: &str) -> Rc<Parsed> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(p) = c.get(code) {
            return p.clone();
        }
        if c.len() > 512 {
            c.clear();
        }
        let p = Rc::new(parse_code(code));
        c.insert(code.to_string(), p.clone());
        p
    })
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// True if the format code displays dates or times.
pub fn is_date_format(code: &str) -> bool {
    let p = parsed(code);
    p.sections.iter().find(|s| !s.is_text).map(|s| s.is_date).unwrap_or(false)
}

/// True if the format code is `General` (or empty).
pub fn is_general(code: &str) -> bool {
    let c = code.trim();
    c.is_empty() || c.eq_ignore_ascii_case("general")
}

/// Format a value with an Excel number format code.
pub fn format_value(v: &Value, code: &str) -> Formatted {
    match v {
        Value::Empty => Formatted { text: String::new(), color: None },
        Value::Number(n) => format_number(*n, code),
        Value::Text(t) => format_text(t, code),
        Value::Bool(b) => Formatted { text: if *b { "TRUE".into() } else { "FALSE".into() }, color: None },
        Value::Error(e) => Formatted { text: e.as_str().into(), color: None },
        Value::Array(a) => format_value(a.data.first().unwrap_or(&Value::Empty), code),
    }
}

fn format_text(t: &str, code: &str) -> Formatted {
    if is_general(code) {
        return Formatted { text: t.to_string(), color: None };
    }
    let p = parsed(code);
    let sec = if p.sections.len() >= 4 {
        Some(&p.sections[3])
    } else {
        p.sections.iter().find(|s| s.is_text)
    };
    match sec {
        None => Formatted { text: t.to_string(), color: None },
        Some(s) => {
            let mut out = String::new();
            for tok in &s.toks {
                match tok {
                    Tok::Lit(l) => out.push_str(l),
                    Tok::At => out.push_str(t),
                    _ => {}
                }
            }
            Formatted { text: out, color: s.color }
        }
    }
}

/// Format a number with an Excel number format code.
pub fn format_number(n: f64, code: &str) -> Formatted {
    if is_general(code) {
        return Formatted { text: format_number_general(n), color: None };
    }
    if !n.is_finite() {
        return Formatted { text: "#NUM!".into(), color: None };
    }
    let p = parsed(code);
    let nums: Vec<&Section> = p.sections.iter().take(3).filter(|s| !s.is_text).collect();
    if nums.is_empty() {
        return Formatted { text: format_number_general(n), color: None };
    }
    let has_cond = nums.iter().any(|s| s.cond.is_some());
    // (section, show minus sign)
    let (sec, minus): (&Section, bool) = if has_cond {
        let chosen = nums.iter().enumerate().find(|(i, s)| match s.cond {
            Some(_) => s.matches(n),
            // An unconditioned second section of three handles negatives.
            None => !(*i == 1 && nums.len() >= 3) || n < 0.0,
        });
        match chosen {
            None => return Formatted { text: "#".repeat(11), color: None },
            Some((i, s)) => {
                // A negative value shows its sign unless the section only accepts negatives.
                let neg_only = matches!(s.cond, Some((CondOp::Lt, x)) | Some((CondOp::Le, x)) if x <= 0.0)
                    || (s.cond.is_none() && i == 1 && nums.len() >= 3);
                (*s, n < 0.0 && !neg_only)
            }
        }
    } else {
        match nums.len() {
            1 => (nums[0], n < 0.0),
            2 => (if n < 0.0 { nums[1] } else { nums[0] }, false),
            _ => {
                let s = if n > 0.0 {
                    nums[0]
                } else if n < 0.0 {
                    nums[1]
                } else {
                    nums[2]
                };
                (s, false)
            }
        }
    };
    let text = render_number_section(sec, n.abs(), minus);
    Formatted { text, color: sec.color }
}

// ---------------------------------------------------------------------------
// Decimal rounding helpers
// ---------------------------------------------------------------------------

/// Decimal digits of a positive number rounded to 15 significant digits:
/// value = 0.d1d2d3... * 10^point.
fn to_dec(x: f64) -> (Vec<u8>, i32) {
    if x == 0.0 {
        return (vec![0], 1);
    }
    let s = format!("{:.14e}", x);
    let (m, e) = s.split_once('e').unwrap();
    let exp: i32 = e.parse().unwrap();
    let mut digits: Vec<u8> = m.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    while digits.len() > 1 && *digits.last().unwrap() == 0 {
        digits.pop();
    }
    (digits, exp + 1)
}

/// Round to `frac` decimal places (half away from zero) and return the integer
/// digits (no leading zeros; empty for zero) and exactly `frac` fraction digits.
fn round_dec(digits: &[u8], point: i32, frac: usize) -> (String, String) {
    let mut d: Vec<u8> = digits.to_vec();
    let mut point = point;
    let keep = point + frac as i32;
    if keep < 0 {
        d.clear();
    } else {
        let keep = keep as usize;
        if keep < d.len() {
            let up = d[keep] >= 5;
            d.truncate(keep);
            if up {
                let mut i = d.len();
                let mut carry = true;
                while carry && i > 0 {
                    i -= 1;
                    if d[i] == 9 {
                        d[i] = 0;
                    } else {
                        d[i] += 1;
                        carry = false;
                    }
                }
                if carry {
                    d.insert(0, 1);
                    point += 1;
                }
            }
        }
    }
    let mut int = String::new();
    let mut fr = String::new();
    if point > 0 {
        for i in 0..point as usize {
            int.push((b'0' + d.get(i).copied().unwrap_or(0)) as char);
        }
    }
    for k in 0..frac as i32 {
        let idx = point + k;
        let digit = if idx < 0 { 0 } else { d.get(idx as usize).copied().unwrap_or(0) };
        fr.push((b'0' + digit) as char);
    }
    let int = int.trim_start_matches('0').to_string();
    (int, fr)
}

fn round_to(x: f64, frac: usize) -> (String, String) {
    let (d, p) = to_dec(x);
    round_dec(&d, p, frac)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_number_section(sec: &Section, v: f64, minus: bool) -> String {
    let body = if sec.is_date {
        render_date(sec, if minus { -v } else { v })
    } else if sec.toks.contains(&Tok::General) {
        let mut out = String::new();
        for t in &sec.toks {
            match t {
                Tok::Lit(l) => out.push_str(l),
                Tok::General => out.push_str(&format_number_general(v)),
                Tok::Percent => out.push('%'),
                _ => {}
            }
        }
        out
    } else {
        render_plain(sec, v)
    };
    if minus && !sec.is_date {
        format!("-{}", body)
    } else {
        body
    }
}

/// Integer digits laid out right-to-left into placeholders.
/// Returns, for each placeholder (left to right), its rendered text.
fn layout_int(digits: &str, phs: &[char], thousands: bool) -> Vec<String> {
    let n = phs.len();
    if n == 0 {
        return vec![];
    }
    let dchars: Vec<char> = digits.chars().collect();
    // Effective digit string including zero padding.
    let mut out: Vec<String> = vec![String::new(); n];
    // Placeholder i (from the left) holds digit index len - (n - i).
    let extra = dchars.len().saturating_sub(n);
    // Total width of the number in digits (for comma positions): count real digits plus '0' pads.
    let mut cells: Vec<(usize, Option<char>)> = Vec::new(); // (placeholder idx, char)
    for (i, ph) in phs.iter().enumerate() {
        let from_right = n - 1 - i;
        if i == 0 && extra > 0 {
            for c in &dchars[..extra] {
                cells.push((0, Some(*c)));
            }
        }
        let di = dchars.len() as isize - 1 - from_right as isize;
        if di >= 0 {
            cells.push((i, Some(dchars[di as usize])));
        } else {
            match ph {
                '0' => cells.push((i, Some('0'))),
                '?' => cells.push((i, Some(' '))),
                _ => cells.push((i, None)),
            }
        }
    }
    let total_digits = cells.iter().filter(|(_, c)| matches!(c, Some(ch) if ch.is_ascii_digit())).count();
    let mut seen = 0usize;
    for (i, c) in cells {
        if let Some(ch) = c {
            out[i].push(ch);
            if ch.is_ascii_digit() {
                seen += 1;
                let pos_from_right = total_digits - seen;
                if thousands && pos_from_right > 0 && pos_from_right.is_multiple_of(3) {
                    out[i].push(',');
                }
            }
        }
    }
    out
}

fn render_plain(sec: &Section, v: f64) -> String {
    let toks = &sec.toks;
    if toks.contains(&Tok::Slash) {
        if let Some(s) = render_fraction(sec, v) {
            return s;
        }
    }
    let exp_idx = toks.iter().position(|t| matches!(t, Tok::Exp { .. }));
    let mant_end = exp_idx.unwrap_or(toks.len());
    let point_idx = toks[..mant_end].iter().position(|t| *t == Tok::Point);
    let int_end = point_idx.unwrap_or(mant_end);

    // Classify commas.
    let mut thousands = false;
    let mut scale_commas = 0i32;
    let mut comma_kind: Vec<u8> = vec![0; toks.len()]; // 0 literal, 1 thousands, 2 scale
    for i in 0..mant_end {
        if toks[i] == Tok::Comma {
            let before = toks[..i].iter().any(|t| matches!(t, Tok::Digit(_)));
            if !before {
                continue;
            }
            let mut j = i + 1;
            while j < mant_end && toks[j] == Tok::Comma {
                j += 1;
            }
            let next_is_digit = j < mant_end && matches!(toks[j], Tok::Digit(_)) && i < int_end;
            if next_is_digit {
                thousands = true;
                comma_kind[i] = 1;
            } else {
                comma_kind[i] = 2;
                scale_commas += 1;
            }
        }
    }
    let percents = toks.iter().filter(|t| **t == Tok::Percent).count() as i32;
    let mut x = v;
    if percents > 0 {
        x *= 10f64.powi(2 * percents);
    }
    if scale_commas > 0 {
        x /= 10f64.powi(3 * scale_commas);
    }

    let int_phs: Vec<char> = toks[..int_end].iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect();
    let frac_phs: Vec<char> = match point_idx {
        Some(p) => toks[p + 1..mant_end].iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect(),
        None => vec![],
    };
    let exp_phs: Vec<char> = match exp_idx {
        Some(e) => toks[e + 1..].iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect(),
        None => vec![],
    };

    let (int_digits, frac_digits, exponent) = if exp_idx.is_some() {
        let n_int = int_phs.len().max(1) as i32;
        if x == 0.0 {
            (String::new(), "0".repeat(frac_phs.len()), 0)
        } else {
            let (d, p) = to_dec(x);
            let e0 = p - 1; // floor(log10(x))
            let engineering = int_phs.len() > 1 && int_phs.contains(&'#');
            let mut exp = if engineering { e0.div_euclid(n_int) * n_int } else { e0 - (n_int - 1) };
            let (mut i, mut f) = round_dec(&d, p - exp, frac_phs.len());
            if i.len() as i32 > n_int {
                exp += if engineering { n_int } else { 1 };
                let r = round_dec(&d, p - exp, frac_phs.len());
                i = r.0;
                f = r.1;
            }
            (i, f, exp)
        }
    } else {
        let (i, f) = round_to(x, frac_phs.len());
        (i, f, 0)
    };

    let int_cells = layout_int(&int_digits, &int_phs, thousands);

    // Fraction placeholders: trailing zero trimming for '#' and '?'.
    let fchars: Vec<char> = frac_digits.chars().collect();
    let mut frac_cells: Vec<String> = fchars.iter().map(|c| c.to_string()).collect();
    for k in (0..frac_phs.len()).rev() {
        if fchars[k] != '0' || frac_phs[k] == '0' {
            break;
        }
        frac_cells[k] = if frac_phs[k] == '?' { " ".into() } else { String::new() };
    }

    let exp_str = {
        let min = exp_phs.iter().filter(|c| **c == '0').count();
        format!("{:0>width$}", exponent.unsigned_abs(), width = min)
    };

    let mut out = String::new();
    let mut ii = 0usize; // int placeholder counter
    let mut fi = 0usize;
    let mut ei = 0usize;
    let mut emitted_int = false;
    for (idx, t) in toks.iter().enumerate() {
        let in_int = idx < int_end;
        let in_exp = exp_idx.map(|e| idx > e).unwrap_or(false);
        match t {
            Tok::Lit(l) => out.push_str(l),
            Tok::Digit(_) => {
                if in_exp {
                    if ei == 0 {
                        out.push_str(&exp_str);
                    }
                    ei += 1;
                } else if in_int {
                    out.push_str(&int_cells[ii]);
                    ii += 1;
                    emitted_int = true;
                } else {
                    if let Some(c) = frac_cells.get(fi) {
                        out.push_str(c);
                    }
                    fi += 1;
                }
            }
            Tok::Point => {
                if Some(idx) == point_idx {
                    if !emitted_int && int_phs.is_empty() && !int_digits.is_empty() {
                        out.push_str(&int_digits);
                    }
                    out.push('.');
                } else {
                    out.push('.');
                }
            }
            Tok::Comma => {
                if comma_kind[idx] == 0 {
                    out.push(',');
                }
            }
            Tok::Percent => out.push('%'),
            Tok::Exp { plus, ch } => {
                if int_phs.is_empty() && !int_digits.is_empty() && point_idx.is_none() {
                    out.push_str(&int_digits);
                }
                out.push(*ch);
                if exponent < 0 {
                    out.push('-');
                } else if *plus {
                    out.push('+');
                }
                if exp_phs.is_empty() {
                    out.push_str(&exponent.unsigned_abs().to_string());
                }
            }
            Tok::Slash => out.push('/'),
            Tok::FixedDen(d) => out.push_str(&d.to_string()),
            Tok::At => {}
            _ => {}
        }
    }
    out
}

fn best_fraction(f: f64, max_den: u32) -> (u64, u64) {
    let mut best = (0u64, 1u64);
    let mut best_err = f64::INFINITY;
    for d in 1..=max_den.max(1) as u64 {
        let n = (f * d as f64).round();
        let err = (f - n / d as f64).abs();
        if err < best_err - 1e-12 {
            best_err = err;
            best = (n as u64, d);
            if err == 0.0 {
                break;
            }
        }
    }
    best
}

fn render_fraction(sec: &Section, v: f64) -> Option<String> {
    let toks = &sec.toks;
    let slash = toks.iter().position(|t| *t == Tok::Slash)?;
    // numerator group: contiguous digits right before slash
    let mut ns = slash;
    while ns > 0 && matches!(toks[ns - 1], Tok::Digit(_)) {
        ns -= 1;
    }
    if ns == slash {
        return None;
    }
    let num_phs: Vec<char> = toks[ns..slash].iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect();
    // integer group: digit tokens before ns
    let int_idx: Vec<usize> = (0..ns).filter(|&k| matches!(toks[k], Tok::Digit(_))).collect();
    let int_phs: Vec<char> = int_idx.iter().map(|&k| if let Tok::Digit(c) = toks[k] { c } else { '#' }).collect();
    // denominator
    let mut de = slash + 1;
    let mut den_phs: Vec<char> = Vec::new();
    let mut fixed: Option<u32> = None;
    if let Some(Tok::FixedDen(d)) = toks.get(de) {
        fixed = Some(*d);
        de += 1;
    } else {
        while let Some(Tok::Digit(c)) = toks.get(de) {
            den_phs.push(*c);
            de += 1;
        }
    }
    let percents = toks.iter().filter(|t| **t == Tok::Percent).count() as i32;
    let x = v * 10f64.powi(2 * percents);
    let has_int = !int_phs.is_empty();
    let (mut ip, frac) = if has_int { (x.floor(), x - x.floor()) } else { (0.0, x) };
    let (mut num, den) = match fixed {
        Some(d) => ((frac * d as f64).round() as u64, d as u64),
        None => {
            let digits = den_phs.len().clamp(1, 7) as u32;
            best_fraction(frac, 10u32.pow(digits) - 1)
        }
    };
    if has_int && num == den && den > 0 {
        ip += 1.0;
        num = 0;
    }
    let zero_frac = has_int && num == 0;
    let int_str = {
        let (i, _) = round_to(ip, 0);
        if zero_frac && i.is_empty() {
            "0".to_string()
        } else {
            i
        }
    };
    let int_cells = layout_int(&int_str, &int_phs, false);
    let num_s = num.to_string();
    let num_str = if num_s.len() < num_phs.len() {
        let pad: String = num_phs[..num_phs.len() - num_s.len()]
            .iter()
            .map(|c| match c {
                '0' => '0',
                '?' => ' ',
                _ => '\u{0}',
            })
            .filter(|c| *c != '\u{0}')
            .collect();
        format!("{}{}", pad, num_s)
    } else {
        num_s
    };
    let den_s = den.to_string();
    let den_str = if fixed.is_none() && den_s.len() < den_phs.len() {
        let pad: String = den_phs[den_s.len()..]
            .iter()
            .filter_map(|c| match c {
                '0' => Some('0'),
                '?' => Some(' '),
                _ => None,
            })
            .collect();
        if den_phs.first() == Some(&'0') {
            format!("{:0>w$}", den_s, w = den_phs.len())
        } else {
            format!("{}{}", den_s, pad)
        }
    } else {
        den_s
    };
    let last_int = int_idx.last().copied();
    let mut out = String::new();
    let mut ii = 0;
    let mut k = 0;
    while k < toks.len() {
        let t = &toks[k];
        if zero_frac && last_int.map(|l| k > l).unwrap_or(false) && k < de {
            // blank out the fraction (and the separator before it)
            let mut w = 0;
            for t2 in &toks[k..de] {
                w += match t2 {
                    Tok::Lit(l) => l.chars().count(),
                    _ => 0,
                };
            }
            w += num_str.chars().count() + 1 + den_str.chars().count();
            out.push_str(&" ".repeat(w));
            k = de;
            continue;
        }
        match t {
            Tok::Lit(l) => out.push_str(l),
            Tok::Digit(_) if k < ns => {
                out.push_str(&int_cells[ii]);
                ii += 1;
            }
            Tok::Digit(_) if k >= ns && k < slash => {
                if k == ns {
                    out.push_str(&num_str);
                }
            }
            Tok::Digit(_) => {
                if k == slash + 1 {
                    out.push_str(&den_str);
                }
            }
            Tok::FixedDen(_) => out.push_str(&den_str),
            Tok::Slash => out.push('/'),
            Tok::Percent => out.push('%'),
            Tok::Point => out.push('.'),
            Tok::Comma => out.push(','),
            _ => {}
        }
        k += 1;
    }
    Some(out)
}

fn render_date(sec: &Section, v: f64) -> String {
    let toks = &sec.toks;
    let has_elapsed = toks.iter().any(|t| matches!(t, Tok::Elapsed(..)));
    if v < 0.0 && !has_elapsed {
        return "#".repeat(11);
    }
    if v >= 2_958_466.0 {
        return "#".repeat(11);
    }
    let sub = toks.iter().filter_map(|t| if let Tok::SubSec(n) = t { Some(*n) } else { None }).max().unwrap_or(0) as u32;
    let unit = 10i64.pow(sub);
    let neg = v < 0.0;
    let av = v.abs();
    let total = (av * 86400.0 * unit as f64).round() as i64; // in sub-second units
    let total_secs = total / unit;
    let subsec = total % unit;
    let days = total_secs / 86400;
    let tod = total_secs % 86400;
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let (y, mo, d) = serial_to_date(days as f64).unwrap_or((1900, 1, 0));
    let wd = weekday(days as f64) as usize;
    let ampm = toks.iter().any(|t| matches!(t, Tok::AmPm { .. }));
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    for t in toks {
        match t {
            Tok::Lit(l) => out.push_str(l),
            Tok::Year(n) => {
                if *n <= 2 {
                    out.push_str(&format!("{:02}", y.rem_euclid(100)));
                } else {
                    out.push_str(&format!("{:04}", y));
                }
            }
            Tok::Month(n) => {
                let name = MONTH_NAMES[(mo as usize).clamp(1, 12) - 1];
                match n {
                    1 => out.push_str(&mo.to_string()),
                    2 => out.push_str(&format!("{:02}", mo)),
                    3 => out.push_str(&name[..3]),
                    5 => out.push_str(&name[..1]),
                    _ => out.push_str(name),
                }
            }
            Tok::Day(n) => match n {
                1 => out.push_str(&d.to_string()),
                2 => out.push_str(&format!("{:02}", d)),
                3 => out.push_str(&DAY_NAMES[wd][..3]),
                _ => out.push_str(DAY_NAMES[wd]),
            },
            Tok::Hour(n) => {
                let hh = if ampm {
                    let x = h % 12;
                    if x == 0 {
                        12
                    } else {
                        x
                    }
                } else {
                    h
                };
                if *n >= 2 {
                    out.push_str(&format!("{:02}", hh));
                } else {
                    out.push_str(&hh.to_string());
                }
            }
            Tok::Minute(n) => {
                if *n >= 2 {
                    out.push_str(&format!("{:02}", m));
                } else {
                    out.push_str(&m.to_string());
                }
            }
            Tok::Second(n) => {
                if *n >= 2 {
                    out.push_str(&format!("{:02}", s));
                } else {
                    out.push_str(&s.to_string());
                }
            }
            Tok::SubSec(n) => {
                out.push('.');
                let digits = format!("{:0w$}", subsec, w = sub as usize);
                out.push_str(&digits[..(*n as usize).min(digits.len())]);
            }
            Tok::AmPm { am, pm } => out.push_str(if h >= 12 { pm } else { am }),
            Tok::Elapsed(c, n) => {
                let val = match c {
                    'h' => total_secs / 3600,
                    'm' => total_secs / 60,
                    _ => total_secs,
                };
                out.push_str(&format!("{:0w$}", val, w = *n as usize));
            }
            _ => {}
        }
    }
    // When elapsed hours are shown, minutes/seconds are within the hour (already).
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64, code: &str) -> String {
        format_value(&Value::Number(v), code).text
    }

    #[test]
    fn basic_numbers() {
        assert_eq!(f(1234.567, "#,##0.00"), "1,234.57");
        assert_eq!(f(1234.567, "0"), "1235");
        assert_eq!(f(0.5, "0"), "1");
        assert_eq!(f(2.5, "0"), "3");
        assert_eq!(f(2.675, "0.00"), "2.68");
        assert_eq!(f(1.005, "0.00"), "1.01");
        assert_eq!(f(0.0, "0.00"), "0.00");
        assert_eq!(f(0.5, "#.##"), ".5");
        assert_eq!(f(5.0, "#.##"), "5.");
        assert_eq!(f(0.25, "#.00"), ".25");
        assert_eq!(f(5.0, "000"), "005");
        assert_eq!(f(-5.0, "0.0"), "-5.0");
        assert_eq!(f(1234567.891, "#,##0"), "1,234,568");
        assert_eq!(f(123.0, "#,##0"), "123");
        assert_eq!(f(0.0, "#,##0"), "0");
        assert_eq!(f(0.0, "#"), "");
        assert_eq!(f(5.0, "0,000"), "0,005");
        assert_eq!(f(1.5, "0.0?"), "1.5 ");
        assert_eq!(f(1.25, "0.0#"), "1.25");
        assert_eq!(f(1.2, "0.0#"), "1.2");
        assert_eq!(f(123456789.0, "000-00-0000"), "123-45-6789");
        assert_eq!(f(1e20, "0"), "100000000000000000000");
        assert_eq!(f(0.000123, "0.000000"), "0.000123");
    }

    #[test]
    fn sections_and_colors() {
        let r = format_value(&Value::Number(-5.0), "0.00;[Red](0.00)");
        assert_eq!(r.text, "(5.00)");
        assert_eq!(r.color, Some(Color(0xFF0000)));
        let r = format_value(&Value::Number(5.0), "0.00;[Red](0.00)");
        assert_eq!(r.text, "5.00");
        assert_eq!(r.color, None);
        assert_eq!(f(0.0, "0;-0;\"zero\""), "zero");
        assert_eq!(f(-3.0, "0;-0;\"zero\""), "-3");
        assert_eq!(f(3.0, "0;-0;\"zero\""), "3");
        assert_eq!(f(0.0, "0;-0;;@"), "");
        let r = format_value(&Value::Number(3.0), "[Blue]0;[Color10]-0");
        assert_eq!(r.color, Some(Color(0x0000FF)));
        let r = format_value(&Value::Number(-3.0), "[Blue]0;[Color10]-0");
        assert_eq!(r.color, Some(Color(0x008000)));
        assert_eq!(r.text, "-3");
    }

    #[test]
    fn conditions() {
        let code = "[>=100][Red]0;[<0]\"neg\";0.0";
        assert_eq!(f(150.0, code), "150");
        assert_eq!(format_value(&Value::Number(150.0), code).color, Some(Color(0xFF0000)));
        assert_eq!(f(-1.0, code), "neg");
        assert_eq!(f(5.0, code), "5.0");
        assert_eq!(f(1500.0, "[<1000]0;0.0,\"K\""), "1.5K");
        assert_eq!(f(500.0, "[<1000]0;0.0,\"K\""), "500");
    }

    #[test]
    fn percent_scaling_scientific() {
        assert_eq!(f(0.256, "0.0%"), "25.6%");
        assert_eq!(f(0.5, "0%"), "50%");
        assert_eq!(f(1500000.0, "#,##0,,"), "2");
        assert_eq!(f(1500000.0, "0.0,,\"M\""), "1.5M");
        assert_eq!(f(12345678.0, "0.00E+00"), "1.23E+07");
        assert_eq!(f(0.00012, "0.00E+00"), "1.20E-04");
        assert_eq!(f(12345.0, "0.0E-0"), "1.2E4");
        assert_eq!(f(9.999, "0.00E+00"), "1.00E+01");
        assert_eq!(f(12345.0, "##0.0E+0"), "12.3E+3");
        assert_eq!(f(0.0, "0.00E+00"), "0.00E+00");
    }

    #[test]
    fn literals() {
        assert_eq!(f(1234.0, "\"$\"#,##0"), "$1,234");
        assert_eq!(f(-1234.0, "$#,##0"), "-$1,234");
        assert_eq!(f(5.0, "0\" units\""), "5 units");
        assert_eq!(f(5.0, "\\#0"), "#5");
        assert_eq!(f(5.0, "0_)"), "5 ");
        assert_eq!(f(5.0, "*-0"), "5");
        assert_eq!(f(5.0, "[$€-407]#,##0.00"), "€5.00");
        assert_eq!(f(5.0, "[$-409]0.00"), "5.00");
        assert_eq!(f(5.0, "#,##0.00 [$USD]"), "5.00 USD");
        assert_eq!(f(-12.5, "General"), "-12.5");
        assert_eq!(f(12.5, "\"Value: \"General"), "Value: 12.5");
        assert_eq!(f(-12.5, "General;(General)"), "(12.5)");
    }

    #[test]
    fn accounting() {
        let acc = "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)";
        assert_eq!(f(1234.5, acc), " $1,234.50 ");
        assert_eq!(f(-1234.5, acc), " $(1,234.50)");
        assert_eq!(f(0.0, acc), " $-   ");
        assert_eq!(format_value(&Value::Text("x".into()), acc).text, " x ");
    }

    #[test]
    fn fractions() {
        assert_eq!(f(0.5, "# ?/?"), " 1/2");
        assert_eq!(f(1.5, "# ?/?"), "1 1/2");
        assert_eq!(f(3.0, "# ?/?"), "3    ");
        assert_eq!(f(0.0, "# ?/?"), "0    ");
        assert_eq!(f(2.333, "# ??/??"), "2  1/3 ");
        assert_eq!(f(0.3125, "# ??/??"), "  5/16");
        assert_eq!(f(1.375, "# ?/8"), "1 3/8");
        assert_eq!(f(1.5, "?/?"), "3/2");
        assert_eq!(f(-1.5, "# ?/?"), "-1 1/2");
        assert_eq!(f(1.14159, "# ???/???"), "1  16/113");
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(f(45292.0, "yyyy-mm-dd"), "2024-01-01");
        assert_eq!(f(45292.75, "m/d/yyyy h:mm AM/PM"), "1/1/2024 6:00 PM");
        assert_eq!(f(45292.0, "dddd, mmmm d, yyyy"), "Monday, January 1, 2024");
        assert_eq!(f(45292.0, "ddd d-mmm-yy"), "Mon 1-Jan-24");
        assert_eq!(f(45292.0, "mmmmm"), "J");
        assert_eq!(f(45292.0, "m/d/yyyy"), "1/1/2024");
        assert_eq!(f(1.5, "[h]:mm"), "36:00");
        assert_eq!(f(1.5, "[mm]:ss"), "2160:00");
        assert_eq!(f(0.5, "h:mm:ss"), "12:00:00");
        assert_eq!(f(0.5, "hh:mm AM/PM"), "12:00 PM");
        assert_eq!(f(0.0, "h:mm AM/PM"), "12:00 AM");
        assert_eq!(f(0.25, "h:mm a/p"), "6:00 a");
        assert_eq!(f(0.5 + 1.5 / 86400.0, "h:mm:ss.0"), "12:00:01.5");
        assert_eq!(f(0.0, "mm:ss"), "00:00");
        // m after h is minutes; m before s is minutes
        assert_eq!(f(45292.0 + (10.0 * 60.0 + 5.0) / 1440.0, "h:m"), "10:5");
        assert_eq!(f(45292.0 + 65.0 / 86400.0, "m:ss"), "1:05");
        assert_eq!(f(45292.0, "\"Date: \"yyyy"), "Date: 2024");
        assert_eq!(f(-1.0, "yyyy-mm-dd"), "###########");
        assert_eq!(f(0.999999999, "h:mm:ss"), "0:00:00");
        assert_eq!(f(60.0, "yyyy-mm-dd"), "1900-02-29");
    }

    #[test]
    fn text_and_others() {
        let t = |s: &str, code: &str| format_value(&Value::Text(s.into()), code).text;
        assert_eq!(t("abc", "\"Item: \"@"), "Item: abc");
        assert_eq!(t("abc", "0.00"), "abc");
        assert_eq!(t("abc", "0;-0;0;\"<\"@\">\""), "<abc>");
        assert_eq!(t("abc", "0;-0;0;\"txt\""), "txt");
        assert_eq!(format_value(&Value::Bool(true), "0.00").text, "TRUE");
        assert_eq!(format_value(&Value::Error(crate::value::ErrorKind::Div0), "0.00").text, "#DIV/0!");
        assert_eq!(format_value(&Value::Empty, "0.00").text, "");
        assert_eq!(f(12.5, "@"), "12.5");
        assert_eq!(f(12.5, "General"), "12.5");
    }

    #[test]
    fn date_detection() {
        assert!(is_date_format("m/d/yyyy"));
        assert!(is_date_format("h:mm AM/PM"));
        assert!(is_date_format("[h]:mm:ss"));
        assert!(is_date_format("[$-409]mmmm d, yyyy"));
        assert!(!is_date_format("0.00"));
        assert!(!is_date_format("General"));
        assert!(!is_date_format("\"day\"0"));
        assert!(!is_date_format("[Red]0.00"));
        assert!(!is_date_format("@"));
        assert!(!is_date_format("_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)"));
    }

    #[test]
    fn builtins() {
        assert_eq!(builtin_format_code(14), Some("m/d/yyyy"));
        assert_eq!(builtin_format_code(49), Some("@"));
        assert_eq!(builtin_format_code(30), None);
        assert_eq!(builtin_format_id("0.00%"), Some(10));
        for (_, code) in BUILTIN_FORMATS {
            let _ = format_value(&Value::Number(1234.5), code);
            let _ = format_value(&Value::Number(-1234.5), code);
            let _ = format_value(&Value::Number(0.0), code);
        }
        for id in 0..=49 {
            if let Some(c) = builtin_format_code(id) {
                let _ = format_value(&Value::Number(-45292.123), c);
            }
        }
    }
}
