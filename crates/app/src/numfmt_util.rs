//! Number-format helpers for the ribbon and the Format Cells dialog.

use engine::Value;

pub const ACCOUNTING: &str = "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)";
pub const COMMA: &str = "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)";

/// (label, code) entries of the ribbon's number format dropdown.
pub const RIBBON_FORMATS: [(&str, &str); 11] = [
    ("General", "General"),
    ("Number", "0.00"),
    ("Currency", "$#,##0.00"),
    ("Accounting", ACCOUNTING),
    ("Short Date", "m/d/yyyy"),
    ("Long Date", "dddd, mmmm d, yyyy"),
    ("Time", "h:mm:ss AM/PM"),
    ("Percentage", "0.00%"),
    ("Fraction", "# ?/?"),
    ("Scientific", "0.00E+00"),
    ("Text", "@"),
];

/// Format Cells categories.
pub const CATEGORIES: [&str; 12] =
    ["General", "Number", "Currency", "Accounting", "Date", "Time", "Percentage", "Fraction", "Scientific", "Text", "Special", "Custom"];

pub const DATE_FORMATS: [&str; 8] =
    ["m/d/yyyy", "dddd, mmmm d, yyyy", "m/d/yy", "mm/dd/yy", "d-mmm", "d-mmm-yy", "mmm-yy", "mmmm d, yyyy"];
pub const TIME_FORMATS: [&str; 6] = ["h:mm:ss AM/PM", "h:mm AM/PM", "h:mm", "h:mm:ss", "m/d/yyyy h:mm", "[h]:mm:ss"];
pub const FRACTION_FORMATS: [&str; 5] = ["# ?/?", "# ??/??", "# ???/???", "# ?/2", "# ?/4"];
pub const SPECIAL_FORMATS: [(&str, &str); 3] =
    [("Zip Code", "00000"), ("Phone Number", "[<=9999999]###-####;(###) ###-####"), ("Social Security Number", "000-00-0000")];
pub const CUSTOM_FORMATS: [&str; 14] = [
    "General",
    "0",
    "0.00",
    "#,##0",
    "#,##0.00",
    "#,##0_);(#,##0)",
    "#,##0_);[Red](#,##0)",
    "#,##0.00_);[Red](#,##0.00)",
    "$#,##0_);($#,##0)",
    "0%",
    "0.00%",
    "0.00E+00",
    "@",
    "yyyy-mm-dd",
];

/// Guess the dialog category for a format code.
pub fn category_of(code: &str) -> &'static str {
    let c = code.trim();
    if c.eq_ignore_ascii_case("general") || c.is_empty() {
        "General"
    } else if c == "@" {
        "Text"
    } else if c == ACCOUNTING || c.starts_with("_(") || c.starts_with("_-") {
        "Accounting"
    } else if c.contains('%') {
        "Percentage"
    } else if c.contains("E+") || c.contains("E-") {
        "Scientific"
    } else if c.contains('?') && c.contains('/') {
        "Fraction"
    } else if engine::numfmt::is_date_format(c) && !c.contains('"') {
        if c.contains('h') || c.contains("s") && !c.contains('y') {
            "Time"
        } else {
            "Date"
        }
    } else if c.contains('$') || c.contains('€') || c.contains('£') {
        "Currency"
    } else if c.chars().all(|ch| "0#,.;()_[]Red-".contains(ch)) {
        "Number"
    } else if SPECIAL_FORMATS.iter().any(|(_, f)| *f == c) {
        "Special"
    } else {
        "Custom"
    }
}

/// Build a Number/Currency/Percentage/Scientific code from options.
pub fn build_number(category: &str, decimals: u32, thousands: bool, symbol: &str, red_negative: bool) -> String {
    let dec = if decimals > 0 { format!(".{}", "0".repeat(decimals as usize)) } else { String::new() };
    match category {
        "Number" => {
            let base = if thousands { format!("#,##0{}", dec) } else { format!("0{}", dec) };
            if red_negative {
                format!("{};[Red]-{}", base, base)
            } else {
                base
            }
        }
        "Currency" => {
            let base = format!("{}#,##0{}", symbol, dec);
            if red_negative {
                format!("{};[Red]-{}", base, base)
            } else {
                base
            }
        }
        "Accounting" => {
            let sym = if symbol.is_empty() { String::new() } else { format!("{}* ", symbol) };
            let sym = if sym.is_empty() { "* ".to_string() } else { sym };
            let q = if decimals > 0 { format!(".{}", "?".repeat(decimals as usize)) } else { String::new() };
            format!("_({s}#,##0{d}_);_({s}(#,##0{d});_({s}\"-\"{q}_);_(@_)", s = sym, d = dec, q = q)
        }
        "Percentage" => format!("0{}%", dec),
        "Scientific" => format!("0{}E+00", dec),
        _ => "General".into(),
    }
}

/// Number of decimals shown by a format code (first section).
pub fn decimals_of(code: &str) -> u32 {
    let first = first_section(code);
    match first.find('.') {
        Some(i) => first[i + 1..].chars().take_while(|c| *c == '0' || *c == '?' || *c == '#').count() as u32,
        None => 0,
    }
}

fn first_section(code: &str) -> &str {
    let mut in_q = false;
    for (i, c) in code.char_indices() {
        match c {
            '"' => in_q = !in_q,
            ';' if !in_q => return &code[..i],
            _ => {}
        }
    }
    code
}

/// Add (delta > 0) or remove decimals from every numeric section of a code.
pub fn adjust_decimals(code: &str, delta: i32, sample: &Value) -> String {
    if code.eq_ignore_ascii_case("general") || code.is_empty() {
        // Start from the decimals currently displayed by General.
        let shown = match sample {
            Value::Number(n) => {
                let s = engine::value::format_number_general(*n);
                s.split('.').nth(1).map(|d| d.len()).unwrap_or(0) as i32
            }
            _ => 0,
        };
        let d = (shown + delta).max(0) as usize;
        return if d == 0 { "0".into() } else { format!("0.{}", "0".repeat(d)) };
    }
    if code == "@" || engine::numfmt::is_date_format(code) && !code.contains('0') {
        return code.to_string();
    }
    // Process each section separately, skipping quoted text and brackets.
    let mut out = String::new();
    let mut section = String::new();
    let mut in_q = false;
    let mut in_b = false;
    let flush = |section: &mut String, out: &mut String| {
        out.push_str(&adjust_section(section, delta));
        section.clear();
    };
    for c in code.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                section.push(c);
            }
            '[' if !in_q => {
                in_b = true;
                section.push(c);
            }
            ']' if !in_q => {
                in_b = false;
                section.push(c);
            }
            ';' if !in_q && !in_b => {
                flush(&mut section, &mut out);
                out.push(';');
            }
            _ => section.push(c),
        }
    }
    flush(&mut section, &mut out);
    out
}

fn adjust_section(s: &str, delta: i32) -> String {
    let chars: Vec<char> = s.chars().collect();
    // find the last digit placeholder outside quotes
    let mut in_q = false;
    let mut last_digit = None;
    let mut dot = None;
    for (i, c) in chars.iter().enumerate() {
        match c {
            '"' => in_q = !in_q,
            '0' | '#' | '?' if !in_q => last_digit = Some(i),
            '.' if !in_q => dot = Some(i),
            _ => {}
        }
    }
    let Some(last) = last_digit else { return s.to_string() };
    let mut v = chars.clone();
    if delta > 0 {
        let ins: Vec<char> = if dot.is_some() { vec!['0'] } else { vec!['.', '0'] };
        for (k, ch) in ins.into_iter().enumerate() {
            v.insert(last + 1 + k, ch);
        }
    } else if let Some(d) = dot {
        if last > d {
            v.remove(last);
            // remove the dot when no decimals remain
            if last == d + 1 {
                v.remove(d);
            }
        }
    }
    v.into_iter().collect()
}
