//! Formula tokenizer and Pratt parser.

use crate::address::{col_from_letters, MAX_ROWS};
use crate::ast::{BinaryOp, Expr, RefPart, Reference, UnaryOp};
use crate::value::ErrorKind;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Number(f64),
    Str(String),
    Error(ErrorKind),
    Word(String),
    Sheet(String),
    Op(&'static str),
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    Hash,
    /// Whitespace between tokens (needed for the intersection operator).
    Space,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at {})", self.message, self.position)
    }
}

fn err<T>(msg: impl Into<String>, pos: usize) -> Result<T, ParseError> {
    Err(ParseError { message: msg.into(), position: pos })
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out: Vec<(Tok, usize)> = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            out.push((Tok::Space, start));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            // A digit-led word followed by '!' is a sheet name like 2024!A1
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.') {
                j += 1;
            }
            if j < chars.len() && chars[j] == '!' && chars[i..j].iter().any(|c| c.is_alphabetic()) {
                out.push((Tok::Sheet(chars[i..j].iter().collect()), start));
                i = j + 1;
                continue;
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let s: String = chars[start..i].iter().collect();
            let n: f64 = s.parse().map_err(|_| ParseError { message: format!("bad number '{}'", s), position: start })?;
            out.push((Tok::Number(n), start));
            continue;
        }
        if c == '"' {
            i += 1;
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    return err("unterminated string", start);
                }
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        s.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                s.push(chars[i]);
                i += 1;
            }
            out.push((Tok::Str(s), start));
            continue;
        }
        if c == '\'' {
            i += 1;
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    return err("unterminated sheet name", start);
                }
                if chars[i] == '\'' {
                    if i + 1 < chars.len() && chars[i + 1] == '\'' {
                        s.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                s.push(chars[i]);
                i += 1;
            }
            if i < chars.len() && chars[i] == '!' {
                i += 1;
                out.push((Tok::Sheet(s), start));
                continue;
            }
            return err("expected '!' after quoted sheet name", i);
        }
        if c == '#' {
            // error literal?
            let rest: String = chars[i..].iter().take(14).collect::<String>().to_ascii_uppercase();
            let mut matched = None;
            for e in ErrorKind::ALL {
                if rest.starts_with(e.as_str()) {
                    matched = Some(e);
                    break;
                }
            }
            let prev_is_ref = matches!(out.last(), Some((Tok::Word(_), _)) | Some((Tok::Number(_), _)) | Some((Tok::RParen, _)));
            if let Some(e) = matched {
                if !prev_is_ref || e != ErrorKind::NA {
                    i += e.as_str().chars().count();
                    out.push((Tok::Error(e), start));
                    continue;
                }
            }
            i += 1;
            out.push((Tok::Hash, start));
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '\\' || c == '$' {
            while i < chars.len()
                && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '.' | '$' | '?' | '\\'))
            {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            if i < chars.len() && chars[i] == '!' {
                i += 1;
                out.push((Tok::Sheet(s), start));
            } else {
                out.push((Tok::Word(s), start));
            }
            continue;
        }
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        let tok = match two.as_str() {
            "<>" => Some(Tok::Op("<>")),
            "<=" => Some(Tok::Op("<=")),
            ">=" => Some(Tok::Op(">=")),
            _ => None,
        };
        if let Some(t) = tok {
            out.push((t, start));
            i += 2;
            continue;
        }
        let t = match c {
            '+' => Tok::Op("+"),
            '-' => Tok::Op("-"),
            '*' => Tok::Op("*"),
            '/' => Tok::Op("/"),
            '^' => Tok::Op("^"),
            '&' => Tok::Op("&"),
            '=' => Tok::Op("="),
            '<' => Tok::Op("<"),
            '>' => Tok::Op(">"),
            '%' => Tok::Op("%"),
            '@' => Tok::Op("@"),
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            ',' => Tok::Comma,
            ';' => Tok::Semi,
            ':' => Tok::Colon,
            _ => return err(format!("unexpected character '{}'", c), i),
        };
        out.push((t, start));
        i += 1;
    }
    Ok(out)
}

/// Parse a cell reference like `$A$1` into a RefPart.
fn parse_cell_part(s: &str) -> Option<RefPart> {
    let b = s.as_bytes();
    let mut i = 0;
    let col_abs = b.first() == Some(&b'$');
    if col_abs {
        i += 1;
    }
    let ls = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == ls || i - ls > 3 {
        return None;
    }
    let col = col_from_letters(&s[ls..i])?;
    let row_abs = i < b.len() && b[i] == b'$';
    if row_abs {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i != b.len() || ds == i {
        return None;
    }
    let row: u32 = s[ds..].parse().ok()?;
    if row == 0 || row > MAX_ROWS {
        return None;
    }
    Some(RefPart::cell(row - 1, col, row_abs, col_abs))
}

/// Column-only part, e.g. `$B` in `$B:$D`.
fn parse_col_part(s: &str) -> Option<RefPart> {
    let (abs, rest) = match s.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, s),
    };
    if rest.is_empty() || !rest.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let col = col_from_letters(rest)?;
    Some(RefPart { row: None, col: Some(col), row_abs: false, col_abs: abs })
}

/// Row-only part, e.g. `$3` in `$3:$5`.
fn parse_row_part(s: &str) -> Option<RefPart> {
    let (abs, rest) = match s.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, s),
    };
    if rest.is_empty() || !rest.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let row: u32 = rest.parse().ok()?;
    if row == 0 || row > MAX_ROWS {
        return None;
    }
    Some(RefPart { row: Some(row - 1), col: None, row_abs: abs, col_abs: false })
}

pub fn normalize_function_name(name: &str) -> String {
    let up = name.to_ascii_uppercase();
    for p in ["_XLFN._XLWS.", "_XLFN.", "_XLWS.", "_XLUDF."] {
        if let Some(r) = up.strip_prefix(p) {
            return r.to_string();
        }
    }
    up
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }
    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.pos + k).map(|t| &t.0)
    }
    fn here(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.1).unwrap_or(self.end)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|t| t.0.clone());
        self.pos += 1;
        t
    }
    fn skip_space(&mut self) {
        while self.peek() == Some(&Tok::Space) {
            self.pos += 1;
        }
    }
    fn expect(&mut self, t: Tok) -> Result<(), ParseError> {
        self.skip_space();
        if self.peek() == Some(&t) {
            self.pos += 1;
            Ok(())
        } else {
            err(format!("expected {:?}", t), self.here())
        }
    }

    fn infix_bp(&self, allow_union: bool) -> Option<(BinaryOp, u8, u8)> {
        // Look past whitespace for binary operators; whitespace itself between two
        // reference-ish operands is the intersection operator.
        let mut k = 0;
        let mut had_space = false;
        while self.peek_at(k) == Some(&Tok::Space) {
            k += 1;
            had_space = true;
        }
        let t = self.peek_at(k)?;
        let r = match t {
            Tok::Op("=") => (BinaryOp::Eq, 1, 2),
            Tok::Op("<>") => (BinaryOp::Ne, 1, 2),
            Tok::Op("<") => (BinaryOp::Lt, 1, 2),
            Tok::Op("<=") => (BinaryOp::Le, 1, 2),
            Tok::Op(">") => (BinaryOp::Gt, 1, 2),
            Tok::Op(">=") => (BinaryOp::Ge, 1, 2),
            Tok::Op("&") => (BinaryOp::Concat, 3, 4),
            Tok::Op("+") => (BinaryOp::Add, 5, 6),
            Tok::Op("-") => (BinaryOp::Sub, 5, 6),
            Tok::Op("*") => (BinaryOp::Mul, 7, 8),
            Tok::Op("/") => (BinaryOp::Div, 7, 8),
            Tok::Op("^") => (BinaryOp::Pow, 9, 10),
            Tok::Colon => (BinaryOp::Range, 17, 18),
            Tok::Comma if allow_union => (BinaryOp::Union, 13, 14),
            Tok::Word(_) | Tok::Sheet(_) | Tok::LParen | Tok::Number(_) if had_space => (BinaryOp::Intersect, 15, 16),
            _ => return None,
        };
        Some(r)
    }

    fn consume_infix(&mut self) {
        self.skip_space();
        if !matches!(self.peek(), Some(Tok::Word(_)) | Some(Tok::Sheet(_)) | Some(Tok::LParen) | Some(Tok::Number(_))) {
            self.pos += 1;
        }
    }

    fn parse_expr(&mut self, min_bp: u8, allow_union: bool) -> Result<Expr, ParseError> {
        self.skip_space();
        let mut lhs = match self.peek() {
            Some(Tok::Op("-")) => {
                self.pos += 1;
                let e = self.parse_expr(11, allow_union)?;
                Expr::Unary(UnaryOp::Neg, Box::new(e))
            }
            Some(Tok::Op("+")) => {
                self.pos += 1;
                let e = self.parse_expr(11, allow_union)?;
                Expr::Unary(UnaryOp::Plus, Box::new(e))
            }
            Some(Tok::Op("@")) => {
                self.pos += 1;
                let e = self.parse_expr(11, allow_union)?;
                Expr::Unary(UnaryOp::Implicit, Box::new(e))
            }
            _ => self.parse_primary()?,
        };
        loop {
            // postfix %
            let mut k = 0;
            while self.peek_at(k) == Some(&Tok::Space) {
                k += 1;
            }
            if self.peek_at(k) == Some(&Tok::Op("%")) {
                if 12 < min_bp {
                    break;
                }
                self.pos += k + 1;
                lhs = Expr::Unary(UnaryOp::Percent, Box::new(lhs));
                continue;
            }
            let Some((op, lbp, rbp)) = self.infix_bp(allow_union) else { break };
            if op == BinaryOp::Intersect && !is_refish(&lhs) {
                break;
            }
            if lbp < min_bp {
                break;
            }
            self.consume_infix();
            let rhs = self.parse_expr(rbp, allow_union)?;
            lhs = if op == BinaryOp::Range { combine_range(lhs, rhs) } else { Expr::Binary(op, Box::new(lhs), Box::new(rhs)) };
        }
        Ok(lhs)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        // after '('
        let mut args = Vec::new();
        self.skip_space();
        if self.peek() == Some(&Tok::RParen) {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            self.skip_space();
            if matches!(self.peek(), Some(Tok::Comma) | Some(Tok::RParen)) {
                args.push(Expr::Missing);
            } else {
                args.push(self.parse_expr(0, false)?);
            }
            self.skip_space();
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => break,
                _ => return err("expected ',' or ')' in argument list", self.here()),
            }
        }
        Ok(args)
    }

    fn parse_array(&mut self) -> Result<Expr, ParseError> {
        let mut rows = vec![vec![]];
        loop {
            self.skip_space();
            let neg = if self.peek() == Some(&Tok::Op("-")) {
                self.pos += 1;
                true
            } else {
                false
            };
            let e = match self.next() {
                Some(Tok::Number(n)) => Expr::Number(if neg { -n } else { n }),
                Some(Tok::Str(s)) if !neg => Expr::Text(s),
                Some(Tok::Error(e)) if !neg => Expr::Error(e),
                Some(Tok::Word(w)) if !neg && w.eq_ignore_ascii_case("TRUE") => Expr::Bool(true),
                Some(Tok::Word(w)) if !neg && w.eq_ignore_ascii_case("FALSE") => Expr::Bool(false),
                _ => return err("invalid array constant", self.here()),
            };
            rows.last_mut().unwrap().push(e);
            self.skip_space();
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::Semi) => rows.push(vec![]),
                Some(Tok::RBrace) => break,
                _ => return err("expected ',', ';' or '}' in array", self.here()),
            }
        }
        Ok(Expr::Array(rows))
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_space();
        let pos = self.here();
        let tok = match self.next() {
            Some(t) => t,
            None => return err("unexpected end of formula", pos),
        };
        let e = match tok {
            Tok::Number(n) => Expr::Number(n),
            Tok::Str(s) => Expr::Text(s),
            Tok::Error(e) => Expr::Error(e),
            Tok::LParen => {
                let inner = self.parse_expr(0, true)?;
                self.expect(Tok::RParen)?;
                Expr::Paren(Box::new(inner))
            }
            Tok::LBrace => self.parse_array()?,
            Tok::Sheet(sheet) => {
                let t = self.next();
                let mut r = match t {
                    Some(Tok::Word(w)) => {
                        if let Some(p) = parse_cell_part(&w).or_else(|| parse_col_part(&w)) {
                            Reference { sheet: Some(sheet), start: p, end: None }
                        } else {
                            // Sheet-scoped defined name: treat as name
                            return Ok(Expr::Name(format!("{}!{}", sheet, w)));
                        }
                    }
                    Some(Tok::Number(n)) => {
                        let p = parse_row_part(&format!("{}", n as u64))
                            .ok_or(ParseError { message: "bad row".into(), position: pos })?;
                        Reference { sheet: Some(sheet), start: p, end: None }
                    }
                    Some(Tok::Error(ErrorKind::Ref)) => return Ok(Expr::Error(ErrorKind::Ref)),
                    _ => return err("expected reference after sheet name", pos),
                };
                // Range on same sheet: Sheet1!A1:B2
                if self.peek() == Some(&Tok::Colon) {
                    let save = self.pos;
                    self.pos += 1;
                    let part = match self.next() {
                        Some(Tok::Word(w)) => parse_cell_part(&w).or_else(|| parse_col_part(&w)),
                        Some(Tok::Number(n)) if n.fract() == 0.0 => parse_row_part(&format!("{}", n as u64)),
                        _ => None,
                    };
                    match part {
                        Some(p) => r.end = Some(p),
                        None => self.pos = save,
                    }
                }
                if r.end.is_none() && (r.start.row.is_none() || r.start.col.is_none()) {
                    return err("incomplete reference", pos);
                }
                Expr::Ref(r)
            }
            Tok::Word(w) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.pos += 1;
                    let args = self.parse_args()?;
                    Expr::Call(normalize_function_name(&w), args)
                } else if w.eq_ignore_ascii_case("TRUE") {
                    Expr::Bool(true)
                } else if w.eq_ignore_ascii_case("FALSE") {
                    Expr::Bool(false)
                } else if let Some(p) = parse_cell_part(&w) {
                    Expr::Ref(Reference { sheet: None, start: p, end: None })
                } else {
                    Expr::Name(w)
                }
            }
            Tok::Op("$") => return err("unexpected '$'", pos),
            other => return err(format!("unexpected token {:?}", other), pos),
        };
        // Postfix: spill ref `A1#` and call-of-result `LAMBDA(x,x)(1)`
        let mut e = e;
        loop {
            match (self.peek(), &e) {
                (Some(Tok::Hash), Expr::Ref(_)) => {
                    self.pos += 1;
                    if let Expr::Ref(r) = e {
                        e = Expr::SpillRef(r);
                    }
                }
                (Some(Tok::LParen), Expr::Call(..)) | (Some(Tok::LParen), Expr::Paren(_)) => {
                    self.pos += 1;
                    let args = self.parse_args()?;
                    e = Expr::CallExpr(Box::new(e), args);
                }
                _ => break,
            }
        }
        Ok(e)
    }
}

fn is_refish(e: &Expr) -> bool {
    match e {
        Expr::Ref(_) | Expr::SpillRef(_) | Expr::Name(_) => true,
        Expr::Call(n, _) => matches!(n.as_str(), "OFFSET" | "INDIRECT" | "INDEX" | "CHOOSE" | "IF"),
        Expr::Paren(e) => is_refish(e),
        Expr::Binary(op, _, _) => matches!(op, BinaryOp::Range | BinaryOp::Intersect | BinaryOp::Union),
        _ => false,
    }
}

fn to_part(e: &Expr) -> Option<(Option<String>, RefPart)> {
    match e {
        Expr::Ref(r) if r.end.is_none() => Some((r.sheet.clone(), r.start.clone())),
        Expr::Name(n) => parse_col_part(n).map(|p| (None, p)),
        Expr::Number(n) if n.fract() == 0.0 && *n >= 1.0 => parse_row_part(&format!("{}", *n as u64)).map(|p| (None, p)),
        _ => None,
    }
}

fn combine_range(lhs: Expr, rhs: Expr) -> Expr {
    if let (Some((s1, p1)), Some((s2, p2))) = (to_part(&lhs), to_part(&rhs)) {
        let both_cells = p1.row.is_some() && p1.col.is_some() && p2.row.is_some() && p2.col.is_some();
        let both_cols = p1.row.is_none() && p2.row.is_none() && p1.col.is_some() && p2.col.is_some();
        let both_rows = p1.col.is_none() && p2.col.is_none() && p1.row.is_some() && p2.row.is_some();
        let same_sheet = s2.is_none() || s1.as_deref().map(|a| a.eq_ignore_ascii_case(s2.as_deref().unwrap())).unwrap_or(false);
        if (both_cells || both_cols || both_rows) && same_sheet {
            return Expr::Ref(Reference { sheet: s1, start: p1, end: Some(p2) });
        }
    }
    Expr::Binary(BinaryOp::Range, Box::new(lhs), Box::new(rhs))
}

/// Parse formula text (without the leading `=`).
pub fn parse_formula(src: &str) -> Result<Expr, ParseError> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0, end: src.len() };
    let e = p.parse_expr(0, false)?;
    p.skip_space();
    if p.pos < p.toks.len() {
        return err("unexpected text after end of formula", p.here());
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(s: &str) -> String {
        parse_formula(s).unwrap().to_formula()
    }

    #[test]
    fn roundtrip() {
        for s in [
            "1+2*3",
            "SUM(A1:B2)",
            "$A$1+Sheet2!B3",
            "'My Sheet'!A1:B5",
            "IF(A1>0,\"yes\",\"no\")",
            "-2^2",
            "A:A",
            "$1:$3",
            "{1,2;3,4}",
            "A1#",
            "SUM((A1:A3,C1:C3))",
            "LAMBDA(x,x+1)(3)",
            "IF(A1,,2)",
            "50%",
            "A1:A3 A2:B2",
            "#N/A",
            "Sheet1!A:C",
        ] {
            assert_eq!(rt(s), s, "roundtrip of {}", s);
        }
        assert_eq!(rt("_xlfn.CONCAT(A1)"), "CONCAT(A1)");
        assert_eq!(rt("sum( a1 , 2 )"), "SUM(A1,2)");
    }

    #[test]
    fn precedence() {
        let e = parse_formula("1+2*3").unwrap();
        match e {
            Expr::Binary(BinaryOp::Add, _, r) => assert!(matches!(*r, Expr::Binary(BinaryOp::Mul, _, _))),
            _ => panic!(),
        }
        // -2^2: negation binds tighter
        match parse_formula("-2^2").unwrap() {
            Expr::Binary(BinaryOp::Pow, l, _) => assert!(matches!(*l, Expr::Unary(UnaryOp::Neg, _))),
            _ => panic!(),
        }
    }

    #[test]
    fn errors() {
        assert!(parse_formula("1+").is_err());
        assert!(parse_formula("SUM(1,2").is_err());
        assert!(parse_formula("\"abc").is_err());
    }

    #[test]
    fn shift() {
        let mut e = parse_formula("A1+$B$2+C$3+$D4").unwrap();
        e.shift_relative(1, 1);
        assert_eq!(e.to_formula(), "B2+$B$2+D$3+$D5");
        let mut e = parse_formula("A1").unwrap();
        e.shift_relative(-1, 0);
        assert_eq!(e.to_formula(), "#REF!");
    }

    #[test]
    fn insert_delete() {
        let mut e = parse_formula("SUM(A1:A10)+B5").unwrap();
        e.adjust_for_insert_delete("Sheet1", "Sheet1", true, 2, 3);
        assert_eq!(e.to_formula(), "SUM(A1:A13)+B8");
        let mut e = parse_formula("SUM(A1:A10)+B5").unwrap();
        e.adjust_for_insert_delete("Sheet1", "Sheet1", true, 4, -1);
        assert_eq!(e.to_formula(), "SUM(A1:A9)+#REF!");
        let mut e = parse_formula("Other!A5").unwrap();
        e.adjust_for_insert_delete("Sheet1", "Sheet1", true, 0, 1);
        assert_eq!(e.to_formula(), "Other!A5");
    }
}
