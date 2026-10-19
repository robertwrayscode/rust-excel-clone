//! xlsx writer built on rust_xlsxwriter.

use std::collections::HashMap;
use std::path::Path;

use rust_xlsxwriter as xw;
use xw::{
    ChartType, ConditionalFormat2ColorScale, ConditionalFormat3ColorScale, ConditionalFormatAverage,
    ConditionalFormatAverageRule, ConditionalFormatCell, ConditionalFormatCellRule, ConditionalFormatDataBar,
    ConditionalFormatDuplicate, ConditionalFormatFormula, ConditionalFormatText, ConditionalFormatTextRule,
    ConditionalFormatTop, ConditionalFormatTopRule, DataValidationRule, FilterCondition, Format, FormatAlign,
    FormatBorder, FormatUnderline, Formula, Note, Url,
};

use crate::address::quote_sheet_name;
use crate::style::{BorderKind, BorderLine, Color, HAlign, Style, StyleId, VAlign, DEFAULT_FONT_SIZE};
use crate::value::{format_number_general, Value};
use crate::workbook::{CfOperator, CfRule, CfStyle, Chart, ChartKind, Sheet, ValidationRule, Workbook};

fn xc(c: Color) -> xw::Color {
    xw::Color::RGB(c.0 & 0xFFFFFF)
}

fn border(f: Format, line: Option<BorderLine>, set: fn(Format, FormatBorder) -> Format, setc: fn(Format, xw::Color) -> Format) -> Format {
    match line {
        None => f,
        Some(l) => {
            let b = match l.kind {
                BorderKind::Thin => FormatBorder::Thin,
                BorderKind::Medium => FormatBorder::Medium,
                BorderKind::Thick => FormatBorder::Thick,
                BorderKind::Dashed => FormatBorder::Dashed,
                BorderKind::Dotted => FormatBorder::Dotted,
                BorderKind::Double => FormatBorder::Double,
            };
            setc(set(f, b), xc(l.color))
        }
    }
}

/// Convert an engine style into a rust_xlsxwriter format.
fn to_format(s: &Style) -> Format {
    let mut f = Format::new();
    if s.bold {
        f = f.set_bold();
    }
    if s.italic {
        f = f.set_italic();
    }
    if s.underline {
        f = f.set_underline(FormatUnderline::Single);
    }
    if s.strike {
        f = f.set_font_strikethrough();
    }
    if (s.font_size() - DEFAULT_FONT_SIZE).abs() > 0.01 {
        f = f.set_font_size(s.font_size() as f64);
    }
    if let Some(n) = &s.font_name {
        f = f.set_font_name(n.as_str());
    }
    if let Some(c) = s.font_color {
        f = f.set_font_color(xc(c));
    }
    if let Some(c) = s.fill {
        f = f.set_pattern(xw::FormatPattern::Solid).set_background_color(xc(c));
    }
    f = match s.h_align {
        HAlign::General => f,
        HAlign::Left => f.set_align(FormatAlign::Left),
        HAlign::Center => f.set_align(FormatAlign::Center),
        HAlign::Right => f.set_align(FormatAlign::Right),
        HAlign::Fill => f.set_align(FormatAlign::Fill),
        HAlign::Justify => f.set_align(FormatAlign::Justify),
        HAlign::CenterAcross => f.set_align(FormatAlign::CenterAcross),
    };
    f = match s.v_align {
        VAlign::Bottom => f,
        VAlign::Top => f.set_align(FormatAlign::Top),
        VAlign::Center => f.set_align(FormatAlign::VerticalCenter),
    };
    if s.wrap {
        f = f.set_text_wrap();
    }
    if s.indent > 0 {
        f = f.set_indent(s.indent);
    }
    if s.rotation != 0 {
        f = f.set_rotation(s.rotation.clamp(-90, 90));
    }
    if !crate::numfmt::is_general(&s.num_fmt) {
        f = f.set_num_format(s.num_fmt.as_str());
    }
    f = border(f, s.borders.left, Format::set_border_left, |f, c| f.set_border_left_color(c));
    f = border(f, s.borders.right, Format::set_border_right, |f, c| f.set_border_right_color(c));
    f = border(f, s.borders.top, Format::set_border_top, |f, c| f.set_border_top_color(c));
    f = border(f, s.borders.bottom, Format::set_border_bottom, |f, c| f.set_border_bottom_color(c));
    if !s.locked {
        f = f.set_unlocked();
    }
    f
}

fn cf_format(s: &CfStyle) -> Format {
    let mut f = Format::new();
    if s.bold {
        f = f.set_bold();
    }
    if s.italic {
        f = f.set_italic();
    }
    if let Some(c) = s.font_color {
        f = f.set_font_color(xc(c));
    }
    if let Some(c) = s.fill {
        f = f.set_background_color(xc(c));
    }
    f
}

fn err(e: xw::XlsxError) -> String {
    format!("Error writing xlsx: {}", e)
}

/// Cached-result text for a formula cell.
fn result_text(v: &Value) -> String {
    match v {
        Value::Empty => "0".into(),
        Value::Number(n) => format!("{}", n),
        Value::Text(s) => s.clone(),
        Value::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        Value::Error(e) => e.as_str().into(),
        Value::Array(a) => a.data.first().map(result_text).unwrap_or_else(|| "0".into()),
    }
}

/// Make a formula number/string suitable for a CF/validation value.
fn cf_value(s: &str) -> Formula {
    Formula::new(s.trim().strip_prefix('=').unwrap_or(s.trim()))
}

pub fn write_xlsx(wb: &Workbook, path: &Path) -> Result<(), String> {
    let mut book = xw::Workbook::new();
    let mut formats: HashMap<StyleId, Format> = HashMap::new();
    let mut fmt = |id: StyleId| -> Format { formats.entry(id).or_insert_with(|| to_format(wb.style(id))).clone() };

    for (si, sheet) in wb.sheets.iter().enumerate() {
        let ws = book.add_worksheet();
        ws.set_name(sheet.name.as_str()).map_err(err)?;
        write_sheet(wb, si, sheet, ws, &mut fmt)?;
    }

    // Defined names
    for n in &wb.names {
        let formula = format!("={}", n.formula.to_formula());
        let name = match n.scope {
            Some(i) if i < wb.sheets.len() => format!("{}!{}", quote_sheet_name(&wb.sheets[i].name), n.name),
            _ => n.name.clone(),
        };
        // Names Excel/rust_xlsxwriter reject are skipped rather than failing the save.
        let _ = book.define_name(name, &formula);
    }

    book.save(path).map_err(err)
}

fn write_sheet(
    wb: &Workbook,
    _si: usize,
    sheet: &Sheet,
    ws: &mut xw::Worksheet,
    fmt: &mut impl FnMut(StyleId) -> Format,
) -> Result<(), String> {
    if sheet.hidden {
        ws.set_hidden(true);
    }
    if let Some(c) = sheet.tab_color {
        ws.set_tab_color(xc(c));
    }
    if !sheet.show_gridlines {
        ws.set_screen_gridlines(false);
    }
    if (sheet.zoom - 1.0).abs() > 0.001 {
        ws.set_zoom((sheet.zoom * 100.0).round().clamp(10.0, 400.0) as u16);
    }
    if sheet.freeze_rows > 0 || sheet.freeze_cols > 0 {
        ws.set_freeze_panes(sheet.freeze_rows, sheet.freeze_cols as u16).map_err(err)?;
    }
    // Column widths / hidden columns.
    let mut cols: Vec<_> = sheet.col_widths.iter().collect();
    cols.sort_by_key(|(c, _)| **c);
    for (c, w) in cols {
        if *c < crate::address::MAX_COLS {
            ws.set_column_width_pixels(*c as u16, w.round().max(0.0) as u32).map_err(err)?;
        }
    }
    let mut hc: Vec<_> = sheet.hidden_cols.iter().copied().collect();
    hc.sort();
    for c in hc {
        ws.set_column_hidden(c as u16).map_err(err)?;
    }
    let mut rows: Vec<_> = sheet.row_heights.iter().collect();
    rows.sort_by_key(|(r, _)| **r);
    for (r, h) in rows {
        ws.set_row_height_pixels(*r, h.round().max(0.0) as u32).map_err(err)?;
    }
    let mut hr: Vec<_> = sheet.hidden_rows.iter().copied().collect();
    hr.sort();
    for r in hr {
        ws.set_row_hidden(r).map_err(err)?;
    }

    // Merges first; the top-left cell is written (overwritten) below.
    for m in &sheet.merges {
        if m.cell_count() < 2 {
            continue;
        }
        let style = sheet.cells.get(&m.top_left()).map(|c| c.style).unwrap_or(0);
        ws.merge_range(m.top, m.left as u16, m.bottom, m.right as u16, "", &fmt(style)).map_err(err)?;
    }

    // Cells
    for (p, cell) in &sheet.cells {
        let (r, c) = (p.row, p.col as u16);
        let f = fmt(cell.style);
        if let Some(link) = sheet.hyperlinks.get(p) {
            if cell.formula.is_none() {
                let text = match &cell.value {
                    Value::Empty => link.clone(),
                    v => crate::numfmt::format_value(v, &wb.style(cell.style).num_fmt).text,
                };
                let target = if let Some(loc) = link.strip_prefix('#') { format!("internal:{}", loc) } else { link.clone() };
                let url = Url::new(target).set_text(text);
                if ws.write_url_with_format(r, c, url, &f).is_ok() {
                    continue;
                }
            }
        }
        if let Some(e) = &cell.formula {
            let formula = Formula::new(e.to_formula()).set_result(result_text(&cell.value));
            let spill = sheet.spill_areas.get(p).copied().filter(|a| a.cell_count() > 1);
            if let Some(a) = spill {
                ws.write_dynamic_array_formula_with_format(a.top, a.left as u16, a.bottom, a.right as u16, formula, &f)
                    .map_err(err)?;
            } else {
                ws.write_formula_with_format(r, c, formula, &f).map_err(err)?;
            }
            continue;
        }
        match &cell.value {
            Value::Empty => {
                if cell.style != 0 {
                    ws.write_blank(r, c, &f).map_err(err)?;
                }
            }
            Value::Number(n) => {
                ws.write_number_with_format(r, c, *n, &f).map_err(err)?;
            }
            Value::Text(s) => {
                ws.write_string_with_format(r, c, s.as_str(), &f).map_err(err)?;
            }
            Value::Bool(b) => {
                ws.write_boolean_with_format(r, c, *b, &f).map_err(err)?;
            }
            Value::Error(e) => {
                // Errors can only be stored as formula results.
                let formula = Formula::new(e.as_str()).set_result(e.as_str());
                ws.write_formula_with_format(r, c, formula, &f).map_err(err)?;
            }
            Value::Array(a) => {
                let v = a.data.first().cloned().unwrap_or(Value::Empty);
                ws.write_string_with_format(r, c, v.display().as_str(), &f).map_err(err)?;
            }
        }
    }
    // Hyperlinks on empty cells.
    for (p, link) in &sheet.hyperlinks {
        if sheet.cells.contains_key(p) {
            continue;
        }
        let target = if let Some(loc) = link.strip_prefix('#') { format!("internal:{}", loc) } else { link.clone() };
        let _ = ws.write_url(p.row, p.col as u16, Url::new(target));
    }

    // Comments / notes
    for (p, text) in &sheet.comments {
        let note = Note::new(text.as_str()).add_author_prefix(false);
        ws.insert_note(p.row, p.col as u16, &note).map_err(err)?;
    }

    // Autofilter
    if let Some(af) = &sheet.autofilter {
        let r = af.range;
        ws.autofilter(r.top, r.left as u16, r.bottom, r.right as u16).map_err(err)?;
        let mut cols: Vec<_> = af.allowed.iter().collect();
        cols.sort_by_key(|(c, _)| **c);
        for (off, set) in cols {
            let mut vals: Vec<&String> = set.iter().collect();
            vals.sort();
            let mut cond = FilterCondition::new();
            for v in vals {
                if v.is_empty() {
                    cond = cond.add_list_blanks_filter();
                } else {
                    cond = cond.add_list_filter(v.as_str());
                }
            }
            let _ = ws.filter_column(r.left as u16 + *off as u16, &cond);
        }
    }

    // Data validation
    for dv in &sheet.validations {
        let r = dv.range;
        let v = match &dv.rule {
            ValidationRule::List(items) => match xw::DataValidation::new().allow_list_strings(items) {
                Ok(v) => v,
                Err(_) => {
                    // too long for an inline list: fall back to a formula string
                    let joined = format!("\"{}\"", items.join(",").replace('"', "\"\""));
                    xw::DataValidation::new().allow_list_formula(Formula::new(joined))
                }
            },
            ValidationRule::ListRange(rr) => {
                let abs = format!("${}${}:${}${}", crate::address::col_to_letters(rr.left), rr.top + 1, crate::address::col_to_letters(rr.right), rr.bottom + 1);
                xw::DataValidation::new().allow_list_formula(Formula::new(abs))
            }
            ValidationRule::WholeNumber { min, max } => {
                xw::DataValidation::new().allow_whole_number_formula(dv_rule(*min, *max))
            }
            ValidationRule::Decimal { min, max } => xw::DataValidation::new().allow_decimal_number_formula(dv_rule(*min, *max)),
            ValidationRule::TextLength { min, max } => xw::DataValidation::new().allow_text_length_formula(dv_rule(*min, *max)),
        };
        ws.add_data_validation(r.top, r.left as u16, r.bottom, r.right as u16, &v).map_err(err)?;
    }

    // Conditional formats
    for cf in &sheet.cond_formats {
        let r = cf.range;
        let (r1, c1, r2, c2) = (r.top, r.left as u16, r.bottom, r.right as u16);
        let res = match &cf.rule {
            CfRule::CellValue { op, a, b, style } => {
                let (a, b) = (cf_value(a), cf_value(b));
                let rule = match op {
                    CfOperator::Between => ConditionalFormatCellRule::Between(a, b),
                    CfOperator::NotBetween => ConditionalFormatCellRule::NotBetween(a, b),
                    CfOperator::Equal => ConditionalFormatCellRule::EqualTo(a),
                    CfOperator::NotEqual => ConditionalFormatCellRule::NotEqualTo(a),
                    CfOperator::Greater => ConditionalFormatCellRule::GreaterThan(a),
                    CfOperator::Less => ConditionalFormatCellRule::LessThan(a),
                    CfOperator::GreaterOrEqual => ConditionalFormatCellRule::GreaterThanOrEqualTo(a),
                    CfOperator::LessOrEqual => ConditionalFormatCellRule::LessThanOrEqualTo(a),
                };
                ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatCell::new().set_rule(rule).set_format(cf_format(style)))
            }
            CfRule::Expression { formula, style } => ws.add_conditional_format(
                r1,
                c1,
                r2,
                c2,
                &ConditionalFormatFormula::new().set_rule(cf_value(formula)).set_format(cf_format(style)),
            ),
            CfRule::TextContains { text, style } => ws.add_conditional_format(
                r1,
                c1,
                r2,
                c2,
                &ConditionalFormatText::new()
                    .set_rule(ConditionalFormatTextRule::Contains(text.clone()))
                    .set_format(cf_format(style)),
            ),
            CfRule::ColorScale { min, mid, max } => match mid {
                Some(m) => ws.add_conditional_format(
                    r1,
                    c1,
                    r2,
                    c2,
                    &ConditionalFormat3ColorScale::new()
                        .set_minimum_color(xc(*min))
                        .set_midpoint_color(xc(*m))
                        .set_maximum_color(xc(*max)),
                ),
                None => ws.add_conditional_format(
                    r1,
                    c1,
                    r2,
                    c2,
                    &ConditionalFormat2ColorScale::new().set_minimum_color(xc(*min)).set_maximum_color(xc(*max)),
                ),
            },
            CfRule::DataBar { color } => {
                ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatDataBar::new().set_fill_color(xc(*color)))
            }
            CfRule::TopBottom { top, rank, percent, style } => {
                let n = (*rank).clamp(1, 1000) as u16;
                let rule = match (top, percent) {
                    (true, false) => ConditionalFormatTopRule::Top(n),
                    (true, true) => ConditionalFormatTopRule::TopPercent(n.min(100)),
                    (false, false) => ConditionalFormatTopRule::Bottom(n),
                    (false, true) => ConditionalFormatTopRule::BottomPercent(n.min(100)),
                };
                ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatTop::new().set_rule(rule).set_format(cf_format(style)))
            }
            CfRule::AboveAverage { above, style } => {
                let rule = if *above { ConditionalFormatAverageRule::AboveAverage } else { ConditionalFormatAverageRule::BelowAverage };
                ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatAverage::new().set_rule(rule).set_format(cf_format(style)))
            }
            CfRule::Duplicates { unique, style } => {
                let mut d = ConditionalFormatDuplicate::new().set_format(cf_format(style));
                if *unique {
                    d = d.invert();
                }
                ws.add_conditional_format(r1, c1, r2, c2, &d)
            }
        };
        res.map_err(err)?;
    }

    // Charts
    for ch in &sheet.charts {
        if let Some(chart) = build_chart(sheet, ch) {
            ws.insert_chart(ch.anchor.row, ch.anchor.col as u16, &chart).map_err(err)?;
        }
    }
    Ok(())
}

fn dv_rule(min: f64, max: f64) -> DataValidationRule<Formula> {
    let n = |x: f64| Formula::new(format_number_general(x));
    match (min.is_finite(), max.is_finite()) {
        (true, true) if min == max => DataValidationRule::EqualTo(n(min)),
        (true, true) => DataValidationRule::Between(n(min), n(max)),
        (true, false) => DataValidationRule::GreaterThanOrEqualTo(n(min)),
        (false, true) => DataValidationRule::LessThanOrEqualTo(n(max)),
        (false, false) => DataValidationRule::Between(n(-1e300), n(1e300)),
    }
}

fn build_chart(sheet: &Sheet, ch: &Chart) -> Option<xw::Chart> {
    let ty = match ch.kind {
        ChartKind::Column => ChartType::Column,
        ChartKind::Bar => ChartType::Bar,
        ChartKind::Line => ChartType::Line,
        ChartKind::Area => ChartType::Area,
        ChartKind::Pie => ChartType::Pie,
        ChartKind::Scatter => ChartType::Scatter,
    };
    let mut chart = xw::Chart::new(ty);
    let name = sheet.name.as_str();
    let d = ch.data;
    // Normalise to "series along columns" coordinates.
    let (series_count, has_cats) = if ch.series_in_columns {
        (d.cols(), ch.has_header && d.cols() > 1)
    } else {
        (d.rows(), ch.has_header && d.rows() > 1)
    };
    let first_series = if has_cats { 1 } else { 0 };
    if series_count <= first_series {
        return None;
    }
    let header = if ch.has_header { 1 } else { 0 };
    for k in first_series..series_count {
        let s = chart.add_series();
        if ch.series_in_columns {
            let col = (d.left + k) as u16;
            if d.top + header > d.bottom {
                continue;
            }
            s.set_values((name, d.top + header, col, d.bottom, col));
            if has_cats {
                s.set_categories((name, d.top + header, d.left as u16, d.bottom, d.left as u16));
            }
            if ch.has_header {
                s.set_name((name, d.top, col));
            }
        } else {
            let row = d.top + k;
            if d.left + header > d.right {
                continue;
            }
            s.set_values((name, row, (d.left + header) as u16, row, d.right as u16));
            if has_cats {
                s.set_categories((name, d.top, (d.left + header) as u16, d.top, d.right as u16));
            }
            if ch.has_header {
                s.set_name((name, row, d.left as u16));
            }
        }
    }
    if !ch.title.is_empty() {
        chart.title().set_name(ch.title.as_str());
    }
    chart.set_width(ch.width.round().max(50.0) as u32);
    chart.set_height(ch.height.round().max(50.0) as u32);
    Some(chart)
}

