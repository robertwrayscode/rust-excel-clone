//! Charts on printed pages: shapes are drawn into a picture (tiny-skia) and
//! the text is added on top as real text, so it stays sharp.

use std::sync::Arc;

use egui::{pos2, Align2, Color32, Pos2, Rect};
use engine::workbook::Chart;
use engine::Workbook;
use wordcore::fonts::{FaceId, FontDb};
use wordcore::layout::{LineStyle, Shape};

/// Picture pixels per chart pixel.
const SUPERSAMPLE: f32 = 3.0;

struct PrintCanvas<'a> {
    db: &'a FontDb,
    face: FaceId,
    pix: tiny_skia::Pixmap,
    /// Text in chart pixels: (top-left, text, size, color).
    texts: Vec<(Pos2, String, f32, Color32)>,
}

fn paint(c: Color32) -> tiny_skia::Paint<'static> {
    let mut p = tiny_skia::Paint::default();
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    p.set_color_rgba8(r, g, b, a);
    p.anti_alias = true;
    p
}

fn path(pts: &[Pos2], close: bool) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    let first = pts.first()?;
    pb.move_to(first.x, first.y);
    for q in &pts[1..] {
        pb.line_to(q.x, q.y);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

impl PrintCanvas<'_> {
    fn transform() -> tiny_skia::Transform {
        tiny_skia::Transform::from_scale(SUPERSAMPLE, SUPERSAMPLE)
    }
    fn width(&self, text: &str, size: f32) -> f32 {
        let db = self.db;
        text.chars().map(|c| db.face(db.resolve_char(self.face, c)).advance(c)).sum::<f32>() * size
    }
}

impl crate::charts::Canvas for PrintCanvas<'_> {
    fn rect_filled(&mut self, r: Rect, c: Color32) {
        if let Some(rect) = tiny_skia::Rect::from_ltrb(r.left(), r.top(), r.right(), r.bottom()) {
            self.pix.fill_rect(rect, &paint(c), Self::transform(), None);
        }
    }
    fn polyline(&mut self, pts: &[Pos2], width: f32, c: Color32) {
        if let Some(p) = path(pts, false) {
            let stroke = tiny_skia::Stroke { width, line_join: tiny_skia::LineJoin::Round, line_cap: tiny_skia::LineCap::Butt, ..Default::default() };
            self.pix.stroke_path(&p, &paint(c), &stroke, Self::transform(), None);
        }
    }
    fn polygon(&mut self, pts: &[Pos2], c: Color32) {
        if let Some(p) = path(pts, true) {
            self.pix.fill_path(&p, &paint(c), tiny_skia::FillRule::Winding, Self::transform(), None);
        }
    }
    fn circle(&mut self, center: Pos2, r: f32, c: Color32) {
        if let Some(p) = tiny_skia::PathBuilder::from_circle(center.x, center.y, r) {
            self.pix.fill_path(&p, &paint(c), tiny_skia::FillRule::Winding, Self::transform(), None);
        }
    }
    fn text_size(&self, text: &str, size: f32) -> egui::Vec2 {
        egui::vec2(self.width(text, size), self.db.face(self.face).line_height() * size)
    }
    fn text(&mut self, pos: Pos2, align: Align2, text: &str, size: f32, c: Color32) {
        let sz = self.text_size(text, size);
        let r = align.anchor_size(pos, sz);
        self.texts.push((r.min, text.to_string(), size, c));
    }
}

/// A 24-bit bottom-up BMP (readable by the PDF writer and directly usable by Windows GDI).
pub fn encode_bmp(pix: &tiny_skia::Pixmap) -> Vec<u8> {
    let (w, h) = (pix.width() as usize, pix.height() as usize);
    let stride = (w * 3).div_ceil(4) * 4;
    let size = 54 + stride * h;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((stride * h) as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    let data = pix.data();
    for y in (0..h).rev() {
        let row = &data[y * w * 4..(y + 1) * w * 4];
        let start = out.len();
        for px in row.chunks_exact(4) {
            // premultiplied over white
            let a = px[3] as u16;
            let over = |c: u8| (c as u16 + (255 - a)) as u8;
            out.extend_from_slice(&[over(px[2]), over(px[1]), over(px[0])]);
        }
        out.resize(start + stride, 0);
    }
    out
}

/// Decode a BMP made by [`encode_bmp`] into RGBA for the preview.
pub fn decode_bmp(b: &[u8]) -> Option<([usize; 2], Vec<u8>)> {
    if b.len() < 54 || &b[0..2] != b"BM" {
        return None;
    }
    let w = i32::from_le_bytes(b[18..22].try_into().ok()?) as usize;
    let h = i32::from_le_bytes(b[22..26].try_into().ok()?) as usize;
    let stride = (w * 3).div_ceil(4) * 4;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = b.get(54 + (h - 1 - y) * stride..54 + (h - y) * stride)?;
        for x in 0..w {
            rgba.extend_from_slice(&[row[x * 3 + 2], row[x * 3 + 1], row[x * 3], 255]);
        }
    }
    Some(([w, h], rgba))
}

/// Shapes for a chart whose top-left corner is at (x, y) points, scaled by `s`
/// (chart pixels are 0.75 pt at 100%).
pub fn chart_shapes(wb: &Workbook, sheet: usize, chart: &Chart, x: f32, y: f32, s: f32) -> Vec<Shape> {
    let db = FontDb::global();
    let (w, h) = (chart.width.max(10.0), chart.height.max(10.0));
    let (pw, ph) = ((w * SUPERSAMPLE).ceil() as u32, (h * SUPERSAMPLE).ceil() as u32);
    let Some(mut pix) = tiny_skia::Pixmap::new(pw.min(6000), ph.min(6000)) else { return Vec::new() };
    pix.fill(tiny_skia::Color::WHITE);
    let mut canvas = PrintCanvas { db, face: db.face_for("Calibri", false, false), pix, texts: Vec::new() };
    let data = crate::charts::extract(wb, sheet, chart);
    let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(w, h));
    crate::charts::draw(&mut canvas, rect, chart, &data);
    let k = 0.75 * s;
    let image = wordcore::ImageData { bytes: encode_bmp(&canvas.pix), format: "bmp".into(), pixel_size: Some((canvas.pix.width(), canvas.pix.height())) };
    let mut shapes = vec![Shape::Image { x, y, w: w * k, h: h * k, image: Arc::new(image) }];
    let face = canvas.face;
    let ascent = db.face(face).ascent;
    for (top_left, text, size, c) in canvas.texts {
        shapes.push(Shape::Text {
            x: x + top_left.x * k,
            y: y + (top_left.y + ascent * size) * k,
            text,
            face,
            size: size * k,
            color: wordcore::Color { r: c.r(), g: c.g(), b: c.b() },
            spacing: 0.0,
            fake_bold: false,
            fake_italic: false,
            link: None,
        });
    }
    // frame
    let gray = wordcore::Color { r: 0xBF, g: 0xBF, b: 0xBF };
    let (x2, y2) = (x + w * k, y + h * k);
    for (a, b, c, d) in [(x, y, x2, y), (x2, y, x2, y2), (x2, y2, x, y2), (x, y2, x, y)] {
        shapes.push(Shape::Line { x1: a, y1: b, x2: c, y2: d, width: 0.5, color: gray, style: LineStyle::Solid });
    }
    shapes
}
