//! The virtualized cell grid: layout math, painting and mouse interaction.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Key, KeyboardShortcut, Modifiers, Painter, Pos2, Rect as ERect, Sense, Shape, Stroke, StrokeKind, TextFormat};
use engine::address::{col_to_letters, CellPos, Rect, MAX_COLS, MAX_ROWS};
use engine::condfmt::CfFormat;
use engine::style::{BorderKind, BorderLine, HAlign, Style, VAlign};
use engine::workbook::{DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT};
use engine::{Value, Workbook};

use crate::app::{bar_editor_id, cell_editor_id, App, EditMode};
use crate::theme::{self, to_c32};

/// Points per Excel font point (96 dpi).
pub const PT_PX: f32 = 96.0 / 72.0;
const OVERFLOW_COLS: u32 = 24;

// ======================================================================
// Axis layout
// ======================================================================

/// Positions along one axis with a default size and sparse custom sizes.
#[derive(Clone)]
pub struct Axis {
    def: f64,
    n: u32,
    idx: Vec<u32>,
    size: Vec<f64>,
    prefix: Vec<f64>,
}

impl Axis {
    pub fn new(def: f32, n: u32, sizes: &HashMap<u32, f32>, hidden: &HashSet<u32>) -> Axis {
        let mut m: Vec<(u32, f64)> = sizes.iter().filter(|(i, _)| **i < n).map(|(i, s)| (*i, *s as f64)).collect();
        for h in hidden {
            if *h < n {
                m.push((*h, 0.0));
            }
        }
        m.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.partial_cmp(&b.1).unwrap()));
        m.dedup_by_key(|x| x.0); // hidden (0.0) sorts first and wins
        let mut prefix = Vec::with_capacity(m.len() + 1);
        prefix.push(0.0);
        let def = def as f64;
        for (_, s) in &m {
            let last = *prefix.last().unwrap();
            prefix.push(last + s - def);
        }
        Axis { def, n, idx: m.iter().map(|x| x.0).collect(), size: m.iter().map(|x| x.1).collect(), prefix }
    }
    pub fn size(&self, i: u32) -> f64 {
        match self.idx.binary_search(&i) {
            Ok(k) => self.size[k],
            Err(_) => self.def,
        }
    }
    pub fn pos(&self, i: u32) -> f64 {
        let k = self.idx.partition_point(|&x| x < i);
        i as f64 * self.def + self.prefix[k]
    }
    /// Index containing position `p` (largest i with pos(i) <= p).
    pub fn index_at(&self, p: f64) -> u32 {
        if p <= 0.0 {
            return self.first_visible(0);
        }
        let (mut lo, mut hi) = (0u32, self.n);
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if self.pos(mid) <= p {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn first_visible(&self, mut i: u32) -> u32 {
        while i + 1 < self.n && self.size(i) == 0.0 {
            i += 1;
        }
        i
    }
}

/// Screen geometry of the grid for one frame.
pub struct Geo {
    pub cells: ERect,
    pub z: f32,
    pub rows: Axis,
    pub cols: Axis,
    pub fr: u32,
    pub fc: u32,
    pub sx: f64,
    pub sy: f64,
}

impl Geo {
    pub fn frozen_w(&self) -> f32 {
        (self.cols.pos(self.fc) * self.z as f64) as f32
    }
    pub fn frozen_h(&self) -> f32 {
        (self.rows.pos(self.fr) * self.z as f64) as f32
    }
    pub fn col_x(&self, c: u32) -> f32 {
        let off = if c >= self.fc { self.sx } else { 0.0 };
        self.cells.left() + ((self.cols.pos(c) - off) * self.z as f64) as f32
    }
    pub fn row_y(&self, r: u32) -> f32 {
        let off = if r >= self.fr { self.sy } else { 0.0 };
        self.cells.top() + ((self.rows.pos(r) - off) * self.z as f64) as f32
    }
    pub fn col_w(&self, c: u32) -> f32 {
        (self.cols.size(c) * self.z as f64) as f32
    }
    pub fn row_h(&self, r: u32) -> f32 {
        (self.rows.size(r) * self.z as f64) as f32
    }
    pub fn cell_rect(&self, p: CellPos) -> ERect {
        ERect::from_min_size(pos2(self.col_x(p.col), self.row_y(p.row)), vec2(self.col_w(p.col), self.row_h(p.row)))
    }
    pub fn range_rect(&self, r: Rect) -> ERect {
        let x0 = self.col_x(r.left);
        let y0 = self.row_y(r.top);
        let x1 = self.col_x(r.right) + self.col_w(r.right);
        let y1 = self.row_y(r.bottom) + self.row_h(r.bottom);
        ERect::from_min_max(pos2(x0, y0), pos2(x1.max(x0), y1.max(y0)))
    }
    pub fn col_at(&self, x: f32) -> u32 {
        let dx = ((x - self.cells.left()) / self.z) as f64;
        let p = if dx < self.cols.pos(self.fc) { dx } else { dx + self.sx };
        self.cols.index_at(p).min(MAX_COLS - 1)
    }
    pub fn row_at(&self, y: f32) -> u32 {
        let dy = ((y - self.cells.top()) / self.z) as f64;
        let p = if dy < self.rows.pos(self.fr) { dy } else { dy + self.sy };
        self.rows.index_at(p).min(MAX_ROWS - 1)
    }
    pub fn cell_at(&self, p: Pos2) -> CellPos {
        CellPos::new(self.row_at(p.y), self.col_at(p.x))
    }
    /// Visible scrollable columns (first, last).
    pub fn scroll_cols(&self) -> (u32, u32) {
        let start = self.cols.pos(self.fc) + self.sx;
        let w = ((self.cells.width() - self.frozen_w()).max(0.0) / self.z) as f64;
        (self.cols.index_at(start).max(self.fc), self.cols.index_at(start + w).min(MAX_COLS - 1))
    }
    pub fn scroll_rows(&self) -> (u32, u32) {
        let start = self.rows.pos(self.fr) + self.sy;
        let h = ((self.cells.height() - self.frozen_h()).max(0.0) / self.z) as f64;
        (self.rows.index_at(start).max(self.fr), self.rows.index_at(start + h).min(MAX_ROWS - 1))
    }
}

// ======================================================================
// Cell text
// ======================================================================

/// Text shown in a cell (number format applied).
pub fn display_text(wb: &Workbook, sheet: usize, p: CellPos) -> String {
    engine::ops::display_text(wb, sheet, p)
}

struct CellText {
    text: String,
    color: Color32,
    font: FontId,
    fake_bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    h: HAlign,
    v: VAlign,
    wrap: bool,
    indent: f32,
    is_number: bool,
    general: bool,
    value: Value,
}

fn resolved_halign(st: &Style, v: &Value, show_formula: bool) -> HAlign {
    match st.h_align {
        HAlign::General => {
            if show_formula {
                return HAlign::Left;
            }
            match v {
                Value::Number(_) => HAlign::Right,
                Value::Bool(_) | Value::Error(_) => HAlign::Center,
                _ => HAlign::Left,
            }
        }
        HAlign::Fill | HAlign::Justify => HAlign::Left,
        HAlign::CenterAcross => HAlign::Center,
        a => a,
    }
}

fn cell_text(app: &App, wb: &Workbook, sheet: usize, p: CellPos, z: f32, cf: Option<&CfFormat>) -> Option<CellText> {
    let s = &wb.sheets[sheet];
    let cell = s.cells.get(&p);
    let st = wb.style(cell.map(|c| c.style).unwrap_or(0));
    let show_formula = app.show_formulas && cell.map(|c| c.formula.is_some()).unwrap_or(false);
    let v = s.value_at(p);
    let (text, fmt_color) = if show_formula {
        (wb.input_text(sheet, p), None)
    } else {
        if v.is_empty() {
            return None;
        }
        let f = engine::numfmt::format_value(&v, &st.num_fmt);
        (f.text, f.color)
    };
    if text.is_empty() {
        return None;
    }
    let is_link = s.hyperlinks.contains_key(&p);
    let color = cf
        .and_then(|c| c.font_color)
        .map(to_c32)
        .or(fmt_color.map(to_c32))
        .or(st.font_color.map(to_c32))
        .unwrap_or(if is_link { Color32::from_rgb(0x05, 0x63, 0xC1) } else { Color32::BLACK });
    let bold = st.bold || cf.map(|c| c.bold).unwrap_or(false);
    let (font, fake_bold) = crate::fonts::cell_font(st.font_name.as_deref(), bold, st.font_size() * PT_PX * z);
    Some(CellText {
        is_number: matches!(v, Value::Number(_)) && !show_formula,
        general: engine::numfmt::is_general(&st.num_fmt),
        h: resolved_halign(st, &v, show_formula),
        v: st.v_align,
        wrap: st.wrap,
        indent: st.indent as f32 * 9.0 * z,
        text,
        color,
        font,
        fake_bold,
        italic: st.italic || cf.map(|c| c.italic).unwrap_or(false),
        underline: st.underline || is_link,
        strike: st.strike,
        value: v,
    })
}

fn layout(painter: &Painter, t: &CellText, text: &str, wrap_width: Option<f32>) -> Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    let stroke = Stroke::new((t.font.size / 14.0).max(1.0), t.color);
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: t.font.clone(),
            color: t.color,
            italics: t.italic,
            underline: if t.underline { stroke } else { Stroke::NONE },
            strikethrough: if t.strike { stroke } else { Stroke::NONE },
            ..Default::default()
        },
    );
    if let Some(w) = wrap_width {
        job.wrap.max_width = w.max(1.0);
    }
    painter.layout_job(job)
}

/// Shrink a General-format number to fit `avail` px, or return hashes.
fn fit_number(painter: &Painter, t: &CellText, avail: f32) -> String {
    let w = |s: &str| layout(painter, t, s, None).size().x;
    if w(&t.text) <= avail {
        return t.text.clone();
    }
    if t.general {
        if let Value::Number(n) = t.value {
            let s = t.text.clone();
            if let Some(dot) = s.find('.') {
                if !s.contains('E') {
                    let decimals = s.len() - dot - 1;
                    for d in (0..decimals).rev() {
                        let cand = format!("{:.*}", d, n);
                        if w(&cand) <= avail {
                            return cand;
                        }
                    }
                }
            }
            for d in (0..5).rev() {
                let cand = format!("{:.*E}", d, n);
                let cand = cand.replace("E", "E+").replace("E+-", "E-");
                if w(&cand) <= avail {
                    return cand;
                }
            }
        }
    }
    let hw = w("#").max(1.0);
    "#".repeat(((avail / hw).floor() as usize).max(1))
}

// ======================================================================
// Interaction state
// ======================================================================

#[derive(Debug, Clone)]
pub enum Drag {
    Cells,
    Rows { anchor: u32 },
    Cols { anchor: u32 },
    ResizeCol { start_x: f32, start_w: f32, cols: Vec<u32> },
    ResizeRow { start_y: f32, start_h: f32, rows: Vec<u32> },
    Fill { src: Rect, target: Rect },
    Point { sheet: usize, anchor: CellPos },
    VScroll { grab: f32 },
    HScroll { grab: f32 },
    Chart { idx: usize, resize: bool, start: Pos2, orig_anchor: CellPos, orig_size: egui::Vec2, offset: egui::Vec2 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CtxTarget {
    #[default]
    Cells,
    Rows,
    Cols,
}

#[derive(Default)]
pub struct GridState {
    pub drag: Option<Drag>,
    pub ctx_target: CtxTarget,
}

// ======================================================================
// show
// ======================================================================

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let full = ui.available_rect_before_wrap();
    let ctx = ui.ctx().clone();
    let z = app.sheet_ref().zoom.clamp(0.1, 4.0);
    let headings = app.show_headings;
    let sb = 16.0;
    let tabs_h = 26.0;
    let hdr_h = if headings { (20.0 * z).max(14.0) } else { 0.0 };
    // row header width from the largest visible row number
    let est_last_row = {
        let first = (app.scroll[1] / DEFAULT_ROW_HEIGHT as f64) as u32;
        first + ((full.height() / (DEFAULT_ROW_HEIGHT * z)) as u32) + 2
    };
    let digits = (est_last_row.max(999) + 1).to_string().len() as f32;
    let hdr_w = if headings { (digits * 7.5 * z.max(0.8) + 10.0).max(26.0) } else { 0.0 };

    let bottom_strip = ERect::from_min_max(pos2(full.left(), full.bottom() - tabs_h), full.right_bottom());
    let vbar = ERect::from_min_max(pos2(full.right() - sb, full.top()), pos2(full.right(), bottom_strip.top()));
    let grid_rect = ERect::from_min_max(full.left_top(), pos2(vbar.left(), bottom_strip.top()));
    let cells_rect = ERect::from_min_max(pos2(grid_rect.left() + hdr_w, grid_rect.top() + hdr_h), grid_rect.right_bottom());

    // ---- geometry ----
    let geo = {
        let s = app.sheet_ref();
        let mut row_sizes = s.row_heights.clone();
        let mut col_sizes = s.col_widths.clone();
        // ignore pathological sizes
        row_sizes.retain(|_, v| v.is_finite() && *v >= 0.0);
        col_sizes.retain(|_, v| v.is_finite() && *v >= 0.0);
        Geo {
            cells: cells_rect,
            z,
            rows: Axis::new(DEFAULT_ROW_HEIGHT, MAX_ROWS, &row_sizes, &s.hidden_rows),
            cols: Axis::new(DEFAULT_COL_WIDTH, MAX_COLS, &col_sizes, &s.hidden_cols),
            fr: s.freeze_rows.min(MAX_ROWS - 1),
            fc: s.freeze_cols.min(MAX_COLS - 1),
            sx: app.scroll[0],
            sy: app.scroll[1],
        }
    };
    let mut geo = geo;

    // ---- scroll to keep the active cell visible ----
    if let Some(p) = app.scroll_to.take() {
        let (mut sx, mut sy) = (geo.sx, geo.sy);
        if p.col >= geo.fc {
            let x0 = geo.cols.pos(p.col) - geo.cols.pos(geo.fc);
            let x1 = geo.cols.pos(p.col + 1) - geo.cols.pos(geo.fc);
            let vw = ((cells_rect.width() - geo.frozen_w()) / z) as f64;
            if x0 < sx {
                sx = x0;
            } else if x1 > sx + vw {
                sx = (x1 - vw).min(x0);
            }
        }
        if p.row >= geo.fr {
            let y0 = geo.rows.pos(p.row) - geo.rows.pos(geo.fr);
            let y1 = geo.rows.pos(p.row + 1) - geo.rows.pos(geo.fr);
            let vh = ((cells_rect.height() - geo.frozen_h()) / z) as f64;
            if y0 < sy {
                sy = y0;
            } else if y1 > sy + vh {
                sy = (y1 - vh).min(y0);
            }
        }
        geo.sx = sx.max(0.0);
        geo.sy = sy.max(0.0);
        app.scroll = [geo.sx, geo.sy];
    }

    // ---- interaction ----
    let grid_id = ui.scope_id().with("grid");
    let resp = ui.interact(grid_rect, grid_id, Sense::click_and_drag());
    handle_wheel(app, &ctx, &resp, &mut geo);
    handle_pointer(app, ui, &ctx, &resp, &mut geo, hdr_w, hdr_h);

    // page sizes for PgUp/PgDn
    let (r0, r1) = geo.scroll_rows();
    let (c0, c1) = geo.scroll_cols();
    app.page_rows = (r1.saturating_sub(r0)).max(1);
    app.page_cols = (c1.saturating_sub(c0)).max(1);

    // ---- painting ----
    let painter = ui.painter_at(grid_rect);
    painter.rect_filled(cells_rect, 0.0, Color32::WHITE);
    let fw = geo.frozen_w();
    let fh = geo.frozen_h();
    let row_bands: Vec<(u32, u32, f32, f32)> = {
        let mut v = Vec::new();
        if geo.fr > 0 {
            v.push((0, geo.fr - 1, cells_rect.top(), cells_rect.top() + fh));
        }
        v.push((r0, r1, cells_rect.top() + fh, cells_rect.bottom()));
        v
    };
    let col_bands: Vec<(u32, u32, f32, f32)> = {
        let mut v = Vec::new();
        if geo.fc > 0 {
            v.push((0, geo.fc - 1, cells_rect.left(), cells_rect.left() + fw));
        }
        v.push((c0, c1, cells_rect.left() + fw, cells_rect.right()));
        v
    };
    let mut areas = Vec::new();
    for (ra, rb, _, _) in &row_bands {
        for (ca, cb, _, _) in &col_bands {
            areas.push(Rect { top: *ra, bottom: *rb, left: *ca, right: *cb });
        }
    }
    let cf = cf_for(app, areas);
    for (ra, rb, y0, y1) in &row_bands {
        for (ca, cb, x0, x1) in &col_bands {
            let clip = ERect::from_min_max(pos2(*x0, *y0), pos2(*x1, *y1));
            if clip.width() <= 0.0 || clip.height() <= 0.0 {
                continue;
            }
            let p = painter.with_clip_rect(clip.intersect(cells_rect));
            paint_cells(app, &p, &geo, *ra, *rb, *ca, *cb, &cf);
            paint_overlays(app, &p, &geo, &ctx);
        }
    }
    // freeze lines
    if geo.fr > 0 {
        painter.hline(cells_rect.x_range(), cells_rect.top() + fh, Stroke::new(1.0, Color32::from_gray(150)));
    }
    if geo.fc > 0 {
        painter.vline(cells_rect.left() + fw, cells_rect.y_range(), Stroke::new(1.0, Color32::from_gray(150)));
    }
    if headings {
        paint_headers(app, &painter, &geo, grid_rect, hdr_w, hdr_h, r0, r1, c0, c1);
    }

    // ---- charts ----
    charts_ui(app, ui, &geo, cells_rect);

    // ---- overlay widgets ----
    filter_buttons(app, ui, &geo, cells_rect);
    validation_dropdown(app, ui, &geo, cells_rect);
    comment_tooltip(app, ui, &geo, &resp);
    cell_editor(app, ui, &geo, cells_rect);

    // ---- context menu ----
    resp.context_menu(|ui| crate::dialogs::grid_context_menu(app, ui));

    // ---- scrollbars & tabs ----
    scrollbars(app, ui, &geo, vbar, bottom_strip);
    painter.rect_filled(ERect::from_min_max(pos2(vbar.left(), bottom_strip.top()), bottom_strip.right_bottom()), 0.0, theme::RIBBON_BG);
    app.scroll = [geo.sx.max(0.0), geo.sy.max(0.0)];
}

fn cf_for(app: &mut App, areas: Vec<Rect>) -> Arc<HashMap<CellPos, CfFormat>> {
    if app.sheet_ref().cond_formats.is_empty() {
        return Arc::new(HashMap::new());
    }
    let key = (app.sheet, areas.clone(), app.generation);
    if let Some((k, m)) = &app.cf_cache {
        if *k == key {
            return m.clone();
        }
    }
    let mut all = HashMap::new();
    for a in &areas {
        all.extend(engine::condfmt::evaluate(&app.doc.wb, app.sheet, *a));
    }
    let m = Arc::new(all);
    app.cf_cache = Some((key, m.clone()));
    m
}

fn handle_wheel(app: &mut App, ctx: &egui::Context, resp: &egui::Response, geo: &mut Geo) {
    if !resp.contains_pointer() {
        return;
    }
    let (delta, zoom, cmd) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command));
    if (zoom - 1.0).abs() > 1e-4 {
        let s = &mut app.doc.wb.sheets[app.sheet];
        s.zoom = (s.zoom * zoom).clamp(0.1, 4.0);
        return;
    }
    if cmd {
        return;
    }
    if delta != egui::Vec2::ZERO {
        geo.sx = (geo.sx - (delta.x / geo.z) as f64).max(0.0);
        geo.sy = (geo.sy - (delta.y / geo.z) as f64).max(0.0);
        app.scroll = [geo.sx, geo.sy];
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_pointer(app: &mut App, ui: &egui::Ui, ctx: &egui::Context, resp: &egui::Response, geo: &mut Geo, hdr_w: f32, hdr_h: f32) {
    let cells = geo.cells;
    let (pressed, down, released, pos, mods, sec) = ctx.input(|i| {
        (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.interact_pos().or(i.pointer.hover_pos()), i.modifiers, i.pointer.secondary_pressed())
    });
    let Some(pos) = pos else { return };
    let hovered = resp.hovered();
    let in_col_hdr = pos.y >= cells.top() - hdr_h && pos.y < cells.top() && pos.x >= cells.left() && hdr_h > 0.0;
    let in_row_hdr = pos.x >= cells.left() - hdr_w && pos.x < cells.left() && pos.y >= cells.top() && hdr_w > 0.0;
    let in_corner = pos.x < cells.left() && pos.y < cells.top() && pos.x >= cells.left() - hdr_w && pos.y >= cells.top() - hdr_h;
    let in_cells = cells.contains(pos);

    // resize borders in headers
    let col_border = if in_col_hdr {
        let c = geo.col_at(pos.x);
        let right = geo.col_x(c) + geo.col_w(c);
        let left = geo.col_x(c);
        if (pos.x - right).abs() <= 4.0 {
            Some(c)
        } else if (pos.x - left).abs() <= 3.0 && c > 0 {
            // border of the previous visible column
            let mut p = c - 1;
            while p > 0 && geo.col_w(p) == 0.0 {
                p -= 1;
            }
            Some(p)
        } else {
            None
        }
    } else {
        None
    };
    let row_border = if in_row_hdr {
        let r = geo.row_at(pos.y);
        let bottom = geo.row_y(r) + geo.row_h(r);
        let top = geo.row_y(r);
        if (pos.y - bottom).abs() <= 3.0 {
            Some(r)
        } else if (pos.y - top).abs() <= 2.0 && r > 0 {
            let mut p = r - 1;
            while p > 0 && geo.row_h(p) == 0.0 {
                p -= 1;
            }
            Some(p)
        } else {
            None
        }
    } else {
        None
    };
    // fill handle
    let primary = app.sel.primary();
    let handle_pos = {
        let rr = geo.range_rect(primary);
        rr.right_bottom()
    };
    let on_fill_handle = in_cells && app.edit.is_none() && app.sel.ranges.len() == 1 && (pos - handle_pos).length() <= 5.0;

    if hovered && app.grid.drag.is_none() {
        if col_border.is_some() {
            ctx.set_cursor_icon(CursorIcon::ResizeColumn);
        } else if row_border.is_some() {
            ctx.set_cursor_icon(CursorIcon::ResizeRow);
        } else if on_fill_handle {
            ctx.set_cursor_icon(CursorIcon::Crosshair);
        } else if in_cells {
            ctx.set_cursor_icon(if app.format_painter.is_some() { CursorIcon::Copy } else { CursorIcon::Cell });
        } else if in_col_hdr {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }
    }

    // double clicks
    if resp.double_clicked() {
        if let Some(c) = col_border {
            autofit_cols(app, ui, if app.sel.primary().top == 0 && app.sel.primary().bottom == MAX_ROWS - 1 && app.sel.primary().left <= c && c <= app.sel.primary().right { (app.sel.primary().left..=app.sel.primary().right).collect() } else { vec![c] });
            app.grid.drag = None;
            return;
        }
        if let Some(r) = row_border {
            autofit_rows(app, ui, vec![r]);
            app.grid.drag = None;
            return;
        }
        if on_fill_handle || matches!(app.grid.drag, Some(Drag::Fill { .. })) {
            app.grid.drag = None;
            if let Some(bottom) = engine::ops::fill_down_extent(&app.doc.wb, app.sheet, primary) {
                let dst = Rect { bottom, ..primary };
                app.fill_to(primary, dst);
            }
            return;
        }
        if in_cells && app.edit.is_none() {
            let p = geo.cell_at(pos);
            let link = app.sheet_ref().hyperlinks.get(&p).cloned();
            if let Some(l) = link {
                follow_link(app, ctx, &l);
                return;
            }
            app.select_cell(p);
            app.begin_edit(None, EditMode::Edit, false);
            app.grid.drag = None;
            return;
        }
    }

    if sec && hovered {
        // right click: select the clicked cell/header if it isn't selected yet
        if in_cells {
            let p = geo.cell_at(pos);
            if !app.sel.contains(p) {
                app.commit_if_editing();
                app.select_cell(p);
            }
            app.grid.ctx_target = CtxTarget::Cells;
            // select a chart under the pointer? (handled by chart widgets)
        } else if in_col_hdr {
            let c = geo.col_at(pos.x);
            let r = app.sel.primary();
            if !(r.top == 0 && r.bottom == MAX_ROWS - 1 && r.left <= c && c <= r.right) {
                app.select_range(Rect { top: 0, bottom: MAX_ROWS - 1, left: c, right: c }, CellPos::new(geo.row_at(cells.top() + 1.0).max(0), c));
            }
            app.grid.ctx_target = CtxTarget::Cols;
        } else if in_row_hdr {
            let rr = geo.row_at(pos.y);
            let r = app.sel.primary();
            if !(r.left == 0 && r.right == MAX_COLS - 1 && r.top <= rr && rr <= r.bottom) {
                app.select_range(Rect { top: rr, bottom: rr, left: 0, right: MAX_COLS - 1 }, CellPos::new(rr, geo.col_at(cells.left() + 1.0)));
            }
            app.grid.ctx_target = CtxTarget::Rows;
        }
    }

    if pressed && hovered {
        app.filter_popup = None;
        app.validation_popup = None;
        if !in_cells || app.edit.is_none() {
            // keyboard focus back to the grid
            if let Some(id) = ctx.memory(|m| m.focused()) {
                if id != cell_editor_id() && id != bar_editor_id() {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
            }
        }
        if let Some(c) = col_border {
            let r = app.sel.primary();
            let cols: Vec<u32> = if r.top == 0 && r.bottom == MAX_ROWS - 1 && r.left <= c && c <= r.right && r.cols() < 5000 { (r.left..=r.right).collect() } else { vec![c] };
            app.grid.drag = Some(Drag::ResizeCol { start_x: pos.x, start_w: geo.cols.size(c) as f32, cols });
        } else if let Some(rw) = row_border {
            let r = app.sel.primary();
            let rows: Vec<u32> = if r.left == 0 && r.right == MAX_COLS - 1 && r.top <= rw && rw <= r.bottom && r.rows() < 100_000 { (r.top..=r.bottom).collect() } else { vec![rw] };
            app.grid.drag = Some(Drag::ResizeRow { start_y: pos.y, start_h: geo.rows.size(rw) as f32, rows });
        } else if in_corner {
            app.commit_if_editing();
            app.select_range(Rect { top: 0, left: 0, bottom: MAX_ROWS - 1, right: MAX_COLS - 1 }, app.sel.active);
        } else if in_col_hdr {
            if app.commit_if_editing() {
                let c = geo.col_at(pos.x);
                let top_row = geo.row_at(cells.top() + 1.0);
                if mods.shift {
                    let a = app.sel.anchor.col;
                    let r = Rect { top: 0, bottom: MAX_ROWS - 1, left: a.min(c), right: a.max(c) };
                    let act = app.sel.active;
                    app.select_range(r, act);
                    app.sel.anchor = CellPos::new(0, a);
                    app.grid.drag = Some(Drag::Cols { anchor: a });
                } else {
                    let r = Rect { top: 0, bottom: MAX_ROWS - 1, left: c, right: c };
                    if mods.command {
                        app.sel.ranges.push(r);
                        app.sel.active = CellPos::new(top_row, c);
                        app.sel.anchor = CellPos::new(0, c);
                    } else {
                        app.select_range(r, CellPos::new(top_row, c));
                    }
                    app.grid.drag = Some(Drag::Cols { anchor: c });
                }
                app.scroll_to = None;
            }
        } else if in_row_hdr {
            if app.commit_if_editing() {
                let rw = geo.row_at(pos.y);
                let left_col = geo.col_at(cells.left() + 1.0);
                if mods.shift {
                    let a = app.sel.anchor.row;
                    let r = Rect { top: a.min(rw), bottom: a.max(rw), left: 0, right: MAX_COLS - 1 };
                    let act = app.sel.active;
                    app.select_range(r, act);
                    app.sel.anchor = CellPos::new(a, 0);
                    app.grid.drag = Some(Drag::Rows { anchor: a });
                } else {
                    let r = Rect { top: rw, bottom: rw, left: 0, right: MAX_COLS - 1 };
                    if mods.command {
                        app.sel.ranges.push(r);
                        app.sel.active = CellPos::new(rw, left_col);
                        app.sel.anchor = CellPos::new(rw, 0);
                    } else {
                        app.select_range(r, CellPos::new(rw, left_col));
                    }
                    app.grid.drag = Some(Drag::Rows { anchor: rw });
                }
                app.scroll_to = None;
            }
        } else if on_fill_handle {
            app.grid.drag = Some(Drag::Fill { src: primary, target: primary });
        } else if in_cells {
            let p = geo.cell_at(pos);
            if app.edit.is_some() && app.pointing_possible() {
                let sheet = app.sheet;
                if mods.shift {
                    if let Some(pt) = app.edit.as_ref().and_then(|e| e.point.clone()) {
                        app.point_to(pt.sheet, pt.anchor, p);
                        app.grid.drag = Some(Drag::Point { sheet, anchor: pt.anchor });
                        return;
                    }
                }
                app.point_to(sheet, p, p);
                app.grid.drag = Some(Drag::Point { sheet, anchor: p });
            } else {
                let editing_here = app.edit.as_ref().map(|e| e.cell == p && e.sheet == app.sheet).unwrap_or(false);
                if editing_here {
                    return;
                }
                if !app.commit_if_editing() {
                    return;
                }
                if mods.shift {
                    app.extend_to(p);
                } else if mods.command {
                    app.sel.ranges.push(app.merge_expand(Rect::single(p)));
                    app.sel.active = p;
                    app.sel.anchor = p;
                    app.sel.ext = p;
                } else {
                    app.select_cell(p);
                }
                app.scroll_to = None;
                app.grid.drag = Some(Drag::Cells);
                // Ctrl+click on a hyperlink follows it
                if mods.command {
                    if let Some(l) = app.sheet_ref().hyperlinks.get(&p).cloned() {
                        follow_link(app, ctx, &l);
                    }
                }
            }
        }
    }

    // ---- continue drags ----
    if down {
        // auto-scroll when dragging past the edges
        let dragging_sel = matches!(app.grid.drag, Some(Drag::Cells | Drag::Rows { .. } | Drag::Cols { .. } | Drag::Fill { .. } | Drag::Point { .. }));
        if dragging_sel {
            let mut scrolled = false;
            if pos.y > cells.bottom() {
                geo.sy += ((pos.y - cells.bottom()) / geo.z) as f64 * 0.5 + 4.0;
                scrolled = true;
            } else if pos.y < cells.top() + geo.frozen_h() && geo.sy > 0.0 && !matches!(app.grid.drag, Some(Drag::Cols { .. })) {
                geo.sy = (geo.sy - (((cells.top() + geo.frozen_h() - pos.y) / geo.z) as f64 * 0.5 + 4.0)).max(0.0);
                scrolled = true;
            }
            if pos.x > cells.right() {
                geo.sx += ((pos.x - cells.right()) / geo.z) as f64 * 0.5 + 4.0;
                scrolled = true;
            } else if pos.x < cells.left() + geo.frozen_w() && geo.sx > 0.0 && !matches!(app.grid.drag, Some(Drag::Rows { .. })) {
                geo.sx = (geo.sx - (((cells.left() + geo.frozen_w() - pos.x) / geo.z) as f64 * 0.5 + 4.0)).max(0.0);
                scrolled = true;
            }
            if scrolled {
                app.scroll = [geo.sx, geo.sy];
                ctx.request_repaint();
            }
        }
        let clamped = pos2(pos.x.clamp(cells.left() + 1.0, cells.right() - 1.0), pos.y.clamp(cells.top() + 1.0, cells.bottom() - 1.0));
        match app.grid.drag.clone() {
            Some(Drag::Cells) => {
                let p = geo.cell_at(clamped);
                if p != app.sel.ext {
                    let r = app.merge_expand(Rect::new(app.sel.anchor, p));
                    if let Some(last) = app.sel.ranges.last_mut() {
                        *last = r;
                    }
                    app.sel.ext = p;
                }
            }
            Some(Drag::Cols { anchor }) => {
                let c = geo.col_at(clamped.x);
                let r = Rect { top: 0, bottom: MAX_ROWS - 1, left: anchor.min(c), right: anchor.max(c) };
                if let Some(last) = app.sel.ranges.last_mut() {
                    *last = r;
                }
            }
            Some(Drag::Rows { anchor }) => {
                let rw = geo.row_at(clamped.y);
                let r = Rect { top: anchor.min(rw), bottom: anchor.max(rw), left: 0, right: MAX_COLS - 1 };
                if let Some(last) = app.sel.ranges.last_mut() {
                    *last = r;
                }
            }
            Some(Drag::ResizeCol { start_x, start_w, cols, .. }) => {
                let w = (start_w + (pos.x - start_x) / geo.z).max(0.0).round();
                for c in &cols {
                    app.doc.wb.sheets[app.sheet].col_widths.insert(*c, w);
                }
                ctx.set_cursor_icon(CursorIcon::ResizeColumn);
                app.set_status(format!("Width: {:.2} ({} pixels)", w / 7.0, w));
            }
            Some(Drag::ResizeRow { start_y, start_h, rows, .. }) => {
                let h = (start_h + (pos.y - start_y) / geo.z).max(0.0).round();
                for r in &rows {
                    app.doc.wb.sheets[app.sheet].row_heights.insert(*r, h);
                }
                ctx.set_cursor_icon(CursorIcon::ResizeRow);
                app.set_status(format!("Height: {:.2} ({} pixels)", h * 0.75, h));
            }
            Some(Drag::Fill { src, .. }) => {
                let p = geo.cell_at(clamped);
                // extend in the dominant direction
                let target = if p.row > src.bottom || p.row < src.top || p.col > src.right || p.col < src.left {
                    let dr = if p.row > src.bottom { p.row - src.bottom } else if p.row < src.top { src.top - p.row } else { 0 };
                    let dc = if p.col > src.right { p.col - src.right } else if p.col < src.left { src.left - p.col } else { 0 };
                    if dr >= dc {
                        if p.row > src.bottom {
                            Rect { bottom: p.row, ..src }
                        } else {
                            Rect { top: p.row, ..src }
                        }
                    } else if p.col > src.right {
                        Rect { right: p.col, ..src }
                    } else {
                        Rect { left: p.col, ..src }
                    }
                } else if src.rows() > 1 && p.row < src.bottom {
                    // dragged back inside: shrink
                    Rect { bottom: p.row.max(src.top), ..src }
                } else if src.cols() > 1 && p.col < src.right {
                    Rect { right: p.col.max(src.left), ..src }
                } else {
                    src
                };
                app.grid.drag = Some(Drag::Fill { src, target });
                ctx.set_cursor_icon(CursorIcon::Crosshair);
            }
            Some(Drag::Point { sheet, anchor }) => {
                let p = geo.cell_at(clamped);
                let cur = app.edit.as_ref().and_then(|e| e.point.as_ref()).map(|pt| pt.cur);
                if cur != Some(p) {
                    app.point_to(sheet, anchor, p);
                }
            }
            _ => {}
        }
    }
    if released {
        match app.grid.drag.take() {
            Some(Drag::Fill { src, target }) => {
                if target != src {
                    app.fill_to(src, target);
                }
            }
            Some(Drag::ResizeCol { cols, start_w, .. }) => {
                // record undo with the original widths
                let sheet = app.sheet;
                let now: Vec<(u32, f32)> = cols.iter().map(|c| (*c, app.doc.wb.sheets[sheet].col_width(*c))).collect();
                for c in &cols {
                    app.doc.wb.sheets[sheet].col_widths.insert(*c, start_w);
                }
                app.modify("Column Width", engine::ops::Scope::Sheet(sheet), |wb, s| {
                    for (c, w) in &now {
                        wb.sheets[s].col_widths.insert(*c, *w);
                    }
                });
                app.status = None;
            }
            Some(Drag::ResizeRow { rows, start_h, .. }) => {
                let sheet = app.sheet;
                let now: Vec<(u32, f32)> = rows.iter().map(|r| (*r, app.doc.wb.sheets[sheet].row_height(*r))).collect();
                for r in &rows {
                    app.doc.wb.sheets[sheet].row_heights.insert(*r, start_h);
                }
                app.modify("Row Height", engine::ops::Scope::Sheet(sheet), |wb, s| {
                    for (r, h) in &now {
                        wb.sheets[s].row_heights.insert(*r, *h);
                    }
                });
                app.status = None;
            }
            Some(Drag::Cells) | Some(Drag::Rows { .. }) | Some(Drag::Cols { .. }) => {
                if app.format_painter.is_some() {
                    app.apply_format_painter();
                }
            }
            Some(other) => {
                if !matches!(other, Drag::Point { .. }) {
                    app.grid.drag = None;
                }
            }
            None => {}
        }
    }
}

fn follow_link(app: &mut App, ctx: &egui::Context, link: &str) {
    if let Some(internal) = link.strip_prefix('#') {
        app.goto(internal);
    } else {
        ctx.open_url(egui::OpenUrl::new_tab(link));
    }
}

// ======================================================================
// Painting
// ======================================================================

fn border_stroke(b: &BorderLine, z: f32) -> (Stroke, bool, bool) {
    let c = to_c32(b.color);
    match b.kind {
        BorderKind::Thin => (Stroke::new(1.0, c), false, false),
        BorderKind::Medium => (Stroke::new(2.0 * z.max(0.5), c), false, false),
        BorderKind::Thick => (Stroke::new(3.0 * z.max(0.5), c), false, false),
        BorderKind::Dashed | BorderKind::Dotted => (Stroke::new(1.0, c), true, false),
        BorderKind::Double => (Stroke::new(1.0, c), false, true),
    }
}

fn draw_border_line(p: &Painter, a: Pos2, b: Pos2, line: &BorderLine, z: f32, horizontal: bool) {
    let (st, dashed, double) = border_stroke(line, z);
    if dashed {
        let dash = if line.kind == BorderKind::Dotted { 1.0 } else { 3.0 };
        p.extend(Shape::dashed_line(&[a, b], st, dash, dash));
    } else if double {
        let o = if horizontal { vec2(0.0, 1.5) } else { vec2(1.5, 0.0) };
        p.line_segment([a - o, b - o], st);
        p.line_segment([a + o, b + o], st);
    } else {
        p.line_segment([a, b], st);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_cells(app: &App, p: &Painter, geo: &Geo, r0: u32, r1: u32, c0: u32, c1: u32, cf: &HashMap<CellPos, CfFormat>) {
    if r1 < r0 || c1 < c0 {
        return;
    }
    let wb = &app.doc.wb;
    let sheet = app.sheet;
    let s = &wb.sheets[sheet];
    let z = geo.z;
    let gridlines = s.show_gridlines;
    let area = Rect { top: r0, left: c0, bottom: r1, right: c1 };
    // merges touching the area
    let merges: Vec<Rect> = s.merges.iter().copied().filter(|m| m.intersects(&area)).collect();
    let merge_of = |q: CellPos| merges.iter().find(|m| m.contains(q)).copied();
    let clip = p.clip_rect();
    let x_left = geo.col_x(c0);
    let x_right = geo.col_x(c1) + geo.col_w(c1);

    let rows: Vec<u32> = (r0..=r1).filter(|r| geo.rows.size(*r) > 0.0).collect();
    let cols: Vec<u32> = (c0..=c1).filter(|c| geo.cols.size(*c) > 0.0).collect();

    // 1) fills (style + conditional)
    for &r in &rows {
        for (q, c) in s.cells.range(CellPos::new(r, c0)..=CellPos::new(r, c1)) {
            if merge_of(*q).is_some() || geo.cols.size(q.col) == 0.0 {
                continue;
            }
            let st = wb.style(c.style);
            let fill = cf.get(q).and_then(|f| f.fill).or(st.fill);
            if let Some(f) = fill {
                p.rect_filled(geo.cell_rect(*q), 0.0, to_c32(f));
            }
        }
        // conditional fills on cells without a stored cell (e.g. spill values)
        for (q, f) in cf.iter() {
            if q.row == r && q.col >= c0 && q.col <= c1 && !s.cells.contains_key(q) {
                if let Some(fc) = f.fill {
                    p.rect_filled(geo.cell_rect(*q), 0.0, to_c32(fc));
                }
            }
        }
    }

    // 2) text layout per row (with overflow), collecting gridline gaps
    struct Item {
        rect: ERect,
        clip: ERect,
        galley: Arc<egui::Galley>,
        pos: Pos2,
        fake_bold: bool,
        color: Color32,
    }
    let mut items: Vec<Item> = Vec::new();
    let mut gaps: HashSet<(u32, u32)> = HashSet::new(); // (row, col) whose left gridline is hidden
    for &r in &rows {
        let lo = c0.saturating_sub(OVERFLOW_COLS);
        let hi = (c1 + OVERFLOW_COLS).min(MAX_COLS - 1);
        let mut occupied: HashSet<u32> = HashSet::new();
        let mut entries: Vec<CellPos> = Vec::new();
        for (q, c) in s.cells.range(CellPos::new(r, lo)..=CellPos::new(r, hi)) {
            if !c.is_blank() {
                occupied.insert(q.col);
                entries.push(*q);
            }
        }
        for c in c0..=c1 {
            if s.spill_owner.contains_key(&CellPos::new(r, c)) {
                occupied.insert(c);
                entries.push(CellPos::new(r, c));
            }
        }
        for q in entries {
            if geo.cols.size(q.col) == 0.0 || merge_of(q).is_some() {
                continue;
            }
            let Some(t) = cell_text(app, wb, sheet, q, z, cf.get(&q)) else { continue };
            let cr = geo.cell_rect(q);
            let pad = 2.5 * z.max(0.6);
            let avail = (cr.width() - 2.0 * pad - t.indent).max(1.0);
            let text_cell_x0 = geo.col_x(q.col);
            // skip cells far outside the visible columns unless they can overflow in
            let outside = q.col < c0 || q.col > c1;
            if t.wrap {
                if outside {
                    continue;
                }
                let g = layout(p, &t, &t.text, Some(avail));
                let pos = text_pos(&t, cr, g.size(), pad);
                items.push(Item { rect: cr, clip: cr, galley: g, pos, fake_bold: t.fake_bold, color: t.color });
                continue;
            }
            let txt = if t.is_number { fit_number(p, &t, avail) } else { t.text.replace('\n', " ") };
            let g = layout(p, &t, &txt, None);
            let w = g.size().x + 2.0 * pad + t.indent;
            let mut ext = cr;
            if !t.is_number && w > cr.width() {
                // overflow into empty neighbours
                let need = w - cr.width();
                match t.h {
                    HAlign::Left | HAlign::General => {
                        let mut extra = 0.0;
                        let mut c = q.col + 1;
                        while extra < need && c < MAX_COLS && c <= q.col + OVERFLOW_COLS && !occupied.contains(&c) && merge_of(CellPos::new(r, c)).is_none() {
                            extra += geo.col_w(c);
                            gaps.insert((r, c));
                            c += 1;
                        }
                        ext.max.x += extra;
                    }
                    HAlign::Right => {
                        let mut extra = 0.0;
                        let mut c = q.col;
                        while extra < need && c > 0 && c + OVERFLOW_COLS > q.col && !occupied.contains(&(c - 1)) && merge_of(CellPos::new(r, c - 1)).is_none() {
                            gaps.insert((r, c));
                            c -= 1;
                            extra += geo.col_w(c);
                        }
                        ext.min.x -= extra;
                    }
                    _ => {
                        let half = need / 2.0;
                        let mut el = 0.0;
                        let mut c = q.col;
                        while el < half && c > 0 && !occupied.contains(&(c - 1)) {
                            gaps.insert((r, c));
                            c -= 1;
                            el += geo.col_w(c);
                        }
                        let mut er = 0.0;
                        let mut c = q.col + 1;
                        while er < half && c < MAX_COLS && !occupied.contains(&c) {
                            er += geo.col_w(c);
                            gaps.insert((r, c));
                            c += 1;
                        }
                        // keep the text centred on the cell
                        let e = el.min(er).max(0.0);
                        ext.min.x -= if el >= half { half.min(el) } else { e };
                        ext.max.x += if er >= half { half.min(er) } else { e };
                    }
                }
            }
            if ext.max.x < clip.left() || ext.min.x > clip.right() {
                continue;
            }
            let _ = text_cell_x0;
            let pos = match t.h {
                HAlign::Right => pos2(cr.right() - pad - g.size().x - t.indent, 0.0),
                HAlign::Center => pos2(cr.center().x - g.size().x / 2.0, 0.0),
                _ => pos2(cr.left() + pad + t.indent, 0.0),
            };
            let y = vpos(&t, cr, g.size().y, pad);
            items.push(Item { rect: cr, clip: ext, galley: g, pos: pos2(pos.x, y), fake_bold: t.fake_bold, color: t.color });
        }
    }

    // 3) gridlines
    if gridlines {
        let gl = Stroke::new(1.0, theme::GRIDLINE);
        for &r in &rows {
            let y = (geo.row_y(r) + geo.row_h(r)).round() - 0.5;
            p.hline(x_left..=x_right, y, gl);
        }
        for &c in &cols {
            let x = (geo.col_x(c) + geo.col_w(c)).round() - 0.5;
            // vertical gridline: per row segment, skipping overflow gaps (gap is the left edge of c+1)
            let mut seg_start: Option<f32> = None;
            for &r in &rows {
                let y0 = geo.row_y(r);
                let y1 = y0 + geo.row_h(r);
                if gaps.contains(&(r, c + 1)) {
                    if let Some(sy) = seg_start.take() {
                        p.vline(x, sy..=y0, gl);
                    }
                } else if seg_start.is_none() {
                    seg_start = Some(y0);
                }
                let _ = y1;
            }
            if let Some(sy) = seg_start {
                let last = *rows.last().unwrap_or(&r1);
                p.vline(x, sy..=geo.row_y(last) + geo.row_h(last), gl);
            }
        }
        // repaint fills over gridlines (filled cells hide gridlines)
        for &r in &rows {
            for (q, c) in s.cells.range(CellPos::new(r, c0)..=CellPos::new(r, c1)) {
                if merge_of(*q).is_some() {
                    continue;
                }
                let fill = cf.get(q).and_then(|f| f.fill).or(wb.style(c.style).fill);
                if let Some(f) = fill {
                    p.rect_filled(geo.cell_rect(*q).expand(0.0), 0.0, to_c32(f));
                }
            }
        }
    }

    // 4) merged cells: background over interior gridlines, then text
    for m in &merges {
        let mr = geo.range_rect(*m);
        let tl = m.top_left();
        let st = wb.cell_style(sheet, tl);
        let fill = cf.get(&tl).and_then(|f| f.fill).or(st.fill).map(to_c32).unwrap_or(Color32::WHITE);
        let inner = if gridlines && st.fill.is_none() { ERect::from_min_max(mr.min, mr.max - vec2(1.0, 1.0)) } else { mr };
        p.rect_filled(inner, 0.0, fill);
        if let Some(t) = cell_text(app, wb, sheet, tl, z, cf.get(&tl)) {
            let pad = 2.5 * z.max(0.6);
            let avail = (mr.width() - 2.0 * pad).max(1.0);
            let g = if t.wrap {
                layout(p, &t, &t.text, Some(avail))
            } else {
                let txt = if t.is_number { fit_number(p, &t, avail) } else { t.text.clone() };
                layout(p, &t, &txt, None)
            };
            let pos = text_pos(&t, mr, g.size(), pad);
            items.push(Item { rect: mr, clip: mr, galley: g, pos, fake_bold: t.fake_bold, color: t.color });
        }
    }

    // 5) data bars
    for (q, f) in cf.iter() {
        if let Some((frac, color)) = f.data_bar {
            if q.row < r0 || q.row > r1 || q.col < c0 || q.col > c1 {
                continue;
            }
            let cr = geo.cell_rect(*q).shrink2(vec2(1.5, 2.0));
            let bar = ERect::from_min_size(cr.min, vec2(cr.width() * frac, cr.height()));
            let c = to_c32(color);
            p.rect_filled(bar, 0.0, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 150));
            p.rect_stroke(bar, 0.0, Stroke::new(1.0, c), StrokeKind::Inside);
        }
    }

    // 6) text
    for it in items {
        let cp = p.with_clip_rect(it.clip.intersect(clip));
        cp.galley(it.pos, it.galley.clone(), it.color);
        if it.fake_bold {
            cp.galley(it.pos + vec2(0.6, 0.0), it.galley, it.color);
        }
        let _ = it.rect;
    }

    // 7) borders
    for &r in &rows {
        for (q, c) in s.cells.range(CellPos::new(r, c0)..=CellPos::new(r, c1)) {
            if c.style == 0 {
                continue;
            }
            let b = &wb.style(c.style).borders;
            if b.is_empty() {
                continue;
            }
            let cr = match merge_of(*q) {
                Some(m) if m.top_left() == *q => geo.range_rect(m),
                Some(_) => continue,
                None => geo.cell_rect(*q),
            };
            let (l, rr, t, bt) = (cr.left().round() - 0.5, cr.right().round() - 0.5, cr.top().round() - 0.5, cr.bottom().round() - 0.5);
            if let Some(line) = &b.top {
                draw_border_line(p, pos2(l, t), pos2(rr, t), line, z, true);
            }
            if let Some(line) = &b.bottom {
                draw_border_line(p, pos2(l, bt), pos2(rr, bt), line, z, true);
            }
            if let Some(line) = &b.left {
                draw_border_line(p, pos2(l, t), pos2(l, bt), line, z, false);
            }
            if let Some(line) = &b.right {
                draw_border_line(p, pos2(rr, t), pos2(rr, bt), line, z, false);
            }
        }
    }

    // 8) comment indicators
    for (q, _) in s.comments.range(CellPos::new(r0, 0)..=CellPos::new(r1, MAX_COLS - 1)) {
        if q.col < c0 || q.col > c1 {
            continue;
        }
        let cr = geo.cell_rect(*q);
        let sz = 6.0 * z.max(0.7);
        p.add(Shape::convex_polygon(vec![pos2(cr.right() - sz, cr.top()), pos2(cr.right(), cr.top()), pos2(cr.right(), cr.top() + sz)], Color32::from_rgb(0xD0, 0x10, 0x10), Stroke::NONE));
    }
}

fn vpos(t: &CellText, cr: ERect, h: f32, pad: f32) -> f32 {
    match t.v {
        VAlign::Top => cr.top() + pad * 0.4,
        VAlign::Center => cr.center().y - h / 2.0,
        VAlign::Bottom => cr.bottom() - h - pad * 0.4,
    }
}

fn text_pos(t: &CellText, cr: ERect, size: egui::Vec2, pad: f32) -> Pos2 {
    let x = match t.h {
        HAlign::Right => cr.right() - pad - size.x - t.indent,
        HAlign::Center => cr.center().x - size.x / 2.0,
        _ => cr.left() + pad + t.indent,
    };
    pos2(x, vpos(t, cr, size.y, pad))
}

/// Selection, active cell, fill handle, copy marquee, reference highlights, spill outline.
fn paint_overlays(app: &App, p: &Painter, geo: &Geo, ctx: &egui::Context) {
    let accent = theme::ACCENT;
    // formula reference highlights
    let mut ref_rects = Vec::new();
    if let Some(e) = &app.edit {
        if e.text.starts_with('=') {
            let sheet_name = &app.sheet_ref().name;
            for (i, r) in crate::editor::find_refs(&e.text).iter().enumerate() {
                let same = match &r.sheet {
                    None => e.sheet == app.sheet,
                    Some(s) => s.eq_ignore_ascii_case(sheet_name),
                };
                if same {
                    let c = theme::REF_COLORS[i % theme::REF_COLORS.len()];
                    let rr = geo.range_rect(clamp_vis(r.rect, geo));
                    p.rect_filled(rr, 0.0, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 28));
                    p.rect_stroke(rr, 0.0, Stroke::new(2.0, c), StrokeKind::Inside);
                    ref_rects.push(rr);
                }
            }
        }
    }
    let editing_formula_elsewhere = app.edit.as_ref().map(|e| e.sheet != app.sheet).unwrap_or(false);
    if !editing_formula_elsewhere {
        // selection tint (the active cell stays white)
        let act = match app.sheet_ref().merge_at(app.sel.active) {
            Some(m) => geo.range_rect(m),
            None => geo.cell_rect(app.sel.active),
        };
        for r in &app.sel.ranges {
            if r.cell_count() <= 1 {
                continue;
            }
            let rr = geo.range_rect(clamp_vis(*r, geo));
            if rr.intersects(act) && r.contains(app.sel.active) {
                let a = act.intersect(rr);
                for part in [
                    ERect::from_min_max(rr.min, pos2(rr.right(), a.top())),
                    ERect::from_min_max(pos2(rr.left(), a.bottom()), rr.max),
                    ERect::from_min_max(pos2(rr.left(), a.top()), pos2(a.left(), a.bottom())),
                    ERect::from_min_max(pos2(a.right(), a.top()), pos2(rr.right(), a.bottom())),
                ] {
                    if part.width() > 0.0 && part.height() > 0.0 {
                        p.rect_filled(part, 0.0, theme::SEL_FILL);
                    }
                }
            } else {
                p.rect_filled(rr, 0.0, theme::SEL_FILL);
            }
        }
        let prim = geo.range_rect(clamp_vis(app.sel.primary(), geo));
        if app.selected_chart.is_none() {
            p.rect_stroke(prim.expand(0.5), 0.0, Stroke::new(2.0, accent), StrokeKind::Middle);
            if app.sel.primary().cell_count() > 1 {
                p.rect_stroke(act.shrink(1.0), 0.0, Stroke::new(1.0, accent), StrokeKind::Inside);
            }
            // fill handle
            if app.edit.is_none() && app.sel.ranges.len() == 1 {
                let h = prim.right_bottom();
                let hr = ERect::from_center_size(h, vec2(7.0, 7.0));
                p.rect_filled(hr.expand(1.0), 0.0, Color32::WHITE);
                p.rect_filled(hr, 0.0, accent);
            }
        }
    }
    // fill drag preview
    if let Some(Drag::Fill { target, .. }) = &app.grid.drag {
        let rr = geo.range_rect(*target);
        p.extend(Shape::dashed_line(&[rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()], Stroke::new(2.0, Color32::from_gray(110)), 4.0, 3.0));
    }
    // copy marquee
    if let Some(c) = &app.clip {
        if c.sheet == app.sheet {
            let rr = geo.range_rect(clamp_vis(c.rect, geo)).shrink(1.0);
            let t = ctx.input(|i| i.time) as f32;
            let off = (t * 12.0) % 8.0;
            let pts = [rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()];
            p.extend(Shape::dashed_line_with_offset(&pts, Stroke::new(2.0, accent), &[4.0], &[4.0], off));
        }
    }
    // spill outline for the active cell's spill range
    let s = app.sheet_ref();
    let anchor = s.spill_owner.get(&app.sel.active).copied().unwrap_or(app.sel.active);
    if let Some(area) = s.spill_areas.get(&anchor) {
        let rr = geo.range_rect(*area);
        p.extend(Shape::dashed_line(&[rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()], Stroke::new(1.0, Color32::from_rgb(0x44, 0x72, 0xC4)), 3.0, 2.0));
    }
}

/// Clamp a (possibly whole-row/column) range to something drawable.
fn clamp_vis(r: Rect, geo: &Geo) -> Rect {
    let (_, rl) = geo.scroll_rows();
    let (_, cl) = geo.scroll_cols();
    Rect { top: r.top, left: r.left, bottom: r.bottom.min(rl + 2).max(r.top), right: r.right.min(cl + 2).max(r.left) }
}

#[allow(clippy::too_many_arguments)]
fn paint_headers(app: &App, painter: &Painter, geo: &Geo, grid: ERect, hdr_w: f32, hdr_h: f32, r0: u32, r1: u32, c0: u32, c1: u32) {
    let cells = geo.cells;
    let col_strip = ERect::from_min_max(pos2(cells.left(), grid.top()), pos2(cells.right(), cells.top()));
    let row_strip = ERect::from_min_max(pos2(grid.left(), cells.top()), pos2(cells.left(), cells.bottom()));
    painter.rect_filled(col_strip, 0.0, theme::HEADER_BG);
    painter.rect_filled(row_strip, 0.0, theme::HEADER_BG);
    let z = geo.z;
    let font = FontId::proportional((11.0 * z.max(0.7)).min(18.0));
    let line = Stroke::new(1.0, theme::HEADER_LINE);
    // selection extents
    let sel_cols: Vec<(u32, u32, bool)> = app.sel.ranges.iter().map(|r| (r.left, r.right, r.top == 0 && r.bottom == MAX_ROWS - 1)).collect();
    let sel_rows: Vec<(u32, u32, bool)> = app.sel.ranges.iter().map(|r| (r.top, r.bottom, r.left == 0 && r.right == MAX_COLS - 1)).collect();
    let col_state = |c: u32| -> u8 {
        let mut s = 0;
        for (a, b, full) in &sel_cols {
            if c >= *a && c <= *b {
                s = s.max(if *full { 2 } else { 1 });
            }
        }
        s
    };
    let row_state = |r: u32| -> u8 {
        let mut s = 0;
        for (a, b, full) in &sel_rows {
            if r >= *a && r <= *b {
                s = s.max(if *full { 2 } else { 1 });
            }
        }
        s
    };
    // columns
    let col_ranges = [(0, geo.fc.saturating_sub(1), geo.fc > 0), (c0, c1, true)];
    for (a, b, ok) in col_ranges {
        if !ok {
            continue;
        }
        let clip = if a == 0 && b + 1 == geo.fc {
            ERect::from_min_max(col_strip.min, pos2(cells.left() + geo.frozen_w(), col_strip.bottom()))
        } else {
            ERect::from_min_max(pos2(cells.left() + geo.frozen_w(), col_strip.top()), col_strip.max)
        };
        let p = painter.with_clip_rect(clip);
        for c in a..=b {
            let w = geo.col_w(c);
            if w <= 0.0 {
                continue;
            }
            let x = geo.col_x(c);
            let r = ERect::from_min_size(pos2(x, col_strip.top()), vec2(w, hdr_h));
            let st = col_state(c);
            if st > 0 {
                p.rect_filled(r, 0.0, if st == 2 { theme::HEADER_FULL_SEL_BG } else { theme::HEADER_SEL_BG });
                p.hline(r.x_range(), r.bottom() - 1.0, Stroke::new(2.0, theme::ACCENT));
            }
            p.vline(r.right().round() - 0.5, r.y_range(), line);
            let tc = if st > 0 { theme::ACCENT_DARK } else { theme::HEADER_TEXT };
            p.text(r.center(), Align2::CENTER_CENTER, col_to_letters(c), font.clone(), tc);
        }
    }
    // rows
    let row_ranges = [(0, geo.fr.saturating_sub(1), geo.fr > 0), (r0, r1, true)];
    for (a, b, ok) in row_ranges {
        if !ok {
            continue;
        }
        let clip = if a == 0 && b + 1 == geo.fr {
            ERect::from_min_max(row_strip.min, pos2(row_strip.right(), cells.top() + geo.frozen_h()))
        } else {
            ERect::from_min_max(pos2(row_strip.left(), cells.top() + geo.frozen_h()), row_strip.max)
        };
        let p = painter.with_clip_rect(clip);
        for r in a..=b {
            let h = geo.row_h(r);
            if h <= 0.0 {
                continue;
            }
            let y = geo.row_y(r);
            let rr = ERect::from_min_size(pos2(row_strip.left(), y), vec2(hdr_w, h));
            let st = row_state(r);
            if st > 0 {
                p.rect_filled(rr, 0.0, if st == 2 { theme::HEADER_FULL_SEL_BG } else { theme::HEADER_SEL_BG });
                p.vline(rr.right() - 1.0, rr.y_range(), Stroke::new(2.0, theme::ACCENT));
            }
            p.hline(rr.x_range(), rr.bottom().round() - 0.5, line);
            let filtered = app.sheet_ref().autofilter.as_ref().map(|af| af.range.top < r && r <= af.range.bottom && !af.allowed.is_empty()).unwrap_or(false);
            let tc = if filtered { Color32::from_rgb(0x00, 0x60, 0xC0) } else if st > 0 { theme::ACCENT_DARK } else { theme::HEADER_TEXT };
            p.text(pos2(rr.right() - 4.0, rr.center().y), Align2::RIGHT_CENTER, (r + 1).to_string(), font.clone(), tc);
        }
    }
    // separators and corner
    painter.hline(grid.left()..=cells.right(), cells.top() - 0.5, line);
    painter.vline(cells.left() - 0.5, grid.top()..=cells.bottom(), line);
    let corner = ERect::from_min_max(grid.min, cells.min);
    painter.rect_filled(corner.shrink(0.5), 0.0, theme::HEADER_BG);
    let tri = 0.55 * corner.height().min(corner.width());
    painter.add(Shape::convex_polygon(
        vec![pos2(corner.right() - 3.0, corner.bottom() - 3.0 - tri), pos2(corner.right() - 3.0, corner.bottom() - 3.0), pos2(corner.right() - 3.0 - tri, corner.bottom() - 3.0)],
        Color32::from_gray(190),
        Stroke::NONE,
    ));
}

// ======================================================================
// Editor overlay
// ======================================================================

/// Layout job coloring references like Excel's formula editor.
pub fn formula_job(text: &str, font: FontId, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let base = TextFormat { font_id: font.clone(), color: Color32::BLACK, ..Default::default() };
    if !text.starts_with('=') {
        job.append(text, 0.0, base);
        return job;
    }
    let chars: Vec<char> = text.chars().collect();
    let refs = crate::editor::find_refs(text);
    let mut i = 0;
    for (k, r) in refs.iter().enumerate() {
        if r.start > i {
            job.append(&chars[i..r.start].iter().collect::<String>(), 0.0, base.clone());
        }
        let c = theme::REF_COLORS[k % theme::REF_COLORS.len()];
        job.append(&chars[r.start..r.end].iter().collect::<String>(), 0.0, TextFormat { color: c, ..base.clone() });
        i = r.end;
    }
    if i < chars.len() {
        job.append(&chars[i..].iter().collect::<String>(), 0.0, base);
    }
    job
}

/// Apply pending cursor/focus requests to an editor widget's stored state.
pub fn prepare_editor_state(ctx: &egui::Context, id: egui::Id, cursor: Option<usize>) {
    if let Some(c) = cursor {
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
        egui::TextEdit::store_state(ctx, id, st);
    }
}

fn cell_editor(app: &mut App, ui: &mut egui::Ui, geo: &Geo, cells: ERect) {
    let Some(e) = app.edit.as_ref() else { return };
    if e.sheet != app.sheet {
        return;
    }
    let cell = e.cell;
    let wb = &app.doc.wb;
    let st = wb.cell_style(app.sheet, cell).clone();
    let base = match app.sheet_ref().merge_at(cell) {
        Some(m) => geo.range_rect(m),
        None => geo.cell_rect(cell),
    };
    let (font, _) = crate::fonts::cell_font(st.font_name.as_deref(), st.bold, st.font_size() * PT_PX * geo.z);
    let text = e.text.clone();
    let galley = ui.painter().layout(text.clone(), font.clone(), Color32::BLACK, if st.wrap { base.width() - 6.0 } else { f32::INFINITY });
    let w = (galley.size().x + 14.0).max(base.width()).min((cells.right() - base.left()).max(base.width()));
    let h = (galley.size().y + 4.0).max(base.height());
    let rect = ERect::from_min_size(base.min, vec2(w, h));
    let painter = ui.painter_at(cells);
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    painter.rect_stroke(rect.expand(1.0), 0.0, Stroke::new(2.0, theme::ACCENT), StrokeKind::Middle);

    let id = cell_editor_id();
    let in_bar = e.in_bar;
    let e = app.edit.as_mut().unwrap();
    let wrap_w = if st.wrap { base.width() - 6.0 } else { f32::INFINITY };
    let font2 = font.clone();
    let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| ui.painter().layout_job(formula_job(buf.as_str(), font2.clone(), wrap_w));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(2.0, 1.0))));
    child.set_clip_rect(cells);
    let te = egui::TextEdit::multiline(&mut e.text)
        .id(id)
        .frame(egui::Frame::NONE)
        .font(font)
        .desired_width(rect.width() - 4.0)
        .desired_rows(1)
        .margin(egui::Margin::ZERO)
        .return_key(Some(KeyboardShortcut::new(Modifiers::ALT, Key::Enter)))
        .lock_focus(true)
        .layouter(&mut layouter);
    let out = te.show(&mut child);
    if out.response.has_focus() {
        if let Some(cr) = out.cursor_range {
            e.cursor = cr.primary.index.0;
        }
        e.in_bar = false;
    }
    if out.response.changed() {
        e.point = None;
        e.ac_index = 0;
    }
    let _ = in_bar;
    // autocomplete popup and function tooltip
    let anchor = pos2(rect.left(), rect.bottom() + 2.0);
    autocomplete_popup(app, ui.ctx(), anchor);
}

/// The function list under the editor while typing a function name, and the syntax tip.
pub fn autocomplete_popup(app: &mut App, ctx: &egui::Context, anchor: Pos2) {
    let list = app.autocomplete();
    let Some(e) = app.edit.as_ref() else { return };
    let mut chosen: Option<&'static str> = None;
    if !list.is_empty() {
        let sel = e.ac_index.min(list.len() - 1);
        egui::Area::new(egui::Id::unique("rexcel-autocomplete")).order(egui::Order::Foreground).fixed_pos(anchor).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
                ui.set_min_width(220.0);
                for (i, f) in list.iter().enumerate() {
                    let r = ui.add(egui::Button::selectable(i == sel, format!("ƒx {}", f.name)).frame_when_inactive(false));
                    if i == sel {
                        r.clone().on_hover_text(f.description);
                    }
                    if r.double_clicked() || r.clicked() {
                        chosen = Some(f.name);
                    }
                }
                let f = list[sel];
                ui.separator();
                ui.add(egui::Label::new(egui::RichText::new(f.description).small().color(theme::MUTED)).wrap());
            });
        });
    } else if let Some((name, arg)) = crate::editor::current_function(&e.text, e.cursor) {
        if let Some(f) = engine::functions::lookup(&name) {
            egui::Area::new(egui::Id::unique("rexcel-syntax")).order(egui::Order::Foreground).fixed_pos(anchor).interactable(false).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::from_rgb(0xFF, 0xFF, 0xF0)).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        // bold the current argument
                        let syn = f.syntax;
                        let (head, args) = syn.split_once('(').unwrap_or((syn, ""));
                        let args = args.trim_end_matches(')');
                        ui.label(egui::RichText::new(format!("{}(", head)).small());
                        let parts: Vec<&str> = args.split(',').collect();
                        let cur = arg.min(parts.len().saturating_sub(1));
                        for (i, a) in parts.iter().enumerate() {
                            let t = egui::RichText::new(format!("{}{}", a, if i + 1 < parts.len() { "," } else { "" })).small();
                            ui.label(if i == cur || (a.contains("...") && arg >= i) { t.strong() } else { t });
                        }
                        ui.label(egui::RichText::new(")").small());
                    });
                });
            });
        }
    }
    if let Some(n) = chosen {
        app.accept_autocomplete(n);
    }
}

// ======================================================================
// Widgets over the grid
// ======================================================================

fn comment_tooltip(app: &App, ui: &egui::Ui, geo: &Geo, resp: &egui::Response) {
    let Some(pos) = resp.hover_pos() else { return };
    if !geo.cells.contains(pos) || app.grid.drag.is_some() {
        return;
    }
    let p = geo.cell_at(pos);
    let s = app.sheet_ref();
    let comment = s.comments.get(&p);
    let link = s.hyperlinks.get(&p);
    if comment.is_none() && link.is_none() {
        return;
    }
    let cr = geo.cell_rect(p);
    egui::Area::new(egui::Id::unique("rexcel-comment")).order(egui::Order::Tooltip).fixed_pos(pos2(cr.right() + 6.0, cr.top())).interactable(false).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).fill(Color32::from_rgb(0xFF, 0xFF, 0xE1)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.set_max_width(260.0);
            if let Some(c) = comment {
                ui.label(c);
            }
            if let Some(l) = link {
                ui.label(egui::RichText::new(format!("{}\nClick once to follow (Ctrl+click when editing).", l)).small().color(theme::MUTED));
            }
        });
    });
}

fn filter_buttons(app: &mut App, ui: &mut egui::Ui, geo: &Geo, cells: ERect) {
    let Some(af) = app.sheet_ref().autofilter.clone() else { return };
    let r = af.range.top;
    if geo.row_h(r) <= 0.0 {
        return;
    }
    let (c0, c1) = geo.scroll_cols();
    for c in af.range.left..=af.range.right {
        if (c >= geo.fc && (c < c0 || c > c1)) || geo.col_w(c) <= 0.0 {
            continue;
        }
        let cr = geo.cell_rect(CellPos::new(r, c));
        let sz = (cr.height() - 4.0).clamp(10.0, 18.0);
        let br = ERect::from_min_size(pos2(cr.right() - sz - 2.0, cr.bottom() - sz - 2.0), vec2(sz, sz));
        if !cells.intersects(br) {
            continue;
        }
        let resp = ui.interact(br, egui::Id::unique(("rexcel-filter", c)), Sense::click());
        let p = ui.painter_at(cells);
        let active = af.allowed.contains_key(&(c - af.range.left));
        p.rect(br, 2.0, if resp.hovered() { Color32::from_gray(225) } else { Color32::from_gray(245) }, Stroke::new(1.0, Color32::from_gray(160)), StrokeKind::Inside);
        let cc = br.center();
        let col = if active { Color32::from_rgb(0x00, 0x60, 0xC0) } else { Color32::from_gray(80) };
        p.add(Shape::convex_polygon(vec![pos2(cc.x - 3.5, cc.y - 1.5), pos2(cc.x + 3.5, cc.y - 1.5), pos2(cc.x, cc.y + 2.5)], col, Stroke::NONE));
        if resp.clicked() {
            app.filter_popup = Some(crate::dialogs::FilterPopup::new(app, c, pos2(br.left(), br.bottom())));
        }
    }
}

fn validation_dropdown(app: &mut App, ui: &mut egui::Ui, geo: &Geo, cells: ERect) {
    if app.edit.is_some() {
        return;
    }
    let p = app.sel.active;
    let Some(rule) = engine::ops::validation_at(&app.doc.wb, app.sheet, p).cloned() else { return };
    if !matches!(rule, engine::workbook::ValidationRule::List(_) | engine::workbook::ValidationRule::ListRange(_)) {
        return;
    }
    let cr = match app.sheet_ref().merge_at(p) {
        Some(m) => geo.range_rect(m),
        None => geo.cell_rect(p),
    };
    let sz = cr.height().clamp(14.0, 20.0);
    let br = ERect::from_min_size(pos2(cr.right() + 1.0, cr.bottom() - sz), vec2(sz, sz));
    if !cells.contains_rect(br) {
        return;
    }
    let resp = ui.interact(br, egui::Id::unique("rexcel-dv-button"), Sense::click());
    let painter = ui.painter_at(cells);
    painter.rect(br, 0.0, if resp.hovered() { Color32::from_gray(225) } else { Color32::from_gray(240) }, Stroke::new(1.0, Color32::from_gray(160)), StrokeKind::Inside);
    let c = br.center();
    painter.add(Shape::convex_polygon(vec![pos2(c.x - 4.0, c.y - 2.0), pos2(c.x + 4.0, c.y - 2.0), pos2(c.x, c.y + 3.0)], Color32::from_gray(60), Stroke::NONE));
    if resp.clicked() {
        app.validation_popup = if app.validation_popup.is_some() { None } else { Some(p) };
    }
    if app.validation_popup == Some(p) {
        let items = engine::ops::validation_list(&app.doc.wb, app.sheet, &rule);
        let mut pick: Option<String> = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::unique("rexcel-dv-list")).order(egui::Order::Foreground).fixed_pos(pos2(cr.left(), cr.bottom() + 1.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
                ui.set_min_width(cr.width().max(80.0));
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for it in &items {
                        if ui.add(egui::Button::selectable(false, it).frame_when_inactive(false)).clicked() {
                            pick = Some(it.clone());
                        }
                    }
                });
            });
        });
        if ui.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer() && !resp.contains_pointer() {
            close = true;
        }
        if let Some(v) = pick {
            let sheet = app.sheet;
            app.modify("Typing", engine::ops::Scope::Cells { sheet, rects: vec![Rect::single(p)] }, |wb, s| {
                let _ = wb.set_input(s, p, &v);
            });
            close = true;
        }
        if close {
            app.validation_popup = None;
        }
    }
}

fn charts_ui(app: &mut App, ui: &mut egui::Ui, geo: &Geo, cells: ERect) {
    let n = app.sheet_ref().charts.len();
    if n == 0 {
        return;
    }
    let (fr, fc) = (app.sheet_ref().freeze_rows, app.sheet_ref().freeze_cols);
    let pointer = ui.input(|i| i.pointer.interact_pos());
    for i in 0..n {
        let chart = app.sheet_ref().charts[i].clone();
        let mut origin = pos2(geo.col_x(chart.anchor.col), geo.row_y(chart.anchor.row));
        let mut size = vec2(chart.width, chart.height) * geo.z;
        if let Some(Drag::Chart { idx, resize, start, orig_size, offset, .. }) = &app.grid.drag {
            if *idx == i {
                if let Some(pp) = pointer {
                    let d = pp - *start;
                    if *resize {
                        size = (*orig_size * geo.z + d).max(vec2(120.0, 80.0));
                    } else {
                        origin += d + *offset * 0.0;
                    }
                }
            }
        }
        let rect = ERect::from_min_size(origin, size);
        if !rect.intersects(cells) {
            continue;
        }
        // A chart anchored in a frozen pane is not clipped by that pane.
        let left = if chart.anchor.col < fc { cells.left() } else { cells.left() + geo.frozen_w() };
        let top = if chart.anchor.row < fr { cells.top() } else { cells.top() + geo.frozen_h() };
        let scroll_region = ERect::from_min_max(pos2(left, top), cells.max);
        let painter = ui.painter_at(scroll_region);
        let data = crate::charts::extract(&app.doc.wb, app.sheet, &chart);
        let selected = app.selected_chart == Some(i);
        crate::charts::paint(&painter, rect, &chart, &data, selected);
        let resp = ui.interact(rect.intersect(scroll_region), egui::Id::unique(("rexcel-chart", app.sheet, i)), Sense::click_and_drag());
        let handle = ERect::from_center_size(rect.right_bottom(), vec2(10.0, 10.0));
        let hresp = ui.interact(handle.intersect(scroll_region), egui::Id::unique(("rexcel-chart-h", app.sheet, i)), Sense::drag());
        if selected {
            for c in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom(), rect.center_top(), rect.center_bottom(), rect.left_center(), rect.right_center()] {
                painter.circle(c, 4.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(100)));
            }
        }
        if hresp.hovered() || matches!(app.grid.drag, Some(Drag::Chart { resize: true, .. })) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
        } else if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Move);
        }
        if resp.clicked() || resp.drag_started() || hresp.drag_started() {
            app.commit_if_editing();
            app.selected_chart = Some(i);
        }
        if resp.double_clicked() {
            app.dialog = Some(crate::dialogs::Dialog::chart(i, &chart));
        }
        if resp.secondary_clicked() {
            app.selected_chart = Some(i);
        }
        resp.context_menu(|ui| {
            if ui.button("Edit Chart...").clicked() {
                app.dialog = Some(crate::dialogs::Dialog::chart(i, &chart));
                ui.close();
            }
            if ui.button("Delete Chart").clicked() {
                app.modify("Delete Chart", engine::ops::Scope::Sheet(app.sheet), |wb, s| {
                    wb.sheets[s].charts.remove(i);
                });
                app.selected_chart = None;
                ui.close();
            }
        });
        if (hresp.drag_started() || resp.drag_started()) && app.grid.drag.is_none() {
            if let Some(pp) = pointer {
                app.grid.drag = Some(Drag::Chart { idx: i, resize: hresp.drag_started(), start: pp, orig_anchor: chart.anchor, orig_size: vec2(chart.width, chart.height), offset: egui::Vec2::ZERO });
            }
        }
        if resp.drag_stopped() || hresp.drag_stopped() {
            if let Some(Drag::Chart { idx, resize, start, orig_anchor, orig_size, .. }) = app.grid.drag.take() {
                if let Some(pp) = pointer.or(ui.input(|i| i.pointer.latest_pos())) {
                    let d = pp - start;
                    if resize {
                        let ns = (orig_size + d / geo.z).max(vec2(120.0, 80.0));
                        app.modify("Resize Chart", engine::ops::Scope::Sheet(app.sheet), |wb, s| {
                            wb.sheets[s].charts[idx].width = ns.x.round();
                            wb.sheets[s].charts[idx].height = ns.y.round();
                        });
                    } else {
                        let tl = pos2(geo.col_x(orig_anchor.col), geo.row_y(orig_anchor.row)) + d;
                        let np = geo.cell_at(pos2(tl.x.max(cells.left()) + 1.0, tl.y.max(cells.top()) + 1.0));
                        app.modify("Move Chart", engine::ops::Scope::Sheet(app.sheet), |wb, s| wb.sheets[s].charts[idx].anchor = np);
                    }
                }
            }
        }
    }
}

fn scrollbars(app: &mut App, ui: &mut egui::Ui, geo: &Geo, vbar: ERect, bottom: ERect) {
    let painter = ui.painter();
    let s = app.sheet_ref();
    let ext = s.used_range().map(|r| r.bottom_right()).unwrap_or(CellPos::new(0, 0));
    let view_h = ((geo.cells.height() - geo.frozen_h()) / geo.z) as f64;
    let view_w = ((geo.cells.width() - geo.frozen_w()) / geo.z) as f64;
    let content_h = (geo.rows.pos(ext.row.max(app.sel.active.row) + 1) - geo.rows.pos(geo.fr)).max(geo.sy + view_h) + view_h;
    let content_w = (geo.cols.pos(ext.col.max(app.sel.active.col) + 1) - geo.cols.pos(geo.fc)).max(geo.sx + view_w) + view_w * 0.5;

    // tabs and horizontal scrollbar share the bottom strip
    let split = bottom.left() + (bottom.width() * 0.62).max(200.0);
    let tabs_rect = ERect::from_min_max(bottom.min, pos2(split.min(bottom.right()), bottom.bottom()));
    let hbar = ERect::from_min_max(pos2(split + 6.0, bottom.top() + 5.0), pos2(vbar.left() - 2.0, bottom.bottom() - 5.0));
    painter.rect_filled(bottom, 0.0, theme::RIBBON_BG);
    painter.hline(bottom.x_range(), bottom.top() + 0.5, Stroke::new(1.0, theme::BORDER));
    painter.rect_filled(vbar, 0.0, theme::RIBBON_BG);
    painter.vline(vbar.left() + 0.5, vbar.y_range(), Stroke::new(1.0, theme::BORDER));

    let vtrack = vbar.shrink2(vec2(3.0, 4.0));
    let mut sy = app.scroll[1];
    let mut sx = app.scroll[0];
    if let Some(ny) = scrollbar(ui, egui::Id::unique("rexcel-vscroll"), vtrack, true, sy, view_h, content_h, app) {
        sy = ny;
    }
    if hbar.width() > 30.0 {
        if let Some(nx) = scrollbar(ui, egui::Id::unique("rexcel-hscroll"), hbar, false, sx, view_w, content_w, app) {
            sx = nx;
        }
    }
    app.scroll = [sx.max(0.0), sy.max(0.0)];
    crate::sheet_tabs::show(app, ui, tabs_rect);
}

#[allow(clippy::too_many_arguments)]
fn scrollbar(ui: &mut egui::Ui, id: egui::Id, track: ERect, vertical: bool, value: f64, view: f64, content: f64, app: &mut App) -> Option<f64> {
    let len = if vertical { track.height() } else { track.width() } as f64;
    let thumb_len = (len * view / content).clamp(18.0, len);
    let max_val = (content - view).max(1.0);
    let t0 = (value / max_val).clamp(0.0, 1.0) * (len - thumb_len);
    let thumb = if vertical {
        ERect::from_min_size(pos2(track.left(), track.top() + t0 as f32), vec2(track.width(), thumb_len as f32))
    } else {
        ERect::from_min_size(pos2(track.left() + t0 as f32, track.top()), vec2(thumb_len as f32, track.height()))
    };
    let resp = ui.interact(track, id, Sense::click_and_drag());
    let painter = ui.painter();
    let col = if resp.dragged() { Color32::from_gray(120) } else if resp.hovered() { Color32::from_gray(160) } else { Color32::from_gray(195) };
    painter.rect_filled(thumb.shrink2(if vertical { vec2(1.5, 0.0) } else { vec2(0.0, 1.5) }), 4.0, col);
    let pos = resp.interact_pointer_pos()?;
    let along = if vertical { pos.y - track.top() } else { pos.x - track.left() } as f64;
    if resp.drag_started() || resp.is_pointer_button_down_on() && !matches!(app.grid.drag, Some(Drag::VScroll { .. } | Drag::HScroll { .. })) {
        let in_thumb = thumb.contains(pos);
        let grab = if in_thumb { along - t0 } else { thumb_len / 2.0 };
        app.grid.drag = Some(if vertical { Drag::VScroll { grab: grab as f32 } } else { Drag::HScroll { grab: grab as f32 } });
    }
    let grab = match app.grid.drag {
        Some(Drag::VScroll { grab }) if vertical => grab as f64,
        Some(Drag::HScroll { grab }) if !vertical => grab as f64,
        _ => return None,
    };
    if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
        app.grid.drag = None;
    }
    let t = ((along - grab) / (len - thumb_len).max(1.0)).clamp(0.0, 1.0);
    Some(t * max_val)
}

// ======================================================================
// Autofit
// ======================================================================

pub fn autofit_cols(app: &mut App, ui: &egui::Ui, cols: Vec<u32>) {
    let wb = &app.doc.wb;
    let s = &wb.sheets[app.sheet];
    let Some(ext) = s.data_extent() else { return };
    let painter = ui.painter();
    let mut widths = Vec::new();
    for c in cols {
        let mut w: f32 = 0.0;
        let mut n = 0;
        for r in ext.top..=ext.bottom {
            let p = CellPos::new(r, c);
            if s.merge_at(p).is_some() {
                continue;
            }
            if let Some(t) = cell_text(app, wb, app.sheet, p, 1.0, None) {
                let g = layout(painter, &t, &t.text.replace('\n', " "), None);
                w = w.max(g.size().x + 8.0 + t.indent);
            }
            n += 1;
            if n > 20000 {
                break;
            }
        }
        if w > 0.0 {
            widths.push((c, w.ceil().min(1500.0)));
        }
    }
    if widths.is_empty() {
        return;
    }
    app.modify("AutoFit Column Width", engine::ops::Scope::Sheet(app.sheet), |wb, sh| {
        for (c, w) in &widths {
            wb.sheets[sh].col_widths.insert(*c, *w);
        }
    });
}

pub fn autofit_rows(app: &mut App, ui: &egui::Ui, rows: Vec<u32>) {
    let wb = &app.doc.wb;
    let s = &wb.sheets[app.sheet];
    let painter = ui.painter();
    let mut heights = Vec::new();
    for r in rows {
        let mut h: f32 = DEFAULT_ROW_HEIGHT;
        for (p, _) in s.cells.range(CellPos::new(r, 0)..=CellPos::new(r, MAX_COLS - 1)) {
            if let Some(t) = cell_text(app, wb, app.sheet, *p, 1.0, None) {
                let g = if t.wrap { layout(painter, &t, &t.text, Some(s.col_width(p.col) - 5.0)) } else { layout(painter, &t, &t.text, None) };
                h = h.max(g.size().y + 5.0);
            }
        }
        heights.push((r, h.ceil()));
    }
    app.modify("AutoFit Row Height", engine::ops::Scope::Sheet(app.sheet), |wb, sh| {
        for (r, h) in &heights {
            if (*h - DEFAULT_ROW_HEIGHT).abs() < 0.5 {
                wb.sheets[sh].row_heights.remove(r);
            } else {
                wb.sheets[sh].row_heights.insert(*r, *h);
            }
        }
    });
}
