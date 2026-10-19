//! Cell addresses, A1-style parsing and column-letter conversion.

use std::fmt;

/// Maximum number of rows in a sheet (same as Excel).
pub const MAX_ROWS: u32 = 1_048_576;
/// Maximum number of columns in a sheet (same as Excel, XFD).
pub const MAX_COLS: u32 = 16_384;

/// A zero-based cell position on a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct CellPos {
    pub row: u32,
    pub col: u32,
}

impl CellPos {
    pub const fn new(row: u32, col: u32) -> Self {
        CellPos { row, col }
    }

    /// Parse a plain A1 address such as `B12` (no `$`, no sheet name).
    pub fn parse_a1(s: &str) -> Option<CellPos> {
        let s = s.trim();
        let s = s.replace('$', "");
        let split = s.find(|c: char| c.is_ascii_digit())?;
        let (letters, digits) = s.split_at(split);
        if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        let col = col_from_letters(letters)?;
        let row: u32 = digits.parse().ok()?;
        if row == 0 || row > MAX_ROWS {
            return None;
        }
        Some(CellPos::new(row - 1, col))
    }

    pub fn to_a1(&self) -> String {
        format!("{}{}", col_to_letters(self.col), self.row + 1)
    }
}

impl fmt::Display for CellPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

/// Convert a zero-based column index to letters (0 -> A, 25 -> Z, 26 -> AA).
pub fn col_to_letters(mut col: u32) -> String {
    let mut out = Vec::new();
    col += 1;
    while col > 0 {
        let rem = (col - 1) % 26;
        out.push(b'A' + rem as u8);
        col = (col - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Convert column letters to a zero-based column index.
pub fn col_from_letters(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 3 {
        return None;
    }
    let mut n: u32 = 0;
    for c in s.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        n = n * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
    }
    if n == 0 || n > MAX_COLS {
        return None;
    }
    Some(n - 1)
}

/// A rectangular block of cells (inclusive bounds), zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
}

impl Rect {
    pub fn new(a: CellPos, b: CellPos) -> Rect {
        Rect {
            top: a.row.min(b.row),
            left: a.col.min(b.col),
            bottom: a.row.max(b.row),
            right: a.col.max(b.col),
        }
    }
    pub fn single(p: CellPos) -> Rect {
        Rect::new(p, p)
    }
    pub fn rows(&self) -> u32 {
        self.bottom - self.top + 1
    }
    pub fn cols(&self) -> u32 {
        self.right - self.left + 1
    }
    pub fn contains(&self, p: CellPos) -> bool {
        p.row >= self.top && p.row <= self.bottom && p.col >= self.left && p.col <= self.right
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.left <= o.right && o.left <= self.right && self.top <= o.bottom && o.top <= self.bottom
    }
    pub fn intersection(&self, o: &Rect) -> Option<Rect> {
        if !self.intersects(o) {
            return None;
        }
        Some(Rect {
            top: self.top.max(o.top),
            left: self.left.max(o.left),
            bottom: self.bottom.min(o.bottom),
            right: self.right.min(o.right),
        })
    }
    pub fn top_left(&self) -> CellPos {
        CellPos::new(self.top, self.left)
    }
    pub fn bottom_right(&self) -> CellPos {
        CellPos::new(self.bottom, self.right)
    }
    pub fn cell_count(&self) -> u64 {
        self.rows() as u64 * self.cols() as u64
    }
    /// Parse `A1` or `A1:B5` (no sheet name).
    pub fn parse(s: &str) -> Option<Rect> {
        let s = s.trim();
        if let Some((a, b)) = s.split_once(':') {
            Some(Rect::new(CellPos::parse_a1(a)?, CellPos::parse_a1(b)?))
        } else {
            CellPos::parse_a1(s).map(Rect::single)
        }
    }
    pub fn to_a1(&self) -> String {
        if self.top == self.bottom && self.left == self.right {
            self.top_left().to_a1()
        } else {
            format!("{}:{}", self.top_left().to_a1(), self.bottom_right().to_a1())
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = CellPos> + '_ {
        let r = *self;
        (r.top..=r.bottom).flat_map(move |row| (r.left..=r.right).map(move |col| CellPos::new(row, col)))
    }
}

/// Quote a sheet name for use in a formula if needed.
pub fn quote_sheet_name(name: &str) -> String {
    let needs = name.is_empty()
        || name.chars().any(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
        || name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        || CellPos::parse_a1(name).is_some();
    if needs {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letters() {
        assert_eq!(col_to_letters(0), "A");
        assert_eq!(col_to_letters(25), "Z");
        assert_eq!(col_to_letters(26), "AA");
        assert_eq!(col_to_letters(701), "ZZ");
        assert_eq!(col_to_letters(702), "AAA");
        assert_eq!(col_to_letters(16383), "XFD");
        for i in 0..20000 {
            if i < MAX_COLS {
                assert_eq!(col_from_letters(&col_to_letters(i)), Some(i));
            }
        }
    }
    #[test]
    fn a1() {
        assert_eq!(CellPos::parse_a1("B3"), Some(CellPos::new(2, 1)));
        assert_eq!(CellPos::parse_a1("$B$3"), Some(CellPos::new(2, 1)));
        assert_eq!(CellPos::parse_a1("3B"), None);
        assert_eq!(Rect::parse("A1:B2").unwrap().cell_count(), 4);
    }
}
