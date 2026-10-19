//! Workbook, sheets and cells.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::address::{CellPos, Rect, MAX_COLS, MAX_ROWS};
use crate::ast::Expr;
use crate::parser::{parse_formula, ParseError};
use crate::style::{Color, Style, StyleId};
use crate::value::{parse_number_text, ErrorKind, Value};

/// One cell's content.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    /// Parsed formula, if the cell holds one.
    pub formula: Option<Box<Expr>>,
    /// Literal value, or the last computed result of the formula (an array for spilling formulas).
    pub value: Value,
    pub style: StyleId,
}

impl Cell {
    pub fn is_blank(&self) -> bool {
        self.formula.is_none() && self.value.is_empty()
    }
    /// The value to show in the cell (top-left of a spilled array).
    pub fn display_value(&self) -> Value {
        match &self.value {
            Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
            v => v.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CfOperator {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
}

/// A conditional formatting rule.
#[derive(Debug, Clone, PartialEq)]
pub enum CfRule {
    /// Compare the cell value with one or two formulas/constants.
    CellValue { op: CfOperator, a: String, b: String, style: CfStyle },
    /// Custom formula, relative to the top-left of the range.
    Expression { formula: String, style: CfStyle },
    /// Text contains.
    TextContains { text: String, style: CfStyle },
    /// 2- or 3-color scale.
    ColorScale { min: Color, mid: Option<Color>, max: Color },
    /// Data bar.
    DataBar { color: Color },
    /// Top/bottom N items (or percent).
    TopBottom { top: bool, rank: u32, percent: bool, style: CfStyle },
    /// Above/below average.
    AboveAverage { above: bool, style: CfStyle },
    /// Duplicate (or unique) values.
    Duplicates { unique: bool, style: CfStyle },
}

/// Formatting applied by a conditional rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CfStyle {
    pub font_color: Option<Color>,
    pub fill: Option<Color>,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalFormat {
    pub range: Rect,
    pub rule: CfRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Column,
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
}

impl ChartKind {
    pub const ALL: [ChartKind; 6] =
        [ChartKind::Column, ChartKind::Bar, ChartKind::Line, ChartKind::Area, ChartKind::Pie, ChartKind::Scatter];
    pub fn name(&self) -> &'static str {
        match self {
            ChartKind::Column => "Column",
            ChartKind::Bar => "Bar",
            ChartKind::Line => "Line",
            ChartKind::Area => "Area",
            ChartKind::Pie => "Pie",
            ChartKind::Scatter => "Scatter",
        }
    }
}

/// An embedded chart. The first row of `data` holds series names and the first
/// column holds category labels when `has_header` is set.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub kind: ChartKind,
    pub title: String,
    pub data: Rect,
    pub has_header: bool,
    /// Series are columns (true) or rows (false).
    pub series_in_columns: bool,
    /// Top-left anchor cell.
    pub anchor: CellPos,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationRule {
    /// Dropdown list of fixed values.
    List(Vec<String>),
    /// Dropdown list from a range on the same sheet.
    ListRange(Rect),
    WholeNumber { min: f64, max: f64 },
    Decimal { min: f64, max: f64 },
    TextLength { min: f64, max: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataValidation {
    pub range: Rect,
    pub rule: ValidationRule,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoFilter {
    pub range: Rect,
    /// Column offset -> allowed display values (None = all).
    pub allowed: HashMap<u32, HashSet<String>>,
}

pub const DEFAULT_COL_WIDTH: f32 = 64.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 20.0;

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub cells: BTreeMap<CellPos, Cell>,
    pub col_widths: HashMap<u32, f32>,
    pub row_heights: HashMap<u32, f32>,
    pub hidden_rows: HashSet<u32>,
    pub hidden_cols: HashSet<u32>,
    pub merges: Vec<Rect>,
    /// Frozen rows / columns (count from the top / left).
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub show_gridlines: bool,
    pub zoom: f32,
    pub cond_formats: Vec<ConditionalFormat>,
    pub validations: Vec<DataValidation>,
    pub charts: Vec<Chart>,
    pub comments: BTreeMap<CellPos, String>,
    pub hyperlinks: BTreeMap<CellPos, String>,
    pub autofilter: Option<AutoFilter>,
    pub tab_color: Option<Color>,
    pub hidden: bool,
    /// Computed: cell -> anchor of the spilled array covering it.
    pub spill_owner: HashMap<CellPos, CellPos>,
    /// Computed: anchor -> spill area.
    pub spill_areas: HashMap<CellPos, Rect>,
}

impl Sheet {
    pub fn new(name: &str) -> Sheet {
        Sheet {
            name: name.to_string(),
            cells: BTreeMap::new(),
            col_widths: HashMap::new(),
            row_heights: HashMap::new(),
            hidden_rows: HashSet::new(),
            hidden_cols: HashSet::new(),
            merges: Vec::new(),
            freeze_rows: 0,
            freeze_cols: 0,
            show_gridlines: true,
            zoom: 1.0,
            cond_formats: Vec::new(),
            validations: Vec::new(),
            charts: Vec::new(),
            comments: BTreeMap::new(),
            hyperlinks: BTreeMap::new(),
            autofilter: None,
            tab_color: None,
            hidden: false,
            spill_owner: HashMap::new(),
            spill_areas: HashMap::new(),
        }
    }

    pub fn col_width(&self, col: u32) -> f32 {
        if self.hidden_cols.contains(&col) {
            return 0.0;
        }
        *self.col_widths.get(&col).unwrap_or(&DEFAULT_COL_WIDTH)
    }
    pub fn row_height(&self, row: u32) -> f32 {
        if self.hidden_rows.contains(&row) {
            return 0.0;
        }
        *self.row_heights.get(&row).unwrap_or(&DEFAULT_ROW_HEIGHT)
    }

    /// Bounding box of all non-empty cells (and spills), or None if the sheet is empty.
    pub fn used_range(&self) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        let mut add = |p: CellPos| {
            r = Some(match r {
                None => Rect::single(p),
                Some(x) => Rect {
                    top: x.top.min(p.row),
                    left: x.left.min(p.col),
                    bottom: x.bottom.max(p.row),
                    right: x.right.max(p.col),
                },
            })
        };
        for (p, c) in &self.cells {
            if !c.is_blank() || c.style != 0 {
                add(*p);
            }
        }
        for a in self.spill_areas.values() {
            add(a.bottom_right());
        }
        r
    }

    /// Bounding box of cells with content (ignoring formatting-only cells).
    pub fn data_extent(&self) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        for (p, c) in &self.cells {
            if c.is_blank() {
                continue;
            }
            r = Some(match r {
                None => Rect::single(*p),
                Some(x) => Rect {
                    top: x.top.min(p.row),
                    left: x.left.min(p.col),
                    bottom: x.bottom.max(p.row),
                    right: x.right.max(p.col),
                },
            });
        }
        for a in self.spill_areas.values() {
            let br = a.bottom_right();
            r = Some(match r {
                None => *a,
                Some(x) => Rect {
                    top: x.top.min(a.top),
                    left: x.left.min(a.left),
                    bottom: x.bottom.max(br.row),
                    right: x.right.max(br.col),
                },
            });
        }
        r
    }

    /// Iterate stored cells inside `rect` in row-major order.
    pub fn cells_in(&self, rect: Rect) -> impl Iterator<Item = (&CellPos, &Cell)> {
        self.cells
            .range(CellPos::new(rect.top, 0)..=CellPos::new(rect.bottom, MAX_COLS))
            .filter(move |(p, _)| p.col >= rect.left && p.col <= rect.right)
    }

    /// Merged area containing `p`, if any.
    pub fn merge_at(&self, p: CellPos) -> Option<Rect> {
        self.merges.iter().copied().find(|m| m.contains(p))
    }

    /// Value displayed at `p`, including spilled values.
    pub fn value_at(&self, p: CellPos) -> Value {
        if let Some(c) = self.cells.get(&p) {
            if !c.is_blank() {
                return c.display_value();
            }
        }
        if let Some(anchor) = self.spill_owner.get(&p) {
            if let Some(c) = self.cells.get(anchor) {
                if let Value::Array(a) = &c.value {
                    let r = (p.row - anchor.row) as usize;
                    let col = (p.col - anchor.col) as usize;
                    if r < a.rows && col < a.cols {
                        return a.get(r, col).clone();
                    }
                }
            }
        }
        Value::Empty
    }
}

/// A defined name (named range or named formula).
#[derive(Debug, Clone, PartialEq)]
pub struct DefinedName {
    pub name: String,
    /// Sheet index for sheet-scoped names.
    pub scope: Option<usize>,
    pub formula: Expr,
}

#[derive(Debug, Clone)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub styles: Vec<Style>,
    style_index: HashMap<Style, StyleId>,
    pub names: Vec<DefinedName>,
    /// Cells found in a circular reference during the last recalculation.
    pub circular: Vec<(usize, CellPos)>,
    /// Manual calculation mode.
    pub manual_calc: bool,
    pub iterative: bool,
}

impl Default for Workbook {
    fn default() -> Self {
        Workbook::new()
    }
}

/// Result of interpreting typed text.
pub struct ParsedInput {
    pub formula: Option<Expr>,
    pub value: Value,
    /// Number format to apply if the cell currently uses General.
    pub suggested_format: Option<&'static str>,
}

/// Interpret what a user typed into a cell.
pub fn parse_input(text: &str) -> Result<ParsedInput, ParseError> {
    if let Some(f) = text.strip_prefix('=') {
        if !f.trim().is_empty() {
            let e = parse_formula(f)?;
            return Ok(ParsedInput { formula: Some(e), value: Value::Empty, suggested_format: None });
        }
    }
    if let Some(t) = text.strip_prefix('\'') {
        return Ok(ParsedInput { formula: None, value: Value::Text(t.to_string()), suggested_format: None });
    }
    let t = text.trim();
    if text.is_empty() {
        return Ok(ParsedInput { formula: None, value: Value::Empty, suggested_format: None });
    }
    if t.eq_ignore_ascii_case("TRUE") {
        return Ok(ParsedInput { formula: None, value: Value::Bool(true), suggested_format: None });
    }
    if t.eq_ignore_ascii_case("FALSE") {
        return Ok(ParsedInput { formula: None, value: Value::Bool(false), suggested_format: None });
    }
    if let Some(e) = ErrorKind::parse(t) {
        return Ok(ParsedInput { formula: None, value: Value::Error(e), suggested_format: None });
    }
    if let Some(n) = parse_number_text(t) {
        let fmt = if t.ends_with('%') {
            Some("0%")
        } else if t.starts_with('$') || t.starts_with("-$") {
            Some("$#,##0.00")
        } else if t.contains(',') && crate::datetime::parse_datetime_text(t).is_none() {
            Some("#,##0")
        } else if t.parse::<f64>().is_err() {
            // date and/or time
            let has_date = crate::datetime::parse_date_text(t).is_some()
                || t.contains('/')
                || t.matches('-').count() >= 2
                || t.chars().any(|c| c.is_alphabetic() && !matches!(c, 'a' | 'A' | 'p' | 'P' | 'm' | 'M'));
            let has_time = t.contains(':');
            match (has_date, has_time) {
                (true, true) => Some("m/d/yyyy h:mm"),
                (true, false) => Some("m/d/yyyy"),
                (false, true) => {
                    if t.to_ascii_uppercase().contains('M') {
                        Some("h:mm AM/PM")
                    } else {
                        Some("h:mm")
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        return Ok(ParsedInput { formula: None, value: Value::Number(n), suggested_format: fmt });
    }
    Ok(ParsedInput { formula: None, value: Value::Text(text.to_string()), suggested_format: None })
}

impl Workbook {
    pub fn new() -> Workbook {
        let mut wb = Workbook {
            sheets: vec![],
            styles: vec![Style::default()],
            style_index: HashMap::new(),
            names: vec![],
            circular: vec![],
            manual_calc: false,
            iterative: false,
        };
        wb.style_index.insert(Style::default(), 0);
        wb.sheets.push(Sheet::new("Sheet1"));
        wb
    }

    /// Workbook with no sheets (used by importers).
    pub fn empty() -> Workbook {
        let mut wb = Workbook::new();
        wb.sheets.clear();
        wb
    }

    // ----- styles -------------------------------------------------------

    pub fn intern_style(&mut self, s: Style) -> StyleId {
        if let Some(id) = self.style_index.get(&s) {
            return *id;
        }
        let id = self.styles.len() as StyleId;
        self.style_index.insert(s.clone(), id);
        self.styles.push(s);
        id
    }

    pub fn style(&self, id: StyleId) -> &Style {
        self.styles.get(id as usize).unwrap_or(&self.styles[0])
    }

    pub fn cell_style(&self, sheet: usize, p: CellPos) -> &Style {
        let id = self.sheets[sheet].cells.get(&p).map(|c| c.style).unwrap_or(0);
        self.style(id)
    }

    /// Apply a modification to the style of every cell in `rect`.
    pub fn update_style(&mut self, sheet: usize, rect: Rect, f: impl Fn(&mut Style)) {
        let mut cache: HashMap<StyleId, StyleId> = HashMap::new();
        for p in rect.iter() {
            let old = self.sheets[sheet].cells.get(&p).map(|c| c.style).unwrap_or(0);
            let new = match cache.get(&old) {
                Some(n) => *n,
                None => {
                    let mut s = self.style(old).clone();
                    f(&mut s);
                    let n = self.intern_style(s);
                    cache.insert(old, n);
                    n
                }
            };
            if new != old {
                let cell = self.sheets[sheet].cells.entry(p).or_default();
                cell.style = new;
            }
            if new == 0 {
                if let Some(c) = self.sheets[sheet].cells.get(&p) {
                    if c.is_blank() && c.style == 0 {
                        self.sheets[sheet].cells.remove(&p);
                    }
                }
            }
        }
    }

    // ----- sheets -------------------------------------------------------

    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn unique_sheet_name(&self, base: &str) -> String {
        if self.sheet_index(base).is_none() {
            return base.to_string();
        }
        let stem = base.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
        let stem = if stem.is_empty() { "Sheet" } else { stem };
        for i in 1.. {
            let n = format!("{}{}", stem, i);
            if self.sheet_index(&n).is_none() {
                return n;
            }
        }
        unreachable!()
    }

    pub fn add_sheet(&mut self, name: Option<&str>) -> usize {
        let name = match name {
            Some(n) => self.unique_sheet_name(n),
            None => {
                let mut i = self.sheets.len() + 1;
                loop {
                    let n = format!("Sheet{}", i);
                    if self.sheet_index(&n).is_none() {
                        break n;
                    }
                    i += 1;
                }
            }
        };
        self.sheets.push(Sheet::new(&name));
        self.sheets.len() - 1
    }

    pub fn insert_sheet_at(&mut self, idx: usize, sheet: Sheet) {
        self.sheets.insert(idx.min(self.sheets.len()), sheet);
        self.fix_name_scopes_after_insert(idx);
    }

    fn fix_name_scopes_after_insert(&mut self, idx: usize) {
        for n in &mut self.names {
            if let Some(s) = n.scope.as_mut() {
                if *s >= idx {
                    *s += 1;
                }
            }
        }
    }

    pub fn rename_sheet(&mut self, idx: usize, new_name: &str) -> Result<(), String> {
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.len() > 31 {
            return Err("Sheet names must be 1 to 31 characters.".into());
        }
        if new_name.chars().any(|c| matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']')) {
            return Err("Sheet names can't contain : \\ / ? * [ ]".into());
        }
        if let Some(i) = self.sheet_index(new_name) {
            if i != idx {
                return Err("That name is already taken.".into());
            }
        }
        let old = self.sheets[idx].name.clone();
        self.sheets[idx].name = new_name.to_string();
        self.for_each_formula(|_, _, e| e.rename_sheet(&old, new_name));
        for n in &mut self.names {
            n.formula.rename_sheet(&old, new_name);
        }
        Ok(())
    }

    pub fn delete_sheet(&mut self, idx: usize) -> Option<Sheet> {
        if self.sheets.len() <= 1 {
            return None;
        }
        let s = self.sheets.remove(idx);
        let name = s.name.clone();
        self.for_each_formula(|_, _, e| e.invalidate_sheet(&name));
        self.names.retain(|n| n.scope != Some(idx));
        for n in &mut self.names {
            n.formula.invalidate_sheet(&name);
            if let Some(sc) = n.scope.as_mut() {
                if *sc > idx {
                    *sc -= 1;
                }
            }
        }
        self.recalculate();
        Some(s)
    }

    pub fn move_sheet(&mut self, from: usize, to: usize) {
        if from == to || from >= self.sheets.len() || to >= self.sheets.len() {
            return;
        }
        let s = self.sheets.remove(from);
        self.sheets.insert(to, s);
        for n in &mut self.names {
            if let Some(sc) = n.scope.as_mut() {
                if *sc == from {
                    *sc = to;
                } else if from < to && *sc > from && *sc <= to {
                    *sc -= 1;
                } else if from > to && *sc >= to && *sc < from {
                    *sc += 1;
                }
            }
        }
    }

    pub fn duplicate_sheet(&mut self, idx: usize) -> usize {
        let mut s = self.sheets[idx].clone();
        s.name = self.unique_sheet_name(&format!("{} (2)", self.sheets[idx].name));
        self.sheets.insert(idx + 1, s);
        self.fix_name_scopes_after_insert(idx + 1);
        self.recalculate();
        idx + 1
    }

    /// Run `f` on every formula in the workbook.
    pub fn for_each_formula(&mut self, mut f: impl FnMut(usize, CellPos, &mut Expr)) {
        for (si, sheet) in self.sheets.iter_mut().enumerate() {
            for (p, c) in sheet.cells.iter_mut() {
                if let Some(e) = c.formula.as_mut() {
                    f(si, *p, e);
                }
            }
        }
    }

    // ----- cells --------------------------------------------------------

    pub fn cell(&self, sheet: usize, p: CellPos) -> Option<&Cell> {
        self.sheets.get(sheet)?.cells.get(&p)
    }

    /// Value displayed at a position (including spilled arrays).
    pub fn value(&self, sheet: usize, p: CellPos) -> Value {
        self.sheets.get(sheet).map(|s| s.value_at(p)).unwrap_or(Value::Empty)
    }

    /// The text to show in the formula bar for a cell.
    pub fn input_text(&self, sheet: usize, p: CellPos) -> String {
        let Some(c) = self.cell(sheet, p) else { return String::new() };
        if let Some(f) = &c.formula {
            return format!("={}", f.to_formula());
        }
        match &c.value {
            Value::Number(n) => {
                let st = self.style(c.style);
                if crate::numfmt::is_date_format(&st.num_fmt) {
                    let has_time = n.fract() != 0.0;
                    let fmt = if *n < 1.0 && *n >= 0.0 {
                        "h:mm:ss AM/PM"
                    } else if has_time {
                        "m/d/yyyy h:mm:ss AM/PM"
                    } else {
                        "m/d/yyyy"
                    };
                    return crate::numfmt::format_value(&c.value, fmt).text;
                }
                if st.num_fmt.contains('%') {
                    return format!("{}%", crate::value::format_number_general(n * 100.0));
                }
                crate::value::format_number_general(*n)
            }
            Value::Text(t) => {
                // Text that would be read as something else needs the apostrophe.
                if t.starts_with('=') || parse_number_text(t).is_some() || t.eq_ignore_ascii_case("TRUE") || t.eq_ignore_ascii_case("FALSE") {
                    format!("'{}", t)
                } else {
                    t.clone()
                }
            }
            v => v.display(),
        }
    }

    /// Set a cell from typed text (a formula if it starts with `=`). Does not recalculate.
    pub fn set_input(&mut self, sheet: usize, p: CellPos, text: &str) -> Result<(), ParseError> {
        let parsed = parse_input(text)?;
        if let Some(fmt) = parsed.suggested_format {
            let cur = self.cell(sheet, p).map(|c| c.style).unwrap_or(0);
            if self.style(cur).num_fmt == "General" {
                self.update_style(sheet, Rect::single(p), |s| s.num_fmt = fmt.to_string());
            }
        }
        let s = &mut self.sheets[sheet];
        let style = s.cells.get(&p).map(|c| c.style).unwrap_or(0);
        if parsed.formula.is_none() && parsed.value.is_empty() {
            if style == 0 {
                s.cells.remove(&p);
            } else if let Some(c) = s.cells.get_mut(&p) {
                c.formula = None;
                c.value = Value::Empty;
            }
            return Ok(());
        }
        let cell = s.cells.entry(p).or_default();
        cell.formula = parsed.formula.map(Box::new);
        cell.value = parsed.value;
        Ok(())
    }

    /// Set a literal value directly.
    pub fn set_value(&mut self, sheet: usize, p: CellPos, v: Value) {
        let s = &mut self.sheets[sheet];
        if v.is_empty() {
            if let Some(c) = s.cells.get_mut(&p) {
                c.formula = None;
                c.value = Value::Empty;
                if c.style == 0 {
                    s.cells.remove(&p);
                }
            }
            return;
        }
        let c = s.cells.entry(p).or_default();
        c.formula = None;
        c.value = v;
    }

    /// Set a formula from text (without `=`).
    pub fn set_formula(&mut self, sheet: usize, p: CellPos, formula: &str) -> Result<(), ParseError> {
        let e = parse_formula(formula.strip_prefix('=').unwrap_or(formula))?;
        let c = self.sheets[sheet].cells.entry(p).or_default();
        c.formula = Some(Box::new(e));
        c.value = Value::Empty;
        Ok(())
    }

    /// Clear contents (and optionally formats) of a rectangle.
    pub fn clear(&mut self, sheet: usize, rect: Rect, contents: bool, formats: bool) {
        let s = &mut self.sheets[sheet];
        let keys: Vec<CellPos> = s.cells_in(rect).map(|(p, _)| *p).collect();
        for p in keys {
            let remove = {
                let c = s.cells.get_mut(&p).unwrap();
                if contents {
                    c.formula = None;
                    c.value = Value::Empty;
                }
                if formats {
                    c.style = 0;
                }
                c.is_blank() && c.style == 0
            };
            if remove {
                s.cells.remove(&p);
            }
        }
        if contents {
            s.comments.retain(|p, _| !rect.contains(*p));
            s.hyperlinks.retain(|p, _| !rect.contains(*p));
        }
        if formats {
            s.merges.retain(|m| !rect.intersects(m));
        }
    }

    // ----- structure ----------------------------------------------------

    /// Insert (`count` > 0) or delete (`count` < 0) rows starting at `at`.
    pub fn insert_rows(&mut self, sheet: usize, at: u32, count: i64) {
        self.shift(sheet, true, at, count);
    }

    /// Insert (`count` > 0) or delete (`count` < 0) columns starting at `at`.
    pub fn insert_cols(&mut self, sheet: usize, at: u32, count: i64) {
        self.shift(sheet, false, at, count);
    }

    fn shift(&mut self, sheet: usize, is_row: bool, at: u32, count: i64) {
        if count == 0 {
            return;
        }
        let target = self.sheets[sheet].name.clone();
        // 1. adjust formulas everywhere
        let names: Vec<String> = self.sheets.iter().map(|s| s.name.clone()).collect();
        self.for_each_formula(|si, _, e| e.adjust_for_insert_delete(&names[si], &target, is_row, at, count));
        for n in &mut self.names {
            let fs = n.scope.map(|i| names[i].clone()).unwrap_or_else(|| target.clone() + "\u{0}");
            n.formula.adjust_for_insert_delete(&fs, &target, is_row, at, count);
        }
        let max = if is_row { MAX_ROWS } else { MAX_COLS } as i64;
        let at64 = at as i64;
        let map = |v: u32| -> Option<u32> {
            let v = v as i64;
            if v < at64 {
                Some(v as u32)
            } else if count > 0 {
                if v + count >= max {
                    None
                } else {
                    Some((v + count) as u32)
                }
            } else if v < at64 - count {
                None
            } else {
                Some((v + count) as u32)
            }
        };
        let map_rect = |r: Rect| -> Option<Rect> {
            let (a, b) = if is_row { (r.top, r.bottom) } else { (r.left, r.right) };
            let na = if (a as i64) >= at64 {
                if count < 0 && (a as i64) < at64 - count {
                    at
                } else {
                    map(a)?
                }
            } else {
                a
            };
            let nb = if (b as i64) >= at64 {
                match map(b) {
                    Some(v) => v,
                    None => {
                        if count < 0 {
                            if at == 0 {
                                return None;
                            }
                            at - 1
                        } else {
                            (max - 1) as u32
                        }
                    }
                }
            } else {
                b
            };
            if nb < na {
                return None;
            }
            Some(if is_row { Rect { top: na, bottom: nb, ..r } } else { Rect { left: na, right: nb, ..r } })
        };
        let s = &mut self.sheets[sheet];
        // 2. move cells
        let old = std::mem::take(&mut s.cells);
        for (p, c) in old {
            let v = if is_row { p.row } else { p.col };
            if let Some(nv) = map(v) {
                let np = if is_row { CellPos::new(nv, p.col) } else { CellPos::new(p.row, nv) };
                s.cells.insert(np, c);
            }
        }
        let remap_map = |m: &mut BTreeMap<CellPos, String>| {
            let old = std::mem::take(m);
            for (p, c) in old {
                let v = if is_row { p.row } else { p.col };
                if let Some(nv) = map(v) {
                    let np = if is_row { CellPos::new(nv, p.col) } else { CellPos::new(p.row, nv) };
                    m.insert(np, c);
                }
            }
        };
        remap_map(&mut s.comments);
        remap_map(&mut s.hyperlinks);
        // 3. sizes and hidden
        let sizes = if is_row { &mut s.row_heights } else { &mut s.col_widths };
        let old = std::mem::take(sizes);
        for (k, v) in old {
            if let Some(nk) = map(k) {
                sizes.insert(nk, v);
            }
        }
        let hidden = if is_row { &mut s.hidden_rows } else { &mut s.hidden_cols };
        let old = std::mem::take(hidden);
        for k in old {
            if let Some(nk) = map(k) {
                hidden.insert(nk);
            }
        }
        // 4. rect-based features
        s.merges = s.merges.iter().filter_map(|m| map_rect(*m)).filter(|m| m.cell_count() > 1).collect();
        s.cond_formats = s
            .cond_formats
            .drain(..)
            .filter_map(|mut c| {
                c.range = map_rect(c.range)?;
                Some(c)
            })
            .collect();
        s.validations = s
            .validations
            .drain(..)
            .filter_map(|mut c| {
                c.range = map_rect(c.range)?;
                Some(c)
            })
            .collect();
        s.charts = s
            .charts
            .drain(..)
            .filter_map(|mut c| {
                c.data = map_rect(c.data)?;
                Some(c)
            })
            .collect();
        if let Some(af) = s.autofilter.as_mut() {
            match map_rect(af.range) {
                Some(r) => af.range = r,
                None => s.autofilter = None,
            }
        }
        self.recalculate();
    }

    // ----- names --------------------------------------------------------

    pub fn define_name(&mut self, name: &str, scope: Option<usize>, formula: &str) -> Result<(), String> {
        let valid = !name.is_empty()
            && name.chars().next().map(|c| c.is_alphabetic() || c == '_' || c == '\\').unwrap_or(false)
            && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '\\')
            && CellPos::parse_a1(name).is_none()
            && !name.eq_ignore_ascii_case("TRUE")
            && !name.eq_ignore_ascii_case("FALSE");
        if !valid {
            return Err(format!("'{}' is not a valid name", name));
        }
        let e = parse_formula(formula.strip_prefix('=').unwrap_or(formula)).map_err(|e| e.to_string())?;
        self.names.retain(|n| !(n.name.eq_ignore_ascii_case(name) && n.scope == scope));
        self.names.push(DefinedName { name: name.to_string(), scope, formula: e });
        Ok(())
    }

    pub fn find_name(&self, name: &str, sheet: usize) -> Option<&DefinedName> {
        self.names
            .iter()
            .find(|n| n.scope == Some(sheet) && n.name.eq_ignore_ascii_case(name))
            .or_else(|| self.names.iter().find(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(name)))
    }

    // ----- calculation --------------------------------------------------

    /// Recalculate every formula in the workbook.
    pub fn recalculate(&mut self) {
        crate::eval::recalculate(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_parsing() {
        let p = parse_input("12.5").unwrap();
        assert_eq!(p.value, Value::Number(12.5));
        let p = parse_input("15%").unwrap();
        assert_eq!(p.value, Value::Number(0.15));
        assert_eq!(p.suggested_format, Some("0%"));
        let p = parse_input("1/2/2024").unwrap();
        assert_eq!(p.suggested_format, Some("m/d/yyyy"));
        let p = parse_input("'123").unwrap();
        assert_eq!(p.value, Value::Text("123".into()));
        let p = parse_input("hello").unwrap();
        assert_eq!(p.value, Value::Text("hello".into()));
    }

    #[test]
    fn insert_rows_moves_cells_and_formulas() {
        let mut wb = Workbook::new();
        wb.set_input(0, CellPos::new(0, 0), "1").unwrap();
        wb.set_input(0, CellPos::new(1, 0), "2").unwrap();
        wb.set_input(0, CellPos::new(2, 0), "=SUM(A1:A2)").unwrap();
        wb.recalculate();
        wb.insert_rows(0, 1, 1);
        assert_eq!(wb.input_text(0, CellPos::new(3, 0)), "=SUM(A1:A3)");
        assert_eq!(wb.value(0, CellPos::new(3, 0)), Value::Number(3.0));
        wb.insert_rows(0, 0, -1);
        assert_eq!(wb.input_text(0, CellPos::new(2, 0)), "=SUM(A1:A2)");
        assert_eq!(wb.value(0, CellPos::new(2, 0)), Value::Number(2.0));
    }

    #[test]
    fn rename_sheet_updates_refs() {
        let mut wb = Workbook::new();
        wb.add_sheet(None);
        wb.set_input(1, CellPos::new(0, 0), "5").unwrap();
        wb.set_input(0, CellPos::new(0, 0), "=Sheet2!A1*2").unwrap();
        wb.recalculate();
        assert_eq!(wb.value(0, CellPos::new(0, 0)), Value::Number(10.0));
        wb.rename_sheet(1, "Data Sheet").unwrap();
        assert_eq!(wb.input_text(0, CellPos::new(0, 0)), "='Data Sheet'!A1*2");
        wb.recalculate();
        assert_eq!(wb.value(0, CellPos::new(0, 0)), Value::Number(10.0));
    }
}
