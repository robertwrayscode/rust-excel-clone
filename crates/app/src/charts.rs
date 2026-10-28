//! Embedded chart rendering (custom painted).

use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

use engine::address::CellPos;
use engine::workbook::{Chart, ChartKind};
use engine::{Value, Workbook};

use crate::theme::SERIES_COLORS;

pub struct ChartData {
    pub categories: Vec<String>,
    pub series: Vec<(String, Vec<Option<f64>>)>,
    /// For scatter charts: x values.
    pub xs: Vec<Option<f64>>,
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        _ => None,
    }
}

pub fn extract(wb: &Workbook, sheet: usize, c: &Chart) -> ChartData {
    let r = c.data;
    let bottom = wb.sheets[sheet].data_extent().map(|e| e.bottom.min(r.bottom)).unwrap_or(r.top).max(r.top);
    let right = wb.sheets[sheet].data_extent().map(|e| e.right.min(r.right)).unwrap_or(r.left).max(r.left);
    let r = engine::Rect { bottom: bottom.min(r.top + 2000), right: right.min(r.left + 200), ..r };
    let get = |row: u32, col: u32| wb.value(sheet, CellPos::new(row, col));
    let text = |row: u32, col: u32| crate::grid::display_text(wb, sheet, CellPos::new(row, col));
    // In "series in rows" mode, transpose the logical grid.
    let (nrows, ncols) = if c.series_in_columns { (r.rows(), r.cols()) } else { (r.cols(), r.rows()) };
    let at = |i: u32, j: u32| -> (u32, u32) { if c.series_in_columns { (r.top + i, r.left + j) } else { (r.top + j, r.left + i) } };
    let header_row = c.has_header && nrows > 1;
    // category column: first column if it's non-numeric (or if header and >1 col)
    let first_col_text = (if header_row { 1 } else { 0 }..nrows).any(|i| {
        let (a, b) = at(i, 0);
        matches!(get(a, b), Value::Text(_))
    });
    let has_cat = ncols > 1 && (first_col_text || (c.kind == ChartKind::Scatter));
    let data_start_row = if header_row { 1 } else { 0 };
    let data_start_col = if has_cat { 1 } else { 0 };
    let mut categories = Vec::new();
    let mut xs = Vec::new();
    for i in data_start_row..nrows {
        if has_cat {
            let (a, b) = at(i, 0);
            categories.push(text(a, b));
            xs.push(num(&get(a, b)));
        } else {
            categories.push((i - data_start_row + 1).to_string());
            xs.push(Some((i - data_start_row + 1) as f64));
        }
    }
    let mut series = Vec::new();
    for j in data_start_col..ncols {
        let name = if header_row {
            let (a, b) = at(0, j);
            text(a, b)
        } else {
            format!("Series{}", j - data_start_col + 1)
        };
        let vals = (data_start_row..nrows)
            .map(|i| {
                let (a, b) = at(i, j);
                num(&get(a, b))
            })
            .collect();
        series.push((name, vals));
    }
    ChartData { categories, series, xs }
}

/// "Nice" axis bounds and step.
fn nice_axis(min: f64, max: f64) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (min.min(0.0), max.max(0.0));
    if (hi - lo).abs() < 1e-12 {
        hi = lo + 1.0;
    }
    let raw = (hi - lo) / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    (lo, hi, step)
}

fn fmt_tick(v: f64, step: f64) -> String {
    if step >= 1.0 && v.abs() >= 1000.0 {
        let s = format!("{:.0}", v);
        // thousands separators
        let neg = s.starts_with('-');
        let digits: Vec<char> = s.trim_start_matches('-').chars().collect();
        let mut out = String::new();
        for (i, c) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(*c);
        }
        if neg {
            format!("-{}", out)
        } else {
            out
        }
    } else {
        let d = if step >= 1.0 { 0 } else { (-step.log10().floor()) as usize };
        format!("{:.*}", d, v)
    }
}

/// Paint a chart into `rect`.
/// Something charts can be drawn on: the screen (egui) or a printed page.
pub trait Canvas {
    fn rect_filled(&mut self, r: Rect, c: Color32);
    fn polyline(&mut self, pts: &[Pos2], width: f32, c: Color32);
    /// Fill a polygon that is star-shaped around its first point (pie slices, area quads).
    fn polygon(&mut self, pts: &[Pos2], c: Color32);
    fn circle(&mut self, center: Pos2, r: f32, c: Color32);
    fn text_size(&self, text: &str, size: f32) -> egui::Vec2;
    fn text(&mut self, pos: Pos2, align: Align2, text: &str, size: f32, c: Color32);
}

/// Drawing on screen.
pub struct ScreenCanvas<'a> {
    pub p: &'a Painter,
}

impl Canvas for ScreenCanvas<'_> {
    fn rect_filled(&mut self, r: Rect, c: Color32) {
        self.p.rect_filled(r, 0.0, c);
    }
    fn polyline(&mut self, pts: &[Pos2], width: f32, c: Color32) {
        self.p.add(Shape::line(pts.to_vec(), Stroke::new(width, c)));
    }
    fn polygon(&mut self, pts: &[Pos2], c: Color32) {
        if pts.len() < 3 {
            return;
        }
        let mut mesh = egui::Mesh::default();
        for q in pts {
            mesh.colored_vertex(*q, c);
        }
        for j in 1..pts.len() as u32 - 1 {
            mesh.add_triangle(0, j, j + 1);
        }
        self.p.add(Shape::mesh(mesh));
    }
    fn circle(&mut self, center: Pos2, r: f32, c: Color32) {
        self.p.circle_filled(center, r, c);
    }
    fn text_size(&self, text: &str, size: f32) -> egui::Vec2 {
        self.p.layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::BLACK).size()
    }
    fn text(&mut self, pos: Pos2, align: Align2, text: &str, size: f32, c: Color32) {
        self.p.text(pos, align, text, FontId::proportional(size), c);
    }
}

/// Draw a chart on screen.
pub fn paint(p: &Painter, rect: Rect, chart: &Chart, data: &ChartData, selected: bool) {
    p.rect(rect, 0.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(if selected { 120 } else { 200 })), StrokeKind::Inside);
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    draw(&mut ScreenCanvas { p: &p }, rect, chart, data);
}

/// Draw a chart's contents (without its frame).
pub fn draw(p: &mut dyn Canvas, rect: Rect, chart: &Chart, data: &ChartData) {
    let text_c = Color32::from_gray(89);
    let title_font = 15.0;
    let small = 11.0;
    let mut top = rect.top() + 8.0;
    if !chart.title.is_empty() {
        p.text(pos2(rect.center().x, top), Align2::CENTER_TOP, &chart.title, title_font, text_c);
        top += 26.0;
    }
    let pie = chart.kind == ChartKind::Pie;
    // legend at the bottom
    let legend_items: Vec<(String, Color32)> = if pie {
        data.categories.iter().enumerate().map(|(i, c)| (c.clone(), SERIES_COLORS[i % SERIES_COLORS.len()])).collect()
    } else {
        data.series.iter().enumerate().map(|(i, s)| (s.0.clone(), SERIES_COLORS[i % SERIES_COLORS.len()])).collect()
    };
    let legend_h = if legend_items.is_empty() { 0.0 } else { 22.0 };
    if !legend_items.is_empty() {
        let widths: Vec<f32> = legend_items.iter().map(|(t, _)| p.text_size(t, small).x + 22.0).collect();
        let total: f32 = widths.iter().sum();
        let mut x = rect.center().x - total / 2.0;
        let y = rect.bottom() - 14.0;
        for ((t, c), w) in legend_items.iter().zip(widths) {
            p.rect_filled(Rect::from_min_size(pos2(x, y - 4.0), vec2(8.0, 8.0)), *c);
            p.text(pos2(x + 12.0, y), Align2::LEFT_CENTER, t, small, text_c);
            x += w;
        }
    }
    let plot_outer = Rect::from_min_max(pos2(rect.left() + 8.0, top), pos2(rect.right() - 12.0, rect.bottom() - 8.0 - legend_h));
    if plot_outer.height() < 20.0 || plot_outer.width() < 20.0 {
        return;
    }
    if pie {
        paint_pie(p, plot_outer, data);
        return;
    }
    let all: Vec<f64> = data.series.iter().flat_map(|s| s.1.iter().flatten().copied()).collect();
    if all.is_empty() {
        p.text(plot_outer.center(), Align2::CENTER_CENTER, "No numeric data", small, text_c);
        return;
    }
    let vmin = all.iter().cloned().fold(f64::INFINITY, f64::min);
    let vmax = all.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi, step) = nice_axis(vmin, vmax);
    let horizontal = chart.kind == ChartKind::Bar;
    let scatter = chart.kind == ChartKind::Scatter;
    // label widths
    let tick_labels: Vec<(f64, String)> = {
        let mut v = Vec::new();
        let mut t = lo;
        let mut guard = 0;
        while t <= hi + step * 0.5 && guard < 50 {
            v.push((t, fmt_tick(t, step)));
            t += step;
            guard += 1;
        }
        v
    };
    let label_w = tick_labels.iter().map(|(_, s)| p.text_size(s, small).x).fold(0.0, f32::max);
    let cat_label_w = data.categories.iter().map(|s| p.text_size(s, small).x).fold(0.0, f32::max).min(120.0);
    let plot = if horizontal {
        Rect::from_min_max(pos2(plot_outer.left() + cat_label_w + 8.0, plot_outer.top()), pos2(plot_outer.right(), plot_outer.bottom() - 18.0))
    } else {
        Rect::from_min_max(pos2(plot_outer.left() + label_w + 8.0, plot_outer.top()), pos2(plot_outer.right(), plot_outer.bottom() - 18.0))
    };
    let grid_c = Color32::from_gray(217);
    let vmap = |v: f64| -> f32 {
        let t = ((v - lo) / (hi - lo)) as f32;
        if horizontal {
            plot.left() + t * plot.width()
        } else {
            plot.bottom() - t * plot.height()
        }
    };
    for (v, s) in &tick_labels {
        let c = vmap(*v);
        if horizontal {
            p.polyline(&[pos2(c, plot.top()), pos2(c, plot.bottom())], 1.0, grid_c);
            p.text(pos2(c, plot.bottom() + 3.0), Align2::CENTER_TOP, s, small, text_c);
        } else {
            p.polyline(&[pos2(plot.left(), c), pos2(plot.right(), c)], 1.0, grid_c);
            p.text(pos2(plot.left() - 4.0, c), Align2::RIGHT_CENTER, s, small, text_c);
        }
    }
    let n = data.categories.len().max(1);
    if scatter {
        let xs: Vec<f64> = data.xs.iter().flatten().copied().collect();
        let (xlo, xhi, xstep) = if xs.is_empty() { (0.0, 1.0, 0.2) } else { nice_axis(xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)) };
        let xmap = |x: f64| plot.left() + ((x - xlo) / (xhi - xlo)) as f32 * plot.width();
        let mut t = xlo;
        let mut guard = 0;
        while t <= xhi + xstep * 0.5 && guard < 50 {
            p.text(pos2(xmap(t), plot.bottom() + 3.0), Align2::CENTER_TOP, &fmt_tick(t, xstep), small, text_c);
            t += xstep;
            guard += 1;
        }
        for (si, (_, vals)) in data.series.iter().enumerate() {
            let col = SERIES_COLORS[si % SERIES_COLORS.len()];
            for (i, v) in vals.iter().enumerate() {
                if let (Some(y), Some(Some(x))) = (v, data.xs.get(i)) {
                    p.circle(pos2(xmap(*x), vmap(*y)), 3.5, col);
                }
            }
        }
        p.polyline(&[plot.left_bottom(), plot.right_bottom()], 1.0, Color32::from_gray(190));
        return;
    }
    // category labels
    let band = if horizontal { plot.height() / n as f32 } else { plot.width() / n as f32 };
    let max_labels = ((if horizontal { plot.height() } else { plot.width() }) / 14.0).max(1.0) as usize;
    let every = n.div_ceil(max_labels).max(1);
    for (i, c) in data.categories.iter().enumerate() {
        if i % every != 0 {
            continue;
        }
        let center = if horizontal { plot.top() + band * (i as f32 + 0.5) } else { plot.left() + band * (i as f32 + 0.5) };
        if horizontal {
            p.text(pos2(plot.left() - 4.0, center), Align2::RIGHT_CENTER, c, small, text_c);
        } else {
            p.text(pos2(center, plot.bottom() + 3.0), Align2::CENTER_TOP, c, small, text_c);
        }
    }
    let zero = vmap(0.0f64.clamp(lo, hi));
    let ns = data.series.len().max(1);
    match chart.kind {
        ChartKind::Column | ChartKind::Bar => {
            let group = band * 0.7;
            let bw = group / ns as f32;
            for (si, (_, vals)) in data.series.iter().enumerate() {
                let col = SERIES_COLORS[si % SERIES_COLORS.len()];
                for (i, v) in vals.iter().enumerate() {
                    let Some(v) = v else { continue };
                    let start = band * i as f32 + band * 0.15 + bw * si as f32;
                    let c = vmap(*v);
                    let r = if horizontal {
                        Rect::from_min_max(pos2(zero.min(c), plot.top() + start), pos2(zero.max(c), plot.top() + start + bw * 0.95))
                    } else {
                        Rect::from_min_max(pos2(plot.left() + start, zero.min(c)), pos2(plot.left() + start + bw * 0.95, zero.max(c)))
                    };
                    p.rect_filled(r, col);
                }
            }
        }
        ChartKind::Line | ChartKind::Area => {
            for (si, (_, vals)) in data.series.iter().enumerate().rev() {
                let col = SERIES_COLORS[si % SERIES_COLORS.len()];
                let pts: Vec<Pos2> = vals
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.map(|v| pos2(plot.left() + band * (i as f32 + 0.5), vmap(v))))
                    .collect();
                if chart.kind == ChartKind::Area && pts.len() >= 2 {
                    let fill = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 200);
                    for w in pts.windows(2) {
                        let quad = vec![w[0], w[1], pos2(w[1].x, zero), pos2(w[0].x, zero)];
                        p.polygon(&quad, fill);
                    }
                }
                if pts.len() >= 2 {
                    p.polyline(&pts, 2.25, col);
                }
                if chart.kind == ChartKind::Line {
                    for q in &pts {
                        p.circle(*q, 3.0, col);
                    }
                }
            }
        }
        _ => {}
    }
    // axis line
    if horizontal {
        p.polyline(&[pos2(zero, plot.top()), pos2(zero, plot.bottom())], 1.0, Color32::from_gray(190));
    } else {
        p.polyline(&[pos2(plot.left(), zero), pos2(plot.right(), zero)], 1.0, Color32::from_gray(190));
    }
}

fn paint_pie(p: &mut dyn Canvas, area: Rect, data: &ChartData) {
    let Some((_, vals)) = data.series.first() else { return };
    let total: f64 = vals.iter().flatten().filter(|v| **v > 0.0).sum();
    if total <= 0.0 {
        return;
    }
    let r = area.width().min(area.height()) / 2.0 - 4.0;
    let c = area.center();
    let mut a0 = -std::f32::consts::FRAC_PI_2;
    for (i, v) in vals.iter().enumerate() {
        let Some(v) = v else { continue };
        if *v <= 0.0 {
            continue;
        }
        let sweep = (*v / total) as f32 * std::f32::consts::TAU;
        let col = SERIES_COLORS[i % SERIES_COLORS.len()];
        // one triangle-fan mesh per slice (no seams), plus an anti-aliased rim
        let steps = ((sweep / std::f32::consts::TAU) * 96.0).ceil().max(2.0) as usize;
        let mut fan = vec![c];
        let mut rim = Vec::with_capacity(steps + 1);
        for j in 0..=steps {
            let a = a0 + sweep * j as f32 / steps as f32;
            let q = c + vec2(a.cos(), a.sin()) * r;
            fan.push(q);
            rim.push(q);
        }
        p.polygon(&fan, col);
        p.polyline(&rim, 1.0, col);
        // separator
        p.polyline(&[c, c + vec2(a0.cos(), a0.sin()) * r], 1.5, Color32::WHITE);
        // percentage label
        let mid = a0 + sweep / 2.0;
        if sweep > 0.25 {
            let lp = c + vec2(mid.cos(), mid.sin()) * r * 0.65;
            p.text(lp, Align2::CENTER_CENTER, &format!("{:.0}%", v / total * 100.0), 11.0, Color32::WHITE);
        }
        a0 += sweep;
    }
}
