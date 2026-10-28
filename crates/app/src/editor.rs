//! Helpers for the formula editor: reference tokens (for highlighting and
//! point mode), F4 absolute-reference cycling, autocomplete context.
//! All indices are character indices.

use engine::address::{col_from_letters, CellPos, Rect, MAX_COLS, MAX_ROWS};

/// A reference found in formula text.
#[derive(Debug, Clone, PartialEq)]
pub struct RefToken {
    pub start: usize,
    pub end: usize,
    pub sheet: Option<String>,
    pub rect: Rect,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '$'
}

/// Parse a single reference part like `$A$1`, `A`, `1`. Returns (row, col).
fn parse_part(s: &str) -> Option<(Option<u32>, Option<u32>)> {
    let t = s.replace('$', "");
    if t.is_empty() {
        return None;
    }
    let split = t.find(|c: char| c.is_ascii_digit()).unwrap_or(t.len());
    let (letters, digits) = t.split_at(split);
    if !letters.chars().all(|c| c.is_ascii_alphabetic()) || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let col = if letters.is_empty() { None } else { Some(col_from_letters(letters)?) };
    let row = if digits.is_empty() {
        None
    } else {
        let r: u32 = digits.parse().ok()?;
        if r == 0 || r > MAX_ROWS {
            return None;
        }
        Some(r - 1)
    };
    if col.is_none() && row.is_none() {
        return None;
    }
    Some((row, col))
}

/// Find cell/range references in formula text (outside string literals).
pub fn find_refs(text: &str) -> Vec<RefToken> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    if chars.first() != Some(&'=') {
        return out;
    }
    let mut i = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            i += 1;
            continue;
        }
        let start = i;
        let mut sheet: Option<String> = None;
        let mut j = i;
        if c == '\'' {
            // quoted sheet name
            let mut k = i + 1;
            let mut name = String::new();
            while k < chars.len() {
                if chars[k] == '\'' {
                    if k + 1 < chars.len() && chars[k + 1] == '\'' {
                        name.push('\'');
                        k += 2;
                        continue;
                    }
                    break;
                }
                name.push(chars[k]);
                k += 1;
            }
            if k + 1 < chars.len() && chars[k + 1] == '!' {
                sheet = Some(name);
                j = k + 2;
            } else {
                i = k + 1;
                continue;
            }
        } else if is_ident(c) {
            // maybe Sheet!
            let mut k = i;
            while k < chars.len() && is_ident(chars[k]) {
                k += 1;
            }
            if k < chars.len() && chars[k] == '!' {
                sheet = Some(chars[i..k].iter().collect());
                j = k + 1;
            }
        } else {
            i += 1;
            continue;
        }
        // identifier must not be preceded by an identifier char
        if start > 0 && is_ident(chars[start - 1]) {
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            continue;
        }
        let mut k = j;
        while k < chars.len() && is_ident(chars[k]) {
            k += 1;
        }
        let a: String = chars[j..k].iter().collect();
        let mut end = k;
        let mut b: Option<String> = None;
        if k < chars.len() && chars[k] == ':' {
            let mut m = k + 1;
            while m < chars.len() && is_ident(chars[m]) {
                m += 1;
            }
            let s: String = chars[k + 1..m].iter().collect();
            if parse_part(&s).is_some() {
                b = Some(s);
                end = m;
            }
        }
        // a function call is not a reference
        let followed_by_paren = end < chars.len() && chars[end] == '(';
        let pa = parse_part(&a);
        let rect = match (pa, b.as_deref().and_then(parse_part)) {
            (Some((Some(r1), Some(c1))), None) => Some(Rect::single(CellPos::new(r1, c1))),
            (Some((r1, c1)), Some((r2, c2))) => {
                // full cols / rows / areas
                match (r1, c1, r2, c2) {
                    (Some(r1), Some(c1), Some(r2), Some(c2)) => Some(Rect::new(CellPos::new(r1, c1), CellPos::new(r2, c2))),
                    (None, Some(c1), None, Some(c2)) => Some(Rect { top: 0, bottom: MAX_ROWS - 1, left: c1.min(c2), right: c1.max(c2) }),
                    (Some(r1), None, Some(r2), None) => Some(Rect { top: r1.min(r2), bottom: r1.max(r2), left: 0, right: MAX_COLS - 1 }),
                    _ => None,
                }
            }
            _ => None,
        };
        match rect {
            Some(rect) if !followed_by_paren && (b.is_some() || pa.map(|p| p.0.is_some() && p.1.is_some()).unwrap_or(false)) => {
                out.push(RefToken { start, end, sheet, rect });
                i = end;
            }
            _ => {
                i = k.max(i + 1);
            }
        }
    }
    out
}

/// Can a reference be inserted at `cursor` (point mode)?
pub fn can_point(text: &str, cursor: usize) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&'=') || cursor > chars.len() {
        return false;
    }
    // Inside a string literal?
    let quotes = chars[..cursor].iter().filter(|c| **c == '"').count();
    if quotes % 2 == 1 {
        return false;
    }
    if cursor < chars.len() && is_ident(chars[cursor]) {
        return false;
    }
    let mut k = cursor;
    while k > 0 && chars[k - 1] == ' ' {
        k -= 1;
    }
    if k == 0 {
        return false;
    }
    matches!(chars[k - 1], '=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ';' | '{')
}

/// The reference token that touches `cursor`, if any.
pub fn ref_at(text: &str, cursor: usize) -> Option<RefToken> {
    find_refs(text).into_iter().find(|r| cursor >= r.start && cursor <= r.end)
}

/// F4: cycle A1 -> $A$1 -> A$1 -> $A1 -> A1 for the reference at the cursor.
/// Returns the new text and new cursor position.
pub fn cycle_absolute(text: &str, cursor: usize) -> Option<(String, usize)> {
    let r = ref_at(text, cursor)?;
    let chars: Vec<char> = text.chars().collect();
    let tok: String = chars[r.start..r.end].iter().collect();
    let (prefix, body) = match tok.rfind('!') {
        Some(i) => (tok[..=i].to_string(), tok[i + 1..].to_string()),
        None => (String::new(), tok.clone()),
    };
    let parts: Vec<&str> = body.split(':').collect();
    // Decide the mode from the first part.
    let mode = |p: &str| -> u8 {
        let col_abs = p.starts_with('$');
        let row_abs = p.trim_start_matches('$').contains('$');
        match (col_abs, row_abs) {
            (false, false) => 0,
            (true, true) => 1,
            (false, true) => 2,
            (true, false) => 3,
        }
    };
    let next = (mode(parts[0]) + 1) % 4;
    let apply = |p: &str| -> String {
        let plain = p.replace('$', "");
        let split = plain.find(|c: char| c.is_ascii_digit()).unwrap_or(plain.len());
        let (l, d) = plain.split_at(split);
        let (ca, ra) = match next {
            0 => (false, false),
            1 => (true, true),
            2 => (false, true),
            _ => (true, false),
        };
        let mut s = String::new();
        if !l.is_empty() {
            if ca {
                s.push('$');
            }
            s.push_str(l);
        }
        if !d.is_empty() {
            if ra {
                s.push('$');
            }
            s.push_str(d);
        }
        s
    };
    let new_body: Vec<String> = parts.iter().map(|p| apply(p)).collect();
    let new_tok = format!("{}{}", prefix, new_body.join(":"));
    let mut out: String = chars[..r.start].iter().collect();
    out.push_str(&new_tok);
    out.extend(chars[r.end..].iter());
    let new_cursor = r.start + new_tok.chars().count();
    Some((out, new_cursor))
}

/// Identifier being typed right before the cursor in a formula (for autocomplete).
/// Returns (start index, text).
pub fn ident_before_cursor(text: &str, cursor: usize) -> Option<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&'=') || cursor > chars.len() {
        return None;
    }
    let quotes = chars[..cursor].iter().filter(|c| **c == '"').count();
    if quotes % 2 == 1 {
        return None;
    }
    if cursor < chars.len() && (chars[cursor].is_alphanumeric() || chars[cursor] == '(') {
        return None;
    }
    let mut k = cursor;
    while k > 0 && (chars[k - 1].is_alphanumeric() || chars[k - 1] == '.' || chars[k - 1] == '_') {
        k -= 1;
    }
    if k == cursor || k == 0 {
        return None;
    }
    if !chars[k].is_alphabetic() {
        return None;
    }
    let prev = chars[k - 1];
    if prev == '$' || prev == '!' || prev == '\'' || prev == ':' {
        return None;
    }
    Some((k, chars[k..cursor].iter().collect()))
}

/// The function call the cursor is inside: (name, zero-based argument index).
pub fn current_function(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&'=') {
        return None;
    }
    let cursor = cursor.min(chars.len());
    let mut depth = 0i32;
    let mut arg = 0usize;
    let mut in_str = false;
    // quote parity before cursor
    let quotes = chars[..cursor].iter().filter(|c| **c == '"').count();
    if quotes % 2 == 1 {
        in_str = true;
    }
    let mut k = cursor;
    while k > 0 {
        k -= 1;
        let c = chars[k];
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        match c {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    let mut s = k;
                    while s > 0 && (chars[s - 1].is_alphanumeric() || chars[s - 1] == '.' || chars[s - 1] == '_') {
                        s -= 1;
                    }
                    if s == k {
                        return None;
                    }
                    let name: String = chars[s..k].iter().collect();
                    return Some((name.to_uppercase(), arg));
                }
                depth -= 1;
            }
            ',' if depth == 0 => arg += 1,
            _ => {}
        }
    }
    None
}

/// Add missing closing parentheses (Excel does this on Enter).
pub fn close_parens(text: &str) -> String {
    if !text.starts_with('=') {
        return text.to_string();
    }
    let mut depth = 0i32;
    let mut in_str = false;
    for c in text.chars() {
        match c {
            '"' => in_str = !in_str,
            '(' if !in_str => depth += 1,
            ')' if !in_str => depth -= 1,
            _ => {}
        }
    }
    let mut s = text.to_string();
    if in_str {
        s.push('"');
    }
    for _ in 0..depth.max(0) {
        s.push(')');
    }
    s
}

/// Format a reference for insertion while pointing.
pub fn ref_text(edit_sheet: usize, point_sheet: usize, sheet_name: &str, rect: Rect) -> String {
    let addr = rect.to_a1();
    if edit_sheet == point_sheet {
        addr
    } else {
        format!("{}!{}", engine::address::quote_sheet_name(sheet_name), addr)
    }
}

/// Parse a Name Box / Go To reference such as `B5`, `A1:C3`, `Sheet2!A1`, `C:C`, `3:5`.
pub fn parse_goto(text: &str) -> Option<(Option<String>, Rect)> {
    let t = text.trim();
    let (sheet, body) = match t.rfind('!') {
        Some(i) => (Some(t[..i].trim_matches('\'').to_string()), &t[i + 1..]),
        None => (None, t),
    };
    let body = body.replace('$', "");
    if let Some(r) = Rect::parse(&body) {
        return Some((sheet, r));
    }
    let (a, b) = body.split_once(':').unwrap_or((&body, &body));
    let pa = parse_part(a)?;
    let pb = parse_part(b)?;
    match (pa, pb) {
        ((None, Some(c1)), (None, Some(c2))) => Some((sheet, Rect { top: 0, bottom: MAX_ROWS - 1, left: c1.min(c2), right: c1.max(c2) })),
        ((Some(r1), None), (Some(r2), None)) => Some((sheet, Rect { top: r1.min(r2), bottom: r1.max(r2), left: 0, right: MAX_COLS - 1 })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs() {
        let r = find_refs("=SUM(A1:B2)+Sheet2!C3*'My S'!$D$4+LOG10(2)+\"A1\"");
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].rect, Rect::parse("A1:B2").unwrap());
        assert_eq!(r[1].sheet.as_deref(), Some("Sheet2"));
        assert_eq!(r[2].sheet.as_deref(), Some("My S"));
        let r = find_refs("=A:A");
        assert_eq!(r[0].rect.bottom, MAX_ROWS - 1);
    }

    #[test]
    fn f4() {
        let (t, c) = cycle_absolute("=A1+B2", 2).unwrap();
        assert_eq!(t, "=$A$1+B2");
        assert_eq!(c, 5);
        let (t, _) = cycle_absolute(&t, 3).unwrap();
        assert_eq!(t, "=A$1+B2");
        let (t, _) = cycle_absolute(&t, 3).unwrap();
        assert_eq!(t, "=$A1+B2");
        let (t, _) = cycle_absolute(&t, 3).unwrap();
        assert_eq!(t, "=A1+B2");
        let (t, _) = cycle_absolute("=SUM(A1:B2)", 7).unwrap();
        assert_eq!(t, "=SUM($A$1:$B$2)");
    }

    #[test]
    fn contexts() {
        assert!(can_point("=", 1));
        assert!(can_point("=SUM(", 5));
        assert!(!can_point("=A", 2));
        assert!(!can_point("abc", 3));
        assert_eq!(ident_before_cursor("=SU", 3), Some((1, "SU".into())));
        assert_eq!(ident_before_cursor("=A1+VLO", 7), Some((4, "VLO".into())));
        assert_eq!(current_function("=SUM(1,IF(A1,", 13), Some(("IF".into(), 1)));
        assert_eq!(close_parens("=SUM(IF(1,2"), "=SUM(IF(1,2))");
        assert_eq!(parse_goto("Sheet2!B3").unwrap().1, Rect::parse("B3").unwrap());
        assert_eq!(parse_goto("C:D").unwrap().1.left, 2);
    }
}
