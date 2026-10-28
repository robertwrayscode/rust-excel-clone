//! Printing: page setup, splitting sheets into pages, the File > Print view
//! (settings plus a page preview) and PDF export.
//!
//! Pages are built as simple drawing lists (rectangles, lines, text) using the
//! RWord font database for measuring, so the preview, the PDF and the printed
//! page all come from the same layout. `print_os` sends the pages to the
//! system's print window.

use std::collections::{HashMap, HashSet};

use egui::{pos2, vec2, Align, Color32, Layout, RichText, Stroke, Ui};
use engine::address::{col_to_letters, CellPos, Rect};
use engine::condfmt::CfFormat;
use engine::style::{BorderKind, BorderLine, HAlign, VAlign};
use engine::{Value, Workbook};
use wordcore::fonts::{FaceId, FontDb};
use wordcore::layout::{DocLayout, LineStyle, Page, Shape};

use crate::app::App;
use crate::theme;

/// Screen pixels (96 dpi) to points.
const PX_PT: f32 = 0.75;

// ======================================================================
// Page setup
// ======================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintWhat {
    ActiveSheet,
    Workbook,
    Selection,
}

impl PrintWhat {
    const ALL: [PrintWhat; 3] = [PrintWhat::ActiveSheet, PrintWhat::Workbook, PrintWhat::Selection];
    fn label(self) -> &'static str {
        match self {
            PrintWhat::ActiveSheet => "Print Active Sheet",
            PrintWhat::Workbook => "Print Entire Workbook",
            PrintWhat::Selection => "Print Selection",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    Letter,
    Legal,
    Tabloid,
    A4,
    A3,
}

impl Paper {
    const ALL: [Paper; 5] = [Paper::Letter, Paper::Legal, Paper::Tabloid, Paper::A4, Paper::A3];
    /// Portrait width and height in points.
    pub fn size(self) -> (f32, f32) {
        match self {
            Paper::Letter => (612.0, 792.0),
            Paper::Legal => (612.0, 1008.0),
            Paper::Tabloid => (792.0, 1224.0),
            Paper::A4 => (595.28, 841.89),
            Paper::A3 => (841.89, 1190.55),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Paper::Letter => "Letter (8.5\" x 11\")",
            Paper::Legal => "Legal (8.5\" x 14\")",
            Paper::Tabloid => "Tabloid (11\" x 17\")",
            Paper::A4 => "A4 (21 cm x 29.7 cm)",
            Paper::A3 => "A3 (29.7 cm x 42 cm)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margins {
    Normal,
    Wide,
    Narrow,
}

impl Margins {
    const ALL: [Margins; 3] = [Margins::Normal, Margins::Wide, Margins::Narrow];
    /// (top, bottom, left, right) in points.
    fn points(self) -> (f32, f32, f32, f32) {
        match self {
            Margins::Normal => (54.0, 54.0, 50.4, 50.4),
            Margins::Wide => (72.0, 72.0, 72.0, 72.0),
            Margins::Narrow => (54.0, 54.0, 18.0, 18.0),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Margins::Normal => "Normal Margins",
            Margins::Wide => "Wide Margins",
            Margins::Narrow => "Narrow Margins",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scaling {
    None,
    FitSheet,
    FitColumns,
    FitRows,
}

impl Scaling {
    const ALL: [Scaling; 4] = [Scaling::None, Scaling::FitSheet, Scaling::FitColumns, Scaling::FitRows];
    fn label(self) -> &'static str {
        match self {
            Scaling::None => "No Scaling",
            Scaling::FitSheet => "Fit Sheet on One Page",
            Scaling::FitColumns => "Fit All Columns on One Page",
            Scaling::FitRows => "Fit All Rows on One Page",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    pub what: PrintWhat,
    pub paper: Paper,
    pub landscape: bool,
    pub margins: Margins,
    pub scaling: Scaling,
    pub gridlines: bool,
    pub headings: bool,
    pub center_h: bool,
    pub page_numbers: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            what: PrintWhat::ActiveSheet,
            paper: Paper::Letter,
            landscape: false,
            margins: Margins::Normal,
            scaling: Scaling::None,
            gridlines: false,
            headings: false,
            center_h: false,
            page_numbers: false,
        }
    }
}

impl PageSetup {
    /// Page width and height in points, after orientation.
    pub fn page_size(&self) -> (f32, f32) {
        let (w, h) = self.paper.size();
        if self.landscape {
            (h, w)
        } else {
            (w, h)
        }
    }
}

// ======================================================================
// Layout
// ======================================================================

fn wcolor(c: engine::style::Color) -> wordcore::Color {
    wordcore::Color { r: c.r(), g: c.g(), b: c.b() }
}

fn rgb(r: u8, g: u8, b: u8) -> wordcore::Color {
    wordcore::Color { r, g, b }
}

/// Width of `text` in points.
fn text_width(db: &FontDb, face: FaceId, size: f32, text: &str) -> f32 {
    let f = db.face(face);
    text.chars().map(|c| db.face(db.resolve_char(face, c)).advance(c)).sum::<f32>() * size * if f.fake_bold { 1.02 } else { 1.0 }
}

/// Drop characters from the end until the text fits.
fn clip_end(db: &FontDb, face: FaceId, size: f32, text: &str, width: f32) -> String {
    let mut out = String::new();
    let mut w = 0.0;
    for c in text.chars() {
        let a = db.face(db.resolve_char(face, c)).advance(c) * size;
        if w + a > width + 0.01 {
            break;
        }
        w += a;
        out.push(c);
    }
    out
}

/// Drop characters from the start until the text fits.
fn clip_start(db: &FontDb, face: FaceId, size: f32, text: &str, width: f32) -> String {
    let mut out: Vec<char> = Vec::new();
    let mut w = 0.0;
    for c in text.chars().rev() {
        let a = db.face(db.resolve_char(face, c)).advance(c) * size;
        if w + a > width + 0.01 {
            break;
        }
        w += a;
        out.push(c);
    }
    out.into_iter().rev().collect()
}

/// Break text into lines no wider than `width`.
fn wrap_lines(db: &FontDb, face: FaceId, size: f32, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let cand = if line.is_empty() { word.to_string() } else { format!("{} {}", line, word) };
            if text_width(db, face, size, &cand) <= width || line.is_empty() {
                line = cand;
                // a single word longer than the line: break it
                while text_width(db, face, size, &line) > width && line.chars().count() > 1 {
                    let head = clip_end(db, face, size, &line, width);
                    let head = if head.is_empty() { line.chars().next().unwrap().to_string() } else { head };
                    let rest = line[head.len()..].to_string();
                    lines.push(head);
                    line = rest;
                }
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines
}

/// Everything needed to draw one cell's text.
#[derive(Clone)]
struct CellText {
    text: String,
    face: FaceId,
    size: f32,
    color: wordcore::Color,
    h: HAlign,
    v: VAlign,
    wrap: bool,
    indent: f32,
    underline: bool,
    strike: bool,
    is_number: bool,
    general: bool,
    value: Value,
}

fn cell_text(db: &FontDb, wb: &Workbook, sheet: usize, p: CellPos, cf: Option<&CfFormat>, show_formulas: bool, s: f32) -> Option<CellText> {
    let sh = &wb.sheets[sheet];
    let cell = sh.cells.get(&p);
    let st = wb.style(cell.map(|c| c.style).unwrap_or(0));
    let show_formula = show_formulas && cell.map(|c| c.formula.is_some()).unwrap_or(false);
    let v = sh.value_at(p);
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
    let is_link = sh.hyperlinks.contains_key(&p);
    let color = cf
        .and_then(|c| c.font_color)
        .or(fmt_color)
        .or(st.font_color)
        .map(wcolor)
        .unwrap_or(if is_link { rgb(0x05, 0x63, 0xC1) } else { rgb(0, 0, 0) });
    let bold = st.bold || cf.map(|c| c.bold).unwrap_or(false);
    let italic = st.italic || cf.map(|c| c.italic).unwrap_or(false);
    let face = db.face_for(st.font_name.as_deref().unwrap_or(engine::style::DEFAULT_FONT), bold, italic);
    let h = match st.h_align {
        HAlign::General => {
            if show_formula {
                HAlign::Left
            } else {
                match v {
                    Value::Number(_) => HAlign::Right,
                    Value::Bool(_) | Value::Error(_) => HAlign::Center,
                    _ => HAlign::Left,
                }
            }
        }
        HAlign::Fill | HAlign::Justify => HAlign::Left,
        HAlign::CenterAcross => HAlign::Center,
        a => a,
    };
    Some(CellText {
        is_number: matches!(v, Value::Number(_)) && !show_formula,
        general: engine::numfmt::is_general(&st.num_fmt),
        text,
        face,
        size: st.font_size() * s,
        color,
        h,
        v: st.v_align,
        wrap: st.wrap,
        indent: st.indent as f32 * 9.0 * PX_PT * s,
        underline: st.underline || is_link,
        strike: st.strike,
        value: v,
    })
}

/// Shrink a General number to fit, else fill with #.
fn fit_number(db: &FontDb, t: &CellText, avail: f32) -> String {
    let w = |x: &str| text_width(db, t.face, t.size, x);
    if w(&t.text) <= avail {
        return t.text.clone();
    }
    if t.general {
        if let Value::Number(n) = t.value {
            if let Some(dot) = t.text.find('.') {
                if !t.text.contains('E') {
                    let decimals = t.text.len() - dot - 1;
                    for d in (0..decimals).rev() {
                        let cand = format!("{:.*}", d, n);
                        if w(&cand) <= avail {
                            return cand;
                        }
                    }
                }
            }
            for d in (0..5).rev() {
                let cand = format!("{:.*E}", d, n).replace('E', "E+").replace("E+-", "E-");
                if w(&cand) <= avail {
                    return cand;
                }
            }
        }
    }
    let hw = w("#").max(0.1);
    "#".repeat(((avail / hw).floor() as usize).max(1))
}

/// Builds the shapes for one page.
struct Painter<'a> {
    db: &'a FontDb,
    shapes: Vec<Shape>,
}

impl Painter<'_> {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: wordcore::Color) {
        if w > 0.0 && h > 0.0 {
            self.shapes.push(Shape::Rect { x, y, w, h, color });
        }
    }
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: wordcore::Color, style: LineStyle) {
        self.shapes.push(Shape::Line { x1, y1, x2, y2, width, color, style });
    }
    fn text(&mut self, x: f32, baseline: f32, text: &str, face: FaceId, size: f32, color: wordcore::Color) {
        if text.is_empty() {
            return;
        }
        let f = self.db.face(face);
        self.shapes.push(Shape::Text {
            x,
            y: baseline,
            text: text.to_string(),
            face,
            size,
            color,
            spacing: 0.0,
            fake_bold: f.fake_bold,
            fake_italic: f.fake_italic,
            link: None,
        });
    }
    /// Text with underline/strikethrough decorations.
    fn styled_text(&mut self, x: f32, baseline: f32, text: &str, t: &CellText) {
        self.text(x, baseline, text, t.face, t.size, t.color);
        let f = self.db.face(t.face);
        let w = text_width(self.db, t.face, t.size, text);
        if t.underline {
            let y = baseline + f.underline_pos * t.size;
            self.line(x, y, x + w, y, (f.underline_thickness * t.size).max(0.3), t.color, LineStyle::Solid);
        }
        if t.strike {
            let y = baseline - f.strike_pos * t.size;
            self.line(x, y, x + w, y, (f.underline_thickness * t.size).max(0.3), t.color, LineStyle::Solid);
        }
    }
}

fn border_style(b: &BorderLine, s: f32) -> (f32, LineStyle) {
    let (w, st) = match b.kind {
        BorderKind::Thin => (0.5, LineStyle::Solid),
        BorderKind::Medium => (1.25, LineStyle::Solid),
        BorderKind::Thick => (2.0, LineStyle::Solid),
        BorderKind::Dashed => (0.5, LineStyle::Dashed),
        BorderKind::Dotted => (0.5, LineStyle::Dotted),
        BorderKind::Double => (1.5, LineStyle::Double),
    };
    ((w * s).max(0.25), st)
}

/// The cells of one sheet to print.
pub struct SheetJob {
    pub sheet: usize,
    pub area: Rect,
}

/// The cells a chart covers.
fn chart_extent(s: &engine::workbook::Sheet, c: &engine::workbook::Chart) -> Rect {
    let mut col = c.anchor.col;
    let mut w = 0.0;
    while w + s.col_width(col) < c.width && col < engine::address::MAX_COLS - 1 {
        w += s.col_width(col);
        col += 1;
    }
    let mut row = c.anchor.row;
    let mut h = 0.0;
    while h + s.row_height(row) < c.height && row < engine::address::MAX_ROWS - 1 {
        h += s.row_height(row);
        row += 1;
    }
    Rect { top: c.anchor.row, left: c.anchor.col, bottom: row, right: col }
}

/// Which areas to print for the current settings. Empty when there is nothing to print.
pub fn jobs(app: &App, setup: &PageSetup) -> Vec<SheetJob> {
    let wb = &app.doc.wb;
    let used = |i: usize| -> Option<Rect> {
        let s = &wb.sheets[i];
        let mut r = s.used_range();
        for c in &s.charts {
            let e = chart_extent(s, c);
            r = Some(match r {
                None => e,
                Some(x) => Rect { top: x.top.min(e.top), left: x.left.min(e.left), bottom: x.bottom.max(e.bottom), right: x.right.max(e.right) },
            });
        }
        r
    };
    match setup.what {
        PrintWhat::ActiveSheet => used(app.sheet).map(|area| vec![SheetJob { sheet: app.sheet, area }]).unwrap_or_default(),
        PrintWhat::Workbook => (0..wb.sheets.len()).filter(|&i| !wb.sheets[i].hidden).filter_map(|i| used(i).map(|area| SheetJob { sheet: i, area })).collect(),
        PrintWhat::Selection => app.sel.ranges.iter().map(|r| SheetJob { sheet: app.sheet, area: *r }).collect(),
    }
}

/// Lay out the jobs as pages.
pub fn layout(wb: &Workbook, jobs: &[SheetJob], setup: &PageSetup, show_formulas: bool) -> Vec<Page> {
    let db = FontDb::global();
    let mut pages = Vec::new();
    for job in jobs {
        layout_sheet(db, wb, job, setup, show_formulas, &mut pages);
    }
    let n = pages.len();
    for (i, p) in pages.iter_mut().enumerate() {
        p.index = i;
        p.number = i as u32 + 1;
        if setup.page_numbers {
            let face = db.face_for("Calibri", false, false);
            let label = format!("Page {} of {}", i + 1, n);
            let w = text_width(db, face, 11.0, &label);
            let (_, bottom, _, _) = setup.margins.points();
            let y = p.height - (bottom * 0.4).max(16.0);
            p.shapes.push(Shape::Text { x: (p.width - w) / 2.0, y, text: label, face, size: 11.0, color: rgb(0, 0, 0), spacing: 0.0, fake_bold: false, fake_italic: false, link: None });
        }
    }
    pages
}

fn layout_sheet(db: &FontDb, wb: &Workbook, job: &SheetJob, setup: &PageSetup, show_formulas: bool, pages: &mut Vec<Page>) {
    let sh = &wb.sheets[job.sheet];
    let area = job.area;
    let (pw, ph) = setup.page_size();
    let (mt, mb, ml, mr) = setup.margins.points();
    let avail_w = (pw - ml - mr).max(36.0);
    let avail_h = (ph - mt - mb).max(36.0);

    let cols: Vec<(u32, f32)> = (area.left..=area.right).map(|c| (c, sh.col_width(c) * PX_PT)).filter(|(_, w)| *w > 0.0).collect();
    let rows: Vec<(u32, f32)> = (area.top..=area.bottom).map(|r| (r, sh.row_height(r) * PX_PT)).filter(|(_, h)| *h > 0.0).collect();
    if cols.is_empty() || rows.is_empty() {
        return;
    }
    let hdr_face = db.face_for("Calibri", false, false);
    let (hdr_w, hdr_h) = if setup.headings {
        let digits = (area.bottom + 1).to_string();
        ((text_width(db, hdr_face, 11.0, &digits) + 10.0).max(24.0), 15.0)
    } else {
        (0.0, 0.0)
    };
    let total_w: f32 = hdr_w + cols.iter().map(|c| c.1).sum::<f32>();
    let total_h: f32 = hdr_h + rows.iter().map(|r| r.1).sum::<f32>();
    let s = match setup.scaling {
        Scaling::None => 1.0,
        Scaling::FitSheet => (avail_w / total_w).min(avail_h / total_h).min(1.0),
        Scaling::FitColumns => (avail_w / total_w).min(1.0),
        Scaling::FitRows => (avail_h / total_h).min(1.0),
    }
    .max(0.1);

    let bands = |items: &[(u32, f32)], hdr: f32, avail: f32| -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0;
        let mut acc = hdr * s;
        for (i, it) in items.iter().enumerate() {
            let w = it.1 * s;
            if i > start && acc + w > avail + 0.01 {
                out.push((start, i));
                start = i;
                acc = hdr * s;
            }
            acc += w;
        }
        out.push((start, items.len()));
        out
    };
    let col_bands = bands(&cols, hdr_w, avail_w);
    let row_bands = bands(&rows, hdr_h, avail_h);
    let cf = if sh.cond_formats.is_empty() { HashMap::new() } else { engine::condfmt::evaluate(wb, job.sheet, area) };

    // Excel's default page order: down, then over.
    for &(c0, c1) in &col_bands {
        for &(r0, r1) in &row_bands {
            let band_cols = &cols[c0..c1];
            let band_rows = &rows[r0..r1];
            let band_w = hdr_w * s + band_cols.iter().map(|c| c.1 * s).sum::<f32>();
            let ox = ml + if setup.center_h { ((avail_w - band_w) / 2.0).max(0.0) } else { 0.0 };
            let oy = mt;
            let mut p = Painter { db, shapes: Vec::new() };
            draw_band(&mut p, wb, job.sheet, band_cols, band_rows, ox, oy, hdr_w * s, hdr_h * s, s, setup, &cf, show_formulas);
            pages.push(Page { width: pw, height: ph, shapes: p.shapes, ..Default::default() });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_band(
    p: &mut Painter,
    wb: &Workbook,
    sheet: usize,
    cols: &[(u32, f32)],
    rows: &[(u32, f32)],
    ox: f32,
    oy: f32,
    hdr_w: f32,
    hdr_h: f32,
    s: f32,
    setup: &PageSetup,
    cf: &HashMap<CellPos, CfFormat>,
    show_formulas: bool,
) {
    let db = p.db;
    let sh = &wb.sheets[sheet];
    // positions of each column / row in this band
    let mut col_x: HashMap<u32, (f32, f32)> = HashMap::new();
    let mut x = ox + hdr_w;
    for &(c, w) in cols {
        col_x.insert(c, (x, w * s));
        x += w * s;
    }
    let x_end = x;
    let mut row_y: HashMap<u32, (f32, f32)> = HashMap::new();
    let mut y = oy + hdr_h;
    for &(r, h) in rows {
        row_y.insert(r, (y, h * s));
        y += h * s;
    }
    let y_end = y;
    let band = Rect { top: rows[0].0, bottom: rows[rows.len() - 1].0, left: cols[0].0, right: cols[cols.len() - 1].0 };
    let cell_rect = |q: CellPos| -> Option<(f32, f32, f32, f32)> {
        let (x, w) = *col_x.get(&q.col)?;
        let (y, h) = *row_y.get(&q.row)?;
        Some((x, y, w, h))
    };
    // A merged area clipped to this band.
    let range_rect = |m: Rect| -> Option<(f32, f32, f32, f32)> {
        let m = m.intersection(&band)?;
        let mut x0 = f32::MAX;
        let mut x1 = f32::MIN;
        let mut y0 = f32::MAX;
        let mut y1 = f32::MIN;
        for c in m.left..=m.right {
            if let Some(&(x, w)) = col_x.get(&c) {
                x0 = x0.min(x);
                x1 = x1.max(x + w);
            }
        }
        for r in m.top..=m.bottom {
            if let Some(&(y, h)) = row_y.get(&r) {
                y0 = y0.min(y);
                y1 = y1.max(y + h);
            }
        }
        (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
    };
    let merges: Vec<Rect> = sh.merges.iter().copied().filter(|m| m.intersects(&band)).collect();
    let merge_of = |q: CellPos| merges.iter().find(|m| m.contains(q)).copied();
    let fill_of = |q: CellPos| -> Option<engine::style::Color> {
        let st = wb.style(sh.cells.get(&q).map(|c| c.style).unwrap_or(0));
        cf.get(&q).and_then(|f| f.fill).or(st.fill)
    };

    // 1) fills
    for &(r, _) in rows {
        for &(c, _) in cols {
            let q = CellPos::new(r, c);
            if merge_of(q).is_some() {
                continue;
            }
            if let (Some(f), Some((x, y, w, h))) = (fill_of(q), cell_rect(q)) {
                p.rect(x, y, w, h, wcolor(f));
            }
        }
    }

    // 2) text layout, with overflow into empty neighbours
    struct Item {
        x: f32,
        baselines: Vec<(f32, String)>,
        t: CellText,
    }
    let mut items: Vec<Item> = Vec::new();
    let mut gaps: HashSet<(u32, u32)> = HashSet::new(); // (row, col) whose left gridline is hidden
    let pad = 2.0 * s;
    let occupied = |q: CellPos| !sh.value_at(q).is_empty() || merge_of(q).is_some();
    for &(r, _) in rows {
        for (ci, &(c, _)) in cols.iter().enumerate() {
            let q = CellPos::new(r, c);
            if merge_of(q).is_some() {
                continue;
            }
            let Some(t) = cell_text(db, wb, sheet, q, cf.get(&q), show_formulas, s) else { continue };
            let Some((cx, cy, cw, ch)) = cell_rect(q) else { continue };
            let avail = (cw - 2.0 * pad - t.indent).max(1.0);
            let f = db.face(t.face);
            if t.wrap {
                let lines = wrap_lines(db, t.face, t.size, &t.text, avail);
                let lh = f.line_height() * t.size;
                let total = lh * lines.len() as f32;
                let top = match t.v {
                    VAlign::Top => cy + pad * 0.5,
                    VAlign::Center => cy + (ch - total) / 2.0,
                    VAlign::Bottom => cy + ch - total - pad * 0.5,
                };
                for (i, l) in lines.into_iter().enumerate() {
                    let ly = top + lh * i as f32;
                    if ly < cy - 0.5 || ly + lh > cy + ch + 0.5 {
                        continue;
                    }
                    let w = text_width(db, t.face, t.size, &l);
                    let lx = match t.h {
                        HAlign::Right => cx + cw - pad - w - t.indent,
                        HAlign::Center => cx + (cw - w) / 2.0,
                        _ => cx + pad + t.indent,
                    };
                    items.push(Item { x: lx, baselines: vec![(ly + f.ascent * t.size, l)], t: t.clone() });
                }
                continue;
            }
            let txt = if t.is_number { fit_number(db, &t, avail) } else { t.text.replace('\n', " ") };
            let w = text_width(db, t.face, t.size, &txt) + 2.0 * pad + t.indent;
            // extend into empty cells inside this band
            let (mut ext_l, mut ext_r) = (cx, cx + cw);
            if !t.is_number && w > cw {
                let need = w - cw;
                let grow_right = |gaps: &mut HashSet<(u32, u32)>, limit: f32| -> f32 {
                    let mut extra = 0.0;
                    let mut j = ci + 1;
                    while extra < limit && j < cols.len() && !occupied(CellPos::new(r, cols[j].0)) {
                        extra += cols[j].1 * s;
                        gaps.insert((r, cols[j].0));
                        j += 1;
                    }
                    extra
                };
                let grow_left = |gaps: &mut HashSet<(u32, u32)>, limit: f32| -> f32 {
                    let mut extra = 0.0;
                    let mut j = ci;
                    while extra < limit && j > 0 && !occupied(CellPos::new(r, cols[j - 1].0)) {
                        gaps.insert((r, cols[j].0));
                        j -= 1;
                        extra += cols[j].1 * s;
                    }
                    extra
                };
                match t.h {
                    HAlign::Left | HAlign::General => ext_r += grow_right(&mut gaps, need),
                    HAlign::Right => ext_l -= grow_left(&mut gaps, need),
                    _ => {
                        let el = grow_left(&mut gaps, need / 2.0);
                        let er = grow_right(&mut gaps, need / 2.0);
                        let e = el.min(er);
                        ext_l -= e;
                        ext_r += e;
                    }
                }
            }
            let room = (ext_r - ext_l - 2.0 * pad - t.indent).max(0.0);
            let shown = if t.is_number {
                txt
            } else {
                match t.h {
                    HAlign::Right => clip_start(db, t.face, t.size, &txt, room),
                    HAlign::Center => {
                        if text_width(db, t.face, t.size, &txt) <= room {
                            txt
                        } else {
                            clip_end(db, t.face, t.size, &txt, room)
                        }
                    }
                    _ => clip_end(db, t.face, t.size, &txt, room),
                }
            };
            let tw = text_width(db, t.face, t.size, &shown);
            let lx = match t.h {
                HAlign::Right => cx + cw - pad - tw - t.indent,
                HAlign::Center => cx + (cw - tw) / 2.0,
                _ => cx + pad + t.indent,
            };
            let base = baseline(db, &t, cy, ch, pad);
            items.push(Item { x: lx, baselines: vec![(base, shown)], t });
        }
    }

    // 3) gridlines
    let grid = rgb(0xA6, 0xA6, 0xA6);
    let gw = 0.4;
    if setup.gridlines {
        let mut yy = oy + hdr_h;
        p.line(ox + hdr_w, yy, x_end, yy, gw, grid, LineStyle::Solid);
        for &(_, h) in rows {
            yy += h * s;
            p.line(ox + hdr_w, yy, x_end, yy, gw, grid, LineStyle::Solid);
        }
        // vertical: left edge of each column, skipping overflow gaps
        let mut xs: Vec<(Option<u32>, f32)> = cols.iter().map(|&(c, _)| (Some(c), col_x[&c].0)).collect();
        xs.push((None, x_end));
        for (c, xx) in xs {
            let mut seg: Option<f32> = None;
            for &(r, _) in rows {
                let (ry, rh) = row_y[&r];
                let hidden = c.map(|c| gaps.contains(&(r, c))).unwrap_or(false);
                if hidden {
                    if let Some(sy) = seg.take() {
                        p.line(xx, sy, xx, ry, gw, grid, LineStyle::Solid);
                    }
                } else if seg.is_none() {
                    seg = Some(ry);
                }
                let _ = rh;
            }
            if let Some(sy) = seg {
                p.line(xx, sy, xx, y_end, gw, grid, LineStyle::Solid);
            }
        }
        // filled cells hide gridlines
        for &(r, _) in rows {
            for &(c, _) in cols {
                let q = CellPos::new(r, c);
                if merge_of(q).is_some() {
                    continue;
                }
                if let (Some(f), Some((x, y, w, h))) = (fill_of(q), cell_rect(q)) {
                    p.rect(x + gw / 2.0, y + gw / 2.0, w - gw, h - gw, wcolor(f));
                }
            }
        }
    }

    // 4) merged cells: background over interior gridlines, then text
    for m in &merges {
        let Some((mx, my, mw, mh)) = range_rect(*m) else { continue };
        let tl = m.top_left();
        let fill = fill_of(tl).map(wcolor);
        let inset = if setup.gridlines { gw / 2.0 } else { 0.0 };
        if let Some(f) = fill {
            p.rect(mx, my, mw, mh, f);
        } else if setup.gridlines {
            p.rect(mx + inset, my + inset, mw - 2.0 * inset, mh - 2.0 * inset, rgb(255, 255, 255));
        }
        if !band.contains(tl) {
            continue;
        }
        if let Some(t) = cell_text(db, wb, sheet, tl, cf.get(&tl), show_formulas, s) {
            let avail = (mw - 2.0 * pad).max(1.0);
            let f = db.face(t.face);
            let lines = if t.wrap { wrap_lines(db, t.face, t.size, &t.text, avail) } else if t.is_number { vec![fit_number(db, &t, avail)] } else { vec![clip_end(db, t.face, t.size, &t.text.replace('\n', " "), avail)] };
            let lh = f.line_height() * t.size;
            let total = lh * lines.len() as f32;
            let top = match t.v {
                VAlign::Top => my + pad * 0.5,
                VAlign::Center => my + (mh - total) / 2.0,
                VAlign::Bottom => my + mh - total - pad * 0.5,
            };
            for (i, l) in lines.into_iter().enumerate() {
                let w = text_width(db, t.face, t.size, &l);
                let lx = match t.h {
                    HAlign::Right => mx + mw - pad - w,
                    HAlign::Center => mx + (mw - w) / 2.0,
                    _ => mx + pad + t.indent,
                };
                let base = top + lh * i as f32 + f.ascent * t.size;
                items.push(Item { x: lx, baselines: vec![(base, l)], t: t.clone() });
            }
        }
    }

    // 5) data bars
    for (q, f) in cf {
        if let (Some((frac, color)), Some((x, y, w, h))) = (f.data_bar, cell_rect(*q)) {
            let c = wcolor(color);
            let light = rgb(((c.r as u16 + 255) / 2) as u8, ((c.g as u16 + 255) / 2) as u8, ((c.b as u16 + 255) / 2) as u8);
            p.rect(x + 1.0 * s, y + 1.5 * s, (w - 2.0 * s) * frac, h - 3.0 * s, light);
        }
    }

    // 6) text
    for it in &items {
        for (base, l) in &it.baselines {
            p.styled_text(it.x, *base, l, &it.t);
        }
    }

    // 7) borders
    for &(r, _) in rows {
        for &(c, _) in cols {
            let q = CellPos::new(r, c);
            let Some(cell) = sh.cells.get(&q) else { continue };
            if cell.style == 0 {
                continue;
            }
            let b = &wb.style(cell.style).borders;
            if b.is_empty() {
                continue;
            }
            let rect = match merge_of(q) {
                Some(m) if m.top_left() == q => range_rect(m),
                Some(_) => continue,
                None => cell_rect(q),
            };
            let Some((x, y, w, h)) = rect else { continue };
            for (line, (x1, y1, x2, y2)) in [(&b.top, (x, y, x + w, y)), (&b.bottom, (x, y + h, x + w, y + h)), (&b.left, (x, y, x, y + h)), (&b.right, (x + w, y, x + w, y + h))] {
                if let Some(line) = line {
                    let (bw, st) = border_style(line, s);
                    p.line(x1, y1, x2, y2, bw, wcolor(line.color), st);
                }
            }
        }
    }

    // 8) charts anchored on this page
    for chart in &sh.charts {
        if let (Some(&(x, _)), Some(&(y, _))) = (col_x.get(&chart.anchor.col), row_y.get(&chart.anchor.row)) {
            p.shapes.extend(crate::print_chart::chart_shapes(wb, sheet, chart, x, y, s));
        }
    }

    // 9) row and column headings
    if setup.headings {
        let face = db.face_for("Calibri", false, false);
        let size = 11.0 * s;
        let bg = rgb(0xF2, 0xF2, 0xF2);
        let line = rgb(0x80, 0x80, 0x80);
        p.rect(ox, oy, hdr_w, hdr_h, bg);
        p.rect(ox + hdr_w, oy, x_end - ox - hdr_w, hdr_h, bg);
        p.rect(ox, oy + hdr_h, hdr_w, y_end - oy - hdr_h, bg);
        let f = db.face(face);
        for &(c, _) in cols {
            let (cx, cw) = col_x[&c];
            let label = col_to_letters(c);
            let w = text_width(db, face, size, &label);
            p.text(cx + (cw - w) / 2.0, oy + hdr_h / 2.0 + (f.ascent - f.descent) * size / 2.0, &label, face, size, rgb(0, 0, 0));
            p.line(cx, oy, cx, oy + hdr_h, 0.4, line, LineStyle::Solid);
        }
        for &(r, _) in rows {
            let (ry, rh) = row_y[&r];
            let label = (r + 1).to_string();
            let w = text_width(db, face, size, &label);
            p.text(ox + (hdr_w - w) / 2.0, ry + rh / 2.0 + (f.ascent - f.descent) * size / 2.0, &label, face, size, rgb(0, 0, 0));
            p.line(ox, ry, ox + hdr_w, ry, 0.4, line, LineStyle::Solid);
        }
        p.line(ox, oy, x_end, oy, 0.4, line, LineStyle::Solid);
        p.line(ox, oy, ox, y_end, 0.4, line, LineStyle::Solid);
        p.line(ox + hdr_w, oy, ox + hdr_w, y_end, 0.4, line, LineStyle::Solid);
        p.line(ox, oy + hdr_h, x_end, oy + hdr_h, 0.4, line, LineStyle::Solid);
        p.line(x_end, oy, x_end, oy + hdr_h, 0.4, line, LineStyle::Solid);
        p.line(ox, y_end, ox + hdr_w, y_end, 0.4, line, LineStyle::Solid);
    }
}

fn baseline(db: &FontDb, t: &CellText, cy: f32, ch: f32, pad: f32) -> f32 {
    let f = db.face(t.face);
    match t.v {
        VAlign::Top => cy + pad * 0.5 + f.ascent * t.size,
        VAlign::Center => cy + ch / 2.0 + (f.ascent - f.descent) * t.size / 2.0,
        VAlign::Bottom => cy + ch - pad * 0.5 - f.descent * t.size,
    }
}

/// Export pages as a PDF file.
pub fn pdf_bytes(pages: &[Page], title: &str) -> Vec<u8> {
    let layout = DocLayout { pages: pages.to_vec(), ..Default::default() };
    let props = wordcore::DocProperties { title: title.to_string(), ..Default::default() };
    wordcore::pdf::export(&layout, FontDb::global(), &props)
}

/// Load the fonts used for printing in the background, so File > Print opens quickly.
pub fn preload() {
    std::thread::spawn(|| {
        let _ = FontDb::global();
    });
}

// ======================================================================
// File > Print view
// ======================================================================

pub struct PrintState {
    /// Preview textures for chart pictures, by image address.
    textures: HashMap<usize, egui::TextureHandle>,
    pub setup: PageSetup,
    pub pages: Vec<Page>,
    pub page: usize,
    /// Settings the pages were laid out for, to know when to redo them.
    laid_out: Option<PageSetup>,
    generation: u64,
}

impl PrintState {
    pub fn new(setup: PageSetup) -> PrintState {
        PrintState { textures: HashMap::new(), setup, pages: Vec::new(), page: 0, laid_out: None, generation: u64::MAX }
    }
}

impl App {
    /// Open File > Print (Ctrl+P).
    pub fn open_print(&mut self) {
        if self.edit.is_some() && !self.commit_edit(0, 0, false) {
            return;
        }
        let setup = self.page_setup.clone();
        self.print = Some(PrintState::new(setup));
    }

    fn print_title(&self) -> String {
        self.doc.title()
    }

    fn refresh_print_pages(&mut self) {
        let Some(ps) = &self.print else { return };
        if ps.laid_out.as_ref() == Some(&ps.setup) && ps.generation == self.generation {
            return;
        }
        let setup = ps.setup.clone();
        let jobs = jobs(self, &setup);
        let pages = layout(&self.doc.wb, &jobs, &setup, self.show_formulas);
        let gen = self.generation;
        if let Some(ps) = &mut self.print {
            ps.page = ps.page.min(pages.len().saturating_sub(1));
            ps.pages = pages;
            ps.textures.clear();
            ps.laid_out = Some(setup);
            ps.generation = gen;
        }
    }

    /// Send the current pages to the system's print window.
    pub fn print_now(&mut self) {
        self.refresh_print_pages();
        let Some(ps) = &self.print else { return };
        if ps.pages.is_empty() {
            self.message("Print", "We couldn't find anything to print.");
            return;
        }
        let title = self.print_title();
        match crate::print_os::print(&ps.pages, &title, &ps.setup) {
            crate::print_os::Outcome::Printed => {
                self.page_setup = ps.setup.clone();
                self.print = None;
                self.set_status("Sent to printer");
            }
            crate::print_os::Outcome::Cancelled => {}
            crate::print_os::Outcome::Failed(e) => self.message("Print", format!("Couldn't print: {}", e)),
        }
    }

    /// Save the current pages as a PDF.
    pub fn export_pdf(&mut self) {
        self.refresh_print_pages();
        let Some(ps) = &self.print else { return };
        if ps.pages.is_empty() {
            self.message("Export to PDF", "We couldn't find anything to print.");
            return;
        }
        let stem = self.doc.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Book1".into());
        let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", stem)).save_file() else { return };
        let bytes = pdf_bytes(&ps.pages, &self.print_title());
        match std::fs::write(&path, bytes) {
            Ok(()) => self.set_status(format!("Saved {}", path.display())),
            Err(e) => self.message("Export to PDF", format!("Couldn't save the PDF: {}", e)),
        }
    }
}

fn combo<T: Copy + PartialEq>(ui: &mut Ui, id: &str, value: &mut T, all: &[T], label: impl Fn(T) -> &'static str) {
    egui::ComboBox::from_id_salt(id).width(250.0).selected_text(label(*value)).show_ui(ui, |ui| {
        for &v in all {
            ui.selectable_value(value, v, label(v));
        }
    });
}

/// The full-window File > Print view.
pub fn show(app: &mut App, ui: &mut Ui) {
    app.refresh_print_pages();
    let ctx = ui.ctx().clone();
    if app.dialog.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if let Some(ps) = app.print.take() {
            app.page_setup = ps.setup;
        }
        return;
    }
    let mut action: Option<&str> = None;
    let mut close = false;
    let modal_open = app.dialog.is_some();
    if !modal_open && ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::P))) {
        action = Some("print");
    }
    egui::Panel::left("print_settings")
        .exact_size(300.0)
        .resizable(false)
        .frame(egui::Frame::new().fill(theme::RIBBON_BG).inner_margin(egui::Margin::same(16)))
        .show(ui, |ui| {
            let Some(ps) = &mut app.print else { return };
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("⬅").size(16.0)).frame_when_inactive(false)).on_hover_text("Back to the sheet (Esc)").clicked() {
                    close = true;
                }
                ui.label(RichText::new("Print").size(24.0).color(theme::TEXT));
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let btn = egui::Button::new(RichText::new("Print").size(15.0).color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(110.0, 44.0));
                if ui.add(btn).on_hover_text("Choose a printer and print (Ctrl+P)").clicked() {
                    action = Some("print");
                }
                if ui.add(egui::Button::new("Save as PDF").min_size(vec2(110.0, 44.0))).on_hover_text("Save these pages as a PDF file").clicked() {
                    action = Some("pdf");
                }
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Settings").strong());
            ui.add_space(4.0);
            combo(ui, "print_what", &mut ps.setup.what, &PrintWhat::ALL, PrintWhat::label);
            let mut orient = ps.setup.landscape;
            egui::ComboBox::from_id_salt("print_orient").width(250.0).selected_text(if orient { "Landscape Orientation" } else { "Portrait Orientation" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut orient, false, "Portrait Orientation");
                ui.selectable_value(&mut orient, true, "Landscape Orientation");
            });
            ps.setup.landscape = orient;
            combo(ui, "print_paper", &mut ps.setup.paper, &Paper::ALL, Paper::label);
            combo(ui, "print_margins", &mut ps.setup.margins, &Margins::ALL, Margins::label);
            combo(ui, "print_scaling", &mut ps.setup.scaling, &Scaling::ALL, Scaling::label);
            ui.add_space(8.0);
            ui.checkbox(&mut ps.setup.gridlines, "Print gridlines");
            ui.checkbox(&mut ps.setup.headings, "Print row and column headings");
            ui.checkbox(&mut ps.setup.center_h, "Center on page horizontally");
            ui.checkbox(&mut ps.setup.page_numbers, "Page numbers at the bottom");
            ui.add_space(12.0);
            ui.label(RichText::new("Printer, copies and page range are chosen in the next window.").size(11.0).color(theme::MUTED));
        });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_gray(0xE6)).inner_margin(egui::Margin::same(16))).show(ui, |ui| {
        let Some(ps) = &mut app.print else { return };
        let n = ps.pages.len();
        let avail = ui.available_rect_before_wrap();
        let nav_h = 32.0;
        let area = egui::Rect::from_min_max(avail.min, pos2(avail.max.x, avail.max.y - nav_h));
        if n == 0 {
            ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, "We couldn't find anything to print.", egui::FontId::proportional(16.0), theme::MUTED);
        } else {
            let page = &ps.pages[ps.page.min(n - 1)];
            let k = (area.width() / page.width).min(area.height() / page.height) * 0.97;
            let size = vec2(page.width * k, page.height * k);
            let r = egui::Rect::from_center_size(area.center(), size);
            let painter = ui.painter_at(area);
            painter.rect_filled(r.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(40));
            painter.rect_filled(r, 0.0, Color32::WHITE);
            paint_page(&painter, r, page, &mut ps.textures);
        }
        let nav = egui::Rect::from_min_max(pos2(avail.min.x, avail.max.y - nav_h), avail.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(nav).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.add_space((nav.width() / 2.0 - 90.0).max(0.0));
            if ui.add_enabled(ps.page > 0, egui::Button::new("◀")).clicked() {
                ps.page -= 1;
            }
            ui.label(format!("Page {} of {}", if n == 0 { 0 } else { ps.page + 1 }, n));
            if ui.add_enabled(ps.page + 1 < n, egui::Button::new("▶")).clicked() {
                ps.page += 1;
            }
        });
        // keyboard paging
        let (pgup, pgdn) = ui.input(|i| (i.key_pressed(egui::Key::PageUp) || i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::ArrowRight)));
        if pgup && ps.page > 0 {
            ps.page -= 1;
        }
        if pgdn && ps.page + 1 < n {
            ps.page += 1;
        }
    });
    if close {
        if let Some(ps) = app.print.take() {
            app.page_setup = ps.setup;
        }
        return;
    }
    match action {
        Some("print") => app.print_now(),
        Some("pdf") => app.export_pdf(),
        _ => {}
    }
}

fn c32(c: wordcore::Color) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

/// Draw a page into `r` on screen.
pub fn paint_page(painter: &egui::Painter, r: egui::Rect, page: &Page, textures: &mut HashMap<usize, egui::TextureHandle>) {
    let db = FontDb::global();
    let k = r.width() / page.width;
    let at = |x: f32, y: f32| pos2(r.min.x + x * k, r.min.y + y * k);
    for s in &page.shapes {
        match s {
            Shape::Rect { x, y, w, h, color } => {
                painter.rect_filled(egui::Rect::from_min_size(at(*x, *y), vec2(w * k, h * k)), 0.0, c32(*color));
            }
            Shape::Line { x1, y1, x2, y2, width, color, .. } => {
                painter.line_segment([at(*x1, *y1), at(*x2, *y2)], Stroke::new((width * k).max(0.5), c32(*color)));
            }
            Shape::Text { x, y, text, face, size, color, .. } => {
                let f = db.face(*face);
                let px = size * k;
                if px < 2.0 {
                    continue;
                }
                let (font, fake_bold) = crate::fonts::cell_font(Some(&f.family), f.bold, px);
                let mut job = egui::text::LayoutJob::default();
                job.append(text, 0.0, egui::TextFormat { font_id: font, color: c32(*color), italics: f.italic, ..Default::default() });
                let g = painter.layout_job(job);
                let top = at(*x, y - f.ascent * size);
                painter.galley(top, g.clone(), c32(*color));
                if fake_bold {
                    painter.galley(top + vec2(0.5, 0.0), g, c32(*color));
                }
            }
            Shape::Image { x, y, w, h, image } => {
                let key = std::sync::Arc::as_ptr(image) as usize;
                if !textures.contains_key(&key) {
                    if let Some((size, rgba)) = crate::print_chart::decode_bmp(&image.bytes) {
                        let img = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                        textures.insert(key, painter.ctx().load_texture(format!("print-image-{}", key), img, egui::TextureOptions::LINEAR));
                    }
                }
                if let Some(t) = textures.get(&key) {
                    let rr = egui::Rect::from_min_size(at(*x, *y), vec2(w * k, h * k));
                    painter.image(t.id(), rr, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wb_with(n: u32) -> Workbook {
        let mut wb = Workbook::new();
        for r in 0..n {
            wb.set_input(0, CellPos::new(r, 0), &format!("Row {}", r + 1)).unwrap();
            wb.set_input(0, CellPos::new(r, 1), &format!("{}", r * 10)).unwrap();
        }
        wb.recalculate();
        wb
    }

    #[test]
    fn paginates_rows() {
        let wb = wb_with(120);
        let area = wb.sheets[0].used_range().unwrap();
        let setup = PageSetup::default();
        let pages = layout(&wb, &[SheetJob { sheet: 0, area }], &setup, false);
        // 15pt rows, 684pt of room on Letter portrait: 45 rows per page
        assert_eq!(pages.len(), 3);
        assert!(pages[0].shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text == "Row 1")));
        let fit = PageSetup { scaling: Scaling::FitSheet, ..PageSetup::default() };
        assert_eq!(layout(&wb, &[SheetJob { sheet: 0, area }], &fit, false).len(), 1);
        let pdf = pdf_bytes(&pages, "Test");
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn paginates_columns_down_then_over() {
        let mut wb = Workbook::new();
        for c in 0..20 {
            wb.set_input(0, CellPos::new(0, c), "x").unwrap();
        }
        let area = wb.sheets[0].used_range().unwrap();
        let pages = layout(&wb, &[SheetJob { sheet: 0, area }], &PageSetup::default(), false);
        // 48pt columns, 511pt of room: 10 per page
        assert_eq!(pages.len(), 2);
        let land = PageSetup { landscape: true, ..PageSetup::default() };
        let pages = layout(&wb, &[SheetJob { sheet: 0, area }], &land, false);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].width, 792.0);
    }

    #[test]
    fn long_text_is_clipped_by_neighbour() {
        let mut wb = Workbook::new();
        wb.set_input(0, CellPos::new(0, 0), "A very long piece of text that overflows").unwrap();
        wb.set_input(0, CellPos::new(0, 1), "B").unwrap();
        let area = wb.sheets[0].used_range().unwrap();
        let pages = layout(&wb, &[SheetJob { sheet: 0, area }], &PageSetup::default(), false);
        let t = pages[0].shapes.iter().find_map(|s| match s { Shape::Text { text, .. } if text.starts_with('A') => Some(text.clone()), _ => None }).unwrap();
        assert!(t.len() < 20, "{}", t);
        assert_eq!(wrap_lines(FontDb::global(), FontDb::global().face_for("Calibri", false, false), 11.0, "one two three", 30.0).len(), 3);
    }
}
