//! Text functions.

use super::{bool_or, num, num_or, text, Registry, VAR};
use crate::ast::Expr;
use crate::eval::Ctx;
use crate::tryv;
use crate::value::{parse_number_text, Array, ErrorKind, Value};

const CAT: &str = "Text";

/// Maximum length of a text value in Excel.
const MAX_TEXT: usize = 32767;

pub fn register(r: &mut Registry) {
    r.raw("ARRAYTOTEXT", 1, 2, arraytotext, CAT, "ARRAYTOTEXT(array, [format])", "Returns an array of text values from any specified range.");
    r.scalar("ASC", 1, 1, asc, CAT, "ASC(text)", "Changes full-width (double-byte) characters to half-width (single-byte) characters.");
    r.scalar("BAHTTEXT", 1, 1, bahttext, CAT, "BAHTTEXT(number)", "Converts a number to Thai text and adds a suffix of \"Baht\".");
    r.scalar("CHAR", 1, 1, char_fn, CAT, "CHAR(number)", "Returns the character specified by the code number.");
    r.scalar("CLEAN", 1, 1, clean, CAT, "CLEAN(text)", "Removes all nonprintable characters from text.");
    r.scalar("CODE", 1, 1, code, CAT, "CODE(text)", "Returns a numeric code for the first character in a text string.");
    r.raw("CONCAT", 1, VAR, concat, CAT, "CONCAT(text1, [text2], ...)", "Combines the text from multiple ranges and/or strings.");
    r.scalar("CONCATENATE", 1, VAR, concatenate, CAT, "CONCATENATE(text1, [text2], ...)", "Joins several text items into one text item.");
    r.scalar("DBCS", 1, 1, dbcs, CAT, "DBCS(text)", "Changes half-width (single-byte) characters to full-width (double-byte) characters.");
    r.scalar("DOLLAR", 1, 2, dollar, CAT, "DOLLAR(number, [decimals])", "Converts a number to text, using the $ (dollar) currency format.");
    r.scalar("EXACT", 2, 2, exact, CAT, "EXACT(text1, text2)", "Checks to see if two text values are identical (case-sensitive).");
    r.scalar("FIND", 2, 3, find, CAT, "FIND(find_text, within_text, [start_num])", "Finds one text value within another (case-sensitive).");
    r.scalar("FINDB", 2, 3, find, CAT, "FINDB(find_text, within_text, [start_num])", "Finds one text value within another (case-sensitive), counting bytes.");
    r.scalar("FIXED", 1, 3, fixed, CAT, "FIXED(number, [decimals], [no_commas])", "Formats a number as text with a fixed number of decimals.");
    r.scalar("LEFT", 1, 2, left, CAT, "LEFT(text, [num_chars])", "Returns the leftmost characters from a text value.");
    r.scalar("LEFTB", 1, 2, left, CAT, "LEFTB(text, [num_bytes])", "Returns the leftmost characters from a text value, counting bytes.");
    r.scalar("LEN", 1, 1, len, CAT, "LEN(text)", "Returns the number of characters in a text string.");
    r.scalar("LENB", 1, 1, len, CAT, "LENB(text)", "Returns the number of bytes used to represent the characters in a text string.");
    r.scalar("LOWER", 1, 1, lower, CAT, "LOWER(text)", "Converts text to lowercase.");
    r.scalar("MID", 3, 3, mid, CAT, "MID(text, start_num, num_chars)", "Returns a specific number of characters from a text string starting at the position you specify.");
    r.scalar("MIDB", 3, 3, mid, CAT, "MIDB(text, start_num, num_bytes)", "Returns a specific number of characters from a text string, counting bytes.");
    r.scalar("NUMBERVALUE", 1, 3, numbervalue, CAT, "NUMBERVALUE(text, [decimal_separator], [group_separator])", "Converts text to a number in a locale-independent manner.");
    r.scalar("PROPER", 1, 1, proper, CAT, "PROPER(text)", "Capitalizes the first letter in each word of a text value.");
    r.scalar("REGEXEXTRACT", 2, 4, regexextract, CAT, "REGEXEXTRACT(text, pattern, [return_mode], [case_sensitivity])", "Extracts strings within the provided text that match the pattern.");
    r.scalar("REGEXREPLACE", 3, 5, regexreplace, CAT, "REGEXREPLACE(text, pattern, replacement, [occurrence], [case_sensitivity])", "Replaces strings within the provided text that match the pattern with replacement.");
    r.scalar("REGEXTEST", 2, 3, regextest, CAT, "REGEXTEST(text, pattern, [case_sensitivity])", "Determines whether any part of text matches the pattern.");
    r.scalar("REPLACE", 4, 4, replace, CAT, "REPLACE(old_text, start_num, num_chars, new_text)", "Replaces characters within text.");
    r.scalar("REPLACEB", 4, 4, replace, CAT, "REPLACEB(old_text, start_num, num_bytes, new_text)", "Replaces characters within text, counting bytes.");
    r.scalar("REPT", 2, 2, rept, CAT, "REPT(text, number_times)", "Repeats text a given number of times.");
    r.scalar("RIGHT", 1, 2, right, CAT, "RIGHT(text, [num_chars])", "Returns the rightmost characters from a text value.");
    r.scalar("RIGHTB", 1, 2, right, CAT, "RIGHTB(text, [num_bytes])", "Returns the rightmost characters from a text value, counting bytes.");
    r.scalar("SEARCH", 2, 3, search, CAT, "SEARCH(find_text, within_text, [start_num])", "Finds one text value within another (not case-sensitive, wildcards allowed).");
    r.scalar("SEARCHB", 2, 3, search, CAT, "SEARCHB(find_text, within_text, [start_num])", "Finds one text value within another (not case-sensitive), counting bytes.");
    r.scalar("SUBSTITUTE", 3, 4, substitute, CAT, "SUBSTITUTE(text, old_text, new_text, [instance_num])", "Substitutes new text for old text in a text string.");
    r.scalar("T", 1, 1, t_fn, CAT, "T(value)", "Converts its arguments to text.");
    r.scalar("TEXT", 2, 2, text_fn, CAT, "TEXT(value, format_text)", "Formats a number and converts it to text.");
    r.raw("TEXTAFTER", 2, 6, textafter, CAT, "TEXTAFTER(text, delimiter, [instance_num], [match_mode], [match_end], [if_not_found])", "Returns text that occurs after a given character or string.");
    r.raw("TEXTBEFORE", 2, 6, textbefore, CAT, "TEXTBEFORE(text, delimiter, [instance_num], [match_mode], [match_end], [if_not_found])", "Returns text that occurs before a given character or string.");
    r.raw("TEXTJOIN", 3, VAR, textjoin, CAT, "TEXTJOIN(delimiter, ignore_empty, text1, [text2], ...)", "Combines the text from multiple ranges and/or strings, with a delimiter.");
    r.raw("TEXTSPLIT", 2, 6, textsplit, CAT, "TEXTSPLIT(text, col_delimiter, [row_delimiter], [ignore_empty], [match_mode], [pad_with])", "Splits text strings by using column and row delimiters.");
    r.scalar("TRIM", 1, 1, trim, CAT, "TRIM(text)", "Removes spaces from text, leaving single spaces between words.");
    r.scalar("UNICHAR", 1, 1, unichar, CAT, "UNICHAR(number)", "Returns the Unicode character that is referenced by the given numeric value.");
    r.scalar("UNICODE", 1, 1, unicode, CAT, "UNICODE(text)", "Returns the number (code point) that corresponds to the first character of the text.");
    r.scalar("UPPER", 1, 1, upper, CAT, "UPPER(text)", "Converts text to uppercase.");
    r.scalar("VALUE", 1, 1, value_fn, CAT, "VALUE(text)", "Converts a text argument to a number.");
    r.scalar("VALUETOTEXT", 1, 2, valuetotext, CAT, "VALUETOTEXT(value, [format])", "Returns text from any specified value.");
}

// ----- helpers ---------------------------------------------------------------

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn text_result(s: String) -> Value {
    if s.chars().count() > MAX_TEXT {
        Value::Error(ErrorKind::Value)
    } else {
        Value::Text(s)
    }
}

/// Integer argument (truncated) with a default when omitted/empty.
fn int_or(args: &[Value], i: usize, default: f64) -> Result<f64, ErrorKind> {
    Ok(num_or(args, i, default)?.trunc())
}

/// Text form used when a value is shown "as is" (errors as their text).
fn value_as_text(v: &Value, strict: bool) -> String {
    match v {
        Value::Text(s) if strict => format!("\"{}\"", s.replace('"', "\"\"")),
        Value::Text(s) => s.clone(),
        Value::Error(e) => e.as_str().to_string(),
        Value::Empty => String::new(),
        v => v.display(),
    }
}

// ----- simple text functions -------------------------------------------------

fn arraytotext(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let arr = match ctx.eval_value(&args[0]) {
        Value::Array(a) => *a,
        v => Array::new(1, 1, vec![v]),
    };
    let fmt = tryv!(ctx.opt_number(args, 1, 0.0)).trunc();
    let strict = match fmt as i64 {
        0 => false,
        1 => true,
        _ => return Value::Error(ErrorKind::Value),
    };
    let mut out = String::new();
    if strict {
        out.push('{');
        for r in 0..arr.rows {
            if r > 0 {
                out.push(';');
            }
            for c in 0..arr.cols {
                if c > 0 {
                    out.push(',');
                }
                out.push_str(&value_as_text(arr.get(r, c), true));
            }
        }
        out.push('}');
    } else {
        let parts: Vec<String> = arr.data.iter().map(|v| value_as_text(v, false)).collect();
        out = parts.join(", ");
    }
    text_result(out)
}

fn valuetotext(args: &[Value]) -> Value {
    let fmt = tryv!(int_or(args, 1, 0.0));
    let strict = match fmt as i64 {
        0 => false,
        1 => true,
        _ => return Value::Error(ErrorKind::Value),
    };
    Value::Text(value_as_text(&args[0], strict))
}

fn asc(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    Value::Text(
        s.chars()
            .map(|c| match c as u32 {
                0x3000 => ' ',
                u @ 0xFF01..=0xFF5E => char::from_u32(u - 0xFEE0).unwrap_or(c),
                _ => c,
            })
            .collect(),
    )
}

fn dbcs(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    Value::Text(
        s.chars()
            .map(|c| match c as u32 {
                0x20 => '\u{3000}',
                u @ 0x21..=0x7E => char::from_u32(u + 0xFEE0).unwrap_or(c),
                _ => c,
            })
            .collect(),
    )
}

const THAI_DIGITS: [&str; 10] = ["ศูนย์", "หนึ่ง", "สอง", "สาม", "สี่", "ห้า", "หก", "เจ็ด", "แปด", "เก้า"];
const THAI_PLACES: [&str; 6] = ["", "สิบ", "ร้อย", "พัน", "หมื่น", "แสน"];

/// Thai words for a non-negative integer (empty for zero).
fn thai_words(n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    let mut out = String::new();
    let high = n / 1_000_000;
    let low = n % 1_000_000;
    if high > 0 {
        out.push_str(&thai_words(high));
        out.push_str("ล้าน");
    }
    let digits: Vec<u64> = {
        let mut v = Vec::new();
        let mut x = low;
        for _ in 0..6 {
            v.push(x % 10);
            x /= 10;
        }
        v
    };
    for place in (0..6).rev() {
        let d = digits[place] as usize;
        if d == 0 {
            continue;
        }
        match place {
            1 => {
                if d == 1 {
                } else if d == 2 {
                    out.push_str("ยี่");
                } else {
                    out.push_str(THAI_DIGITS[d]);
                }
                out.push_str("สิบ");
            }
            0 => {
                if d == 1 && (low > 1 || high > 0) {
                    out.push_str("เอ็ด");
                } else {
                    out.push_str(THAI_DIGITS[d]);
                }
            }
            _ => {
                out.push_str(THAI_DIGITS[d]);
                out.push_str(THAI_PLACES[place]);
            }
        }
    }
    out
}

fn bahttext(args: &[Value]) -> Value {
    let n = tryv!(num(args, 0));
    let neg = n < 0.0;
    let cents_total = (n.abs() * 100.0).round();
    if cents_total > 1e17 {
        return Value::Error(ErrorKind::Num);
    }
    let cents_total = cents_total as u64;
    let baht = cents_total / 100;
    let satang = cents_total % 100;
    let mut out = String::new();
    if neg && cents_total > 0 {
        out.push_str("ลบ");
    }
    if baht == 0 && satang == 0 {
        out.push_str("ศูนย์บาทถ้วน");
        return Value::Text(out);
    }
    if baht > 0 {
        out.push_str(&thai_words(baht));
        out.push_str("บาท");
    }
    if satang == 0 {
        out.push_str("ถ้วน");
    } else {
        out.push_str(&thai_words(satang));
        out.push_str("สตางค์");
    }
    Value::Text(out)
}

/// Windows-1252 code points for bytes 0x80..0x9F.
const CP1252_HIGH: [u32; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F,
    0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E,
    0x0178,
];

fn char_fn(args: &[Value]) -> Value {
    let n = tryv!(num(args, 0)).trunc();
    if !(1.0..=255.0).contains(&n) {
        return Value::Error(ErrorKind::Value);
    }
    let n = n as u32;
    let cp = if (0x80..0xA0).contains(&n) { CP1252_HIGH[(n - 0x80) as usize] } else { n };
    Value::Text(char::from_u32(cp).map(|c| c.to_string()).unwrap_or_default())
}

fn code(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let Some(c) = s.chars().next() else { return Value::Error(ErrorKind::Value) };
    let u = c as u32;
    let code = if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        u
    } else if let Some(i) = CP1252_HIGH.iter().position(|x| *x == u) {
        0x80 + i as u32
    } else {
        63
    };
    Value::Number(code as f64)
}

fn clean(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    Value::Text(s.chars().filter(|c| (*c as u32) >= 32).collect())
}

fn concat(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let mut out = String::new();
    for a in args {
        if matches!(a, Expr::Missing) {
            continue;
        }
        let arr = tryv!(ctx.eval_array(a));
        for v in arr.data.iter() {
            out.push_str(&tryv!(v.to_text()));
        }
        if out.len() > MAX_TEXT * 4 {
            return Value::Error(ErrorKind::Value);
        }
    }
    text_result(out)
}

fn concatenate(args: &[Value]) -> Value {
    let mut out = String::new();
    for v in args {
        out.push_str(&tryv!(v.to_text()));
    }
    text_result(out)
}

/// Round half away from zero to `decimals` places (negative = left of the point).
fn round_to(n: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals.abs());
    if decimals >= 0 {
        let x = (n.abs() * f * (1.0 + 4.0 * f64::EPSILON)).round() / f;
        x.copysign(n)
    } else {
        ((n.abs() / f).round() * f).copysign(n)
    }
}

/// Format |n| with `decimals` decimals and optional thousands separators.
fn format_fixed(n: f64, decimals: i32, commas: bool) -> String {
    let r = round_to(n, decimals);
    let d = decimals.max(0) as usize;
    let s = format!("{:.*}", d, r.abs());
    let (int_part, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let int_part = if commas {
        let b = int_part.as_bytes();
        let mut o = String::new();
        for (i, ch) in b.iter().enumerate() {
            if i > 0 && (b.len() - i) % 3 == 0 {
                o.push(',');
            }
            o.push(*ch as char);
        }
        o
    } else {
        int_part
    };
    match frac {
        Some(f) => format!("{}.{}", int_part, f),
        None => int_part,
    }
}

fn dollar(args: &[Value]) -> Value {
    let n = tryv!(num(args, 0));
    let d = tryv!(int_or(args, 1, 2.0));
    if d > 127.0 {
        return Value::Error(ErrorKind::Value);
    }
    let d = d as i32;
    let r = round_to(n, d);
    let body = format!("${}", format_fixed(r, d, true));
    if r < 0.0 {
        Value::Text(format!("({})", body))
    } else {
        Value::Text(body)
    }
}

fn fixed(args: &[Value]) -> Value {
    let n = tryv!(num(args, 0));
    let d = tryv!(int_or(args, 1, 2.0));
    if d > 127.0 {
        return Value::Error(ErrorKind::Value);
    }
    let no_commas = tryv!(bool_or(args, 2, false));
    let d = d as i32;
    let r = round_to(n, d);
    let body = format_fixed(r, d, !no_commas);
    Value::Text(if r < 0.0 { format!("-{}", body) } else { body })
}

fn exact(args: &[Value]) -> Value {
    let a = tryv!(text(args, 0));
    let b = tryv!(text(args, 1));
    Value::Bool(a == b)
}

fn find(args: &[Value]) -> Value {
    let needle = chars(&tryv!(text(args, 0)));
    let hay = chars(&tryv!(text(args, 1)));
    let start = tryv!(int_or(args, 2, 1.0));
    if start < 1.0 || start > hay.len() as f64 + 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    let start = start as usize - 1;
    if needle.is_empty() {
        return Value::Number(start as f64 + 1.0);
    }
    if needle.len() > hay.len() {
        return Value::Error(ErrorKind::Value);
    }
    for i in start..=hay.len() - needle.len() {
        if hay[i..i + needle.len()] == needle[..] {
            return Value::Number(i as f64 + 1.0);
        }
    }
    Value::Error(ErrorKind::Value)
}

#[derive(Clone, PartialEq)]
enum Wild {
    Char(char),
    Any,
    One,
}

fn compile_wild(p: &str) -> Vec<Wild> {
    let mut out = Vec::new();
    let mut it = p.chars();
    while let Some(c) = it.next() {
        match c {
            '~' => match it.next() {
                Some(n) => out.push(Wild::Char(n)),
                None => out.push(Wild::Char('~')),
            },
            '*' => out.push(Wild::Any),
            '?' => out.push(Wild::One),
            c => out.push(Wild::Char(c)),
        }
    }
    out
}

/// Does the pattern match some prefix of `s`?
fn wild_prefix(p: &[Wild], s: &[char]) -> bool {
    match p.first() {
        None => true,
        Some(Wild::Any) => (0..=s.len()).any(|i| wild_prefix(&p[1..], &s[i..])),
        Some(Wild::One) => !s.is_empty() && wild_prefix(&p[1..], &s[1..]),
        Some(Wild::Char(c)) => !s.is_empty() && s[0] == *c && wild_prefix(&p[1..], &s[1..]),
    }
}

fn lower_str(s: &str) -> String {
    s.chars().flat_map(|c| c.to_lowercase().next()).collect()
}

fn search(args: &[Value]) -> Value {
    let needle = lower_str(&tryv!(text(args, 0)));
    let hay = chars(&lower_str(&tryv!(text(args, 1))));
    let start = tryv!(int_or(args, 2, 1.0));
    if start < 1.0 || start > hay.len() as f64 + 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    let start = start as usize - 1;
    if needle.is_empty() {
        return Value::Number(start as f64 + 1.0);
    }
    let mut pat = compile_wild(&needle);
    // A trailing '*' never changes the match position.
    while pat.len() > 1 && pat.last() == Some(&Wild::Any) {
        pat.pop();
    }
    for i in start..hay.len() {
        if wild_prefix(&pat, &hay[i..]) {
            return Value::Number(i as f64 + 1.0);
        }
    }
    Value::Error(ErrorKind::Value)
}

fn left(args: &[Value]) -> Value {
    let s = chars(&tryv!(text(args, 0)));
    let n = tryv!(int_or(args, 1, 1.0));
    if n < 0.0 {
        return Value::Error(ErrorKind::Value);
    }
    let n = (n as usize).min(s.len());
    Value::Text(s[..n].iter().collect())
}

fn right(args: &[Value]) -> Value {
    let s = chars(&tryv!(text(args, 0)));
    let n = tryv!(int_or(args, 1, 1.0));
    if n < 0.0 {
        return Value::Error(ErrorKind::Value);
    }
    let n = (n as usize).min(s.len());
    Value::Text(s[s.len() - n..].iter().collect())
}

fn mid(args: &[Value]) -> Value {
    let s = chars(&tryv!(text(args, 0)));
    let start = tryv!(num(args, 1)).trunc();
    let n = tryv!(num(args, 2)).trunc();
    if start < 1.0 || n < 0.0 {
        return Value::Error(ErrorKind::Value);
    }
    let a = ((start as usize) - 1).min(s.len());
    let b = (a + (n.min(1e9) as usize)).min(s.len());
    Value::Text(s[a..b].iter().collect())
}

fn len(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    Value::Number(s.chars().count() as f64)
}

fn lower(args: &[Value]) -> Value {
    Value::Text(tryv!(text(args, 0)).to_lowercase())
}

fn upper(args: &[Value]) -> Value {
    Value::Text(tryv!(text(args, 0)).to_uppercase())
}

fn proper(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let mut out = String::with_capacity(s.len());
    let mut prev_letter = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if prev_letter {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            prev_letter = true;
        } else {
            out.push(c);
            prev_letter = false;
        }
    }
    Value::Text(out)
}

fn numbervalue(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let dec = match args.get(1) {
        None | Some(Value::Empty) => '.',
        Some(v) => match tryv!(v.to_text()).chars().next() {
            Some(c) => c,
            None => return Value::Error(ErrorKind::Value),
        },
    };
    let grp = match args.get(2) {
        None | Some(Value::Empty) => ',',
        Some(v) => match tryv!(v.to_text()).chars().next() {
            Some(c) => c,
            None => return Value::Error(ErrorKind::Value),
        },
    };
    if dec == grp && args.len() > 2 {
        return Value::Error(ErrorKind::Value);
    }
    let mut body: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if body.is_empty() {
        return Value::Number(0.0);
    }
    let mut pct = 0;
    while let Some(b) = body.strip_suffix('%') {
        body = b.to_string();
        pct += 1;
    }
    let mut out = String::new();
    let mut seen_dec = false;
    for c in body.chars() {
        if c == dec {
            if seen_dec {
                return Value::Error(ErrorKind::Value);
            }
            seen_dec = true;
            out.push('.');
        } else if c == grp {
            if seen_dec {
                return Value::Error(ErrorKind::Value);
            }
        } else {
            out.push(c);
        }
    }
    let n = if out.is_empty() || out == "." {
        if pct > 0 || out == "." { 0.0 } else { return Value::Error(ErrorKind::Value) }
    } else {
        match parse_number_text(&out) {
            Some(n) => n,
            None => return Value::Error(ErrorKind::Value),
        }
    };
    Value::number(n / 100f64.powi(pct))
}

fn replace(args: &[Value]) -> Value {
    let s = chars(&tryv!(text(args, 0)));
    let start = tryv!(num(args, 1)).trunc();
    let n = tryv!(num(args, 2)).trunc();
    let new = tryv!(text(args, 3));
    if start < 1.0 || n < 0.0 {
        return Value::Error(ErrorKind::Value);
    }
    let a = ((start as usize) - 1).min(s.len());
    let b = (a + n.min(1e9) as usize).min(s.len());
    let mut out: String = s[..a].iter().collect();
    out.push_str(&new);
    out.extend(s[b..].iter());
    text_result(out)
}

fn rept(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let n = tryv!(num(args, 1)).trunc();
    if n < 0.0 {
        return Value::Error(ErrorKind::Value);
    }
    if s.is_empty() {
        return Value::Text(String::new());
    }
    if n * s.chars().count() as f64 > MAX_TEXT as f64 {
        return Value::Error(ErrorKind::Value);
    }
    Value::Text(s.repeat(n as usize))
}

fn substitute(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let old = tryv!(text(args, 1));
    let new = tryv!(text(args, 2));
    let inst = match args.get(3) {
        None => None,
        Some(v) => {
            let n = tryv!(v.to_number()).trunc();
            if n < 1.0 {
                return Value::Error(ErrorKind::Value);
            }
            Some(n as usize)
        }
    };
    if old.is_empty() {
        return Value::Text(s);
    }
    match inst {
        None => text_result(s.replace(&old, &new)),
        Some(k) => {
            match s.match_indices(&old).nth(k - 1) {
                Some((i, _)) => {
                    let mut out = String::with_capacity(s.len());
                    out.push_str(&s[..i]);
                    out.push_str(&new);
                    out.push_str(&s[i + old.len()..]);
                    text_result(out)
                }
                None => Value::Text(s),
            }
        }
    }
}

fn t_fn(args: &[Value]) -> Value {
    match &args[0] {
        Value::Text(s) => Value::Text(s.clone()),
        Value::Error(e) => Value::Error(*e),
        _ => Value::Text(String::new()),
    }
}

fn text_fn(args: &[Value]) -> Value {
    let v = match &args[0] {
        Value::Error(e) => return Value::Error(*e),
        Value::Text(s) => match parse_number_text(s) {
            Some(n) => Value::Number(n),
            None => Value::Text(s.clone()),
        },
        Value::Empty => Value::Number(0.0),
        v => v.clone(),
    };
    let code = tryv!(text(args, 1));
    Value::Text(crate::numfmt::format_value(&v, &code).text)
}

fn trim(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    Value::Text(s.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "))
}

fn unichar(args: &[Value]) -> Value {
    let n = tryv!(num(args, 0)).trunc();
    if n < 1.0 || n > 0x10FFFF as f64 {
        return Value::Error(ErrorKind::Value);
    }
    match char::from_u32(n as u32) {
        Some(c) => Value::Text(c.to_string()),
        None => Value::Error(ErrorKind::NA),
    }
}

fn unicode(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    match s.chars().next() {
        Some(c) => Value::Number(c as u32 as f64),
        None => Value::Error(ErrorKind::Value),
    }
}

fn value_fn(args: &[Value]) -> Value {
    match &args[0] {
        Value::Number(n) => Value::Number(*n),
        Value::Empty => Value::Number(0.0),
        Value::Text(s) => {
            if s.trim().is_empty() {
                return Value::Error(ErrorKind::Value);
            }
            match parse_number_text(s) {
                Some(n) => Value::number(n),
                None => Value::Error(ErrorKind::Value),
            }
        }
        Value::Error(e) => Value::Error(*e),
        _ => Value::Error(ErrorKind::Value),
    }
}

// ----- TEXTBEFORE / TEXTAFTER / TEXTSPLIT / TEXTJOIN -----------------------

/// Evaluate a delimiter argument to a list of strings.
fn delimiters(ctx: &mut Ctx, e: &Expr) -> Result<Vec<String>, ErrorKind> {
    let arr = ctx.eval_array(e)?;
    arr.data.iter().map(|v| v.to_text()).collect()
}

/// Find all non-overlapping delimiter occurrences as (start, end) char indices.
fn find_delims(s: &[char], delims: &[Vec<char>], ci: bool) -> Vec<(usize, usize)> {
    let norm = |c: char| if ci { c.to_lowercase().next().unwrap_or(c) } else { c };
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let mut hit = None;
        for d in delims {
            if d.is_empty() || i + d.len() > s.len() {
                continue;
            }
            if d.iter().zip(&s[i..i + d.len()]).all(|(a, b)| norm(*a) == norm(*b)) {
                // Prefer the longest delimiter at this position.
                if hit.map(|h| d.len() > h).unwrap_or(true) {
                    hit = Some(d.len());
                }
            }
        }
        match hit {
            Some(l) => {
                out.push((i, i + l));
                i += l;
            }
            None => i += 1,
        }
    }
    out
}

fn text_before_after(ctx: &mut Ctx, args: &[Expr], after: bool) -> Value {
    let textv = ctx.eval_value(&args[0]);
    let delims = match delimiters(ctx, &args[1]) {
        Ok(d) => d,
        Err(e) => return Value::Error(e),
    };
    let inst = tryv!(ctx.opt_number(args, 2, 1.0)).trunc();
    let mode = tryv!(ctx.opt_number(args, 3, 0.0)).trunc();
    let match_end = tryv!(ctx.opt_number(args, 4, 0.0)) != 0.0;
    let not_found = match args.get(5) {
        None | Some(Expr::Missing) => Value::Error(ErrorKind::NA),
        Some(e) => ctx.eval_scalar(e),
    };
    if mode != 0.0 && mode != 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    let ci = mode == 1.0;
    let dchars: Vec<Vec<char>> = delims.iter().map(|d| chars(d)).collect();
    let one = |v: &Value| -> Value {
        let s = match v.to_text() {
            Ok(s) => chars(&s),
            Err(e) => return Value::Error(e),
        };
        if inst == 0.0 || inst.abs() > s.len().max(1) as f64 {
            return Value::Error(ErrorKind::Value);
        }
        let any_empty = dchars.iter().any(|d| d.is_empty());
        let occ = find_delims(&s, &dchars, ci);
        let k = inst.abs() as usize;
        let found: Option<(usize, usize)> = if inst > 0.0 {
            if any_empty && k == 1 {
                Some((0, 0))
            } else if k <= occ.len() {
                Some(occ[k - 1])
            } else if match_end && k == occ.len() + 1 {
                Some((s.len(), s.len()))
            } else {
                None
            }
        } else if any_empty && k == 1 {
            Some((s.len(), s.len()))
        } else if k <= occ.len() {
            Some(occ[occ.len() - k])
        } else if match_end && k == occ.len() + 1 {
            Some((0, 0))
        } else {
            None
        };
        match found {
            Some((a, b)) => Value::Text(if after { s[b..].iter().collect() } else { s[..a].iter().collect() }),
            None => not_found.clone(),
        }
    };
    match textv {
        Value::Array(a) => {
            let data = a.data.iter().map(one).collect();
            Value::from(Array::new(a.rows, a.cols, data))
        }
        Value::Error(e) => Value::Error(e),
        v => one(&v),
    }
}

fn textbefore(ctx: &mut Ctx, args: &[Expr]) -> Value {
    text_before_after(ctx, args, false)
}

fn textafter(ctx: &mut Ctx, args: &[Expr]) -> Value {
    text_before_after(ctx, args, true)
}

fn split_by(s: &[char], delims: &[Vec<char>], ci: bool) -> Vec<String> {
    let occ = find_delims(s, delims, ci);
    let mut out = Vec::with_capacity(occ.len() + 1);
    let mut last = 0;
    for (a, b) in occ {
        out.push(s[last..a].iter().collect());
        last = b;
    }
    out.push(s[last..].iter().collect());
    out
}

fn textsplit(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let s = match ctx.eval_scalar(&args[0]).to_text() {
        Ok(s) => chars(&s),
        Err(e) => return Value::Error(e),
    };
    let col_d = match &args[1] {
        Expr::Missing => vec![],
        e => match delimiters(ctx, e) {
            Ok(d) => d,
            Err(e) => return Value::Error(e),
        },
    };
    let row_d = match args.get(2) {
        None | Some(Expr::Missing) => vec![],
        Some(e) => match delimiters(ctx, e) {
            Ok(d) => d,
            Err(e) => return Value::Error(e),
        },
    };
    let ignore_empty = tryv!(ctx.opt_bool(args, 3, false));
    let mode = tryv!(ctx.opt_number(args, 4, 0.0)).trunc();
    if mode != 0.0 && mode != 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    let ci = mode == 1.0;
    let pad = match args.get(5) {
        None | Some(Expr::Missing) => Value::Error(ErrorKind::NA),
        Some(e) => ctx.eval_scalar(e),
    };
    let col_c: Vec<Vec<char>> = col_d.iter().map(|d| chars(d)).filter(|d| !d.is_empty()).collect();
    let row_c: Vec<Vec<char>> = row_d.iter().map(|d| chars(d)).filter(|d| !d.is_empty()).collect();
    if col_c.is_empty() && row_c.is_empty() {
        return Value::Error(ErrorKind::Value);
    }
    let mut rows: Vec<String> = if row_c.is_empty() { vec![s.iter().collect()] } else { split_by(&s, &row_c, ci) };
    if ignore_empty {
        rows.retain(|r| !r.is_empty());
    }
    let mut table: Vec<Vec<Value>> = Vec::new();
    for r in rows {
        let rc = chars(&r);
        let mut cells: Vec<String> = if col_c.is_empty() { vec![r.clone()] } else { split_by(&rc, &col_c, ci) };
        if ignore_empty {
            cells.retain(|c| !c.is_empty());
        }
        if ignore_empty && cells.is_empty() {
            continue;
        }
        table.push(cells.into_iter().map(Value::Text).collect());
    }
    if table.is_empty() {
        return Value::Error(ErrorKind::Calc);
    }
    let cols = table.iter().map(|r| r.len()).max().unwrap_or(1);
    let nrows = table.len();
    let mut data = Vec::with_capacity(nrows * cols);
    for mut row in table {
        row.resize(cols, pad.clone());
        data.extend(row);
    }
    Value::from(Array::new(nrows, cols, data))
}

fn textjoin(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let delims = match delimiters(ctx, &args[0]) {
        Ok(d) => d,
        Err(e) => return Value::Error(e),
    };
    let ignore_empty = tryv!(ctx.eval_bool(&args[1]));
    let mut items: Vec<String> = Vec::new();
    for a in &args[2..] {
        if matches!(a, Expr::Missing) {
            if !ignore_empty {
                items.push(String::new());
            }
            continue;
        }
        let arr = tryv!(ctx.eval_array(a));
        for v in arr.data.iter() {
            let t = tryv!(v.to_text());
            if ignore_empty && t.is_empty() {
                continue;
            }
            items.push(t);
        }
    }
    let mut out = String::new();
    for (i, it) in items.iter().enumerate() {
        if i > 0 && !delims.is_empty() {
            out.push_str(&delims[(i - 1) % delims.len()]);
        }
        out.push_str(it);
        if out.len() > MAX_TEXT * 4 {
            return Value::Error(ErrorKind::Value);
        }
    }
    text_result(out)
}

// ----- regular expressions ---------------------------------------------------

fn regextest(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let p = tryv!(text(args, 1));
    let ci = tryv!(int_or(args, 2, 0.0));
    if ci != 0.0 && ci != 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    let re = match regex::Regex::new(&p, ci == 1.0) {
        Some(r) => r,
        None => return Value::Error(ErrorKind::Value),
    };
    let s = chars(&s);
    Value::Bool(re.find_at(&s, 0).is_some())
}

fn regexextract(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let p = tryv!(text(args, 1));
    let mode = tryv!(int_or(args, 2, 0.0));
    let ci = tryv!(int_or(args, 3, 0.0));
    if !(0.0..=2.0).contains(&mode) || (ci != 0.0 && ci != 1.0) {
        return Value::Error(ErrorKind::Value);
    }
    let re = match regex::Regex::new(&p, ci == 1.0) {
        Some(r) => r,
        None => return Value::Error(ErrorKind::Value),
    };
    let s = chars(&s);
    let sub = |a: Option<(usize, usize)>| -> Value {
        match a {
            Some((x, y)) => Value::Text(s[x..y].iter().collect()),
            None => Value::Text(String::new()),
        }
    };
    match mode as i64 {
        0 => match re.find_at(&s, 0) {
            Some(caps) => sub(caps[0]),
            None => Value::Error(ErrorKind::NA),
        },
        1 => {
            let all = re.find_all(&s);
            if all.is_empty() {
                return Value::Error(ErrorKind::NA);
            }
            Value::from(Array::column(all.into_iter().map(|c| sub(c[0])).collect()))
        }
        _ => match re.find_at(&s, 0) {
            Some(caps) => {
                if caps.len() <= 1 {
                    return Value::Error(ErrorKind::NA);
                }
                Value::from(Array::row(caps[1..].iter().map(|c| sub(*c)).collect()))
            }
            None => Value::Error(ErrorKind::NA),
        },
    }
}

fn regexreplace(args: &[Value]) -> Value {
    let s = tryv!(text(args, 0));
    let p = tryv!(text(args, 1));
    let rep = chars(&tryv!(text(args, 2)));
    let occ = tryv!(int_or(args, 3, 0.0));
    let ci = tryv!(int_or(args, 4, 0.0));
    if ci != 0.0 && ci != 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    let re = match regex::Regex::new(&p, ci == 1.0) {
        Some(r) => r,
        None => return Value::Error(ErrorKind::Value),
    };
    let s = chars(&s);
    let all = re.find_all(&s);
    let selected: Vec<usize> = if occ == 0.0 {
        (0..all.len()).collect()
    } else if occ > 0.0 {
        let k = occ as usize;
        if k <= all.len() { vec![k - 1] } else { vec![] }
    } else {
        let k = (-occ) as usize;
        if k <= all.len() { vec![all.len() - k] } else { vec![] }
    };
    let mut out = String::new();
    let mut last = 0;
    for i in selected {
        let caps = &all[i];
        let (a, b) = caps[0].unwrap();
        out.extend(s[last..a].iter());
        // Expand $n / ${n} references.
        let mut j = 0;
        while j < rep.len() {
            let c = rep[j];
            if c == '$' && j + 1 < rep.len() {
                if rep[j + 1] == '$' {
                    out.push('$');
                    j += 2;
                    continue;
                }
                let (digits_start, braced) = if rep[j + 1] == '{' { (j + 2, true) } else { (j + 1, false) };
                let mut k = digits_start;
                while k < rep.len() && rep[k].is_ascii_digit() && (braced || k - digits_start < 2) {
                    k += 1;
                }
                if k > digits_start && (!braced || (k < rep.len() && rep[k] == '}')) {
                    let n: usize = rep[digits_start..k].iter().collect::<String>().parse().unwrap_or(usize::MAX);
                    if let Some(Some((x, y))) = caps.get(n) {
                        out.extend(s[*x..*y].iter());
                    }
                    j = if braced { k + 1 } else { k };
                    continue;
                }
            }
            out.push(c);
            j += 1;
        }
        last = b;
    }
    out.extend(s[last..].iter());
    text_result(out)
}

/// A small backtracking regular-expression engine (PCRE-like subset):
/// literals, `.`, classes `[...]`/`[^...]`, `\d \w \s \D \W \S \b \B`,
/// anchors `^ $`, groups `( )`, `(?: )`, alternation `|`, and quantifiers
/// `* + ? {n} {n,} {n,m}` with lazy `?` variants.
mod regex {
    #[derive(Debug, Clone)]
    enum ClassItem {
        Range(char, char),
        Digit(bool),
        Word(bool),
        Space(bool),
    }

    #[derive(Debug, Clone)]
    struct Class {
        neg: bool,
        items: Vec<ClassItem>,
    }

    fn is_word(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    impl Class {
        fn matches(&self, c: char, ci: bool) -> bool {
            let test = |c: char| {
                self.items.iter().any(|it| match it {
                    ClassItem::Range(a, b) => *a <= c && c <= *b,
                    ClassItem::Digit(neg) => c.is_ascii_digit() != *neg,
                    ClassItem::Word(neg) => is_word(c) != *neg,
                    ClassItem::Space(neg) => c.is_whitespace() != *neg,
                })
            };
            let mut m = test(c);
            if !m && ci {
                m = c.to_lowercase().any(&test) || c.to_uppercase().any(&test);
            }
            m != self.neg
        }
    }

    #[derive(Debug, Clone)]
    enum Node {
        Empty,
        Char(char),
        Any,
        Class(Class),
        Start,
        End,
        WordB(bool),
        Group(Box<Node>, Option<usize>),
        Concat(Vec<Node>),
        Alt(Vec<Node>),
        Repeat(Box<Node>, u32, Option<u32>, bool),
    }

    struct Parser {
        p: Vec<char>,
        i: usize,
        groups: usize,
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.p.get(self.i).copied()
        }
        fn alt(&mut self) -> Option<Node> {
            let mut alts = vec![self.concat()?];
            while self.peek() == Some('|') {
                self.i += 1;
                alts.push(self.concat()?);
            }
            Some(if alts.len() == 1 { alts.pop().unwrap() } else { Node::Alt(alts) })
        }
        fn concat(&mut self) -> Option<Node> {
            let mut items = Vec::new();
            while let Some(c) = self.peek() {
                if c == '|' || c == ')' {
                    break;
                }
                let atom = self.atom()?;
                let atom = self.quantifier(atom)?;
                items.push(atom);
            }
            Some(match items.len() {
                0 => Node::Empty,
                1 => items.pop().unwrap(),
                _ => Node::Concat(items),
            })
        }
        fn number(&mut self) -> Option<u32> {
            let st = self.i;
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.i += 1;
            }
            if st == self.i {
                return None;
            }
            self.p[st..self.i].iter().collect::<String>().parse().ok()
        }
        fn quantifier(&mut self, atom: Node) -> Option<Node> {
            let (min, max) = match self.peek() {
                Some('*') => {
                    self.i += 1;
                    (0, None)
                }
                Some('+') => {
                    self.i += 1;
                    (1, None)
                }
                Some('?') => {
                    self.i += 1;
                    (0, Some(1))
                }
                Some('{') => {
                    let save = self.i;
                    self.i += 1;
                    let Some(a) = self.number() else {
                        self.i = save;
                        return Some(atom);
                    };
                    let b = if self.peek() == Some(',') {
                        self.i += 1;
                        if self.peek() == Some('}') { None } else { Some(self.number()?) }
                    } else {
                        Some(a)
                    };
                    if self.peek() != Some('}') {
                        self.i = save;
                        return Some(atom);
                    }
                    self.i += 1;
                    if let Some(b) = b {
                        if b < a || b > 1000 {
                            return None;
                        }
                    }
                    if a > 1000 {
                        return None;
                    }
                    (a, b)
                }
                _ => return Some(atom),
            };
            if matches!(atom, Node::Start | Node::End | Node::WordB(_)) {
                return None;
            }
            let mut greedy = true;
            if self.peek() == Some('?') {
                self.i += 1;
                greedy = false;
            } else if self.peek() == Some('+') {
                // possessive: treat as greedy
                self.i += 1;
            }
            if matches!(self.peek(), Some('*') | Some('+') | Some('?')) {
                return None;
            }
            Some(Node::Repeat(Box::new(atom), min, max, greedy))
        }
        fn escape(&mut self, in_class: bool) -> Option<Result<char, ClassItem>> {
            let c = self.peek()?;
            self.i += 1;
            Some(match c {
                'd' => Err(ClassItem::Digit(false)),
                'D' => Err(ClassItem::Digit(true)),
                'w' => Err(ClassItem::Word(false)),
                'W' => Err(ClassItem::Word(true)),
                's' => Err(ClassItem::Space(false)),
                'S' => Err(ClassItem::Space(true)),
                'n' => Ok('\n'),
                't' => Ok('\t'),
                'r' => Ok('\r'),
                'f' => Ok('\u{c}'),
                'v' => Ok('\u{b}'),
                'b' if in_class => Ok('\u{8}'),
                c if c.is_ascii_alphanumeric() && !in_class => return None,
                c => Ok(c),
            })
        }
        fn class(&mut self) -> Option<Node> {
            // after '['
            let mut neg = false;
            if self.peek() == Some('^') {
                neg = true;
                self.i += 1;
            }
            let mut items = Vec::new();
            let mut first = true;
            loop {
                let c = self.peek()?;
                if c == ']' && !first {
                    self.i += 1;
                    break;
                }
                first = false;
                self.i += 1;
                let lo = if c == '\\' {
                    match self.escape(true)? {
                        Ok(ch) => ch,
                        Err(item) => {
                            items.push(item);
                            continue;
                        }
                    }
                } else {
                    c
                };
                if self.peek() == Some('-') && self.p.get(self.i + 1).map(|c| *c != ']').unwrap_or(false) {
                    self.i += 1;
                    let mut hi = self.peek()?;
                    self.i += 1;
                    if hi == '\\' {
                        hi = self.escape(true)?.ok()?;
                    }
                    if hi < lo {
                        return None;
                    }
                    items.push(ClassItem::Range(lo, hi));
                } else {
                    items.push(ClassItem::Range(lo, lo));
                }
            }
            Some(Node::Class(Class { neg, items }))
        }
        fn atom(&mut self) -> Option<Node> {
            let c = self.peek()?;
            self.i += 1;
            Some(match c {
                '.' => Node::Any,
                '^' => Node::Start,
                '$' => Node::End,
                '[' => self.class()?,
                '(' => {
                    let cap = if self.peek() == Some('?') {
                        if self.p.get(self.i + 1) == Some(&':') {
                            self.i += 2;
                            None
                        } else {
                            return None;
                        }
                    } else {
                        self.groups += 1;
                        Some(self.groups)
                    };
                    let inner = self.alt()?;
                    if self.peek() != Some(')') {
                        return None;
                    }
                    self.i += 1;
                    Node::Group(Box::new(inner), cap)
                }
                ')' | '*' | '+' | '?' => return None,
                '\\' => {
                    match self.peek() {
                        Some('b') => {
                            self.i += 1;
                            return Some(Node::WordB(true));
                        }
                        Some('B') => {
                            self.i += 1;
                            return Some(Node::WordB(false));
                        }
                        _ => {}
                    }
                    match self.escape(false)? {
                        Ok(ch) => Node::Char(ch),
                        Err(item) => Node::Class(Class { neg: false, items: vec![item] }),
                    }
                }
                c => Node::Char(c),
            })
        }
    }

    #[derive(Debug, Clone)]
    enum Inst {
        Char(char),
        Any,
        Class(Class),
        Split(usize, usize),
        Jmp(usize),
        Save(usize),
        Bol,
        Eol,
        WordB(bool),
        /// Record the current position in loop register.
        Mark(usize),
        /// Fail if no progress since the matching Mark.
        Progress(usize),
        Match,
    }

    pub struct Regex {
        prog: Vec<Inst>,
        ngroups: usize,
        nmarks: usize,
        ci: bool,
    }

    struct Compiler {
        prog: Vec<Inst>,
        nmarks: usize,
    }

    impl Compiler {
        fn emit(&mut self, i: Inst) -> usize {
            self.prog.push(i);
            self.prog.len() - 1
        }
        fn compile(&mut self, n: &Node) {
            match n {
                Node::Empty => {}
                Node::Char(c) => {
                    self.emit(Inst::Char(*c));
                }
                Node::Any => {
                    self.emit(Inst::Any);
                }
                Node::Class(c) => {
                    self.emit(Inst::Class(c.clone()));
                }
                Node::Start => {
                    self.emit(Inst::Bol);
                }
                Node::End => {
                    self.emit(Inst::Eol);
                }
                Node::WordB(b) => {
                    self.emit(Inst::WordB(*b));
                }
                Node::Group(inner, cap) => {
                    if let Some(k) = cap {
                        self.emit(Inst::Save(2 * k));
                    }
                    self.compile(inner);
                    if let Some(k) = cap {
                        self.emit(Inst::Save(2 * k + 1));
                    }
                }
                Node::Concat(items) => items.iter().for_each(|i| self.compile(i)),
                Node::Alt(alts) => {
                    let mut jumps = Vec::new();
                    for (i, a) in alts.iter().enumerate() {
                        if i + 1 < alts.len() {
                            let split = self.emit(Inst::Split(0, 0));
                            self.compile(a);
                            jumps.push(self.emit(Inst::Jmp(0)));
                            let next = self.prog.len();
                            self.prog[split] = Inst::Split(split + 1, next);
                        } else {
                            self.compile(a);
                        }
                    }
                    let end = self.prog.len();
                    for j in jumps {
                        self.prog[j] = Inst::Jmp(end);
                    }
                }
                Node::Repeat(inner, min, max, greedy) => {
                    for _ in 0..*min {
                        self.compile(inner);
                    }
                    match max {
                        None => {
                            let mark = self.nmarks;
                            self.nmarks += 1;
                            let split = self.emit(Inst::Split(0, 0));
                            self.emit(Inst::Mark(mark));
                            self.compile(inner);
                            self.emit(Inst::Progress(mark));
                            self.emit(Inst::Jmp(split));
                            let end = self.prog.len();
                            self.prog[split] = if *greedy { Inst::Split(split + 1, end) } else { Inst::Split(end, split + 1) };
                        }
                        Some(m) => {
                            let mut splits = Vec::new();
                            for _ in *min..*m {
                                splits.push(self.emit(Inst::Split(0, 0)));
                                self.compile(inner);
                            }
                            let end = self.prog.len();
                            for s in splits {
                                self.prog[s] = if *greedy { Inst::Split(s + 1, end) } else { Inst::Split(end, s + 1) };
                            }
                        }
                    }
                }
            }
        }
    }

    /// Capture spans (char indices) of a match; index 0 is the whole match.
    pub type Caps = Vec<Option<(usize, usize)>>;

    const STEP_LIMIT: usize = 2_000_000;

    impl Regex {
        pub fn new(pattern: &str, ci: bool) -> Option<Regex> {
            let mut p = Parser { p: pattern.chars().collect(), i: 0, groups: 0 };
            let node = p.alt()?;
            if p.i != p.p.len() {
                return None;
            }
            let mut c = Compiler { prog: Vec::new(), nmarks: 0 };
            c.compile(&node);
            c.emit(Inst::Match);
            Some(Regex { prog: c.prog, ngroups: p.groups, nmarks: c.nmarks, ci })
        }

        fn eq(&self, a: char, b: char) -> bool {
            a == b || (self.ci && a.to_lowercase().eq(b.to_lowercase()))
        }

        fn run(&self, s: &[char], start: usize, steps: &mut usize) -> Option<Caps> {
            let nslots = 2 * (self.ngroups + 1);
            let mut init = vec![usize::MAX; nslots + self.nmarks];
            init[0] = start;
            let mut stack: Vec<(usize, usize, Vec<usize>)> = vec![(0, start, init)];
            while let Some((mut pc, mut pos, mut slots)) = stack.pop() {
                loop {
                    *steps += 1;
                    if *steps > STEP_LIMIT {
                        return None;
                    }
                    match &self.prog[pc] {
                        Inst::Char(c) => {
                            if pos < s.len() && self.eq(s[pos], *c) {
                                pos += 1;
                                pc += 1;
                            } else {
                                break;
                            }
                        }
                        Inst::Any => {
                            if pos < s.len() && s[pos] != '\n' {
                                pos += 1;
                                pc += 1;
                            } else {
                                break;
                            }
                        }
                        Inst::Class(c) => {
                            if pos < s.len() && c.matches(s[pos], self.ci) {
                                pos += 1;
                                pc += 1;
                            } else {
                                break;
                            }
                        }
                        Inst::Split(a, b) => {
                            stack.push((*b, pos, slots.clone()));
                            pc = *a;
                        }
                        Inst::Jmp(a) => pc = *a,
                        Inst::Save(k) => {
                            slots[*k] = pos;
                            pc += 1;
                        }
                        Inst::Bol => {
                            if pos == 0 {
                                pc += 1;
                            } else {
                                break;
                            }
                        }
                        Inst::Eol => {
                            if pos == s.len() {
                                pc += 1;
                            } else {
                                break;
                            }
                        }
                        Inst::WordB(want) => {
                            let a = pos > 0 && is_word(s[pos - 1]);
                            let b = pos < s.len() && is_word(s[pos]);
                            if (a != b) == *want {
                                pc += 1;
                            } else {
                                break;
                            }
                        }
                        Inst::Mark(k) => {
                            slots[nslots + k] = pos;
                            pc += 1;
                        }
                        Inst::Progress(k) => {
                            if slots[nslots + k] == pos {
                                break;
                            }
                            pc += 1;
                        }
                        Inst::Match => {
                            slots[1] = pos;
                            let caps = (0..=self.ngroups)
                                .map(|g| {
                                    let (a, b) = (slots[2 * g], slots[2 * g + 1]);
                                    if a == usize::MAX || b == usize::MAX { None } else { Some((a, b)) }
                                })
                                .collect();
                            return Some(caps);
                        }
                    }
                }
            }
            None
        }

        /// First match starting at or after `from`.
        pub fn find_at(&self, s: &[char], from: usize) -> Option<Caps> {
            let mut steps = 0;
            for st in from..=s.len() {
                if let Some(c) = self.run(s, st, &mut steps) {
                    return Some(c);
                }
                if steps > STEP_LIMIT {
                    return None;
                }
            }
            None
        }

        /// All non-overlapping matches.
        pub fn find_all(&self, s: &[char]) -> Vec<Caps> {
            let mut out = Vec::new();
            let mut from = 0;
            while from <= s.len() {
                let Some(c) = self.find_at(s, from) else { break };
                let (a, b) = c[0].unwrap();
                from = if b > a { b } else { b + 1 };
                out.push(c);
            }
            out
        }
    }
}
