//! Commands invoked from the ribbon, menus and keyboard shortcuts.

use engine::address::{CellPos, Rect, MAX_COLS, MAX_ROWS};
use engine::ops::{self, PasteOptions, PasteWhat, Scope, SortKey};
use engine::style::{BorderKind, BorderLine, Color, HAlign, Style, VAlign};
use engine::workbook::{AutoFilter, Chart, ChartKind};
use engine::Value;

use crate::app::{App, EditMode, FontFlag};
use crate::dialogs::Dialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderPreset {
    All,
    Outside,
    ThickOutside,
    Top,
    Bottom,
    Left,
    Right,
    DoubleBottom,
    None,
}

impl App {
    // ------------------------------------------------------------------
    // clipboard
    // ------------------------------------------------------------------

    pub fn copy(&mut self, cut: bool) {
        if self.sel.ranges.len() > 1 {
            self.message("RExcel", "This action won't work on multiple selections.");
            return;
        }
        let r = self.sel.primary();
        let bounded = if r.cell_count() > 5_000_000 {
            match self.sheet_ref().used_range() {
                Some(u) => r.intersection(&Rect { top: 0, left: 0, bottom: u.bottom, right: u.right }).unwrap_or(Rect::single(r.top_left())),
                None => Rect::single(r.top_left()),
            }
        } else {
            r
        };
        let clip = ops::copy(&self.doc.wb, self.sheet, bounded, cut);
        crate::clipboard::set_text(&clip.text);
        self.clip = Some(clip);
        self.set_status(if cut { "Select destination and press ENTER or choose Paste" } else { "Select destination and press ENTER or choose Paste" });
    }

    /// Paste the internal clipboard, or text from another application.
    pub fn paste_text_or_clip(&mut self, text: Option<String>) {
        let text = text.or_else(crate::clipboard::get_text);
        let internal = match (&self.clip, &text) {
            (Some(c), Some(t)) => normalize(&c.text) == normalize(t),
            (Some(_), None) => true,
            _ => false,
        };
        if internal {
            self.paste_clip(PasteOptions::default());
        } else if let Some(t) = text {
            if t.is_empty() {
                return;
            }
            let at = self.sel.primary().top_left();
            let area = ops::text_paste_rect_for(at, &t);
            self.modify("Paste", Scope::Cells { sheet: self.sheet, rects: vec![area] }, |wb, s| {
                ops::paste_text(wb, s, at, &t);
            });
            self.select_range(area, at);
        }
    }

    pub fn paste_clip(&mut self, opts: PasteOptions) {
        let Some(clip) = self.clip.clone() else { return };
        if clip.cut {
            if clip.sheet >= self.doc.wb.sheets.len() {
                return;
            }
            let dst = self.sel.primary().top_left();
            let label = "Cut and Paste";
            self.modify(label, Scope::Workbook, |wb, s| {
                ops::cut_paste(wb, &clip, s, dst);
            });
            let dest = Rect { top: dst.row, left: dst.col, bottom: (dst.row + clip.rows() - 1).min(MAX_ROWS - 1), right: (dst.col + clip.cols() - 1).min(MAX_COLS - 1) };
            self.select_range(dest, dst);
            self.clip = None;
            return;
        }
        let dest = self.sel.primary();
        let area = ops::paste_area(&clip, dest, opts.transpose);
        let scope = if opts.what == PasteWhat::All || opts.what == PasteWhat::Formats {
            Scope::Sheet(self.sheet)
        } else {
            Scope::Cells { sheet: self.sheet, rects: vec![area] }
        };
        self.modify("Paste", scope, |wb, s| {
            ops::paste(wb, s, dest, &clip, opts);
        });
        self.select_range(area, area.top_left());
    }

    pub fn start_format_painter(&mut self) {
        let r = self.sel.primary();
        let r = if r.cell_count() > 100_000 { Rect::single(r.top_left()) } else { r };
        self.format_painter = Some(ops::copy(&self.doc.wb, self.sheet, r, false));
        self.set_status("Format Painter: select the cells to format");
    }

    pub fn apply_format_painter(&mut self) {
        let Some(clip) = self.format_painter.take() else { return };
        let dest = self.sel.primary();
        let opts = PasteOptions { what: PasteWhat::Formats, ..Default::default() };
        self.modify("Format Painter", Scope::Sheet(self.sheet), |wb, s| {
            ops::paste(wb, s, dest, &clip, opts);
        });
        self.status = None;
    }

    // ------------------------------------------------------------------
    // contents
    // ------------------------------------------------------------------

    pub fn clear_selection(&mut self, contents: bool, formats: bool) {
        let rects = self.sel_rects_bounded();
        let scope = if formats { Scope::Sheet(self.sheet) } else { self.cells_scope() };
        self.modify(if formats { "Clear" } else { "Clear Contents" }, scope, |wb, s| {
            for r in &rects {
                wb.clear(s, *r, contents, formats);
            }
        });
    }

    pub fn clear_comments(&mut self) {
        let rects = self.sel.ranges.clone();
        self.modify("Clear Comments", Scope::Sheet(self.sheet), |wb, s| {
            wb.sheets[s].comments.retain(|p, _| !rects.iter().any(|r| r.contains(*p)));
        });
    }

    pub fn clear_hyperlinks(&mut self) {
        let rects = self.sel.ranges.clone();
        self.modify("Clear Hyperlinks", Scope::Sheet(self.sheet), |wb, s| {
            wb.sheets[s].hyperlinks.retain(|p, _| !rects.iter().any(|r| r.contains(*p)));
        });
    }

    pub fn set_active_value(&mut self, v: Value, fmt: &str) {
        let p = self.sel.active;
        let fmt = fmt.to_string();
        self.modify("Typing", Scope::Cells { sheet: self.sheet, rects: vec![Rect::single(p)] }, |wb, s| {
            wb.set_value(s, p, v);
            wb.update_style(s, Rect::single(p), |st| st.num_fmt = fmt.clone());
        });
    }

    pub fn fill_down_right(&mut self, down: bool) {
        let r = self.sel.primary();
        let r = self.sel_rects_bounded().last().copied().unwrap_or(r);
        let scope = Scope::Cells { sheet: self.sheet, rects: vec![r] };
        self.modify(if down { "Fill Down" } else { "Fill Right" }, scope, |wb, s| {
            if down {
                ops::fill_down(wb, s, r)
            } else {
                ops::fill_right(wb, s, r)
            }
        });
    }

    /// Fill-handle drag result.
    pub fn fill_to(&mut self, src: Rect, dst: Rect) {
        let union = Rect { top: src.top.min(dst.top), left: src.left.min(dst.left), bottom: src.bottom.max(dst.bottom), right: src.right.max(dst.right) };
        self.modify("Auto Fill", Scope::Cells { sheet: self.sheet, rects: vec![union] }, |wb, s| ops::fill(wb, s, src, dst));
        self.select_range(dst, dst.top_left().max(src.top_left()).min(dst.bottom_right()));
        self.sel.active = dst.top_left();
    }

    // ------------------------------------------------------------------
    // formatting
    // ------------------------------------------------------------------

    pub fn active_style(&self) -> Style {
        self.doc.wb.cell_style(self.sheet, self.sel.active).clone()
    }

    pub fn apply_style(&mut self, label: &str, f: impl Fn(&mut Style)) {
        let rects = self.sel_rects_bounded();
        let scope = self.cells_scope();
        self.modify(label, scope, |wb, s| {
            for r in &rects {
                wb.update_style(s, *r, &f);
            }
        });
    }

    pub fn toggle_font_flag(&mut self, flag: FontFlag) {
        let cur = self.active_style();
        let on = !match flag {
            FontFlag::Bold => cur.bold,
            FontFlag::Italic => cur.italic,
            FontFlag::Underline => cur.underline,
            FontFlag::Strike => cur.strike,
        };
        self.apply_style("Font", |s| match flag {
            FontFlag::Bold => s.bold = on,
            FontFlag::Italic => s.italic = on,
            FontFlag::Underline => s.underline = on,
            FontFlag::Strike => s.strike = on,
        });
    }

    pub fn set_number_format(&mut self, code: &str) {
        let code = code.to_string();
        self.apply_style("Number Format", |s| s.num_fmt = code.clone());
    }

    pub fn change_decimals(&mut self, delta: i32) {
        let st = self.active_style();
        let v = self.doc.wb.value(self.sheet, self.sel.active);
        let code = crate::numfmt_util::adjust_decimals(&st.num_fmt, delta, &v);
        self.set_number_format(&code);
    }

    pub fn set_h_align(&mut self, a: HAlign) {
        let cur = self.active_style().h_align;
        let a = if cur == a { HAlign::General } else { a };
        self.apply_style("Alignment", |s| s.h_align = a);
    }

    pub fn set_v_align(&mut self, a: VAlign) {
        self.apply_style("Alignment", |s| s.v_align = a);
    }

    pub fn toggle_wrap(&mut self) {
        let on = !self.active_style().wrap;
        self.apply_style("Wrap Text", |s| s.wrap = on);
    }

    pub fn change_indent(&mut self, delta: i32) {
        self.apply_style("Indent", |s| {
            s.indent = (s.indent as i32 + delta).clamp(0, 15) as u8;
            if s.indent > 0 && s.h_align == HAlign::General {
                s.h_align = HAlign::Left;
            }
        });
    }

    pub fn change_font_size(&mut self, delta: i32) {
        const SIZES: [f32; 16] = [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0];
        let cur = self.active_style().font_size();
        let next = if delta > 0 {
            SIZES.iter().copied().find(|s| *s > cur).unwrap_or(cur + 4.0)
        } else {
            SIZES.iter().rev().copied().find(|s| *s < cur).unwrap_or((cur - 1.0).max(1.0))
        };
        self.apply_style("Font Size", |s| s.set_font_size(next));
    }

    pub fn merge(&mut self, center: bool, across: bool) {
        let rects = self.sel.ranges.clone();
        self.modify("Merge Cells", Scope::Sheet(self.sheet), |wb, s| {
            for r in &rects {
                if r.cell_count() < 2 || r.cell_count() > 1_000_000 {
                    continue;
                }
                let parts: Vec<Rect> = if across {
                    (r.top..=r.bottom).map(|row| Rect { top: row, bottom: row, left: r.left, right: r.right }).collect()
                } else {
                    vec![*r]
                };
                for m in parts {
                    if m.cell_count() < 2 {
                        continue;
                    }
                    // keep the upper-left value only (Excel warns; we just do it)
                    let tl = m.top_left();
                    let keys: Vec<CellPos> = wb.sheets[s].cells_in(m).map(|(p, _)| *p).filter(|p| *p != tl).collect();
                    for k in keys {
                        if let Some(c) = wb.sheets[s].cells.get_mut(&k) {
                            c.formula = None;
                            c.value = Value::Empty;
                        }
                    }
                    wb.sheets[s].merges.retain(|x| !x.intersects(&m));
                    wb.sheets[s].merges.push(m);
                    if center {
                        wb.update_style(s, Rect::single(tl), |st| st.h_align = HAlign::Center);
                    }
                }
            }
        });
        let r = self.merge_expand(self.sel.primary());
        self.select_range(r, r.top_left());
    }

    pub fn unmerge(&mut self) {
        let rects = self.sel.ranges.clone();
        self.modify("Unmerge Cells", Scope::Sheet(self.sheet), |wb, s| {
            wb.sheets[s].merges.retain(|m| !rects.iter().any(|r| r.intersects(m)));
        });
    }

    pub fn is_merged_selection(&self) -> bool {
        let r = self.sel.primary();
        self.sheet_ref().merges.iter().any(|m| m.intersects(&r))
    }

    pub fn apply_borders(&mut self, preset: BorderPreset, color: Color) {
        let rects = self.sel_rects_bounded();
        let scope = Scope::Cells {
            sheet: self.sheet,
            rects: rects.iter().map(|r| Rect { top: r.top.saturating_sub(1), left: r.left.saturating_sub(1), bottom: (r.bottom + 1).min(MAX_ROWS - 1), right: (r.right + 1).min(MAX_COLS - 1) }).collect(),
        };
        self.modify("Borders", scope, |wb, s| {
            let thin = Some(BorderLine { kind: BorderKind::Thin, color });
            let thick = Some(BorderLine { kind: BorderKind::Thick, color });
            let dbl = Some(BorderLine { kind: BorderKind::Double, color });
            for r in &rects {
                let r = *r;
                let row = |top: u32| Rect { top, bottom: top, ..r };
                let col = |left: u32| Rect { left, right: left, ..r };
                match preset {
                    BorderPreset::None => {
                        wb.update_style(s, r, |st| st.borders = Default::default());
                    }
                    BorderPreset::All => {
                        wb.update_style(s, r, |st| {
                            st.borders.left = thin;
                            st.borders.right = thin;
                            st.borders.top = thin;
                            st.borders.bottom = thin;
                        });
                    }
                    BorderPreset::Outside | BorderPreset::ThickOutside => {
                        let line = if preset == BorderPreset::Outside { thin } else { thick };
                        wb.update_style(s, row(r.top), |st| st.borders.top = line);
                        wb.update_style(s, row(r.bottom), |st| st.borders.bottom = line);
                        wb.update_style(s, col(r.left), |st| st.borders.left = line);
                        wb.update_style(s, col(r.right), |st| st.borders.right = line);
                    }
                    BorderPreset::Top => wb.update_style(s, row(r.top), |st| st.borders.top = thin),
                    BorderPreset::Bottom => wb.update_style(s, row(r.bottom), |st| st.borders.bottom = thin),
                    BorderPreset::DoubleBottom => wb.update_style(s, row(r.bottom), |st| st.borders.bottom = dbl),
                    BorderPreset::Left => wb.update_style(s, col(r.left), |st| st.borders.left = thin),
                    BorderPreset::Right => wb.update_style(s, col(r.right), |st| st.borders.right = thin),
                }
            }
        });
    }

    pub fn open_format_cells(&mut self) {
        self.commit_if_editing();
        let st = self.active_style();
        let v = self.doc.wb.value(self.sheet, self.sel.active);
        self.dialog = Some(Dialog::FormatCells(Box::new(crate::format_dialog::FormatState::new(st, v))));
    }

    // ------------------------------------------------------------------
    // structure
    // ------------------------------------------------------------------

    fn full_rows(&self) -> bool {
        let r = self.sel.primary();
        r.left == 0 && r.right == MAX_COLS - 1
    }
    fn full_cols(&self) -> bool {
        let r = self.sel.primary();
        r.top == 0 && r.bottom == MAX_ROWS - 1
    }

    pub fn insert_rows(&mut self) {
        let r = self.sel.primary();
        let (at, n) = (r.top, r.rows().min(10_000) as i64);
        self.modify("Insert Rows", engine::ops::Scope::Workbook, |wb, s| wb.insert_rows(s, at, n));
    }
    pub fn insert_cols(&mut self) {
        let r = self.sel.primary();
        let (at, n) = (r.left, r.cols().min(1000) as i64);
        self.modify("Insert Columns", engine::ops::Scope::Workbook, |wb, s| wb.insert_cols(s, at, n));
    }
    pub fn delete_rows(&mut self) {
        let r = self.sel.primary();
        let (at, n) = (r.top, r.rows() as i64);
        self.modify("Delete Rows", engine::ops::Scope::Workbook, |wb, s| wb.insert_rows(s, at, -n));
        self.select_cell(CellPos::new(at.min(MAX_ROWS - 1), self.sel.active.col));
    }
    pub fn delete_cols(&mut self) {
        let r = self.sel.primary();
        let (at, n) = (r.left, r.cols() as i64);
        self.modify("Delete Columns", engine::ops::Scope::Workbook, |wb, s| wb.insert_cols(s, at, -n));
        self.select_cell(CellPos::new(self.sel.active.row, at.min(MAX_COLS - 1)));
    }
    pub fn insert_cells_smart(&mut self) {
        if self.full_cols() {
            self.insert_cols()
        } else {
            self.insert_rows()
        }
    }
    pub fn delete_cells_smart(&mut self) {
        if self.full_cols() && !self.full_rows() {
            self.delete_cols()
        } else {
            self.delete_rows()
        }
    }

    pub fn hide_rows_cols(&mut self, rows: bool, hide: bool) {
        let r = self.sel.primary();
        let ext = self.sheet_ref().used_range().unwrap_or(Rect::single(CellPos::new(0, 0)));
        self.modify(if hide { "Hide" } else { "Unhide" }, Scope::Sheet(self.sheet), |wb, s| {
            let sh = &mut wb.sheets[s];
            if rows {
                let bottom = if r.bottom == MAX_ROWS - 1 { ext.bottom.max(r.top + 1000).min(MAX_ROWS - 1) } else { r.bottom };
                for i in r.top..=bottom {
                    if hide {
                        sh.hidden_rows.insert(i);
                    } else {
                        sh.hidden_rows.remove(&i);
                        if sh.row_heights.get(&i) == Some(&0.0) {
                            sh.row_heights.remove(&i);
                        }
                    }
                }
            } else {
                let right = if r.right == MAX_COLS - 1 { ext.right.max(r.left + 100).min(MAX_COLS - 1) } else { r.right };
                for i in r.left..=right {
                    if hide {
                        sh.hidden_cols.insert(i);
                    } else {
                        sh.hidden_cols.remove(&i);
                        if sh.col_widths.get(&i) == Some(&0.0) {
                            sh.col_widths.remove(&i);
                        }
                    }
                }
            }
        });
    }

    pub fn set_col_width(&mut self, w: f32) {
        let r = self.sel.primary();
        self.modify("Column Width", Scope::Sheet(self.sheet), |wb, s| {
            for c in r.left..=r.right.min(r.left + 16383) {
                wb.sheets[s].col_widths.insert(c, w.max(0.0));
            }
        });
    }

    pub fn set_row_height(&mut self, h: f32) {
        let r = self.sel.primary();
        self.modify("Row Height", Scope::Sheet(self.sheet), |wb, s| {
            for row in r.top..=r.bottom.min(r.top + 100_000) {
                wb.sheets[s].row_heights.insert(row, h.max(0.0));
            }
        });
    }

    pub fn insert_sheet(&mut self) {
        let at = self.sheet;
        self.modify("Insert Sheet", Scope::Workbook, |wb, _| {
            let i = wb.add_sheet(None);
            let sh = wb.sheets.remove(i);
            wb.insert_sheet_at(at + 1, sh);
        });
        self.reset_views_keep();
        self.set_sheet_force(at + 1);
    }

    pub fn add_sheet_end(&mut self) {
        self.modify("Insert Sheet", Scope::Workbook, |wb, _| {
            wb.add_sheet(None);
        });
        let n = self.doc.wb.sheets.len() - 1;
        self.reset_views_keep();
        self.set_sheet_force(n);
    }

    pub fn delete_sheet(&mut self, idx: usize) {
        if self.doc.wb.sheets.iter().filter(|s| !s.hidden).count() <= 1 {
            self.message("RExcel", "A workbook must contain at least one visible worksheet.");
            return;
        }
        self.modify("Delete Sheet", Scope::Workbook, |wb, _| {
            wb.delete_sheet(idx);
        });
        self.reset_views_keep();
        let n = idx.min(self.doc.wb.sheets.len() - 1);
        self.set_sheet_force(n);
    }

    pub fn duplicate_sheet(&mut self, idx: usize) {
        self.modify("Copy Sheet", Scope::Workbook, |wb, _| {
            wb.duplicate_sheet(idx);
        });
        self.reset_views_keep();
        self.set_sheet_force(idx + 1);
    }

    pub fn move_sheet(&mut self, from: usize, to: usize) {
        if from == to || to >= self.doc.wb.sheets.len() {
            return;
        }
        self.modify("Move Sheet", Scope::Workbook, |wb, _| wb.move_sheet(from, to));
        self.reset_views_keep();
        self.set_sheet_force(to);
    }

    pub fn rename_sheet(&mut self, idx: usize, name: &str) {
        if self.doc.wb.sheets[idx].name == name.trim() {
            return;
        }
        let mut wb2 = self.doc.wb.clone();
        match wb2.rename_sheet(idx, name) {
            Ok(()) => {
                let n = name.to_string();
                self.modify("Rename Sheet", Scope::Workbook, |wb, _| {
                    let _ = wb.rename_sheet(idx, &n);
                });
            }
            Err(e) => self.message("RExcel", e),
        }
    }

    pub fn set_sheet_hidden(&mut self, idx: usize, hidden: bool) {
        if hidden && self.doc.wb.sheets.iter().filter(|s| !s.hidden).count() <= 1 {
            self.message("RExcel", "A workbook must contain at least one visible worksheet.");
            return;
        }
        self.modify(if hidden { "Hide Sheet" } else { "Unhide Sheet" }, Scope::Workbook, |wb, _| wb.sheets[idx].hidden = hidden);
        if hidden && idx == self.sheet {
            if let Some(i) = self.doc.wb.sheets.iter().position(|s| !s.hidden) {
                self.set_sheet_force(i);
            }
        }
        if !hidden {
            self.set_sheet_force(idx);
        }
    }

    pub fn reset_views_keep(&mut self) {
        let sel = self.sel.clone();
        let scroll = self.scroll;
        self.reset_views();
        self.sel = sel;
        self.scroll = scroll;
    }

    pub fn set_sheet_force(&mut self, idx: usize) {
        let idx = idx.min(self.doc.wb.sheets.len() - 1);
        if idx != self.sheet {
            self.sheet = idx;
            self.sel = crate::app::Selection::single(CellPos::new(0, 0));
            self.scroll = [0.0, 0.0];
        }
        self.selected_chart = None;
        self.touch();
    }

    // ------------------------------------------------------------------
    // view
    // ------------------------------------------------------------------

    pub fn set_zoom(&mut self, z: f32) {
        let z = (z * 100.0).round() / 100.0;
        self.doc.wb.sheets[self.sheet].zoom = z.clamp(0.1, 4.0);
    }

    pub fn freeze_at_selection(&mut self) {
        let p = self.sel.active;
        // Freeze above/left of the active cell, relative to the current scroll
        self.modify("Freeze Panes", Scope::Sheet(self.sheet), |wb, s| {
            wb.sheets[s].freeze_rows = p.row;
            wb.sheets[s].freeze_cols = p.col;
        });
        self.scroll = [0.0, 0.0];
    }

    pub fn freeze(&mut self, rows: u32, cols: u32) {
        self.modify("Freeze Panes", Scope::Sheet(self.sheet), |wb, s| {
            wb.sheets[s].freeze_rows = rows;
            wb.sheets[s].freeze_cols = cols;
        });
        self.scroll = [0.0, 0.0];
    }

    // ------------------------------------------------------------------
    // formulas
    // ------------------------------------------------------------------

    /// AutoSum (or Average/Count/Max/Min) of the adjacent range.
    pub fn autosum(&mut self, func: &str) {
        self.commit_if_editing();
        let r = self.sel.primary();
        let wb = &self.doc.wb;
        if r.cell_count() > 1 && r.cell_count() < 100_000 {
            // multi-cell: put totals below each column of the selection
            let below = r.bottom + 1;
            if below >= MAX_ROWS {
                return;
            }
            let target = Rect { top: below, bottom: below, left: r.left, right: r.right };
            let f = func.to_string();
            self.modify("AutoSum", Scope::Cells { sheet: self.sheet, rects: vec![target] }, |wb, s| {
                for c in r.left..=r.right {
                    let rng = Rect { top: r.top, bottom: r.bottom, left: c, right: c };
                    let _ = wb.set_input(s, CellPos::new(below, c), &format!("={}({})", f, rng.to_a1()));
                }
            });
            return;
        }
        let p = self.sel.active;
        let range = ops::autosum_range(wb, self.sheet, p);
        let text = match range {
            Some(rg) => format!("={}({})", func, rg.to_a1()),
            None => format!("={}()", func),
        };
        let cursor = if range.is_some() { text.chars().count() } else { text.chars().count() - 1 };
        self.begin_edit(Some(text), EditMode::Edit, false);
        if let Some(e) = self.edit.as_mut() {
            e.cursor = cursor;
            e.set_cursor = Some(cursor);
        }
    }

    pub fn open_insert_function(&mut self) {
        self.dialog = Some(Dialog::InsertFunction { search: String::new(), category: "All".into(), selected: None });
    }

    pub fn insert_function_text(&mut self, name: &str) {
        if self.edit.is_none() {
            self.begin_edit(Some("=".into()), EditMode::Edit, false);
        }
        if let Some(e) = self.edit.as_mut() {
            let mut chars: Vec<char> = e.text.chars().collect();
            let ins: Vec<char> = if chars.is_empty() { format!("={}(", name) } else { format!("{}(", name) }.chars().collect();
            let at = e.cursor.min(chars.len());
            chars.splice(at..at, ins.iter().cloned());
            e.text = chars.into_iter().collect();
            e.cursor = at + ins.len();
            e.set_cursor = Some(e.cursor);
            e.request_focus = true;
        }
    }

    // ------------------------------------------------------------------
    // data
    // ------------------------------------------------------------------

    /// The range a data command should work on: the selection, or the current region.
    pub fn data_range(&self) -> Rect {
        let r = self.sel.primary();
        if r.cell_count() == 1 {
            ops::current_region(&self.doc.wb, self.sheet, self.sel.active)
        } else {
            match self.sheet_ref().data_extent() {
                Some(e) => r.intersection(&e).unwrap_or(r),
                None => r,
            }
        }
    }

    pub fn sort_simple(&mut self, ascending: bool) {
        let r = self.data_range();
        let header = ops::guess_header(&self.doc.wb, self.sheet, r);
        let col = self.sel.active.col.clamp(r.left, r.right);
        self.modify("Sort", Scope::Sheet(self.sheet), |wb, s| ops::sort(wb, s, r, &[SortKey { col, ascending }], header));
        self.select_range(r, r.top_left());
    }

    pub fn open_custom_sort(&mut self) {
        let r = self.data_range();
        let header = ops::guess_header(&self.doc.wb, self.sheet, r);
        self.dialog = Some(Dialog::Sort { range: r, header, keys: vec![(r.left.max(self.sel.active.col.min(r.right)), true)] });
    }

    pub fn toggle_autofilter(&mut self) {
        if self.sheet_ref().autofilter.is_some() {
            self.modify("Filter", Scope::Sheet(self.sheet), |wb, s| {
                let sh = &mut wb.sheets[s];
                if let Some(af) = sh.autofilter.take() {
                    for r in af.range.top..=af.range.bottom {
                        sh.hidden_rows.remove(&r);
                    }
                }
            });
            return;
        }
        let r = self.data_range();
        self.modify("Filter", Scope::Sheet(self.sheet), |wb, s| {
            wb.sheets[s].autofilter = Some(AutoFilter { range: r, allowed: Default::default() });
        });
    }

    /// Re-apply the autofilter: hide rows whose values are not allowed.
    pub fn apply_filter(sheet: &mut engine::Sheet, wb_values: &dyn Fn(CellPos) -> String) {
        let Some(af) = sheet.autofilter.clone() else { return };
        for r in af.range.top + 1..=af.range.bottom {
            let visible = af.allowed.iter().all(|(off, set)| set.contains(&wb_values(CellPos::new(r, af.range.left + off))));
            if visible {
                sheet.hidden_rows.remove(&r);
            } else {
                sheet.hidden_rows.insert(r);
            }
        }
    }

    pub fn open_validation(&mut self) {
        let r = self.sel.primary();
        let existing = ops::validation_at(&self.doc.wb, self.sheet, self.sel.active).cloned();
        self.dialog = Some(Dialog::validation(r, existing));
    }

    pub fn edit_comment(&mut self) {
        let p = self.sel.active;
        let text = self.sheet_ref().comments.get(&p).cloned().unwrap_or_default();
        self.dialog = Some(Dialog::Comment { cell: p, text });
    }

    pub fn open_hyperlink(&mut self) {
        let p = self.sel.active;
        let url = self.sheet_ref().hyperlinks.get(&p).cloned().unwrap_or_default();
        let display = crate::grid::display_text(&self.doc.wb, self.sheet, p);
        self.dialog = Some(Dialog::Hyperlink { cell: p, url, display });
    }

    pub fn open_find(&mut self, replace: bool) {
        self.commit_if_editing();
        let mut st = self.find.take().unwrap_or_default();
        st.replace_mode = replace;
        st.focus = true;
        self.find = Some(st);
    }

    pub fn insert_chart(&mut self, kind: ChartKind) {
        let r = self.data_range();
        if r.cell_count() < 2 {
            self.message("Insert Chart", "Select the data you want to chart first.");
            return;
        }
        let header = ops::guess_header(&self.doc.wb, self.sheet, r) || matches!(self.doc.wb.value(self.sheet, r.top_left()), Value::Text(_) | Value::Empty);
        let anchor = CellPos::new(r.top, (r.right + 2).min(MAX_COLS - 1));
        let chart = Chart {
            kind,
            title: if kind == ChartKind::Pie || r.cols() == 2 {
                crate::grid::display_text(&self.doc.wb, self.sheet, CellPos::new(r.top, r.left + 1))
            } else {
                "Chart Title".into()
            },
            data: r,
            has_header: header,
            series_in_columns: true,
            anchor,
            width: 480.0,
            height: 288.0,
        };
        self.modify("Insert Chart", Scope::Sheet(self.sheet), |wb, s| wb.sheets[s].charts.push(chart));
        self.selected_chart = Some(self.sheet_ref().charts.len() - 1);
        self.scroll_to = Some(anchor);
    }

    pub fn set_calc_manual(&mut self, manual: bool) {
        self.doc.wb.manual_calc = manual;
        if !manual {
            self.doc.wb.recalculate();
            self.touch();
        }
    }
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end_matches('\n').to_string()
}
