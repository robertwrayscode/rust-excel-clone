//! Cell values and Excel-style type coercion.

use std::cmp::Ordering;
use std::fmt;

/// Excel error values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    Null,
    Div0,
    Value,
    Ref,
    Name,
    Num,
    NA,
    GettingData,
    Spill,
    Calc,
}

impl ErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::Null => "#NULL!",
            ErrorKind::Div0 => "#DIV/0!",
            ErrorKind::Value => "#VALUE!",
            ErrorKind::Ref => "#REF!",
            ErrorKind::Name => "#NAME?",
            ErrorKind::Num => "#NUM!",
            ErrorKind::NA => "#N/A",
            ErrorKind::GettingData => "#GETTING_DATA",
            ErrorKind::Spill => "#SPILL!",
            ErrorKind::Calc => "#CALC!",
        }
    }
    pub fn parse(s: &str) -> Option<ErrorKind> {
        Some(match s.to_ascii_uppercase().as_str() {
            "#NULL!" => ErrorKind::Null,
            "#DIV/0!" => ErrorKind::Div0,
            "#VALUE!" => ErrorKind::Value,
            "#REF!" => ErrorKind::Ref,
            "#NAME?" => ErrorKind::Name,
            "#NUM!" => ErrorKind::Num,
            "#N/A" => ErrorKind::NA,
            "#GETTING_DATA" => ErrorKind::GettingData,
            "#SPILL!" => ErrorKind::Spill,
            "#CALC!" => ErrorKind::Calc,
            _ => return None,
        })
    }
    /// Code used by ERROR.TYPE.
    pub fn code(&self) -> i32 {
        match self {
            ErrorKind::Null => 1,
            ErrorKind::Div0 => 2,
            ErrorKind::Value => 3,
            ErrorKind::Ref => 4,
            ErrorKind::Name => 5,
            ErrorKind::Num => 6,
            ErrorKind::NA => 7,
            ErrorKind::GettingData => 8,
            ErrorKind::Spill => 9,
            ErrorKind::Calc => 14,
        }
    }
    pub const ALL: [ErrorKind; 10] = [
        ErrorKind::Null,
        ErrorKind::Div0,
        ErrorKind::Value,
        ErrorKind::Ref,
        ErrorKind::Name,
        ErrorKind::Num,
        ErrorKind::NA,
        ErrorKind::GettingData,
        ErrorKind::Spill,
        ErrorKind::Calc,
    ];
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A 2D array of values, stored row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Value>,
}

impl Array {
    pub fn new(rows: usize, cols: usize, data: Vec<Value>) -> Array {
        debug_assert_eq!(rows * cols, data.len());
        Array { rows, cols, data }
    }
    pub fn filled(rows: usize, cols: usize, v: Value) -> Array {
        Array { rows, cols, data: vec![v; rows * cols] }
    }
    pub fn from_rows(rows: Vec<Vec<Value>>) -> Array {
        let r = rows.len();
        let c = rows.iter().map(|x| x.len()).max().unwrap_or(0);
        let mut data = Vec::with_capacity(r * c);
        for mut row in rows {
            row.resize(c, Value::Error(ErrorKind::NA));
            data.extend(row);
        }
        Array { rows: r, cols: c, data }
    }
    pub fn column(values: Vec<Value>) -> Array {
        Array { rows: values.len(), cols: 1, data: values }
    }
    pub fn row(values: Vec<Value>) -> Array {
        Array { rows: 1, cols: values.len(), data: values }
    }
    pub fn get(&self, r: usize, c: usize) -> &Value {
        &self.data[r * self.cols + c]
    }
    pub fn get_mut(&mut self, r: usize, c: usize) -> &mut Value {
        &mut self.data[r * self.cols + c]
    }
    /// Get with Excel broadcasting rules: a 1-wide/1-tall array repeats.
    pub fn get_broadcast(&self, r: usize, c: usize) -> Value {
        let rr = if self.rows == 1 { 0 } else { r };
        let cc = if self.cols == 1 { 0 } else { c };
        if rr < self.rows && cc < self.cols {
            self.get(rr, cc).clone()
        } else {
            Value::Error(ErrorKind::NA)
        }
    }
    pub fn row_values(&self, r: usize) -> &[Value] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }
    pub fn transpose(&self) -> Array {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.get(r, c).clone());
            }
        }
        Array { rows: self.cols, cols: self.rows, data }
    }
}

/// A computed or literal value.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Empty,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(ErrorKind),
    Array(Box<Array>),
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::number(n)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Number(n as f64)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Text(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}
impl From<ErrorKind> for Value {
    fn from(e: ErrorKind) -> Self {
        Value::Error(e)
    }
}
impl From<Array> for Value {
    fn from(a: Array) -> Self {
        Value::Array(Box::new(a))
    }
}

impl Value {
    /// Make a number, turning NaN/inf into #NUM!.
    pub fn number(n: f64) -> Value {
        if n.is_finite() {
            Value::Number(if n == 0.0 { 0.0 } else { n })
        } else {
            Value::Error(ErrorKind::Num)
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Empty)
    }
    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }
    pub fn as_error(&self) -> Option<ErrorKind> {
        match self {
            Value::Error(e) => Some(*e),
            _ => None,
        }
    }

    /// Reduce an array to its top-left element (implicit intersection fallback).
    pub fn scalar(self) -> Value {
        match self {
            Value::Array(a) => {
                if a.data.is_empty() {
                    Value::Error(ErrorKind::Calc)
                } else {
                    a.data.into_iter().next().unwrap()
                }
            }
            v => v,
        }
    }

    /// Coerce to a number following Excel rules for formula operands.
    pub fn to_number(&self) -> Result<f64, ErrorKind> {
        match self {
            Value::Empty => Ok(0.0),
            Value::Number(n) => Ok(*n),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Text(s) => parse_number_text(s).ok_or(ErrorKind::Value),
            Value::Error(e) => Err(*e),
            Value::Array(a) => a.data.first().map(|v| v.to_number()).unwrap_or(Err(ErrorKind::Value)),
        }
    }

    /// Coerce to text following Excel rules.
    pub fn to_text(&self) -> Result<String, ErrorKind> {
        match self {
            Value::Empty => Ok(String::new()),
            Value::Number(n) => Ok(format_number_general(*n)),
            Value::Bool(b) => Ok(if *b { "TRUE".into() } else { "FALSE".into() }),
            Value::Text(s) => Ok(s.clone()),
            Value::Error(e) => Err(*e),
            Value::Array(a) => a.data.first().map(|v| v.to_text()).unwrap_or(Ok(String::new())),
        }
    }

    /// Coerce to boolean following Excel rules.
    pub fn to_bool(&self) -> Result<bool, ErrorKind> {
        match self {
            Value::Empty => Ok(false),
            Value::Number(n) => Ok(*n != 0.0),
            Value::Bool(b) => Ok(*b),
            Value::Text(s) => match s.to_ascii_uppercase().as_str() {
                "TRUE" => Ok(true),
                "FALSE" => Ok(false),
                _ => Err(ErrorKind::Value),
            },
            Value::Error(e) => Err(*e),
            Value::Array(a) => a.data.first().map(|v| v.to_bool()).unwrap_or(Err(ErrorKind::Value)),
        }
    }

    /// Text shown in a cell with General format.
    pub fn display(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Number(n) => format_number_general(*n),
            Value::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
            Value::Text(s) => s.clone(),
            Value::Error(e) => e.as_str().into(),
            Value::Array(a) => a.data.first().map(|v| v.display()).unwrap_or_default(),
        }
    }

    pub fn type_rank(&self) -> u8 {
        // Excel sort order: numbers < text < logical < errors < blanks
        match self {
            Value::Number(_) => 0,
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
            Value::Error(_) => 3,
            Value::Empty => 4,
            Value::Array(_) => 5,
        }
    }

    /// Compare two values as Excel comparison operators do (case-insensitive text,
    /// numbers < text < booleans, blank equals 0/""/FALSE).
    pub fn compare(&self, other: &Value) -> Ordering {
        let a = self.blank_as(other);
        let b = other.blank_as(self);
        match (&a, &b) {
            (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (Value::Text(x), Value::Text(y)) => compare_text(x, y),
            (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
            _ => a.type_rank().cmp(&b.type_rank()),
        }
    }

    fn blank_as(&self, other: &Value) -> Value {
        match (self, other) {
            (Value::Empty, Value::Number(_)) => Value::Number(0.0),
            (Value::Empty, Value::Text(_)) => Value::Text(String::new()),
            (Value::Empty, Value::Bool(_)) => Value::Bool(false),
            (Value::Empty, Value::Empty) => Value::Number(0.0),
            _ => self.clone(),
        }
    }

    /// Value equality as the `=` operator sees it.
    pub fn excel_eq(&self, other: &Value) -> bool {
        self.compare(other) == Ordering::Equal
    }
}

/// Case-insensitive text comparison (Excel style).
pub fn compare_text(a: &str, b: &str) -> Ordering {
    let la = a.to_lowercase();
    let lb = b.to_lowercase();
    la.cmp(&lb)
}

/// Parse text as Excel would when coercing to number: plain numbers, percentages,
/// currency, thousands separators, and dates/times.
pub fn parse_number_text(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(n) = parse_plain_number(t) {
        return Some(n);
    }
    // Negative in parentheses: (123)
    if t.starts_with('(') && t.ends_with(')') {
        return parse_number_text(&t[1..t.len() - 1]).map(|n| -n);
    }
    if let Some(rest) = t.strip_suffix('%') {
        return parse_number_text(rest).map(|n| n / 100.0);
    }
    let (neg, body) = if let Some(r) = t.strip_prefix('-') { (true, r.trim()) } else { (false, t) };
    for sym in ["$", "€", "£", "¥"] {
        if let Some(r) = body.strip_prefix(sym) {
            return parse_number_text(r).map(|n| if neg { -n } else { n });
        }
    }
    if body.contains(',') {
        let cleaned: String = body.chars().filter(|c| *c != ',').collect();
        if valid_thousands(body) {
            if let Some(n) = parse_plain_number(&cleaned) {
                return Some(if neg { -n } else { n });
            }
        }
    }
    crate::datetime::parse_datetime_text(t)
}

fn valid_thousands(s: &str) -> bool {
    let int_part = s.split('.').next().unwrap_or("");
    let groups: Vec<&str> = int_part.split(',').collect();
    if groups.len() < 2 {
        return false;
    }
    if groups[0].is_empty() || groups[0].len() > 3 {
        return false;
    }
    groups[1..].iter().all(|g| g.len() == 3 && g.chars().all(|c| c.is_ascii_digit()))
}

fn parse_plain_number(t: &str) -> Option<f64> {
    let b = t.as_bytes();
    // Reject things Rust accepts but Excel doesn't (inf, nan, hex, etc.)
    if !b.iter().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'+' | b'e' | b'E')) {
        return None;
    }
    if !b.iter().any(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<f64>().ok()
}

/// Format a number the way Excel's General format does (up to ~11 significant
/// characters in a cell; we use 15 significant digits and trim).
pub fn format_number_general(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    if !n.is_finite() {
        return "#NUM!".into();
    }
    let abs = n.abs();
    if (1e-9..1e15).contains(&abs) {
        // Round to 15 significant digits
        let digits = 15 - (abs.log10().floor() as i32 + 1);
        let digits = digits.clamp(0, 15) as usize;
        let s = format!("{:.*}", digits, n);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        if s == "-0" {
            "0".into()
        } else {
            s
        }
    } else {
        let s = format!("{:.14E}", n);
        // e.g. 1.23450000000000E20 -> 1.2345E+20
        let (mant, exp) = s.split_once('E').unwrap();
        let mant = if mant.contains('.') { mant.trim_end_matches('0').trim_end_matches('.') } else { mant };
        let exp: i32 = exp.parse().unwrap();
        format!("{}E{}{:02}", mant, if exp < 0 { "-" } else { "+" }, exp.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn general_format() {
        assert_eq!(format_number_general(1.0), "1");
        assert_eq!(format_number_general(0.1 + 0.2), "0.3");
        assert_eq!(format_number_general(-2.5), "-2.5");
        assert_eq!(format_number_general(1e20), "1E+20");
        assert_eq!(format_number_general(123456789.0), "123456789");
        assert_eq!(format_number_general(1.5e-12), "1.5E-12");
    }
    #[test]
    fn coercion() {
        assert_eq!(Value::from("1,234.5").to_number(), Ok(1234.5));
        assert_eq!(Value::from("50%").to_number(), Ok(0.5));
        assert_eq!(Value::from("$10").to_number(), Ok(10.0));
        assert_eq!(Value::from("abc").to_number(), Err(ErrorKind::Value));
        assert_eq!(Value::from("inf").to_number(), Err(ErrorKind::Value));
    }
    #[test]
    fn compare() {
        assert!(Value::from("abc").excel_eq(&Value::from("ABC")));
        assert_eq!(Value::Number(5.0).compare(&Value::from("a")), Ordering::Less);
        assert!(Value::Empty.excel_eq(&Value::Number(0.0)));
    }
}
