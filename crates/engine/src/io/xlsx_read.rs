//! xlsx reader (zip + XML), preserving styles and sheet features.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::xml::{decode_excel_escapes, parse, El};
use crate::address::{CellPos, Rect};
use crate::ast::Expr;
use crate::parser::parse_formula;
use crate::style::{BorderKind, BorderLine, Borders, Color, HAlign, Style, VAlign, DEFAULT_FONT};
use crate::value::{ErrorKind, Value};
use crate::workbook::{
    AutoFilter, CfOperator, CfRule, CfStyle, Chart, ChartKind, ConditionalFormat, DataValidation, ValidationRule,
    Workbook, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT,
};

struct Pkg {
    zip: zip::ZipArchive<File>,
}

impl Pkg {
    fn read(&mut self, path: &str) -> Option<String> {
        let path = path.trim_start_matches('/');
        let names: Vec<String> = self.zip.file_names().filter_map(|s| s.ok().map(|s| s.to_string())).collect();
        // exact match first, then case-insensitive
        let name = names.iter().find(|n| *n == path).or_else(|| names.iter().find(|n| n.eq_ignore_ascii_case(path)))?;
        let mut f = self.zip.by_name(name).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        Some(match String::from_utf8(buf) {
            Ok(s) => s,
            Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        })
    }
    fn xml(&mut self, path: &str) -> Option<El> {
        let s = self.read(path)?;
        parse(&s).ok()
    }
    /// Relationships of a part: id -> (type suffix, resolved target, external).
    fn rels(&mut self, part: &str) -> HashMap<String, (String, String, bool)> {
        let (dir, file) = match part.rfind('/') {
            Some(i) => (&part[..i], &part[i + 1..]),
            None => ("", part),
        };
        let rels_path = if dir.is_empty() { format!("_rels/{}.rels", file) } else { format!("{}/_rels/{}.rels", dir, file) };
        let mut out = HashMap::new();
        if let Some(root) = self.xml(&rels_path) {
            for r in root.children_named("Relationship") {
                let id = r.attr("Id").unwrap_or("").to_string();
                let ty = r.attr("Type").unwrap_or("");
                let ty = ty.rsplit('/').next().unwrap_or("").to_string();
                let target = r.attr("Target").unwrap_or("");
                let external = r.attr("TargetMode").map(|m| m.eq_ignore_ascii_case("External")).unwrap_or(false);
                let resolved = if external { target.to_string() } else { resolve_path(dir, target) };
                out.insert(id, (ty, resolved, external));
            }
        }
        out
    }
}

fn resolve_path(dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = if dir.is_empty() { vec![] } else { dir.split('/').collect() };
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

// ---------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------

fn rgb_to_hls(c: Color) -> (f64, f64, f64) {
    let r = c.r() as f64 / 255.0;
    let g = c.g() as f64 / 255.0;
    let b = c.b() as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, l, 0.0);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, l, s)
}

fn hls_to_rgb(h: f64, l: f64, s: f64) -> Color {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return Color::rgb(v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color::rgb(f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

fn apply_tint(c: Color, tint: f64) -> Color {
    if tint == 0.0 {
        return c;
    }
    let (h, l, s) = rgb_to_hls(c);
    let l = if tint < 0.0 { l * (1.0 + tint) } else { l * (1.0 - tint) + tint };
    hls_to_rgb(h, l.clamp(0.0, 1.0), s)
}

struct Ctx {
    theme: Vec<Color>,
}

impl Ctx {
    /// Parse a CT_Color element. Returns None for automatic colors.
    fn color(&self, e: &El) -> Option<Color> {
        if e.attr_bool("auto", false) {
            return None;
        }
        let base = if let Some(rgb) = e.attr("rgb") {
            Color::from_hex(rgb)?
        } else if let Some(t) = e.attr_u32("theme") {
            *self.theme.get(t as usize)?
        } else if let Some(i) = e.attr_u32("indexed") {
            crate::numfmt::indexed_color(i)?
        } else {
            return None;
        };
        Some(apply_tint(base, e.attr_f64("tint").unwrap_or(0.0)))
    }
}

fn parse_theme(root: &El) -> Vec<Color> {
    let mut map: HashMap<String, Color> = HashMap::new();
    if let Some(scheme) = root.find("clrScheme") {
        for c in &scheme.children {
            let col = c
                .child("srgbClr")
                .and_then(|x| x.attr("val"))
                .or_else(|| c.child("sysClr").and_then(|x| x.attr("lastClr")))
                .and_then(Color::from_hex);
            if let Some(col) = col {
                map.insert(c.name.clone(), col);
            }
        }
    }
    let order = ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];
    let defaults = [0xFFFFFF, 0x000000, 0xE7E6E6, 0x44546A, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47, 0x0563C1, 0x954F72];
    order.iter().zip(defaults).map(|(n, d)| map.get(*n).copied().unwrap_or(Color(d))).collect()
}

fn default_theme() -> Vec<Color> {
    parse_theme(&El::default())
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct FontInfo {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    size: Option<f64>,
    name: Option<String>,
    color: Option<Color>,
}

struct StyleSheet {
    /// cellXfs index -> Style
    xfs: Vec<Style>,
    dxfs: Vec<CfStyle>,
}

fn border_kind(s: &str) -> Option<BorderKind> {
    Some(match s {
        "thin" => BorderKind::Thin,
        "medium" => BorderKind::Medium,
        "thick" => BorderKind::Thick,
        "double" => BorderKind::Double,
        "dotted" | "hair" => BorderKind::Dotted,
        "dashed" | "mediumDashed" | "dashDot" | "mediumDashDot" | "dashDotDot" | "mediumDashDotDot" | "slantDashDot" => {
            BorderKind::Dashed
        }
        _ => return None,
    })
}

fn parse_font(ctx: &Ctx, f: &El) -> FontInfo {
    let flag = |n: &str| f.child(n).map(|e| e.attr_bool("val", true)).unwrap_or(false);
    FontInfo {
        bold: flag("b"),
        italic: flag("i"),
        underline: f.child("u").map(|u| u.attr("val").map(|v| v != "none").unwrap_or(true)).unwrap_or(false),
        strike: flag("strike"),
        size: f.child("sz").and_then(|e| e.attr_f64("val")),
        name: f.child("name").and_then(|e| e.attr("val")).map(|s| s.to_string()),
        color: f.child("color").and_then(|c| ctx.color(c)),
    }
}

fn fill_color(ctx: &Ctx, fill: &El, dxf: bool) -> Option<Color> {
    let pf = fill.child("patternFill")?;
    let pt = pf.attr("patternType").unwrap_or(if dxf { "solid" } else { "none" });
    if pt == "none" {
        return None;
    }
    let fg = pf.child("fgColor").and_then(|c| ctx.color(c));
    let bg = pf.child("bgColor").and_then(|c| ctx.color(c));
    if dxf {
        bg.or(fg)
    } else if pt == "gray125" || pt == "gray0625" {
        fg.or(Some(Color(0xD9D9D9)))
    } else {
        fg.or(bg)
    }
}

fn parse_styles(ctx: &Ctx, root: Option<&El>) -> StyleSheet {
    let mut sheet = StyleSheet { xfs: vec![Style::default()], dxfs: vec![] };
    let Some(root) = root else { return sheet };
    let mut numfmts: HashMap<u32, String> = HashMap::new();
    if let Some(nf) = root.child("numFmts") {
        for n in nf.children_named("numFmt") {
            if let (Some(id), Some(code)) = (n.attr_u32("numFmtId"), n.attr("formatCode")) {
                numfmts.insert(id, code.to_string());
            }
        }
    }
    let fonts: Vec<FontInfo> =
        root.child("fonts").map(|f| f.children_named("font").map(|x| parse_font(ctx, x)).collect()).unwrap_or_default();
    let default_font_name = fonts.first().and_then(|f| f.name.clone()).unwrap_or_else(|| DEFAULT_FONT.to_string());
    let fills: Vec<Option<Color>> =
        root.child("fills").map(|f| f.children_named("fill").map(|x| fill_color(ctx, x, false)).collect()).unwrap_or_default();
    let borders: Vec<Borders> = root
        .child("borders")
        .map(|b| {
            b.children_named("border")
                .map(|x| {
                    let side = |n: &str| -> Option<BorderLine> {
                        let e = x.child(n).or_else(|| match n {
                            "left" => x.child("start"),
                            "right" => x.child("end"),
                            _ => None,
                        })?;
                        let kind = border_kind(e.attr("style")?)?;
                        let color = e.child("color").and_then(|c| ctx.color(c)).unwrap_or(Color::BLACK);
                        Some(BorderLine { kind, color })
                    };
                    Borders { left: side("left"), right: side("right"), top: side("top"), bottom: side("bottom") }
                })
                .collect()
        })
        .unwrap_or_default();
    let num_fmt_code = |id: u32| -> String {
        numfmts
            .get(&id)
            .cloned()
            .or_else(|| crate::numfmt::builtin_format_code(id).map(|s| s.to_string()))
            .unwrap_or_else(|| match id {
                27..=36 | 50..=58 => "m/d/yyyy".to_string(),
                _ => "General".to_string(),
            })
    };
    if let Some(cx) = root.child("cellXfs") {
        sheet.xfs.clear();
        for xf in cx.children_named("xf") {
            let mut s = Style::default();
            let font = fonts.get(xf.attr_u32("fontId").unwrap_or(0) as usize).cloned().unwrap_or_default();
            s.bold = font.bold;
            s.italic = font.italic;
            s.underline = font.underline;
            s.strike = font.strike;
            if let Some(sz) = font.size {
                s.set_font_size(sz as f32);
            }
            s.font_name = match font.name {
                Some(n) if n != default_font_name && n != DEFAULT_FONT => Some(n),
                _ => None,
            };
            s.font_color = font.color.filter(|c| *c != Color::BLACK);
            s.fill = fills.get(xf.attr_u32("fillId").unwrap_or(0) as usize).copied().flatten();
            s.borders = borders.get(xf.attr_u32("borderId").unwrap_or(0) as usize).copied().unwrap_or_default();
            s.num_fmt = num_fmt_code(xf.attr_u32("numFmtId").unwrap_or(0));
            if let Some(al) = xf.child("alignment") {
                s.h_align = match al.attr("horizontal").unwrap_or("general") {
                    "left" => HAlign::Left,
                    "center" => HAlign::Center,
                    "right" => HAlign::Right,
                    "fill" => HAlign::Fill,
                    "justify" | "distributed" => HAlign::Justify,
                    "centerContinuous" => HAlign::CenterAcross,
                    _ => HAlign::General,
                };
                s.v_align = match al.attr("vertical").unwrap_or("bottom") {
                    "top" => VAlign::Top,
                    "center" | "justify" | "distributed" => VAlign::Center,
                    _ => VAlign::Bottom,
                };
                s.wrap = al.attr_bool("wrapText", false);
                s.indent = al.attr_u32("indent").unwrap_or(0).min(250) as u8;
                let rot = al.attr_u32("textRotation").unwrap_or(0) as i32;
                s.rotation = match rot {
                    0..=90 => rot as i16,
                    91..=180 => -((rot - 90) as i16),
                    255 => 90,
                    _ => 0,
                };
            }
            if let Some(p) = xf.child("protection") {
                s.locked = p.attr_bool("locked", true);
            }
            sheet.xfs.push(s);
        }
        if sheet.xfs.is_empty() {
            sheet.xfs.push(Style::default());
        }
    }
    if let Some(dx) = root.child("dxfs") {
        for d in dx.children_named("dxf") {
            let mut cs = CfStyle::default();
            if let Some(f) = d.child("font") {
                let fi = parse_font(ctx, f);
                cs.bold = fi.bold;
                cs.italic = fi.italic;
                cs.font_color = fi.color;
            }
            if let Some(f) = d.child("fill") {
                cs.fill = fill_color(ctx, f, true);
            }
            sheet.dxfs.push(cs);
        }
    }
    sheet
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Excel column width (characters) to pixels.
pub fn width_chars_to_px(w: f64) -> f32 {
    if w <= 0.0 {
        0.0
    } else if w < 1.0 {
        (w * 12.0 + 0.5).floor() as f32
    } else {
        (w * 7.0 + 0.5).floor() as f32
    }
}

fn strip_sheet(r: &str) -> &str {
    match r.rfind('!') {
        Some(i) => &r[i + 1..],
        None => r,
    }
}

fn parse_rect(r: &str) -> Option<Rect> {
    let r = strip_sheet(r.trim().trim_start_matches('='));
    Rect::parse(&r.replace('$', ""))
}

fn sqref_rects(s: &str) -> Vec<Rect> {
    s.split_whitespace().filter_map(parse_rect).collect()
}

fn clean_formula(f: &str) -> String {
    f.replace("_xlpm.", "")
}

fn parse_expr(f: &str) -> Option<Expr> {
    parse_formula(&clean_formula(f)).ok()
}

fn number_text(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|f| f.is_finite())
}

// ---------------------------------------------------------------------------
// Workbook
// ---------------------------------------------------------------------------

struct SheetRef {
    name: String,
    hidden: bool,
    path: String,
}

pub fn read_xlsx(path: &Path) -> Result<Workbook, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open {}: {}", path.display(), e))?;
    let zip = zip::ZipArchive::new(file).map_err(|e| format!("Not a valid xlsx file: {}", e))?;
    let mut pkg = Pkg { zip };

    // Locate the workbook part.
    let root_rels = pkg.rels("");
    let wb_path = root_rels
        .values()
        .find(|(t, _, _)| t == "officeDocument")
        .map(|(_, p, _)| p.clone())
        .unwrap_or_else(|| "xl/workbook.xml".to_string());
    let wb_xml = pkg.xml(&wb_path).ok_or("Missing workbook.xml")?;
    let wb_rels = pkg.rels(&wb_path);

    let theme = wb_rels
        .values()
        .find(|(t, _, _)| t == "theme")
        .map(|(_, p, _)| p.clone())
        .and_then(|p| pkg.xml(&p))
        .map(|t| parse_theme(&t))
        .unwrap_or_else(default_theme);
    let ctx = Ctx { theme };

    let styles_xml = wb_rels.values().find(|(t, _, _)| t == "styles").map(|(_, p, _)| p.clone()).and_then(|p| pkg.xml(&p));
    let styles = parse_styles(&ctx, styles_xml.as_ref());

    let shared: Vec<String> = wb_rels
        .values()
        .find(|(t, _, _)| t == "sharedStrings")
        .map(|(_, p, _)| p.clone())
        .and_then(|p| pkg.xml(&p))
        .map(|sst| sst.children_named("si").map(|si| decode_excel_escapes(&si.rich_text())).collect())
        .unwrap_or_default();

    let date1904 = wb_xml.child("workbookPr").map(|p| p.attr_bool("date1904", false)).unwrap_or(false);

    let mut sheets: Vec<SheetRef> = Vec::new();
    if let Some(ss) = wb_xml.child("sheets") {
        for s in ss.children_named("sheet") {
            let name = s.attr("name").unwrap_or("Sheet").to_string();
            let hidden = matches!(s.attr("state"), Some("hidden") | Some("veryHidden"));
            let rid = s.attr("id").unwrap_or("");
            let Some((ty, p, _)) = wb_rels.get(rid) else { continue };
            if ty != "worksheet" {
                // chartsheets / dialog sheets are not supported: keep an empty sheet
                sheets.push(SheetRef { name, hidden, path: String::new() });
                continue;
            }
            sheets.push(SheetRef { name, hidden, path: p.clone() });
        }
    }
    if sheets.is_empty() {
        return Err("Workbook has no sheets".into());
    }

    let mut wb = Workbook::empty();
    // Map xf index -> style id.
    let xf_ids: Vec<u32> = styles.xfs.iter().map(|s| wb.intern_style(s.clone())).collect();

    for sr in &sheets {
        let idx = wb.add_sheet(Some(&sr.name));
        wb.sheets[idx].hidden = sr.hidden;
        if sr.path.is_empty() {
            continue;
        }
        let Some(xml) = pkg.xml(&sr.path) else { continue };
        let rels = pkg.rels(&sr.path);
        read_sheet(&mut wb, idx, &xml, &rels, &shared, &xf_ids, &styles, &ctx, date1904);
        // comments
        for (ty, p, _) in rels.values() {
            if ty == "comments" {
                if let Some(c) = pkg.xml(p) {
                    read_comments(&mut wb, idx, &c);
                }
            }
        }
        // drawings -> charts
        let drawing_paths: Vec<String> = rels.values().filter(|(t, _, _)| t == "drawing").map(|(_, p, _)| p.clone()).collect();
        for dp in drawing_paths {
            read_drawing(&mut pkg, &mut wb, idx, &dp);
        }
    }

    // Defined names
    if let Some(dn) = wb_xml.child("definedNames") {
        for n in dn.children_named("definedName") {
            let Some(name) = n.attr("name") else { continue };
            if name.starts_with("_xlnm.") || name.starts_with("_xl") {
                continue;
            }
            let scope = n.attr_u32("localSheetId").map(|i| i as usize).filter(|i| *i < wb.sheets.len());
            let _ = wb.define_name(name, scope, &clean_formula(n.text.trim()));
        }
    }
    if wb.sheets.is_empty() {
        wb.add_sheet(None);
    }
    Ok(wb)
}

#[allow(clippy::too_many_arguments)]
fn read_sheet(
    wb: &mut Workbook,
    idx: usize,
    xml: &El,
    rels: &HashMap<String, (String, String, bool)>,
    shared: &[String],
    xf_ids: &[u32],
    styles: &StyleSheet,
    ctx: &Ctx,
    date1904: bool,
) {
    // sheet properties
    if let Some(pr) = xml.child("sheetPr") {
        if let Some(tc) = pr.child("tabColor") {
            wb.sheets[idx].tab_color = ctx.color(tc);
        }
    }
    if let Some(views) = xml.child("sheetViews") {
        if let Some(v) = views.child("sheetView") {
            let s = &mut wb.sheets[idx];
            s.show_gridlines = v.attr_bool("showGridLines", true);
            if let Some(z) = v.attr_f64("zoomScale") {
                if z > 0.0 {
                    s.zoom = (z / 100.0) as f32;
                }
            }
            if let Some(p) = v.child("pane") {
                if matches!(p.attr("state"), Some("frozen") | Some("frozenSplit")) {
                    s.freeze_cols = p.attr_f64("xSplit").unwrap_or(0.0) as u32;
                    s.freeze_rows = p.attr_f64("ySplit").unwrap_or(0.0) as u32;
                }
            }
        }
    }
    if let Some(cols) = xml.child("cols") {
        for c in cols.children_named("col") {
            let min = c.attr_u32("min").unwrap_or(1).max(1) - 1;
            let max = (c.attr_u32("max").unwrap_or(min + 1).max(1) - 1).min(crate::address::MAX_COLS - 1);
            let hidden = c.attr_bool("hidden", false);
            let width = c.attr_f64("width").map(width_chars_to_px);
            for col in min..=max {
                let s = &mut wb.sheets[idx];
                if hidden {
                    s.hidden_cols.insert(col);
                }
                if let Some(w) = width {
                    if w > 0.0 && (w - DEFAULT_COL_WIDTH).abs() >= 0.5 {
                        s.col_widths.insert(col, w);
                    }
                }
            }
        }
    }

    let mut array_ranges: Vec<(CellPos, Rect)> = Vec::new();
    // shared formula index -> (anchor, expr)
    let mut shared_f: HashMap<String, (CellPos, Expr)> = HashMap::new();

    if let Some(data) = xml.child("sheetData") {
        let mut next_row: u32 = 0;
        for row in data.children_named("row") {
            let r = row.attr_u32("r").map(|r| r.max(1) - 1).unwrap_or(next_row);
            next_row = r + 1;
            {
                let s = &mut wb.sheets[idx];
                if row.attr_bool("hidden", false) {
                    s.hidden_rows.insert(r);
                }
                if let Some(ht) = row.attr_f64("ht") {
                    let px = (ht * 4.0 / 3.0).round() as f32;
                    if px > 0.0 && (px - DEFAULT_ROW_HEIGHT).abs() >= 0.5 {
                        s.row_heights.insert(r, px);
                    }
                }
            }
            let mut next_col: u32 = 0;
            for c in row.children_named("c") {
                let pos = c.attr("r").and_then(CellPos::parse_a1).unwrap_or(CellPos::new(r, next_col));
                next_col = pos.col + 1;
                let style = c.attr_u32("s").and_then(|s| xf_ids.get(s as usize).copied()).unwrap_or(0);
                let t = c.attr("t").unwrap_or("n");
                let v = c.child("v").map(|v| v.text.as_str());
                let value = match t {
                    "s" => v
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .and_then(|i| shared.get(i))
                        .map(|s| Value::Text(s.clone()))
                        .unwrap_or(Value::Empty),
                    "str" => v.map(|v| Value::Text(decode_excel_escapes(v))).unwrap_or(Value::Empty),
                    "inlineStr" => c.child("is").map(|is| Value::Text(decode_excel_escapes(&is.rich_text()))).unwrap_or(Value::Empty),
                    "b" => v.map(|v| Value::Bool(v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))).unwrap_or(Value::Empty),
                    "e" => v.map(|v| Value::Error(ErrorKind::parse(v.trim()).unwrap_or(ErrorKind::Value))).unwrap_or(Value::Empty),
                    "d" => v
                        .and_then(crate::datetime::parse_datetime_text)
                        .map(Value::Number)
                        .unwrap_or_else(|| v.map(|v| Value::Text(v.to_string())).unwrap_or(Value::Empty)),
                    _ => v
                        .and_then(number_text)
                        .map(|n| Value::Number(if date1904 && is_date_style(styles, c) { n + 1462.0 } else { n }))
                        .unwrap_or(Value::Empty),
                };
                let mut formula: Option<Expr> = None;
                if let Some(f) = c.child("f") {
                    let ft = f.attr("t").unwrap_or("normal");
                    let text = f.text.trim();
                    match ft {
                        "shared" => {
                            let si = f.attr("si").unwrap_or("").to_string();
                            if !text.is_empty() {
                                if let Some(e) = parse_expr(text) {
                                    shared_f.insert(si, (pos, e.clone()));
                                    formula = Some(e);
                                }
                            } else if let Some((anchor, e)) = shared_f.get(&si) {
                                let mut e = e.clone();
                                e.shift_relative(pos.row as i64 - anchor.row as i64, pos.col as i64 - anchor.col as i64);
                                formula = Some(e);
                            }
                        }
                        "array" => {
                            if let Some(rr) = f.attr("ref").and_then(parse_rect) {
                                array_ranges.push((pos, rr));
                            }
                            if !text.is_empty() {
                                formula = parse_expr(text);
                            }
                        }
                        "dataTable" => {}
                        _ => {
                            if !text.is_empty() {
                                formula = parse_expr(text);
                            }
                        }
                    }
                }
                if formula.is_none() && value.is_empty() && style == 0 {
                    continue;
                }
                let cell = wb.sheets[idx].cells.entry(pos).or_default();
                cell.style = style;
                cell.value = value;
                cell.formula = formula.map(Box::new);
            }
        }
    }
    // Clear cached spill values of array formulas (the engine recomputes them).
    {
        let s = &mut wb.sheets[idx];
        for (anchor, rr) in &array_ranges {
            if rr.cell_count() <= 1 || rr.cell_count() > 1_000_000 {
                continue;
            }
            for p in rr.iter() {
                if p == *anchor {
                    continue;
                }
                let remove = match s.cells.get_mut(&p) {
                    Some(c) if c.formula.is_none() => {
                        c.value = Value::Empty;
                        c.style == 0
                    }
                    _ => false,
                };
                if remove {
                    s.cells.remove(&p);
                }
            }
        }
    }

    if let Some(m) = xml.child("mergeCells") {
        for mc in m.children_named("mergeCell") {
            if let Some(r) = mc.attr("ref").and_then(parse_rect) {
                if r.cell_count() > 1 {
                    wb.sheets[idx].merges.push(r);
                }
            }
        }
    }

    if let Some(af) = xml.child("autoFilter") {
        if let Some(range) = af.attr("ref").and_then(parse_rect) {
            let mut allowed: HashMap<u32, HashSet<String>> = HashMap::new();
            for fc in af.children_named("filterColumn") {
                let col = fc.attr_u32("colId").unwrap_or(0);
                if let Some(filters) = fc.child("filters") {
                    let mut set: HashSet<String> = filters.children_named("filter").filter_map(|f| f.attr("val").map(|s| s.to_string())).collect();
                    if filters.attr_bool("blank", false) {
                        set.insert(String::new());
                    }
                    allowed.insert(col, set);
                }
            }
            wb.sheets[idx].autofilter = Some(AutoFilter { range, allowed });
        }
    }

    if let Some(hl) = xml.child("hyperlinks") {
        for h in hl.children_named("hyperlink") {
            let Some(r) = h.attr("ref").and_then(parse_rect) else { continue };
            let target = if let Some(id) = h.attr("id") {
                rels.get(id).map(|(_, t, _)| {
                    let mut t = t.clone();
                    if let Some(loc) = h.attr("location") {
                        t.push('#');
                        t.push_str(loc);
                    }
                    t
                })
            } else {
                h.attr("location").map(|l| format!("#{}", l))
            };
            if let Some(t) = target {
                wb.sheets[idx].hyperlinks.insert(r.top_left(), t);
            }
        }
    }

    // Conditional formats, in priority order (the file groups them by range).
    let mut cfs: Vec<(u32, ConditionalFormat)> = Vec::new();
    for cf in xml.children_named("conditionalFormatting") {
        let ranges = sqref_rects(cf.attr("sqref").unwrap_or(""));
        for rule in cf.children_named("cfRule") {
            if let Some(r) = parse_cf_rule(rule, styles, ctx) {
                let prio = rule.attr_u32("priority").unwrap_or(u32::MAX);
                for range in &ranges {
                    cfs.push((prio, ConditionalFormat { range: *range, rule: r.clone() }));
                }
            }
        }
    }
    cfs.sort_by_key(|(p, _)| *p);
    wb.sheets[idx].cond_formats.extend(cfs.into_iter().map(|(_, c)| c));

    if let Some(dvs) = xml.child("dataValidations") {
        for dv in dvs.children_named("dataValidation") {
            let ranges = sqref_rects(dv.attr("sqref").unwrap_or(""));
            if let Some(rule) = parse_validation(dv) {
                for range in ranges {
                    wb.sheets[idx].validations.push(DataValidation { range, rule: rule.clone() });
                }
            }
        }
    }
}

fn is_date_style(styles: &StyleSheet, c: &El) -> bool {
    c.attr_u32("s")
        .and_then(|s| styles.xfs.get(s as usize))
        .map(|s| crate::numfmt::is_date_format(&s.num_fmt))
        .unwrap_or(false)
}

fn parse_cf_rule(rule: &El, styles: &StyleSheet, ctx: &Ctx) -> Option<CfRule> {
    let style = rule.attr_u32("dxfId").and_then(|i| styles.dxfs.get(i as usize)).cloned().unwrap_or_default();
    let formulas: Vec<String> = rule.children_named("formula").map(|f| f.text.trim().to_string()).collect();
    let f = |i: usize| formulas.get(i).cloned().unwrap_or_default();
    Some(match rule.attr("type")? {
        "cellIs" => {
            let op = match rule.attr("operator").unwrap_or("equal") {
                "between" => CfOperator::Between,
                "notBetween" => CfOperator::NotBetween,
                "notEqual" => CfOperator::NotEqual,
                "greaterThan" => CfOperator::Greater,
                "lessThan" => CfOperator::Less,
                "greaterThanOrEqual" => CfOperator::GreaterOrEqual,
                "lessThanOrEqual" => CfOperator::LessOrEqual,
                _ => CfOperator::Equal,
            };
            CfRule::CellValue { op, a: f(0), b: f(1), style }
        }
        "expression" => CfRule::Expression { formula: f(0), style },
        "containsText" => CfRule::TextContains { text: rule.attr("text").unwrap_or("").to_string(), style },
        "colorScale" => {
            let cs = rule.child("colorScale")?;
            let colors: Vec<Color> = cs.children_named("color").filter_map(|c| ctx.color(c)).collect();
            match colors.len() {
                2 => CfRule::ColorScale { min: colors[0], mid: None, max: colors[1] },
                n if n >= 3 => CfRule::ColorScale { min: colors[0], mid: Some(colors[1]), max: colors[2] },
                _ => return None,
            }
        }
        "dataBar" => {
            let db = rule.child("dataBar")?;
            let color = db.child("color").and_then(|c| ctx.color(c)).unwrap_or(Color(0x638EC6));
            CfRule::DataBar { color }
        }
        "top10" => CfRule::TopBottom {
            top: !rule.attr_bool("bottom", false),
            rank: rule.attr_u32("rank").unwrap_or(10),
            percent: rule.attr_bool("percent", false),
            style,
        },
        "aboveAverage" => CfRule::AboveAverage { above: rule.attr_bool("aboveAverage", true), style },
        "duplicateValues" => CfRule::Duplicates { unique: false, style },
        "uniqueValues" => CfRule::Duplicates { unique: true, style },
        _ => return None,
    })
}

fn parse_validation(dv: &El) -> Option<ValidationRule> {
    let f1 = dv.child("formula1").map(|f| f.text.trim().to_string()).unwrap_or_default();
    let f2 = dv.child("formula2").map(|f| f.text.trim().to_string()).unwrap_or_default();
    let ty = dv.attr("type").unwrap_or("none");
    if ty == "list" {
        if f1.starts_with('"') {
            let inner = f1.trim_matches('"').replace("\"\"", "\"");
            return Some(ValidationRule::List(inner.split(',').map(|s| s.to_string()).collect()));
        }
        return parse_rect(&f1).map(ValidationRule::ListRange);
    }
    let a = number_text(&f1);
    let b = number_text(&f2);
    let (min, max) = match dv.attr("operator").unwrap_or("between") {
        "between" => (a?, b?),
        "greaterThan" | "greaterThanOrEqual" => (a?, f64::INFINITY),
        "lessThan" | "lessThanOrEqual" => (f64::NEG_INFINITY, a?),
        "equal" => (a?, a?),
        _ => return None,
    };
    Some(match ty {
        "whole" => ValidationRule::WholeNumber { min, max },
        "decimal" => ValidationRule::Decimal { min, max },
        "textLength" => ValidationRule::TextLength { min, max },
        _ => return None,
    })
}

fn read_comments(wb: &mut Workbook, idx: usize, root: &El) {
    let authors: Vec<String> = root.child("authors").map(|a| a.children_named("author").map(|x| x.text.clone()).collect()).unwrap_or_default();
    let Some(list) = root.child("commentList") else { return };
    for c in list.children_named("comment") {
        let Some(pos) = c.attr("ref").and_then(|r| CellPos::parse_a1(r.split(':').next().unwrap_or(r))) else { continue };
        let mut text = c.child("text").map(|t| t.rich_text()).unwrap_or_default();
        if let Some(a) = c.attr_u32("authorId").and_then(|i| authors.get(i as usize)) {
            let prefix = format!("{}:\n", a);
            if !a.is_empty() && text.starts_with(&prefix) {
                text = text[prefix.len()..].to_string();
            }
        }
        wb.sheets[idx].comments.insert(pos, decode_excel_escapes(&text));
    }
}

fn read_drawing(pkg: &mut Pkg, wb: &mut Workbook, idx: usize, path: &str) {
    let Some(root) = pkg.xml(path) else { return };
    let rels = pkg.rels(path);
    for anchor in &root.children {
        if !matches!(anchor.name.as_str(), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
            continue;
        }
        let Some(gf) = anchor.child("graphicFrame") else { continue };
        let Some(chart_el) = gf.find("chart") else { continue };
        let Some(rid) = chart_el.attr("id") else { continue };
        let Some((_, chart_path, _)) = rels.get(rid) else { continue };
        let Some(chart_xml) = pkg.xml(chart_path) else { continue };
        let from = anchor.child("from");
        let at = |e: Option<&El>, n: &str| e.and_then(|f| f.child(n)).and_then(|x| x.text.trim().parse::<u32>().ok()).unwrap_or(0);
        let pos = CellPos::new(at(from, "row"), at(from, "col"));
        // size in EMU (9525 per pixel)
        let ext = anchor.child("ext").or_else(|| gf.child("xfrm").and_then(|x| x.child("ext")));
        let (mut w, mut h) = ext
            .map(|e| (e.attr_f64("cx").unwrap_or(0.0) / 9525.0, e.attr_f64("cy").unwrap_or(0.0) / 9525.0))
            .unwrap_or((0.0, 0.0));
        if (w <= 0.0 || h <= 0.0) && anchor.name == "twoCellAnchor" {
            let to = anchor.child("to");
            let s = &wb.sheets[idx];
            let (c1, c2) = (at(from, "col"), at(to, "col"));
            let (r1, r2) = (at(from, "row"), at(to, "row"));
            let off = |e: Option<&El>, n: &str| e.and_then(|f| f.child(n)).and_then(|x| x.text.trim().parse::<f64>().ok()).unwrap_or(0.0) / 9525.0;
            w = (c1..c2).map(|c| s.col_width(c) as f64).sum::<f64>() - off(from, "colOff") + off(to, "colOff");
            h = (r1..r2).map(|r| s.row_height(r) as f64).sum::<f64>() - off(from, "rowOff") + off(to, "rowOff");
        }
        if w <= 0.0 {
            w = 480.0;
        }
        if h <= 0.0 {
            h = 288.0;
        }
        if let Some(chart) = parse_chart(&chart_xml, pos, w as f32, h as f32) {
            wb.sheets[idx].charts.push(chart);
        }
    }
}

fn parse_chart(root: &El, anchor: CellPos, width: f32, height: f32) -> Option<Chart> {
    let chart = root.child("chart")?;
    let plot = chart.child("plotArea")?;
    let mut kind = None;
    let mut body = None;
    for c in &plot.children {
        let k = match c.name.as_str() {
            "barChart" | "bar3DChart" => {
                if c.child("barDir").and_then(|b| b.attr("val")) == Some("bar") {
                    Some(ChartKind::Bar)
                } else {
                    Some(ChartKind::Column)
                }
            }
            "lineChart" | "line3DChart" | "stockChart" | "radarChart" => Some(ChartKind::Line),
            "areaChart" | "area3DChart" => Some(ChartKind::Area),
            "pieChart" | "pie3DChart" | "doughnutChart" | "ofPieChart" => Some(ChartKind::Pie),
            "scatterChart" | "bubbleChart" => Some(ChartKind::Scatter),
            _ => None,
        };
        if k.is_some() {
            kind = k;
            body = Some(c);
            break;
        }
    }
    let kind = kind?;
    let body = body?;
    let title = chart
        .child("title")
        .map(|t| {
            let mut ts = Vec::new();
            t.find_all("t", &mut ts);
            ts.iter().map(|x| x.text.as_str()).collect::<String>()
        })
        .unwrap_or_default();
    let mut rects: Vec<Rect> = Vec::new();
    let mut has_header = false;
    let mut vertical: Option<bool> = None;
    for ser in body.children_named("ser") {
        let get_ref = |n: &str| -> Option<Rect> {
            let e = ser.child(n)?;
            let f = e.find("f")?;
            parse_rect(f.text.trim().trim_start_matches('(').trim_end_matches(')'))
        };
        if let Some(r) = get_ref("tx") {
            has_header = true;
            rects.push(r);
        }
        for n in ["cat", "xVal"] {
            if let Some(r) = get_ref(n) {
                has_header = true;
                rects.push(r);
            }
        }
        for n in ["val", "yVal"] {
            if let Some(r) = get_ref(n) {
                if vertical.is_none() {
                    vertical = Some(r.cols() == 1 && (r.rows() > 1 || r.cols() == 1));
                }
                rects.push(r);
            }
        }
    }
    let first = *rects.first()?;
    let data = rects.iter().fold(first, |a, r| Rect {
        top: a.top.min(r.top),
        left: a.left.min(r.left),
        bottom: a.bottom.max(r.bottom),
        right: a.right.max(r.right),
    });
    Some(Chart {
        kind,
        title,
        data,
        has_header,
        series_in_columns: vertical.unwrap_or(true),
        anchor,
        width,
        height,
    })
}
