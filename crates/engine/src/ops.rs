//! Editing operations used by the GUI: undo/redo snapshots, copy/paste,
//! fill series, sort, find/replace and a few data tools.

use std::collections::{BTreeMap, HashSet};

use crate::address::{CellPos, Rect, MAX_COLS, MAX_ROWS};
use crate::ast::Expr;
use crate::style::Style;
use crate::value::{compare_text, Value};
use crate::workbook::{Cell, Sheet, ValidationRule, Workbook};

// =====================================================================
// Undo / redo
// =====================================================================

/// What part of the workbook an operation may change.
#[derive(Debug, Clone)]
pub enum Scope {
    /// Cell contents/styles (plus comments and hyperlinks) inside some rectangles of one sheet.
    Cells { sheet: usize, rects: Vec<Rect> },
    /// Everything on one sheet (structure, merges, rules, sizes...).
    Sheet(usize),
    /// The entire workbook (sheet list, names, cross-sheet formula rewrites...).
    Workbook,
}

#[derive(Debug, Clone)]
enum Saved {
    Cells {
        sheet: usize,
        rects: Vec<Rect>,
        cells: Vec<(CellPos, Cell)>,
        comments: Vec<(CellPos, String)>,
        links: Vec<(CellPos, String)>,
    },
    Sheet { idx: usize, sheet: Box<Sheet> },
    Workbook(Box<Workbook>),
}

fn rect_cells(sheet: &Sheet, rects: &[Rect]) -> Vec<(CellPos, Cell)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in rects {
        for (p, c) in sheet.cells_in(*r) {
            if seen.insert(*p) {
                out.push((*p, c.clone()));
            }
        }
    }
    out
}

fn capture(wb: &Workbook, scope: &Scope) -> Saved {
    match scope {
        Scope::Cells { sheet, rects } => {
            let s = &wb.sheets[*sheet];
            let inside = |p: &CellPos| rects.iter().any(|r| r.contains(*p));
            Saved::Cells {
                sheet: *sheet,
                rects: rects.clone(),
                cells: rect_cells(s, rects),
                comments: s.comments.iter().filter(|(p, _)| inside(p)).map(|(p, c)| (*p, c.clone())).collect(),
                links: s.hyperlinks.iter().filter(|(p, _)| inside(p)).map(|(p, c)| (*p, c.clone())).collect(),
            }
        }
        Scope::Sheet(i) => Saved::Sheet { idx: *i, sheet: Box::new(wb.sheets[*i].clone()) },
        Scope::Workbook => Saved::Workbook(Box::new(wb.clone())),
    }
}

/// Put a saved state back, returning the state it replaced (for the opposite stack).
fn restore(wb: &mut Workbook, saved: Saved) -> Saved {
    match saved {
        Saved::Cells { sheet, rects, cells, comments, links } => {
            let current = capture(wb, &Scope::Cells { sheet, rects: rects.clone() });
            let s = &mut wb.sheets[sheet];
            for r in &rects {
                let keys: Vec<CellPos> = s.cells_in(*r).map(|(p, _)| *p).collect();
                for k in keys {
                    s.cells.remove(&k);
                }
                s.comments.retain(|p, _| !r.contains(*p));
                s.hyperlinks.retain(|p, _| !r.contains(*p));
            }
            for (p, c) in cells {
                s.cells.insert(p, c);
            }
            for (p, c) in comments {
                s.comments.insert(p, c);
            }
            for (p, c) in links {
                s.hyperlinks.insert(p, c);
            }
            current
        }
        Saved::Sheet { idx, sheet } => {
            let cur = std::mem::replace(&mut wb.sheets[idx], *sheet);
            Saved::Sheet { idx, sheet: Box::new(cur) }
        }
        Saved::Workbook(w) => {
            let cur = std::mem::replace(wb, *w);
            Saved::Workbook(Box::new(cur))
        }
    }
}

/// One undoable step.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub label: String,
    /// Sheet and selection to restore when this step is undone/redone.
    pub sheet: usize,
    pub selection: Rect,
    saved: Saved,
}

/// Undo and redo stacks of workbook snapshots.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
}

/// Result of an undo or redo.
#[derive(Debug, Clone)]
pub struct Restored {
    pub label: String,
    pub sheet: usize,
    pub selection: Rect,
}

impl UndoStack {
    pub const LIMIT: usize = 100;

    pub fn new() -> UndoStack {
        UndoStack::default()
    }

    /// Record the current state of `scope` before changing it.
    pub fn record(&mut self, wb: &Workbook, scope: Scope, label: &str, sheet: usize, selection: Rect) {
        let saved = capture(wb, &scope);
        self.undo.push(UndoEntry { label: label.to_string(), sheet, selection, saved });
        if self.undo.len() > Self::LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    /// Drop the most recent entry (when an operation turned out to change nothing).
    pub fn discard_last(&mut self) {
        self.undo.pop();
    }

    /// Undo the last step. The workbook is recalculated unless in manual mode.
    pub fn undo(&mut self, wb: &mut Workbook) -> Option<Restored> {
        let e = self.undo.pop()?;
        let inverse = restore(wb, e.saved);
        let r = Restored { label: e.label.clone(), sheet: e.sheet, selection: e.selection };
        self.redo.push(UndoEntry { label: e.label, sheet: e.sheet, selection: e.selection, saved: inverse });
        if !wb.manual_calc {
            wb.recalculate();
        }
        Some(r)
    }

    pub fn redo(&mut self, wb: &mut Workbook) -> Option<Restored> {
        let e = self.redo.pop()?;
        let inverse = restore(wb, e.saved);
        let r = Restored { label: e.label.clone(), sheet: e.sheet, selection: e.selection };
        self.undo.push(UndoEntry { label: e.label, sheet: e.sheet, selection: e.selection, saved: inverse });
        if !wb.manual_calc {
            wb.recalculate();
        }
        Some(r)
    }
}

// =====================================================================
// Clipboard
// =====================================================================

/// Copied cells.
#[derive(Debug, Clone)]
pub struct Clip {
    pub sheet: usize,
    pub sheet_name: String,
    pub rect: Rect,
    /// Row-major cells (rows x cols); None = blank cell with default style.
    pub cells: Vec<Option<Cell>>,
    /// Merges relative to the top-left of `rect`.
    pub merges: Vec<Rect>,
    pub comments: Vec<(u32, u32, String)>,
    pub cut: bool,
    /// Tab-separated text placed on the system clipboard.
    pub text: String,
}

impl Clip {
    pub fn rows(&self) -> u32 {
        self.rect.rows()
    }
    pub fn cols(&self) -> u32 {
        self.rect.cols()
    }
    pub fn get(&self, r: u32, c: u32) -> Option<&Cell> {
        self.cells.get((r * self.cols() + c) as usize).and_then(|c| c.as_ref())
    }
}

/// Text shown in a cell (number format applied).
pub fn display_text(wb: &Workbook, sheet: usize, p: CellPos) -> String {
    let v = wb.value(sheet, p);
    if v.is_empty() {
        return String::new();
    }
    let st = wb.cell_style(sheet, p);
    crate::numfmt::format_value(&v, &st.num_fmt).text
}

/// Copy a rectangle.
pub fn copy(wb: &Workbook, sheet: usize, rect: Rect, cut: bool) -> Clip {
    let s = &wb.sheets[sheet];
    let mut cells = vec![None; rect.cell_count() as usize];
    let cols = rect.cols();
    for (p, c) in s.cells_in(rect) {
        let i = ((p.row - rect.top) * cols + (p.col - rect.left)) as usize;
        cells[i] = Some(c.clone());
    }
    // Values of spilled cells whose anchor is outside the copied area.
    for (p, anchor) in &s.spill_owner {
        if rect.contains(*p) && !rect.contains(*anchor) {
            let i = ((p.row - rect.top) * cols + (p.col - rect.left)) as usize;
            let v = s.value_at(*p);
            let style = cells[i].as_ref().map(|c| c.style).unwrap_or(0);
            cells[i] = Some(Cell { formula: None, value: v, style });
        }
    }
    let merges = s
        .merges
        .iter()
        .filter(|m| rect.contains(m.top_left()) && rect.contains(m.bottom_right()))
        .map(|m| Rect { top: m.top - rect.top, left: m.left - rect.left, bottom: m.bottom - rect.top, right: m.right - rect.left })
        .collect();
    let comments = s
        .comments
        .iter()
        .filter(|(p, _)| rect.contains(**p))
        .map(|(p, t)| (p.row - rect.top, p.col - rect.left, t.clone()))
        .collect();
    Clip {
        sheet,
        sheet_name: s.name.clone(),
        rect,
        cells,
        merges,
        comments,
        cut,
        text: to_tsv(wb, sheet, rect),
    }
}

/// Tab-separated text of the displayed values in `rect`.
pub fn to_tsv(wb: &Workbook, sheet: usize, rect: Rect) -> String {
    let mut out = String::new();
    // Avoid giant strings for whole-column selections: stop at the data extent.
    let ext = wb.sheets[sheet].data_extent();
    let bottom = ext.map(|e| e.bottom.min(rect.bottom)).unwrap_or(rect.top).max(rect.top);
    let right = ext.map(|e| e.right.min(rect.right)).unwrap_or(rect.left).max(rect.left);
    for r in rect.top..=bottom {
        for c in rect.left..=right {
            if c > rect.left {
                out.push('\t');
            }
            let t = display_text(wb, sheet, CellPos::new(r, c));
            if t.contains('\t') || t.contains('\n') || t.contains('"') {
                out.push('"');
                out.push_str(&t.replace('"', "\"\""));
                out.push('"');
            } else {
                out.push_str(&t);
            }
        }
        out.push('\n');
    }
    out
}

/// Parse tab-separated text (with Excel-style quoting) into rows of fields.
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut at_field_start = true;
    while i < chars.len() {
        let c = chars[i];
        if at_field_start && c == '"' {
            // quoted field
            i += 1;
            loop {
                if i >= chars.len() {
                    break;
                }
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        field.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                field.push(chars[i]);
                i += 1;
            }
            at_field_start = false;
            continue;
        }
        at_field_start = false;
        match c {
            '\t' => {
                row.push(std::mem::take(&mut field));
                at_field_start = true;
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                at_field_start = true;
            }
            c => field.push(c),
        }
        i += 1;
    }
    row.push(field);
    rows.push(row);
    rows
}

/// Paste text (e.g. from another application) at `at`, interpreting each field
/// as typed input. Returns the affected rectangle.
pub fn paste_text(wb: &mut Workbook, sheet: usize, at: CellPos, text: &str) -> Rect {
    let rows = parse_tsv(text);
    let h = rows.len() as u32;
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(1) as u32;
    for (ri, row) in rows.iter().enumerate() {
        for (ci, f) in row.iter().enumerate() {
            let p = CellPos::new(at.row + ri as u32, at.col + ci as u32);
            if p.row >= MAX_ROWS || p.col >= MAX_COLS {
                continue;
            }
            if wb.set_input(sheet, p, f).is_err() {
                wb.set_value(sheet, p, Value::Text(f.clone()));
            }
        }
    }
    text_paste_rect(at, h, w)
}

/// Rectangle covered by pasting `text` at `at`.
pub fn text_paste_rect_for(at: CellPos, text: &str) -> Rect {
    let rows = parse_tsv(text);
    let h = rows.len() as u32;
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(1) as u32;
    text_paste_rect(at, h, w)
}

fn text_paste_rect(at: CellPos, h: u32, w: u32) -> Rect {
    Rect {
        top: at.row,
        left: at.col,
        bottom: (at.row + h.max(1) - 1).min(MAX_ROWS - 1),
        right: (at.col + w.max(1) - 1).min(MAX_COLS - 1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasteWhat {
    #[default]
    All,
    Formulas,
    Values,
    Formats,
    /// Values plus number formats.
    ValuesAndFormats,
    AllExceptBorders,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasteOp {
    #[default]
    None,
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PasteOptions {
    pub what: PasteWhat,
    pub op: PasteOp,
    pub skip_blanks: bool,
    pub transpose: bool,
}

/// The area a paste into `dest` will cover (tiling when `dest` is a multiple of the clip size).
pub fn paste_area(clip: &Clip, dest: Rect, transpose: bool) -> Rect {
    let (h, w) = if transpose { (clip.cols(), clip.rows()) } else { (clip.rows(), clip.cols()) };
    let (mut th, mut tw) = (1, 1);
    if dest.rows() % h == 0 && dest.cols() % w == 0 && dest.cell_count() > 1 {
        th = dest.rows() / h;
        tw = dest.cols() / w;
    }
    Rect {
        top: dest.top,
        left: dest.left,
        bottom: (dest.top as u64 + (h * th) as u64 - 1).min(MAX_ROWS as u64 - 1) as u32,
        right: (dest.left as u64 + (w * tw) as u64 - 1).min(MAX_COLS as u64 - 1) as u32,
    }
}

fn apply_op(op: PasteOp, old: &Value, new: &Value) -> Value {
    let a = match old {
        Value::Empty => 0.0,
        Value::Number(n) => *n,
        _ => return new.clone(),
    };
    let b = match new {
        Value::Empty => 0.0,
        Value::Number(n) => *n,
        _ => return old.clone(),
    };
    Value::number(match op {
        PasteOp::None => b,
        PasteOp::Add => a + b,
        PasteOp::Subtract => a - b,
        PasteOp::Multiply => a * b,
        PasteOp::Divide => {
            if b == 0.0 {
                return Value::Error(crate::value::ErrorKind::Div0);
            }
            a / b
        }
    })
}

/// Paste a (copied, not cut) clip into `dest`. Returns the pasted area.
pub fn paste(wb: &mut Workbook, sheet: usize, dest: Rect, clip: &Clip, opts: PasteOptions) -> Rect {
    let area = paste_area(clip, dest, opts.transpose);
    let (h, w) = if opts.transpose { (clip.cols(), clip.rows()) } else { (clip.rows(), clip.cols()) };
    for p in area.iter() {
        let dr = (p.row - area.top) % h;
        let dc = (p.col - area.left) % w;
        let (sr, sc) = if opts.transpose { (dc, dr) } else { (dr, dc) };
        let src = clip.get(sr, sc);
        let src_blank = src.map(|c| c.is_blank()).unwrap_or(true);
        if opts.skip_blanks && src_blank {
            continue;
        }
        let src_pos = CellPos::new(clip.rect.top + sr, clip.rect.left + sc);
        let drow = p.row as i64 - src_pos.row as i64;
        let dcol = p.col as i64 - src_pos.col as i64;
        let src_style = src.map(|c| c.style).unwrap_or(0);
        let paste_values = matches!(opts.what, PasteWhat::All | PasteWhat::Formulas | PasteWhat::Values | PasteWhat::ValuesAndFormats | PasteWhat::AllExceptBorders);
        let paste_formulas = matches!(opts.what, PasteWhat::All | PasteWhat::Formulas | PasteWhat::AllExceptBorders);
        if paste_values {
            let old = wb.value(sheet, p);
            match src {
                Some(c) if c.formula.is_some() && paste_formulas && opts.op == PasteOp::None => {
                    let mut e: Expr = (**c.formula.as_ref().unwrap()).clone();
                    e.shift_relative(drow, dcol);
                    let cell = wb.sheets[sheet].cells.entry(p).or_default();
                    cell.formula = Some(Box::new(e));
                    cell.value = Value::Empty;
                }
                _ => {
                    let v = src.map(|c| c.display_value()).unwrap_or(Value::Empty);
                    let v = if opts.op == PasteOp::None { v } else { apply_op(opts.op, &old, &v) };
                    wb.set_value(sheet, p, v);
                }
            }
        }
        let new_style = match opts.what {
            PasteWhat::All => Some(src_style),
            PasteWhat::Formats => Some(src_style),
            PasteWhat::AllExceptBorders => {
                let cur_borders = wb.cell_style(sheet, p).borders;
                let mut st: Style = wb.style(src_style).clone();
                st.borders = cur_borders;
                Some(wb.intern_style(st))
            }
            PasteWhat::ValuesAndFormats => {
                let fmt = wb.style(src_style).num_fmt.clone();
                let mut st = wb.cell_style(sheet, p).clone();
                st.num_fmt = fmt;
                Some(wb.intern_style(st))
            }
            _ => None,
        };
        if let Some(sid) = new_style {
            let cur = wb.sheets[sheet].cells.get(&p).map(|c| c.style).unwrap_or(0);
            if cur != sid {
                wb.sheets[sheet].cells.entry(p).or_default().style = sid;
            }
            if let Some(c) = wb.sheets[sheet].cells.get(&p) {
                if c.is_blank() && c.style == 0 {
                    wb.sheets[sheet].cells.remove(&p);
                }
            }
        }
    }
    if matches!(opts.what, PasteWhat::All | PasteWhat::Formats | PasteWhat::AllExceptBorders) && !opts.transpose {
        let s = &mut wb.sheets[sheet];
        s.merges.retain(|m| !area.intersects(m));
        for m in &clip.merges {
            let r = Rect { top: area.top + m.top, left: area.left + m.left, bottom: area.top + m.bottom, right: area.left + m.right };
            if r.bottom < MAX_ROWS && r.right < MAX_COLS {
                s.merges.push(r);
            }
        }
    }
    if opts.what == PasteWhat::All && !opts.transpose {
        for (r, c, t) in &clip.comments {
            wb.sheets[sheet].comments.insert(CellPos::new(area.top + r, area.left + c), t.clone());
        }
    }
    area
}

/// Move the cut cells to `dst` (top-left), rewriting references that point into
/// the source area. Returns the destination rectangle.
pub fn cut_paste(wb: &mut Workbook, clip: &Clip, dst_sheet: usize, dst: CellPos) -> Rect {
    let src = clip.rect;
    let src_sheet = clip.sheet;
    let rows = src.rows();
    let cols = src.cols();
    let dest = Rect {
        top: dst.row,
        left: dst.col,
        bottom: (dst.row + rows - 1).min(MAX_ROWS - 1),
        right: (dst.col + cols - 1).min(MAX_COLS - 1),
    };
    let drow = dst.row as i64 - src.top as i64;
    let dcol = dst.col as i64 - src.left as i64;
    let src_name = wb.sheets[src_sheet].name.clone();
    // Take the source cells out (current contents, not the clip snapshot).
    let s = &mut wb.sheets[src_sheet];
    let keys: Vec<CellPos> = s.cells_in(src).map(|(p, _)| *p).collect();
    let mut moved: Vec<(CellPos, Cell)> = keys.into_iter().map(|p| (p, s.cells.remove(&p).unwrap())).collect();
    let moved_comments: Vec<(CellPos, String)> = s.comments.iter().filter(|(p, _)| src.contains(**p)).map(|(p, t)| (*p, t.clone())).collect();
    s.comments.retain(|p, _| !src.contains(*p));
    let moved_merges: Vec<Rect> = s.merges.iter().copied().filter(|m| src.contains(m.top_left()) && src.contains(m.bottom_right())).collect();
    s.merges.retain(|m| !(src.contains(m.top_left()) && src.contains(m.bottom_right())));

    if dst_sheet == src_sheet {
        // Rewrite references everywhere (including in the moved formulas).
        let names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
        wb.for_each_formula(|si, _, e| e.move_refs(&names[si], &src_name, src, drow, dcol));
        for (_, c) in moved.iter_mut() {
            if let Some(f) = c.formula.as_mut() {
                f.move_refs(&src_name, &src_name, src, drow, dcol);
            }
        }
        for n in wb.names.iter_mut() {
            let fs = n.scope.map(|i| names[i].clone()).unwrap_or_default();
            n.formula.move_refs(&fs, &src_name, src, drow, dcol);
        }
    } else {
        // Moving to another sheet: keep moved formulas pointing at the source sheet.
        for (_, c) in moved.iter_mut() {
            if let Some(f) = c.formula.as_mut() {
                f.walk_refs_mut(&mut |e| {
                    if let Expr::Ref(r) | Expr::SpillRef(r) = e {
                        if r.sheet.is_none() {
                            r.sheet = Some(src_name.clone());
                        }
                    }
                });
            }
        }
    }
    let d = &mut wb.sheets[dst_sheet];
    let keys: Vec<CellPos> = d.cells_in(dest).map(|(p, _)| *p).collect();
    for k in keys {
        d.cells.remove(&k);
    }
    d.merges.retain(|m| !dest.intersects(m));
    for (p, c) in moved {
        let np = CellPos::new((p.row as i64 + drow) as u32, (p.col as i64 + dcol) as u32);
        if np.row < MAX_ROWS && np.col < MAX_COLS {
            d.cells.insert(np, c);
        }
    }
    for (p, t) in moved_comments {
        d.comments.insert(CellPos::new((p.row as i64 + drow) as u32, (p.col as i64 + dcol) as u32), t);
    }
    for m in moved_merges {
        d.merges.push(Rect {
            top: (m.top as i64 + drow) as u32,
            left: (m.left as i64 + dcol) as u32,
            bottom: (m.bottom as i64 + drow) as u32,
            right: (m.right as i64 + dcol) as u32,
        });
    }
    dest
}

// =====================================================================
// Fill
// =====================================================================

const DAYS_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAYS_LONG: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS_SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTHS_LONG: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
];

#[derive(Clone, Copy, PartialEq)]
enum Case {
    Title,
    Upper,
    Lower,
}

fn case_of(s: &str) -> Case {
    if s.chars().all(|c| !c.is_lowercase()) {
        Case::Upper
    } else if s.chars().all(|c| !c.is_uppercase()) {
        Case::Lower
    } else {
        Case::Title
    }
}

fn apply_case(s: &str, c: Case) -> String {
    match c {
        Case::Title => s.to_string(),
        Case::Upper => s.to_uppercase(),
        Case::Lower => s.to_lowercase(),
    }
}

/// A recognised list item: (list, index, case).
fn list_item(s: &str) -> Option<(&'static [&'static str], usize, Case)> {
    let t = s.trim();
    for list in [&DAYS_LONG[..], &DAYS_SHORT[..], &MONTHS_LONG[..], &MONTHS_SHORT[..]] {
        if let Some(i) = list.iter().position(|x| x.eq_ignore_ascii_case(t)) {
            return Some((list, i, case_of(t)));
        }
    }
    None
}

/// Split "Item 12" into ("Item ", 12, digits width).
fn text_with_number(s: &str) -> Option<(String, i64, usize, String)> {
    // trailing number first, else leading number
    let bytes: Vec<char> = s.chars().collect();
    let mut end = bytes.len();
    while end > 0 && !bytes[end - 1].is_ascii_digit() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    let digits: String = bytes[start..end].iter().collect();
    if digits.len() > 15 {
        return None;
    }
    let prefix: String = bytes[..start].iter().collect();
    let suffix: String = bytes[end..].iter().collect();
    if !suffix.is_empty() && !prefix.is_empty() {
        // Only one of prefix/suffix may be non-empty for a pattern like "Q1 2024"? Accept trailing number only.
        return None;
    }
    Some((prefix, digits.parse().ok()?, digits.len(), suffix))
}

/// Source item along a fill line.
#[derive(Clone)]
struct Src {
    cell: Option<Cell>,
    pos: CellPos,
}

fn linear_fit(ys: &[f64]) -> (f64, f64) {
    // returns (intercept at index 0, step)
    let n = ys.len() as f64;
    if ys.len() == 1 {
        return (ys[0], 0.0);
    }
    let mx = (n - 1.0) / 2.0;
    let my = ys.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, y) in ys.iter().enumerate() {
        let dx = i as f64 - mx;
        num += dx * (y - my);
        den += dx * dx;
    }
    let step = if den == 0.0 { 0.0 } else { num / den };
    (my - step * mx, step)
}

/// Compute the cell for position `k` (relative index; k >= n continues after the
/// source, k < 0 goes before it) of a fill line.
fn series_cells(wb: &Workbook, srcs: &[Src], ks: &[i64], dest: &[CellPos]) -> Vec<Option<Cell>> {
    let n = srcs.len() as i64;
    let vals: Vec<Option<&Cell>> = srcs.iter().map(|s| s.cell.as_ref()).collect();
    let is_num = |c: &Option<&Cell>| matches!(c, Some(c) if c.formula.is_none() && matches!(c.value, Value::Number(_)));
    let all_numbers = vals.iter().all(is_num);
    let style_of = |c: &Option<&Cell>| c.map(|c| c.style).unwrap_or(0);
    let mut out = Vec::with_capacity(ks.len());
    if all_numbers && !vals.is_empty() {
        let ys: Vec<f64> = vals.iter().map(|c| if let Value::Number(x) = c.unwrap().value { x } else { 0.0 }).collect();
        let is_date = vals.iter().any(|c| crate::numfmt::is_date_format(&wb.style(c.unwrap().style).num_fmt));
        let (b, step) = if ys.len() == 1 {
            (ys[0], if is_date { 1.0 } else { 0.0 })
        } else {
            linear_fit(&ys)
        };
        for &k in ks {
            let src = &vals[k.rem_euclid(n) as usize];
            let v = b + step * k as f64;
            out.push(Some(Cell { formula: None, value: Value::number(round15(v)), style: style_of(src) }));
        }
        return out;
    }
    // list items (days / months)
    let items: Vec<Option<(&[&str], usize, Case)>> =
        vals.iter().map(|c| c.and_then(|c| if let Value::Text(t) = &c.value { if c.formula.is_none() { list_item(t) } else { None } } else { None })).collect();
    if !items.is_empty() && items.iter().all(|i| i.is_some()) {
        let first = items[0].unwrap();
        let same_list = items.iter().all(|i| std::ptr::eq(i.unwrap().0.as_ptr(), first.0.as_ptr()));
        if same_list {
            let len = first.0.len() as i64;
            let step = if items.len() >= 2 { items[1].unwrap().1 as i64 - first.1 as i64 } else { 1 };
            for &k in ks {
                let src = &vals[k.rem_euclid(n) as usize];
                let idx = (first.1 as i64 + step * k).rem_euclid(len) as usize;
                out.push(Some(Cell {
                    formula: None,
                    value: Value::Text(apply_case(first.0[idx], first.2)),
                    style: style_of(src),
                }));
            }
            return out;
        }
    }
    // "Item 1" patterns
    let pats: Vec<Option<(String, i64, usize, String)>> =
        vals.iter().map(|c| c.and_then(|c| if let (None, Value::Text(t)) = (&c.formula, &c.value) { text_with_number(t) } else { None })).collect();
    if !pats.is_empty() && pats.iter().all(|p| p.is_some()) {
        let p0 = pats[0].clone().unwrap();
        if pats.iter().all(|p| {
            let p = p.as_ref().unwrap();
            p.0 == p0.0 && p.3 == p0.3
        }) {
            let ys: Vec<f64> = pats.iter().map(|p| p.as_ref().unwrap().1 as f64).collect();
            let (b, step) = if ys.len() == 1 { (ys[0], 1.0) } else { linear_fit(&ys) };
            for &k in ks {
                let src = &vals[k.rem_euclid(n) as usize];
                let num = (b + step * k as f64).round() as i64;
                let digits = if p0.2 > 1 && num >= 0 { format!("{:0width$}", num, width = p0.2) } else { num.abs().to_string() };
                out.push(Some(Cell {
                    formula: None,
                    value: Value::Text(format!("{}{}{}", p0.0, digits, p0.3)),
                    style: style_of(src),
                }));
            }
            return out;
        }
    }
    // Copy cyclically (formulas shifted, text-with-number cells incremented individually).
    for (i, &k) in ks.iter().enumerate() {
        let si = k.rem_euclid(n) as usize;
        let src = &srcs[si];
        let cell = src.cell.as_ref().map(|c| {
            let mut c = c.clone();
            if let Some(f) = c.formula.as_mut() {
                let d = dest[i];
                f.shift_relative(d.row as i64 - src.pos.row as i64, d.col as i64 - src.pos.col as i64);
                c.value = Value::Empty;
            } else if let Value::Text(t) = &c.value {
                if let Some((pre, num, w, suf)) = text_with_number(t) {
                    let cycle = k.div_euclid(n);
                    let nn = num + cycle;
                    let digits = if w > 1 && nn >= 0 { format!("{:0width$}", nn, width = w) } else { nn.to_string() };
                    c.value = Value::Text(format!("{}{}{}", pre, digits, suf));
                }
            }
            c
        });
        out.push(cell);
    }
    out
}

fn round15(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let mag = v.abs().log10().floor() as i32;
    let scale = 10f64.powi(14 - mag);
    if !scale.is_finite() || scale == 0.0 {
        return v;
    }
    (v * scale).round() / scale
}

/// Fill handle: extend the source block `src` to `dst` (which must contain `src`
/// and extend it in exactly one direction). If `dst` is smaller than `src`
/// (dragged back inside), the excess source cells are cleared.
pub fn fill(wb: &mut Workbook, sheet: usize, src: Rect, dst: Rect) {
    if dst == src {
        return;
    }
    if src.contains(dst.top_left()) && src.contains(dst.bottom_right()) {
        // shrink: clear cells of src not in dst
        for r in src.top..=src.bottom {
            for c in src.left..=src.right {
                let p = CellPos::new(r, c);
                if !dst.contains(p) {
                    wb.clear(sheet, Rect::single(p), true, false);
                }
            }
        }
        return;
    }
    let vertical = dst.top < src.top || dst.bottom > src.bottom;
    let get = |wb: &Workbook, p: CellPos| -> Src { Src { cell: wb.sheets[sheet].cells.get(&p).cloned(), pos: p } };
    if vertical {
        for col in src.left..=src.right {
            let srcs: Vec<Src> = (src.top..=src.bottom).map(|r| get(wb, CellPos::new(r, col))).collect();
            let mut ks = Vec::new();
            let mut dest = Vec::new();
            for r in dst.top..=dst.bottom {
                if r >= src.top && r <= src.bottom {
                    continue;
                }
                ks.push(r as i64 - src.top as i64);
                dest.push(CellPos::new(r, col));
            }
            let cells = series_cells(wb, &srcs, &ks, &dest);
            put_cells(wb, sheet, &dest, cells);
        }
    } else {
        for row in src.top..=src.bottom {
            let srcs: Vec<Src> = (src.left..=src.right).map(|c| get(wb, CellPos::new(row, c))).collect();
            let mut ks = Vec::new();
            let mut dest = Vec::new();
            for c in dst.left..=dst.right {
                if c >= src.left && c <= src.right {
                    continue;
                }
                ks.push(c as i64 - src.left as i64);
                dest.push(CellPos::new(row, c));
            }
            let cells = series_cells(wb, &srcs, &ks, &dest);
            put_cells(wb, sheet, &dest, cells);
        }
    }
}

fn put_cells(wb: &mut Workbook, sheet: usize, dest: &[CellPos], cells: Vec<Option<Cell>>) {
    let s = &mut wb.sheets[sheet];
    for (p, c) in dest.iter().zip(cells) {
        match c {
            Some(c) if !(c.is_blank() && c.style == 0) => {
                s.cells.insert(*p, c);
            }
            _ => {
                s.cells.remove(p);
            }
        }
    }
}

/// Ctrl+D: copy the top row of `rect` down (or the row above for a single-row selection).
pub fn fill_down(wb: &mut Workbook, sheet: usize, rect: Rect) {
    let (src_row, start) = if rect.rows() == 1 {
        if rect.top == 0 {
            return;
        }
        (rect.top - 1, rect.top)
    } else {
        (rect.top, rect.top + 1)
    };
    for c in rect.left..=rect.right {
        let src = wb.sheets[sheet].cells.get(&CellPos::new(src_row, c)).cloned();
        for r in start..=rect.bottom {
            copy_cell_to(wb, sheet, &src, CellPos::new(src_row, c), CellPos::new(r, c));
        }
    }
}

/// Ctrl+R: copy the left column of `rect` right (or the column to the left for a single column).
pub fn fill_right(wb: &mut Workbook, sheet: usize, rect: Rect) {
    let (src_col, start) = if rect.cols() == 1 {
        if rect.left == 0 {
            return;
        }
        (rect.left - 1, rect.left)
    } else {
        (rect.left, rect.left + 1)
    };
    for r in rect.top..=rect.bottom {
        let src = wb.sheets[sheet].cells.get(&CellPos::new(r, src_col)).cloned();
        for c in start..=rect.right {
            copy_cell_to(wb, sheet, &src, CellPos::new(r, src_col), CellPos::new(r, c));
        }
    }
}

fn copy_cell_to(wb: &mut Workbook, sheet: usize, src: &Option<Cell>, from: CellPos, to: CellPos) {
    let s = &mut wb.sheets[sheet];
    match src {
        None => {
            s.cells.remove(&to);
        }
        Some(c) => {
            let mut c = c.clone();
            if let Some(f) = c.formula.as_mut() {
                f.shift_relative(to.row as i64 - from.row as i64, to.col as i64 - from.col as i64);
                c.value = Value::Empty;
            }
            s.cells.insert(to, c);
        }
    }
}

/// Last row of contiguous data in the column(s) next to `rect` (for double-clicking the fill handle).
pub fn fill_down_extent(wb: &Workbook, sheet: usize, rect: Rect) -> Option<u32> {
    let s = &wb.sheets[sheet];
    let has = |p: CellPos| !s.value_at(p).is_empty();
    let mut candidates = Vec::new();
    if rect.left > 0 {
        candidates.push(rect.left - 1);
    }
    if rect.right + 1 < MAX_COLS {
        candidates.push(rect.right + 1);
    }
    for col in candidates {
        if !has(CellPos::new(rect.bottom, col)) && !has(CellPos::new(rect.bottom + 1, col)) {
            continue;
        }
        let mut r = rect.bottom + 1;
        while r < MAX_ROWS && has(CellPos::new(r, col)) {
            r += 1;
        }
        if r - 1 > rect.bottom {
            return Some(r - 1);
        }
    }
    None
}

// =====================================================================
// Sort
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SortKey {
    /// Absolute column index.
    pub col: u32,
    pub ascending: bool,
}

fn sort_cmp(a: &Value, b: &Value, ascending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    // blanks always last
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let o = match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Text(x), Value::Text(y)) => compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Error(_), Value::Error(_)) => Ordering::Equal,
        _ => a.type_rank().cmp(&b.type_rank()),
    };
    if ascending {
        o
    } else {
        o.reverse()
    }
}

/// Sort the rows of `rect` by `keys`. With `has_header` the first row stays put.
pub fn sort(wb: &mut Workbook, sheet: usize, rect: Rect, keys: &[SortKey], has_header: bool) {
    let top = if has_header { rect.top + 1 } else { rect.top };
    if top > rect.bottom || keys.is_empty() {
        return;
    }
    let s = &wb.sheets[sheet];
    // Bound by data to avoid sorting a million empty rows.
    let bottom = match s.data_extent() {
        Some(e) => e.bottom.min(rect.bottom),
        None => return,
    };
    if bottom < top {
        return;
    }
    let mut rows: Vec<(u32, Vec<Value>)> = (top..=bottom)
        .map(|r| (r, keys.iter().map(|k| s.value_at(CellPos::new(r, k.col))).collect()))
        .collect();
    rows.sort_by(|a, b| {
        for (i, k) in keys.iter().enumerate() {
            let o = sort_cmp(&a.1[i], &b.1[i], k.ascending);
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    let area = Rect { top, left: rect.left, bottom, right: rect.right };
    let s = &mut wb.sheets[sheet];
    let mut old: BTreeMap<CellPos, Cell> = BTreeMap::new();
    let keys_in: Vec<CellPos> = s.cells_in(area).map(|(p, _)| *p).collect();
    for k in keys_in {
        old.insert(k, s.cells.remove(&k).unwrap());
    }
    let old_heights: Vec<Option<f32>> = (top..=bottom).map(|r| s.row_heights.get(&r).copied()).collect();
    for (new_i, (old_row, _)) in rows.iter().enumerate() {
        let new_row = top + new_i as u32;
        for c in rect.left..=rect.right.min(MAX_COLS - 1) {
            if let Some(mut cell) = old.remove(&CellPos::new(*old_row, c)) {
                if let Some(f) = cell.formula.as_mut() {
                    f.shift_relative(new_row as i64 - *old_row as i64, 0);
                    cell.value = Value::Empty;
                }
                s.cells.insert(CellPos::new(new_row, c), cell);
            }
        }
        // whole-row sorts carry row heights along
        if rect.left == 0 && rect.right == MAX_COLS - 1 {
            match old_heights[(*old_row - top) as usize] {
                Some(h) => {
                    s.row_heights.insert(new_row, h);
                }
                None => {
                    s.row_heights.remove(&new_row);
                }
            }
        }
    }
}

/// Guess whether the first row of `rect` is a header (text over non-text data).
pub fn guess_header(wb: &Workbook, sheet: usize, rect: Rect) -> bool {
    if rect.rows() < 2 {
        return false;
    }
    let s = &wb.sheets[sheet];
    let mut header_text = 0;
    let mut differs = false;
    for c in rect.left..=rect.right.min(rect.left + 50) {
        let a = s.value_at(CellPos::new(rect.top, c));
        let b = s.value_at(CellPos::new(rect.top + 1, c));
        if matches!(a, Value::Text(_)) {
            header_text += 1;
            if !matches!(b, Value::Text(_)) && !b.is_empty() {
                differs = true;
            }
        }
        let sa = wb.cell_style(sheet, CellPos::new(rect.top, c));
        let sb = wb.cell_style(sheet, CellPos::new(rect.top + 1, c));
        if sa.bold && !sb.bold {
            differs = true;
        }
    }
    header_text > 0 && differs
}

/// Expand a single cell to the surrounding contiguous data region (Ctrl+A / sort / filter).
pub fn current_region(wb: &Workbook, sheet: usize, p: CellPos) -> Rect {
    let s = &wb.sheets[sheet];
    let filled = |r: u32, c: u32| !s.value_at(CellPos::new(r, c)).is_empty();
    let mut r = Rect::single(p);
    loop {
        let mut grown = false;
        // try each side
        if r.top > 0 && (r.left.saturating_sub(1)..=(r.right + 1).min(MAX_COLS - 1)).any(|c| filled(r.top - 1, c)) {
            r.top -= 1;
            grown = true;
        }
        if r.bottom + 1 < MAX_ROWS && (r.left.saturating_sub(1)..=(r.right + 1).min(MAX_COLS - 1)).any(|c| filled(r.bottom + 1, c)) {
            r.bottom += 1;
            grown = true;
        }
        if r.left > 0 && (r.top.saturating_sub(1)..=(r.bottom + 1).min(MAX_ROWS - 1)).any(|rr| filled(rr, r.left - 1)) {
            r.left -= 1;
            grown = true;
        }
        if r.right + 1 < MAX_COLS && (r.top.saturating_sub(1)..=(r.bottom + 1).min(MAX_ROWS - 1)).any(|rr| filled(rr, r.right + 1)) {
            r.right += 1;
            grown = true;
        }
        if !grown || r.cell_count() > 50_000_000 {
            break;
        }
    }
    r
}

// =====================================================================
// Find & replace
// =====================================================================

#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    pub query: String,
    pub match_case: bool,
    pub entire_cell: bool,
    /// Search formulas (input text) instead of displayed values.
    pub in_formulas: bool,
    /// Limit to one sheet (None = whole workbook).
    pub sheet: Option<usize>,
}

fn haystack(wb: &Workbook, sheet: usize, p: CellPos, in_formulas: bool) -> String {
    if in_formulas {
        wb.input_text(sheet, p)
    } else {
        display_text(wb, sheet, p)
    }
}

fn has_wild(q: &str) -> bool {
    q.contains('*') || q.contains('?')
}

/// Does the text match the query?
pub fn text_matches(text: &str, opts: &FindOptions) -> bool {
    if opts.query.is_empty() {
        return false;
    }
    if has_wild(&opts.query) {
        let pat = if opts.entire_cell { opts.query.clone() } else { format!("*{}*", opts.query) };
        if !crate::criteria::wildcard_match(&pat, text) {
            return false;
        }
        if opts.match_case {
            // wildcard matching is case-insensitive; require literal parts to match exactly
            return opts.query.split(['*', '?']).all(|part| text.contains(part));
        }
        return true;
    }
    if opts.entire_cell {
        if opts.match_case {
            text == opts.query
        } else {
            text.to_lowercase() == opts.query.to_lowercase()
        }
    } else if opts.match_case {
        text.contains(&opts.query)
    } else {
        text.to_lowercase().contains(&opts.query.to_lowercase())
    }
}

pub fn cell_matches(wb: &Workbook, sheet: usize, p: CellPos, opts: &FindOptions) -> bool {
    text_matches(&haystack(wb, sheet, p, opts.in_formulas), opts)
}

/// All matching cells, in sheet order then row-major order.
pub fn find_all(wb: &Workbook, opts: &FindOptions) -> Vec<(usize, CellPos)> {
    let mut out = Vec::new();
    for (si, s) in wb.sheets.iter().enumerate() {
        if let Some(only) = opts.sheet {
            if only != si {
                continue;
            }
        }
        let mut positions: Vec<CellPos> = s.cells.iter().filter(|(_, c)| !c.is_blank()).map(|(p, _)| *p).collect();
        if !opts.in_formulas {
            positions.extend(s.spill_owner.keys().copied());
            positions.sort();
        }
        for p in positions {
            if cell_matches(wb, si, p, opts) {
                out.push((si, p));
            }
        }
    }
    out
}

/// Next match after `from` (wrapping around).
pub fn find_next(wb: &Workbook, opts: &FindOptions, from: (usize, CellPos)) -> Option<(usize, CellPos)> {
    let all = find_all(wb, opts);
    if all.is_empty() {
        return None;
    }
    all.iter().copied().find(|&(s, p)| (s, p) > from).or(Some(all[0]))
}

fn replace_text(text: &str, opts: &FindOptions, with: &str) -> String {
    if opts.entire_cell || has_wild(&opts.query) {
        return with.to_string();
    }
    if opts.match_case {
        return text.replace(&opts.query, with);
    }
    // case-insensitive replace
    let lower = text.to_lowercase();
    let q = opts.query.to_lowercase();
    let mut out = String::new();
    let mut i = 0;
    while let Some(j) = lower[i..].find(&q) {
        let start = i + j;
        out.push_str(&text[i..start]);
        out.push_str(with);
        i = start + q.len();
        if q.is_empty() {
            break;
        }
    }
    out.push_str(&text[i..]);
    out
}

/// Replace in one cell; returns true if it changed. Does not recalculate.
pub fn replace_in_cell(wb: &mut Workbook, sheet: usize, p: CellPos, opts: &FindOptions, with: &str) -> bool {
    let input = wb.input_text(sheet, p);
    let probe = FindOptions { in_formulas: true, ..opts.clone() };
    if !text_matches(&input, &probe) {
        return false;
    }
    let new = replace_text(&input, opts, with);
    if new == input {
        return false;
    }
    if wb.set_input(sheet, p, &new).is_err() {
        wb.set_value(sheet, p, Value::Text(new));
    }
    true
}

/// Replace every match; returns the number of cells changed. Does not recalculate.
pub fn replace_all(wb: &mut Workbook, opts: &FindOptions, with: &str) -> usize {
    let found = find_all(wb, &FindOptions { in_formulas: true, ..opts.clone() });
    let mut n = 0;
    for (s, p) in found {
        if replace_in_cell(wb, s, p, opts, with) {
            n += 1;
        }
    }
    n
}

// =====================================================================
// Data tools
// =====================================================================

/// Remove rows of `rect` whose values in `cols` duplicate an earlier row.
/// Returns the number of rows removed. Remaining rows move up within `rect`.
pub fn remove_duplicates(wb: &mut Workbook, sheet: usize, rect: Rect, cols: &[u32], has_header: bool) -> usize {
    let top = if has_header { rect.top + 1 } else { rect.top };
    let s = &wb.sheets[sheet];
    let bottom = match s.data_extent() {
        Some(e) => e.bottom.min(rect.bottom),
        None => return 0,
    };
    if bottom < top {
        return 0;
    }
    let mut seen = HashSet::new();
    let mut keep = Vec::new();
    for r in top..=bottom {
        let key: Vec<String> = cols
            .iter()
            .map(|c| match s.value_at(CellPos::new(r, *c)) {
                Value::Text(t) => format!("t{}", t.to_lowercase()),
                v => format!("v{}", v.display()),
            })
            .collect();
        if seen.insert(key) {
            keep.push(r);
        }
    }
    let removed = (bottom - top + 1) as usize - keep.len();
    if removed == 0 {
        return 0;
    }
    let area = Rect { top, left: rect.left, bottom, right: rect.right };
    let s = &mut wb.sheets[sheet];
    let mut old: BTreeMap<CellPos, Cell> = BTreeMap::new();
    let keys: Vec<CellPos> = s.cells_in(area).map(|(p, _)| *p).collect();
    for k in keys {
        old.insert(k, s.cells.remove(&k).unwrap());
    }
    for (i, r) in keep.iter().enumerate() {
        let nr = top + i as u32;
        for c in rect.left..=rect.right {
            if let Some(mut cell) = old.remove(&CellPos::new(*r, c)) {
                if let Some(f) = cell.formula.as_mut() {
                    f.shift_relative(nr as i64 - *r as i64, 0);
                }
                s.cells.insert(CellPos::new(nr, c), cell);
            }
        }
    }
    removed
}

/// Split the text in the first column of `rect` by any of `delims` into columns to the right.
pub fn text_to_columns(wb: &mut Workbook, sheet: usize, rect: Rect, delims: &[char], treat_consecutive_as_one: bool) -> Rect {
    let col = rect.left;
    let mut max_w = 1;
    let bottom = wb.sheets[sheet].data_extent().map(|e| e.bottom.min(rect.bottom)).unwrap_or(rect.top);
    for r in rect.top..=bottom {
        let p = CellPos::new(r, col);
        let v = wb.value(sheet, p);
        let Value::Text(t) = v else { continue };
        let mut parts: Vec<&str> = t.split(|c| delims.contains(&c)).collect();
        if treat_consecutive_as_one {
            parts.retain(|p| !p.is_empty());
        }
        max_w = max_w.max(parts.len() as u32);
        for (i, part) in parts.iter().enumerate() {
            let q = CellPos::new(r, col + i as u32);
            if q.col >= MAX_COLS {
                break;
            }
            if wb.set_input(sheet, q, part.trim()).is_err() {
                wb.set_value(sheet, q, Value::Text(part.to_string()));
            }
        }
    }
    Rect { top: rect.top, left: col, bottom, right: (col + max_w - 1).min(MAX_COLS - 1) }
}

/// Columns a text-to-columns split could write to (for undo scope).
pub fn text_to_columns_extent(wb: &Workbook, sheet: usize, rect: Rect, delims: &[char]) -> Rect {
    let mut max_w = 1;
    let bottom = wb.sheets[sheet].data_extent().map(|e| e.bottom.min(rect.bottom)).unwrap_or(rect.top);
    for r in rect.top..=bottom {
        if let Value::Text(t) = wb.value(sheet, CellPos::new(r, rect.left)) {
            max_w = max_w.max(t.split(|c| delims.contains(&c)).count() as u32);
        }
    }
    Rect { top: rect.top, left: rect.left, bottom: bottom.max(rect.top), right: (rect.left + max_w - 1).min(MAX_COLS - 1) }
}

/// Goal Seek: change `changing` until `set_cell` evaluates to `target`.
/// Returns the found input value; the workbook is left with that value.
pub fn goal_seek(wb: &mut Workbook, sheet: usize, set_cell: CellPos, target: f64, changing: CellPos) -> Result<f64, String> {
    if wb.cell(sheet, set_cell).and_then(|c| c.formula.as_ref()).is_none() {
        return Err("Set cell must contain a formula.".into());
    }
    if wb.cell(sheet, changing).map(|c| c.formula.is_some()).unwrap_or(false) {
        return Err("By changing cell must contain a value.".into());
    }
    let original = wb.value(sheet, changing);
    let f = |wb: &mut Workbook, x: f64| -> Option<f64> {
        wb.set_value(sheet, changing, Value::Number(x));
        wb.recalculate();
        match wb.value(sheet, set_cell) {
            Value::Number(y) => Some(y - target),
            _ => None,
        }
    };
    let x0 = original.to_number().unwrap_or(0.0);
    let mut a = x0;
    let mut b = if x0 == 0.0 { 1.0 } else { x0 * 1.1 };
    let mut fa = f(wb, a);
    let tol = 1e-9 * target.abs().max(1.0);
    if let Some(v) = fa {
        if v.abs() < tol {
            return Ok(a);
        }
    }
    let mut fb = f(wb, b);
    // secant iterations
    for _ in 0..200 {
        match (fa, fb) {
            (Some(ya), Some(yb)) => {
                if yb.abs() < tol {
                    return Ok(b);
                }
                let denom = yb - ya;
                let next = if denom.abs() < 1e-300 { b + (b - a).abs().max(1e-6) } else { b - yb * (b - a) / denom };
                if !next.is_finite() {
                    break;
                }
                a = b;
                fa = fb;
                b = next;
                fb = f(wb, b);
            }
            _ => {
                // nudge away from an error region
                b = (a + b) / 2.0 + 0.5;
                fb = f(wb, b);
                if fa.is_none() {
                    a = b + 1.0;
                    fa = f(wb, a);
                }
            }
        }
    }
    // Fallback: bracket and bisect
    let mut lo = -1.0f64;
    let mut hi = 1.0f64;
    for _ in 0..60 {
        if let (Some(l), Some(h)) = (f(wb, lo), f(wb, hi)) {
            if l.signum() != h.signum() {
                for _ in 0..200 {
                    let mid = (lo + hi) / 2.0;
                    let Some(m) = f(wb, mid) else { break };
                    if m.abs() < tol {
                        return Ok(mid);
                    }
                    if m.signum() == l.signum() {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                return Ok((lo + hi) / 2.0);
            }
        }
        lo *= 2.0;
        hi *= 2.0;
    }
    wb.set_value(sheet, changing, original);
    wb.recalculate();
    Err("Goal Seek could not find a solution.".into())
}

/// The range AutoSum would use for a formula at `p`: numbers above, else to the left.
pub fn autosum_range(wb: &Workbook, sheet: usize, p: CellPos) -> Option<Rect> {
    let s = &wb.sheets[sheet];
    let is_num = |q: CellPos| matches!(s.value_at(q), Value::Number(_));
    let filled = |q: CellPos| !s.value_at(q).is_empty();
    // above
    if p.row > 0 && filled(CellPos::new(p.row - 1, p.col)) {
        let mut top = p.row - 1;
        // skip a text header immediately above the numbers
        while top > 0 && is_num(CellPos::new(top - 1, p.col)) {
            top -= 1;
        }
        if is_num(CellPos::new(top, p.col)) {
            return Some(Rect { top, left: p.col, bottom: p.row - 1, right: p.col });
        }
    }
    if p.col > 0 && filled(CellPos::new(p.row, p.col - 1)) {
        let mut left = p.col - 1;
        while left > 0 && is_num(CellPos::new(p.row, left - 1)) {
            left -= 1;
        }
        if is_num(CellPos::new(p.row, left)) {
            return Some(Rect { top: p.row, left, bottom: p.row, right: p.col - 1 });
        }
    }
    None
}

// =====================================================================
// Data validation
// =====================================================================

/// Allowed values of a list validation rule.
pub fn validation_list(wb: &Workbook, sheet: usize, rule: &ValidationRule) -> Vec<String> {
    match rule {
        ValidationRule::List(v) => v.clone(),
        ValidationRule::ListRange(r) => {
            let mut out = Vec::new();
            let bottom = r.bottom.min(r.top + 1000);
            for row in r.top..=bottom {
                for c in r.left..=r.right.min(r.left + 50) {
                    let t = display_text(wb, sheet, CellPos::new(row, c));
                    if !t.is_empty() {
                        out.push(t);
                    }
                }
            }
            out
        }
        _ => vec![],
    }
}

/// The validation rule covering a cell.
pub fn validation_at(wb: &Workbook, sheet: usize, p: CellPos) -> Option<&ValidationRule> {
    wb.sheets[sheet].validations.iter().rev().find(|v| v.range.contains(p)).map(|v| &v.rule)
}

/// Check typed input against the validation rule for the cell.
pub fn validate_input(wb: &Workbook, sheet: usize, p: CellPos, input: &str) -> Result<(), String> {
    let Some(rule) = validation_at(wb, sheet, p) else { return Ok(()) };
    if input.is_empty() || input.starts_with('=') {
        return Ok(());
    }
    let parsed = crate::workbook::parse_input(input).map_err(|e| e.to_string())?;
    let num = match parsed.value {
        Value::Number(n) => Some(n),
        _ => None,
    };
    let fmt = |x: f64| crate::value::format_number_general(x);
    let bounds = |min: f64, max: f64| -> String {
        match (min.is_finite(), max.is_finite()) {
            (true, true) => format!("between {} and {}", fmt(min), fmt(max)),
            (true, false) => format!("greater than or equal to {}", fmt(min)),
            (false, true) => format!("less than or equal to {}", fmt(max)),
            _ => "valid".into(),
        }
    };
    match rule {
        ValidationRule::List(_) | ValidationRule::ListRange(_) => {
            let list = validation_list(wb, sheet, rule);
            if list.iter().any(|x| x.eq_ignore_ascii_case(input.trim())) {
                Ok(())
            } else {
                Err("This value doesn't match the data validation restrictions defined for this cell.".into())
            }
        }
        ValidationRule::WholeNumber { min, max } => match num {
            Some(n) if n.fract() == 0.0 && n >= *min && n <= *max => Ok(()),
            _ => Err(format!("The value must be a whole number {}.", bounds(*min, *max))),
        },
        ValidationRule::Decimal { min, max } => match num {
            Some(n) if n >= *min && n <= *max => Ok(()),
            _ => Err(format!("The value must be a number {}.", bounds(*min, *max))),
        },
        ValidationRule::TextLength { min, max } => {
            let l = input.chars().count() as f64;
            if l >= *min && l <= *max {
                Ok(())
            } else {
                Err(format!("The text length must be {}.", bounds(*min, *max)))
            }
        }
    }
}

// =====================================================================
// Navigation helpers
// =====================================================================

/// Ctrl+Arrow: jump to the edge of the data region in direction (dr, dc).
pub fn data_edge(wb: &Workbook, sheet: usize, from: CellPos, dr: i32, dc: i32) -> CellPos {
    let s = &wb.sheets[sheet];
    let filled = |p: CellPos| !s.value_at(p).is_empty();
    let step = |p: CellPos| -> Option<CellPos> {
        let r = p.row as i64 + dr as i64;
        let c = p.col as i64 + dc as i64;
        if r < 0 || c < 0 || r >= MAX_ROWS as i64 || c >= MAX_COLS as i64 {
            None
        } else {
            Some(CellPos::new(r as u32, c as u32))
        }
    };
    let Some(next) = step(from) else { return from };
    let ext = s.data_extent();
    if filled(from) && filled(next) {
        // move to the last filled cell
        let mut cur = next;
        while let Some(n) = step(cur) {
            if !filled(n) {
                break;
            }
            cur = n;
        }
        return cur;
    }
    // move to the next filled cell, or the sheet edge
    let mut cur = next;
    loop {
        if filled(cur) {
            return cur;
        }
        // beyond the data extent nothing can be found: go to the edge
        if let Some(e) = ext {
            let beyond = (dr > 0 && cur.row > e.bottom) || (dc > 0 && cur.col > e.right) || (dr < 0 && cur.row < e.top) || (dc < 0 && cur.col < e.left);
            if beyond {
                break;
            }
        } else {
            break;
        }
        match step(cur) {
            Some(n) => cur = n,
            None => return cur,
        }
    }
    let r = if dr > 0 { MAX_ROWS - 1 } else if dr < 0 { 0 } else { from.row };
    let c = if dc > 0 { MAX_COLS - 1 } else if dc < 0 { 0 } else { from.col };
    CellPos::new(r, c)
}
