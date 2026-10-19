//! Application state, the frame loop, keyboard handling and cell editing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use egui::{Event, Key, Modifiers};
use engine::address::{CellPos, Rect, MAX_COLS, MAX_ROWS};
use engine::condfmt::CfFormat;
use engine::ops::{self, Clip, Scope, UndoStack};
use engine::{Value, Workbook};

use crate::dialogs::Dialog;
use crate::editor;
use crate::Args;

pub const EDIT_CELL_ID: &str = "rexcel-cell-editor";
pub const EDIT_BAR_ID: &str = "rexcel-formula-bar";

pub fn cell_editor_id() -> egui::Id {
    egui::Id::unique(EDIT_CELL_ID)
}
pub fn bar_editor_id() -> egui::Id {
    egui::Id::unique(EDIT_BAR_ID)
}

/// The open document.
pub struct Doc {
    pub wb: Workbook,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub undo: UndoStack,
}

impl Doc {
    pub fn new() -> Doc {
        Doc { wb: Workbook::new(), path: None, dirty: false, undo: UndoStack::new() }
    }
    pub fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Book1".into())
    }
}

/// The selected ranges and the active cell.
#[derive(Debug, Clone)]
pub struct Selection {
    pub ranges: Vec<Rect>,
    pub active: CellPos,
    /// Fixed corner of the last range (for Shift+click / Shift+arrows).
    pub anchor: CellPos,
    /// Moving corner of the last range.
    pub ext: CellPos,
}

impl Selection {
    pub fn single(p: CellPos) -> Selection {
        Selection { ranges: vec![Rect::single(p)], active: p, anchor: p, ext: p }
    }
    pub fn primary(&self) -> Rect {
        *self.ranges.last().unwrap()
    }
    pub fn contains(&self, p: CellPos) -> bool {
        self.ranges.iter().any(|r| r.contains(p))
    }
    pub fn is_single_cell(&self) -> bool {
        self.ranges.len() == 1 && self.ranges[0].cell_count() == 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Started by typing: arrow keys commit (or point in formulas).
    Enter,
    /// F2 / double click / formula bar: arrow keys move the caret.
    Edit,
    /// Arrow keys / clicks insert references into a formula.
    Point,
}

#[derive(Debug, Clone)]
pub struct PointRef {
    pub start: usize,
    pub end: usize,
    pub anchor: CellPos,
    pub cur: CellPos,
    pub sheet: usize,
}

#[derive(Debug, Clone)]
pub struct EditState {
    pub sheet: usize,
    pub cell: CellPos,
    pub text: String,
    pub original: String,
    pub mode: EditMode,
    pub in_bar: bool,
    pub cursor: usize,
    pub point: Option<PointRef>,
    pub request_focus: bool,
    pub set_cursor: Option<usize>,
    pub ac_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RibbonTab {
    Home,
    Insert,
    Formulas,
    Data,
    View,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intercept {
    None,
    Grid,
    Editor,
}

/// Something waiting for the "save changes?" answer.
#[derive(Debug, Clone)]
pub enum Pending {
    New,
    Open(Option<PathBuf>),
    Close,
}

pub struct ScreenshotState {
    pub path: PathBuf,
    pub requested: bool,
    pub done: Arc<AtomicBool>,
}

pub struct App {
    pub doc: Doc,
    pub sheet: usize,
    pub sel: Selection,
    /// Scroll offset (unzoomed pixels) of the scrollable (non-frozen) area.
    pub scroll: [f64; 2],
    views: HashMap<usize, (Selection, [f64; 2])>,
    pub edit: Option<EditState>,
    pub clip: Option<Clip>,
    pub dialog: Option<Dialog>,
    pub find: Option<crate::dialogs::FindState>,
    pub ribbon_tab: RibbonTab,
    pub show_formulas: bool,
    pub show_headings: bool,
    pub show_formula_bar: bool,
    pub status: Option<String>,
    pub recent: Vec<PathBuf>,
    pending_events: Vec<Event>,
    intercept: Intercept,
    pub format_painter: Option<Clip>,
    pub selected_chart: Option<usize>,
    pub cf_cache: Option<((usize, Vec<Rect>, u64), Arc<HashMap<CellPos, CfFormat>>)>,
    pub generation: u64,
    pub scroll_to: Option<CellPos>,
    pub page_rows: u32,
    pub page_cols: u32,
    pub args: Args,
    pub frame_no: u64,
    pub screenshot: Option<ScreenshotState>,
    pub allow_close: bool,
    pub font_color: engine::style::Color,
    pub fill_color: engine::style::Color,
    pub grid: crate::grid::GridState,
    pub tabs: crate::sheet_tabs::TabsState,
    pub filter_popup: Option<crate::dialogs::FilterPopup>,
    pub validation_popup: Option<CellPos>,
    pub functions: Vec<&'static engine::functions::FnDef>,
    pub name_box: String,
    pub pending_close: Option<Pending>,
    suppress_text: bool,
}

impl App {
    pub fn new(ctx: &egui::Context, args: Args) -> App {
        let mut app = App {
            doc: Doc::new(),
            sheet: 0,
            sel: Selection::single(CellPos::new(0, 0)),
            scroll: [0.0, 0.0],
            views: HashMap::new(),
            edit: None,
            clip: None,
            dialog: None,
            find: None,
            ribbon_tab: RibbonTab::Home,
            show_formulas: false,
            show_headings: true,
            show_formula_bar: true,
            status: None,
            recent: crate::fileio::load_recent(),
            pending_events: Vec::new(),
            intercept: Intercept::Grid,
            format_painter: None,
            selected_chart: None,
            cf_cache: None,
            generation: 0,
            scroll_to: None,
            page_rows: 30,
            page_cols: 10,
            args: args.clone(),
            frame_no: 0,
            screenshot: args.screenshot.clone().map(|p| ScreenshotState { path: p, requested: false, done: Arc::new(AtomicBool::new(false)) }),
            allow_close: false,
            font_color: engine::style::Color::RED,
            fill_color: engine::style::Color::YELLOW,
            grid: Default::default(),
            tabs: Default::default(),
            filter_popup: None,
            validation_popup: None,
            functions: engine::functions::all(),
            name_box: String::new(),
            pending_close: None,
            suppress_text: false,
        };
        if args.demo {
            app.doc.wb = crate::demo::build();
            app.doc.wb.recalculate();
        }
        if let Some(f) = &args.file {
            app.open_path(f.clone());
        }
        let _ = ctx;
        app
    }

    // ------------------------------------------------------------------
    // basic accessors
    // ------------------------------------------------------------------

    pub fn sheet_ref(&self) -> &engine::Sheet {
        &self.doc.wb.sheets[self.sheet]
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = Some(s.into());
    }

    pub fn message(&mut self, title: &str, text: impl Into<String>) {
        self.dialog = Some(Dialog::Message { title: title.to_string(), text: text.into() });
    }

    /// Bump after any change that affects rendering caches.
    pub fn touch(&mut self) {
        self.generation += 1;
        self.cf_cache = None;
    }

    /// Record undo for `scope`, run `f`, then mark dirty and recalculate.
    pub fn modify(&mut self, label: &str, scope: Scope, f: impl FnOnce(&mut Workbook, usize)) {
        let sel = self.sel.primary();
        self.doc.undo.record(&self.doc.wb, scope, label, self.sheet, sel);
        f(&mut self.doc.wb, self.sheet);
        self.after_change();
    }

    pub fn after_change(&mut self) {
        self.doc.dirty = true;
        if !self.doc.wb.manual_calc {
            self.doc.wb.recalculate();
        }
        if self.sheet >= self.doc.wb.sheets.len() {
            self.sheet = self.doc.wb.sheets.len() - 1;
        }
        self.touch();
    }

    /// Selection ranges clamped to the used area (for formatting whole rows/columns).
    pub fn sel_rects_bounded(&self) -> Vec<Rect> {
        let used = self.sheet_ref().used_range();
        self.sel
            .ranges
            .iter()
            .filter_map(|r| {
                if r.cell_count() <= 2_000_000 {
                    Some(*r)
                } else {
                    let u = used?;
                    let bound = Rect { top: 0, left: 0, bottom: u.bottom.max(100), right: u.right.max(30) };
                    r.intersection(&bound)
                }
            })
            .collect()
    }

    pub fn cells_scope(&self) -> Scope {
        Scope::Cells { sheet: self.sheet, rects: self.sel_rects_bounded() }
    }

    // ------------------------------------------------------------------
    // sheets & selection
    // ------------------------------------------------------------------

    pub fn set_sheet(&mut self, idx: usize) {
        if idx >= self.doc.wb.sheets.len() || idx == self.sheet {
            return;
        }
        self.views.insert(self.sheet, (self.sel.clone(), self.scroll));
        self.sheet = idx;
        let (sel, scroll) = self.views.get(&idx).cloned().unwrap_or((Selection::single(CellPos::new(0, 0)), [0.0, 0.0]));
        self.sel = sel;
        self.scroll = scroll;
        self.selected_chart = None;
        self.filter_popup = None;
        self.touch();
    }

    pub fn reset_views(&mut self) {
        self.views.clear();
        self.sel = Selection::single(CellPos::new(0, 0));
        self.scroll = [0.0, 0.0];
        if self.sheet >= self.doc.wb.sheets.len() {
            self.sheet = 0;
        }
    }

    pub fn merge_expand(&self, mut r: Rect) -> Rect {
        let merges = &self.sheet_ref().merges;
        if merges.is_empty() {
            return r;
        }
        loop {
            let mut grown = false;
            for m in merges {
                if r.intersects(m) {
                    let n = Rect { top: r.top.min(m.top), left: r.left.min(m.left), bottom: r.bottom.max(m.bottom), right: r.right.max(m.right) };
                    if n != r {
                        r = n;
                        grown = true;
                    }
                }
            }
            if !grown {
                return r;
            }
        }
    }

    pub fn select_cell(&mut self, p: CellPos) {
        let r = self.merge_expand(Rect::single(p));
        let a = r.top_left();
        self.sel = Selection { ranges: vec![r], active: a, anchor: a, ext: p };
        self.scroll_to = Some(p);
        self.selected_chart = None;
    }

    pub fn select_range(&mut self, r: Rect, active: CellPos) {
        let r = self.merge_expand(r);
        self.sel = Selection { ranges: vec![r], active, anchor: r.top_left(), ext: r.bottom_right() };
        self.scroll_to = Some(active);
        self.selected_chart = None;
    }

    /// Extend the last range to `ext` (Shift+click / Shift+arrows).
    pub fn extend_to(&mut self, ext: CellPos) {
        let r = self.merge_expand(Rect::new(self.sel.anchor, ext));
        if let Some(last) = self.sel.ranges.last_mut() {
            *last = r;
        }
        self.sel.ext = ext;
        self.scroll_to = Some(ext);
    }

    fn is_hidden(&self, row: Option<u32>, col: Option<u32>) -> bool {
        let s = self.sheet_ref();
        row.map(|r| s.hidden_rows.contains(&r) || s.row_heights.get(&r) == Some(&0.0)).unwrap_or(false)
            || col.map(|c| s.hidden_cols.contains(&c) || s.col_widths.get(&c) == Some(&0.0)).unwrap_or(false)
    }

    /// One visible step from `p` in direction (dr, dc).
    pub fn step(&self, p: CellPos, dr: i32, dc: i32) -> CellPos {
        let mut r = p.row as i64;
        let mut c = p.col as i64;
        // leave merged areas from their far edge
        if let Some(m) = self.sheet_ref().merge_at(p) {
            if dr > 0 {
                r = m.bottom as i64;
            }
            if dc > 0 {
                c = m.right as i64;
            }
        }
        loop {
            let nr = r + dr as i64;
            let nc = c + dc as i64;
            if nr < 0 || nc < 0 || nr >= MAX_ROWS as i64 || nc >= MAX_COLS as i64 {
                return p;
            }
            r = nr;
            c = nc;
            if !self.is_hidden(if dr != 0 { Some(r as u32) } else { None }, if dc != 0 { Some(c as u32) } else { None }) {
                break;
            }
        }
        let np = CellPos::new(r as u32, c as u32);
        match self.sheet_ref().merge_at(np) {
            Some(m) => m.top_left(),
            None => np,
        }
    }

    pub fn move_active(&mut self, dr: i32, dc: i32, extend: bool, jump: bool) {
        let from = if extend { self.sel.ext } else { self.sel.active };
        let to = if jump {
            ops::data_edge(&self.doc.wb, self.sheet, from, dr, dc)
        } else {
            let mut p = from;
            let n = dr.unsigned_abs().max(dc.unsigned_abs()).max(1);
            for _ in 0..n {
                p = self.step(p, dr.signum(), dc.signum());
            }
            p
        };
        if extend {
            self.extend_to(to);
        } else {
            self.select_cell(to);
        }
    }

    /// Enter/Tab movement: inside a multi-cell selection the active cell cycles within it.
    pub fn move_within_selection(&mut self, dr: i32, dc: i32) {
        let r = self.sel.primary();
        if self.sel.ranges.len() == 1 && r.cell_count() == 1 || self.merge_expand(Rect::single(self.sel.active)) == r {
            let p = self.step(self.sel.active, dr, dc);
            self.select_cell(p);
            return;
        }
        let a = self.sel.active;
        let (mut row, mut col) = (a.row as i64, a.col as i64);
        if dr != 0 {
            row += dr as i64;
            if row > r.bottom as i64 {
                row = r.top as i64;
                col += 1;
                if col > r.right as i64 {
                    col = r.left as i64;
                }
            } else if row < r.top as i64 {
                row = r.bottom as i64;
                col -= 1;
                if col < r.left as i64 {
                    col = r.right as i64;
                }
            }
        } else {
            col += dc as i64;
            if col > r.right as i64 {
                col = r.left as i64;
                row += 1;
                if row > r.bottom as i64 {
                    row = r.top as i64;
                }
            } else if col < r.left as i64 {
                col = r.right as i64;
                row -= 1;
                if row < r.top as i64 {
                    row = r.bottom as i64;
                }
            }
        }
        self.sel.active = CellPos::new(row as u32, col as u32);
        self.scroll_to = Some(self.sel.active);
    }

    pub fn select_all(&mut self) {
        let r = ops::current_region(&self.doc.wb, self.sheet, self.sel.active);
        let all = Rect { top: 0, left: 0, bottom: MAX_ROWS - 1, right: MAX_COLS - 1 };
        let target = if r.cell_count() > 1 && self.sel.primary() != r { r } else { all };
        let a = self.sel.active;
        self.sel = Selection { ranges: vec![target], active: a, anchor: target.top_left(), ext: target.bottom_right() };
    }

    pub fn select_columns(&mut self) {
        let r = self.sel.primary();
        let nr = Rect { top: 0, bottom: MAX_ROWS - 1, left: r.left, right: r.right };
        let a = self.sel.active;
        self.sel = Selection { ranges: vec![nr], active: a, anchor: CellPos::new(0, r.left), ext: CellPos::new(MAX_ROWS - 1, r.right) };
    }

    pub fn select_rows(&mut self) {
        let r = self.sel.primary();
        let nr = Rect { top: r.top, bottom: r.bottom, left: 0, right: MAX_COLS - 1 };
        let a = self.sel.active;
        self.sel = Selection { ranges: vec![nr], active: a, anchor: CellPos::new(r.top, 0), ext: CellPos::new(r.bottom, MAX_COLS - 1) };
    }

    /// Go to a typed reference or defined name. Returns false if not understood.
    pub fn goto(&mut self, text: &str) -> bool {
        let t = text.trim();
        if t.is_empty() {
            return false;
        }
        if let Some((sheet, r)) = editor::parse_goto(t) {
            if let Some(name) = sheet {
                match self.doc.wb.sheet_index(&name) {
                    Some(i) => self.set_sheet(i),
                    None => return false,
                }
            }
            let a = r.top_left();
            self.select_range(r, a);
            return true;
        }
        if let Some(n) = self.doc.wb.find_name(t, self.sheet).cloned() {
            if let engine::ast::Expr::Ref(rf) = &n.formula {
                if let Some(s) = &rf.sheet {
                    if let Some(i) = self.doc.wb.sheet_index(s) {
                        self.set_sheet(i);
                    }
                }
                let r = rf.rect();
                self.select_range(r, r.top_left());
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------
    // files
    // ------------------------------------------------------------------

    pub fn window_title(&self) -> String {
        format!("{}{} - RExcel", self.doc.title(), if self.doc.dirty { "*" } else { "" })
    }

    pub fn request(&mut self, p: Pending) {
        if self.doc.dirty {
            self.dialog = Some(Dialog::SaveChanges(p));
        } else {
            self.do_pending(p);
        }
    }

    pub fn do_pending(&mut self, p: Pending) {
        match p {
            Pending::New => {
                self.doc = Doc::new();
                self.after_load();
            }
            Pending::Open(path) => {
                let path = path.or_else(crate::fileio::pick_open);
                if let Some(p) = path {
                    self.open_path(p);
                }
            }
            Pending::Close => {
                self.allow_close = true;
                self.pending_close = Some(Pending::Close);
            }
        }
    }

    fn after_load(&mut self) {
        self.sheet = 0;
        self.edit = None;
        self.clip = None;
        self.find = None;
        self.filter_popup = None;
        self.selected_chart = None;
        self.reset_views();
        self.touch();
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match crate::fileio::open(&path) {
            Ok(wb) => {
                self.doc = Doc { wb, path: Some(path.clone()), dirty: false, undo: UndoStack::new() };
                crate::fileio::add_recent(&mut self.recent, &path);
                self.after_load();
                self.set_status(format!("Opened {}", path.display()));
            }
            Err(e) => self.message("RExcel", format!("Couldn't open {}:\n{}", path.display(), e)),
        }
    }

    pub fn save(&mut self) -> bool {
        match self.doc.path.clone() {
            Some(p) if crate::fileio::ext(&p) != "xls" && crate::fileio::ext(&p) != "ods" && crate::fileio::ext(&p) != "xlsm" => self.save_to(p),
            _ => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        match crate::fileio::pick_save(self.doc.path.as_deref()) {
            Some(p) => self.save_to(p),
            None => false,
        }
    }

    pub fn save_to(&mut self, p: PathBuf) -> bool {
        self.commit_if_editing();
        match crate::fileio::save(&self.doc.wb, self.sheet, &p) {
            Ok(()) => {
                if crate::fileio::is_single_sheet_format(&p) && self.doc.wb.sheets.len() > 1 {
                    self.set_status("Saved the active sheet only (text formats hold one sheet).");
                } else {
                    self.set_status(format!("Saved {}", p.display()));
                }
                self.doc.path = Some(p.clone());
                self.doc.dirty = false;
                crate::fileio::add_recent(&mut self.recent, &p);
                true
            }
            Err(e) => {
                self.message("RExcel", format!("Couldn't save {}:\n{}", p.display(), e));
                false
            }
        }
    }

    // ------------------------------------------------------------------
    // editing
    // ------------------------------------------------------------------

    pub fn begin_edit(&mut self, initial: Option<String>, mode: EditMode, in_bar: bool) {
        if self.edit.is_some() {
            return;
        }
        let cell = self.sel.active;
        let original = self.doc.wb.input_text(self.sheet, cell);
        let text = initial.unwrap_or_else(|| original.clone());
        let cursor = text.chars().count();
        self.edit = Some(EditState {
            sheet: self.sheet,
            cell,
            text,
            original,
            mode,
            in_bar,
            cursor,
            point: None,
            request_focus: true,
            set_cursor: Some(cursor),
            ac_index: 0,
        });
        self.validation_popup = None;
        self.scroll_to = Some(cell);
    }

    pub fn cancel_edit(&mut self) {
        if let Some(e) = self.edit.take() {
            if e.sheet != self.sheet {
                self.set_sheet(e.sheet);
            }
        }
    }

    pub fn commit_if_editing(&mut self) -> bool {
        if self.edit.is_some() {
            self.commit_edit(0, 0, false)
        } else {
            true
        }
    }

    /// Commit the editor, then move by (dr, dc). Returns false if the entry was rejected.
    pub fn commit_edit(&mut self, dr: i32, dc: i32, all_selected: bool) -> bool {
        let Some(e) = self.edit.take() else { return true };
        let mut text = e.text.clone();
        if text.starts_with('=') {
            text = editor::close_parens(&text);
        }
        if text == e.original && !all_selected {
            if e.sheet != self.sheet {
                self.set_sheet(e.sheet);
            }
            self.after_commit_move(dr, dc);
            return true;
        }
        if let Err(msg) = ops::validate_input(&self.doc.wb, e.sheet, e.cell, &text) {
            self.edit = Some(e);
            self.dialog = Some(Dialog::Message { title: "RExcel".into(), text: msg });
            return false;
        }
        let parsed = match engine::workbook::parse_input(&text) {
            Ok(p) => p,
            Err(err) => {
                let mut e = e;
                e.request_focus = true;
                self.edit = Some(e);
                self.dialog = Some(Dialog::Message {
                    title: "RExcel".into(),
                    text: format!("There's a problem with this formula.\n\n{}", err.message),
                });
                return false;
            }
        };
        if e.sheet != self.sheet {
            self.set_sheet(e.sheet);
        }
        let targets: Vec<Rect> = if all_selected { self.sel_rects_bounded() } else { vec![Rect::single(e.cell)] };
        let sel = self.sel.primary();
        self.doc.undo.record(&self.doc.wb, Scope::Cells { sheet: e.sheet, rects: targets.clone() }, "Typing", e.sheet, sel);
        let wb = &mut self.doc.wb;
        for r in &targets {
            for p in r.iter() {
                if let (Some(f), true) = (&parsed.formula, p != e.cell) {
                    let mut f = f.clone();
                    f.shift_relative(p.row as i64 - e.cell.row as i64, p.col as i64 - e.cell.col as i64);
                    let c = wb.sheets[e.sheet].cells.entry(p).or_default();
                    c.formula = Some(Box::new(f));
                    c.value = Value::Empty;
                } else {
                    let _ = wb.set_input(e.sheet, p, &text);
                }
            }
        }
        if text.contains('\n') && !text.starts_with('=') {
            wb.update_style(e.sheet, Rect::single(e.cell), |s| s.wrap = true);
            let lines = text.lines().count() as f32;
            let cur = wb.sheets[e.sheet].row_height(e.cell.row);
            let want = engine::workbook::DEFAULT_ROW_HEIGHT * lines.min(10.0);
            if want > cur && !wb.sheets[e.sheet].row_heights.contains_key(&e.cell.row) {
                wb.sheets[e.sheet].row_heights.insert(e.cell.row, want);
            }
        }
        self.after_change();
        self.after_commit_move(dr, dc);
        true
    }

    fn after_commit_move(&mut self, dr: i32, dc: i32) {
        if dr != 0 || dc != 0 {
            self.move_within_selection(dr, dc);
        }
        self.scroll_to = Some(self.sel.active);
    }

    /// Insert or replace the pointed reference in the formula being edited.
    pub fn point_to(&mut self, sheet: usize, anchor: CellPos, cur: CellPos) {
        let names: Vec<String> = self.doc.wb.sheets.iter().map(|s| s.name.clone()).collect();
        let Some(e) = self.edit.as_mut() else { return };
        let rect = Rect::new(anchor, cur);
        let txt = editor::ref_text(e.sheet, sheet, &names[sheet], rect);
        let mut chars: Vec<char> = e.text.chars().collect();
        let (start, end) = match &e.point {
            Some(p) if p.end == e.cursor && p.end <= chars.len() => (p.start, p.end),
            _ => (e.cursor.min(chars.len()), e.cursor.min(chars.len())),
        };
        let ins: Vec<char> = txt.chars().collect();
        chars.splice(start..end, ins.iter().cloned());
        e.text = chars.into_iter().collect();
        let new_end = start + ins.len();
        e.point = Some(PointRef { start, end: new_end, anchor, cur, sheet });
        e.cursor = new_end;
        e.set_cursor = Some(new_end);
        e.mode = EditMode::Point;
        e.request_focus = true;
    }

    /// Is the editor in a state where clicking a cell inserts a reference?
    pub fn pointing_possible(&self) -> bool {
        match &self.edit {
            Some(e) => {
                if !e.text.starts_with('=') {
                    return false;
                }
                if let Some(p) = &e.point {
                    if p.end == e.cursor {
                        return true;
                    }
                }
                editor::can_point(&e.text, e.cursor)
            }
            None => false,
        }
    }

    /// Function names matching the identifier being typed.
    pub fn autocomplete(&self) -> Vec<&'static engine::functions::FnDef> {
        let Some(e) = &self.edit else { return vec![] };
        let Some((_, prefix)) = editor::ident_before_cursor(&e.text, e.cursor) else { return vec![] };
        let up = prefix.to_uppercase();
        self.functions.iter().copied().filter(|f| f.name.starts_with(&up)).take(12).collect()
    }

    pub fn accept_autocomplete(&mut self, name: &str) {
        let Some(e) = self.edit.as_mut() else { return };
        let Some((start, _)) = editor::ident_before_cursor(&e.text, e.cursor) else { return };
        let mut chars: Vec<char> = e.text.chars().collect();
        let ins: Vec<char> = format!("{}(", name).chars().collect();
        chars.splice(start..e.cursor.min(chars.len()), ins.iter().cloned());
        e.text = chars.into_iter().collect();
        e.cursor = start + ins.len();
        e.set_cursor = Some(e.cursor);
        e.ac_index = 0;
        e.request_focus = true;
    }

    // ------------------------------------------------------------------
    // frame
    // ------------------------------------------------------------------

    fn decide_intercept(&mut self, ctx: &egui::Context) {
        let focused = ctx.memory(|m| m.focused());
        let modal = self.dialog.as_ref().map(|d| d.is_modal()).unwrap_or(false) || self.filter_popup.is_some();
        self.intercept = if modal {
            Intercept::None
        } else if self.edit.is_some() {
            if focused == Some(cell_editor_id()) || focused == Some(bar_editor_id()) {
                Intercept::Editor
            } else if focused.is_none() {
                Intercept::Editor
            } else {
                Intercept::None
            }
        } else if focused.is_none() {
            Intercept::Grid
        } else {
            Intercept::None
        };
    }

    fn take_events(&mut self, raw: &mut egui::RawInput) {
        match self.intercept {
            Intercept::None => {}
            Intercept::Grid => {
                let mut keep = Vec::new();
                for ev in raw.events.drain(..) {
                    match &ev {
                        Event::Key { .. } | Event::Text(_) | Event::Copy | Event::Cut | Event::Paste(_) => self.pending_events.push(ev),
                        _ => keep.push(ev),
                    }
                }
                raw.events = keep;
            }
            Intercept::Editor => {
                let ac_visible = !self.autocomplete().is_empty();
                let arrows_commit = self.edit.as_ref().map(|e| e.mode != EditMode::Edit).unwrap_or(false);
                let mut keep = Vec::new();
                for ev in raw.events.drain(..) {
                    let take = match &ev {
                        Event::Key { key, modifiers, pressed: true, .. } => match key {
                            Key::Enter => !modifiers.alt,
                            Key::Tab | Key::Escape | Key::F4 | Key::F2 | Key::PageUp | Key::PageDown => true,
                            Key::ArrowUp | Key::ArrowDown => ac_visible || arrows_commit,
                            Key::ArrowLeft | Key::ArrowRight => arrows_commit,
                            _ => false,
                        },
                        Event::Key { key, pressed: false, .. } => matches!(key, Key::Enter | Key::Tab | Key::Escape),
                        _ => false,
                    };
                    if take {
                        self.pending_events.push(ev);
                    } else {
                        keep.push(ev);
                    }
                }
                raw.events = keep;
            }
        }
    }

    fn process_events(&mut self, ctx: &egui::Context) {
        let events = std::mem::take(&mut self.pending_events);
        self.suppress_text = false;
        for ev in events {
            if self.dialog.as_ref().map(|d| d.is_modal()).unwrap_or(false) {
                break;
            }
            if self.edit.is_some() {
                self.editor_event(ev);
            } else {
                self.grid_event(ctx, ev);
            }
        }
    }

    fn editor_event(&mut self, ev: Event) {
        let Event::Key { key, modifiers: m, pressed: true, .. } = ev else { return };
        let ac = self.autocomplete();
        match key {
            Key::Enter => {
                self.commit_edit(if m.shift { -1 } else { 1 }, 0, m.command);
            }
            Key::Tab => {
                if !ac.is_empty() {
                    let i = self.edit.as_ref().map(|e| e.ac_index).unwrap_or(0).min(ac.len() - 1);
                    let name = ac[i].name;
                    self.accept_autocomplete(name);
                } else {
                    self.commit_edit(0, if m.shift { -1 } else { 1 }, false);
                }
            }
            Key::Escape => self.cancel_edit(),
            Key::F2 => {
                if let Some(e) = self.edit.as_mut() {
                    e.mode = if e.mode == EditMode::Edit { EditMode::Enter } else { EditMode::Edit };
                    e.point = None;
                }
            }
            Key::F4 => {
                if let Some(e) = self.edit.as_mut() {
                    if let Some((t, c)) = editor::cycle_absolute(&e.text, e.cursor) {
                        e.text = t;
                        e.cursor = c;
                        e.set_cursor = Some(c);
                        e.point = None;
                    }
                }
            }
            Key::PageUp | Key::PageDown => {
                let n = self.page_rows.max(1) as i32;
                if self.commit_edit(0, 0, false) {
                    self.move_active(if key == Key::PageUp { -n } else { n }, 0, false, false);
                }
            }
            Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => {
                let (dr, dc) = match key {
                    Key::ArrowUp => (-1, 0),
                    Key::ArrowDown => (1, 0),
                    Key::ArrowLeft => (0, -1),
                    _ => (0, 1),
                };
                if !ac.is_empty() && dc == 0 {
                    if let Some(e) = self.edit.as_mut() {
                        let n = ac.len();
                        e.ac_index = ((e.ac_index as i64 + dr as i64).rem_euclid(n as i64)) as usize;
                    }
                    return;
                }
                let mode = self.edit.as_ref().map(|e| e.mode).unwrap_or(EditMode::Edit);
                if mode == EditMode::Edit {
                    return;
                }
                if self.pointing_possible() {
                    let e = self.edit.as_ref().unwrap();
                    let (sheet, anchor, cur) = match &e.point {
                        Some(p) if p.end == e.cursor => (p.sheet, p.anchor, p.cur),
                        _ => (self.sheet, e.cell, e.cell),
                    };
                    let saved = self.sheet;
                    self.sheet = sheet;
                    let next = self.step(cur, dr, dc);
                    self.sheet = saved;
                    if m.shift {
                        self.point_to(sheet, anchor, next);
                    } else {
                        self.point_to(sheet, next, next);
                    }
                    self.scroll_to = Some(next);
                } else {
                    self.commit_edit(dr, dc, false);
                    if dr != 0 || dc != 0 {
                        // commit_edit moved within selection; arrows move the cell instead
                    }
                }
            }
            _ => {}
        }
    }

    fn grid_event(&mut self, ctx: &egui::Context, ev: Event) {
        match ev {
            Event::Copy => self.copy(false),
            Event::Cut => self.copy(true),
            Event::Paste(text) => self.paste_text_or_clip(Some(text)),
            Event::Text(t) => {
                if self.suppress_text || t.chars().all(|c| c.is_control()) {
                    return;
                }
                if self.selected_chart.is_some() {
                    return;
                }
                self.begin_edit(Some(t), EditMode::Enter, false);
            }
            Event::Key { key, modifiers: m, pressed: true, .. } => self.grid_key(ctx, key, m),
            _ => {}
        }
    }

    fn grid_key(&mut self, ctx: &egui::Context, key: Key, m: Modifiers) {
        let cmd = m.command;
        let shift = m.shift;
        match key {
            Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight if !m.alt => {
                let (dr, dc) = match key {
                    Key::ArrowUp => (-1, 0),
                    Key::ArrowDown => (1, 0),
                    Key::ArrowLeft => (0, -1),
                    _ => (0, 1),
                };
                if let Some(ci) = self.selected_chart {
                    // nudge the chart
                    if let Some(c) = self.doc.wb.sheets[self.sheet].charts.get(ci).cloned() {
                        let np = CellPos::new((c.anchor.row as i64 + dr as i64).max(0) as u32, (c.anchor.col as i64 + dc as i64).max(0) as u32);
                        self.modify("Move Chart", Scope::Sheet(self.sheet), |wb, s| wb.sheets[s].charts[ci].anchor = np);
                    }
                    return;
                }
                self.move_active(dr, dc, shift, cmd);
            }
            Key::ArrowDown if m.alt => {
                if ops::validation_at(&self.doc.wb, self.sheet, self.sel.active).is_some() {
                    self.validation_popup = Some(self.sel.active);
                }
            }
            Key::Home => {
                let fr = self.sheet_ref().freeze_rows;
                let fc = self.sheet_ref().freeze_cols;
                let target = if cmd { CellPos::new(fr, fc) } else { CellPos::new(self.sel.active.row, fc.min(self.sel.active.col)) };
                if shift {
                    self.extend_to(target)
                } else {
                    self.select_cell(target)
                }
            }
            Key::End if cmd => {
                let ext = self.sheet_ref().data_extent().map(|e| e.bottom_right()).unwrap_or(CellPos::new(0, 0));
                if shift {
                    self.extend_to(ext)
                } else {
                    self.select_cell(ext)
                }
            }
            Key::PageUp | Key::PageDown => {
                if cmd {
                    let n = self.doc.wb.sheets.len();
                    let mut i = self.sheet as i64;
                    loop {
                        i += if key == Key::PageUp { -1 } else { 1 };
                        if i < 0 || i >= n as i64 {
                            break;
                        }
                        if !self.doc.wb.sheets[i as usize].hidden {
                            self.set_sheet(i as usize);
                            break;
                        }
                    }
                    return;
                }
                let n = if m.alt { self.page_cols.max(1) as i32 } else { self.page_rows.max(1) as i32 };
                let d = if key == Key::PageUp { -n } else { n };
                if m.alt {
                    self.move_active(0, d, shift, false);
                } else {
                    self.move_active(d, 0, shift, false);
                    self.scroll_page(d);
                }
            }
            Key::Tab => self.move_within_selection(0, if shift { -1 } else { 1 }),
            Key::Enter => {
                if cmd {
                    return;
                }
                self.move_within_selection(if shift { -1 } else { 1 }, 0)
            }
            Key::F2 if shift => self.edit_comment(),
            Key::F2 => self.begin_edit(None, EditMode::Edit, false),
            Key::F3 if shift => self.open_insert_function(),
            Key::F3 if cmd => self.dialog = Some(Dialog::name_manager()),
            Key::F5 => self.dialog = Some(Dialog::GoTo { text: String::new() }),
            Key::F9 => {
                self.doc.wb.recalculate();
                self.touch();
                self.set_status("Calculated");
            }
            Key::F11 if shift => self.insert_sheet(),
            Key::F12 => {
                self.save_as();
            }
            Key::Escape => {
                self.clip = None;
                self.format_painter = None;
                self.selected_chart = None;
                self.validation_popup = None;
            }
            Key::Delete => {
                if let Some(ci) = self.selected_chart.take() {
                    self.modify("Delete Chart", Scope::Sheet(self.sheet), |wb, s| {
                        if ci < wb.sheets[s].charts.len() {
                            wb.sheets[s].charts.remove(ci);
                        }
                    });
                } else {
                    self.clear_selection(true, false);
                }
            }
            Key::Backspace => {
                if self.selected_chart.is_none() {
                    self.begin_edit(Some(String::new()), EditMode::Enter, false);
                }
            }
            Key::Space if cmd && shift => {
                self.suppress_text = true;
                self.select_all();
            }
            Key::Space if cmd => {
                self.suppress_text = true;
                self.select_columns();
            }
            Key::Space if shift => {
                self.suppress_text = true;
                self.select_rows();
            }
            Key::A if cmd => self.select_all(),
            Key::Z if cmd && shift => self.redo(),
            Key::Z if cmd => self.undo(),
            Key::Y if cmd => self.redo(),
            Key::B if cmd => self.toggle_font_flag(FontFlag::Bold),
            Key::I if cmd => self.toggle_font_flag(FontFlag::Italic),
            Key::U if cmd => self.toggle_font_flag(FontFlag::Underline),
            Key::Num5 if cmd && !shift => self.toggle_font_flag(FontFlag::Strike),
            Key::Num1 if cmd && shift => self.set_number_format("#,##0.00"),
            Key::Num4 if cmd && shift => self.set_number_format("$#,##0.00"),
            Key::Num5 if cmd && shift => self.set_number_format("0%"),
            Key::Num3 if cmd && shift => self.set_number_format("d-mmm-yy"),
            Key::Num1 if cmd => self.open_format_cells(),
            Key::Num9 if cmd => self.hide_rows_cols(true, !shift),
            Key::Num0 if cmd => self.hide_rows_cols(false, !shift),
            Key::D if cmd => self.fill_down_right(true),
            Key::R if cmd => self.fill_down_right(false),
            Key::Semicolon | Key::Colon if cmd => {
                self.suppress_text = true;
                let now = engine::datetime::now_serial();
                if shift || key == Key::Colon {
                    self.set_active_value(Value::Number(now.fract()), "h:mm AM/PM");
                } else {
                    self.set_active_value(Value::Number(now.floor()), "m/d/yyyy");
                }
            }
            Key::Backtick if cmd => {
                self.show_formulas = !self.show_formulas;
            }
            Key::F if cmd => self.open_find(false),
            Key::H if cmd => self.open_find(true),
            Key::G if cmd => self.dialog = Some(Dialog::GoTo { text: String::new() }),
            Key::K if cmd => self.open_hyperlink(),
            Key::L if cmd && shift => self.toggle_autofilter(),
            Key::N if cmd => self.request(Pending::New),
            Key::O if cmd => self.request(Pending::Open(None)),
            Key::S if cmd && shift => {
                self.save_as();
            }
            Key::S if cmd => {
                self.save();
            }
            Key::P if cmd => self.message("Print", "Printing is not available in RExcel yet."),
            Key::Plus | Key::Equals if cmd => self.insert_cells_smart(),
            Key::Minus if cmd => self.delete_cells_smart(),
            Key::Equals if m.alt => self.autosum("SUM"),
            Key::W if cmd => self.request(Pending::Close),
            Key::Q if cmd => self.request(Pending::Close),
            _ => {}
        }
        let _ = ctx;
    }

    pub fn scroll_page(&mut self, rows: i32) {
        let h = engine::workbook::DEFAULT_ROW_HEIGHT as f64;
        self.scroll[1] = (self.scroll[1] + rows as f64 * h).max(0.0);
    }

    pub fn undo(&mut self) {
        if self.edit.is_some() {
            return;
        }
        if let Some(r) = self.doc.undo.undo(&mut self.doc.wb) {
            self.restore_after_undo(r, "Undo");
        }
    }

    pub fn redo(&mut self) {
        if self.edit.is_some() {
            return;
        }
        if let Some(r) = self.doc.undo.redo(&mut self.doc.wb) {
            self.restore_after_undo(r, "Redo");
        }
    }

    fn restore_after_undo(&mut self, r: ops::Restored, what: &str) {
        self.doc.dirty = true;
        if r.sheet < self.doc.wb.sheets.len() {
            if r.sheet != self.sheet {
                self.views.clear();
                self.sheet = r.sheet;
            }
            self.select_range(r.selection, r.selection.top_left());
        } else {
            self.sheet = 0;
            self.reset_views();
        }
        self.selected_chart = None;
        self.clip = None;
        self.touch();
        self.set_status(format!("{}: {}", what, r.label));
    }

    /// Apply cursor and focus requests to the in-cell editor or formula bar
    /// before the widgets are drawn.
    fn apply_editor_focus(&mut self, ctx: &egui::Context) {
        let dialog_open = self.dialog.is_some() || self.find.is_some() || self.filter_popup.is_some() || self.tabs.rename.is_some();
        let cur_sheet = self.sheet;
        let Some(e) = self.edit.as_mut() else { return };
        if e.sheet != cur_sheet {
            e.in_bar = true;
        }
        if let Some(c) = e.set_cursor.take() {
            crate::grid::prepare_editor_state(ctx, cell_editor_id(), Some(c));
            crate::grid::prepare_editor_state(ctx, bar_editor_id(), Some(c));
        }
        let target = if e.in_bar { bar_editor_id() } else { cell_editor_id() };
        let focused = ctx.memory(|m| m.focused());
        if e.request_focus || (focused.is_none() && !dialog_open) {
            e.request_focus = false;
            ctx.memory_mut(|m| m.request_focus(target));
        }
    }

    /// Run scripted actions (testing aid for screenshots).
    fn run_script_action(&mut self, a: &str) {
        let (name, arg) = a.split_once(':').unwrap_or((a, ""));
        match name {
            "select" => {
                self.goto(arg);
            }
            "type" => {
                self.begin_edit(Some(arg.to_string()), EditMode::Enter, false);
            }
            "commit" => {
                self.commit_edit(1, 0, false);
            }
            "format_cells" => self.open_format_cells(),
            "find" => self.open_find(true),
            "insert_function" => self.open_insert_function(),
            "paste_special" => self.dialog = Some(Dialog::PasteSpecial(Default::default())),
            "sort" => self.open_custom_sort(),
            "copy" => self.copy(false),
            "sheet" => {
                if let Ok(i) = arg.parse::<usize>() {
                    self.set_sheet(i);
                }
            }
            "tab" => {
                self.ribbon_tab = match arg {
                    "insert" => RibbonTab::Insert,
                    "formulas" => RibbonTab::Formulas,
                    "data" => RibbonTab::Data,
                    "view" => RibbonTab::View,
                    _ => RibbonTab::Home,
                }
            }
            "freeze" => self.freeze_at_selection(),
            "zoom" => {
                if let Ok(z) = arg.parse::<f32>() {
                    self.set_zoom(z / 100.0);
                }
            }
            "chart" => self.selected_chart = arg.parse().ok(),
            "filter" => self.toggle_autofilter(),
            "cf" => self.dialog = Some(Dialog::cond_format(crate::dialogs::CfKind::Greater)),
            "validation" => self.open_validation(),
            "name_manager" => self.dialog = Some(Dialog::name_manager()),
            "goal_seek" => self.dialog = Some(Dialog::GoalSeek { set_cell: "B2".into(), to_value: String::new(), changing: String::new() }),
            "show_formulas" => self.show_formulas = true,
            _ => eprintln!("unknown --action {}", a),
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.decide_intercept(ctx);
        self.take_events(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame_no += 1;

        // scripted actions for automated screenshots
        if self.frame_no == 3 {
            let actions = std::mem::take(&mut self.args.actions);
            for a in &actions {
                self.run_script_action(a);
            }
        }

        // drag & drop open
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(p) = dropped.into_iter().next() {
            self.request(Pending::Open(Some(p)));
        }

        // close requests
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.doc.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !matches!(self.dialog, Some(Dialog::SaveChanges(_))) {
                self.dialog = Some(Dialog::SaveChanges(Pending::Close));
            }
        }
        if let Some(Pending::Close) = self.pending_close.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        self.process_events(&ctx);
        self.apply_editor_focus(&ctx);

        egui::Panel::top("ribbon")
            .frame(egui::Frame::new().fill(crate::theme::RIBBON_BG).inner_margin(egui::Margin::ZERO))
            .show(ui, |ui| crate::ribbon::show(self, ui));
        if self.show_formula_bar {
            egui::Panel::top("formula_bar")
                .frame(egui::Frame::new().fill(egui::Color32::WHITE).inner_margin(egui::Margin::symmetric(4, 3)).stroke(egui::Stroke::new(1.0, crate::theme::BORDER)))
                .show(ui, |ui| crate::ribbon::formula_bar(self, ui));
        }
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(egui::Frame::new().fill(crate::theme::RIBBON_BG).inner_margin(egui::Margin::symmetric(8, 2)))
            .show(ui, |ui| crate::status_bar::show(self, ui));
        egui::CentralPanel::no_frame().show(ui, |ui| crate::grid::show(self, ui));

        crate::dialogs::show(self, &ctx);

        let title = self.window_title();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // marching ants animation
        if self.clip.is_some() || self.edit.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        // automated screenshot
        if let Some(s) = &mut self.screenshot {
            if s.done.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if !s.requested && self.frame_no >= 8 {
                s.requested = true;
                let path = s.path.clone();
                let done = s.done.clone();
                let c2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
                    for p in &img.pixels {
                        rgba.extend_from_slice(&p.to_srgba_unmultiplied());
                    }
                    let png = crate::png::encode_rgba(img.size[0] as u32, img.size[1] as u32, &rgba);
                    if let Err(e) = std::fs::write(&path, png) {
                        eprintln!("screenshot: {}", e);
                    }
                    done.store(true, Ordering::SeqCst);
                    c2.request_repaint();
                });
            }
            ctx.request_repaint();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFlag {
    Bold,
    Italic,
    Underline,
    Strike,
}
