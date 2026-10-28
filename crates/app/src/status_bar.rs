//! Status bar: mode, selection statistics, calc mode, zoom.

use engine::address::CellPos;
use engine::Value;

use crate::app::{App, EditMode};
use crate::theme;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal_centered(|ui| {
        let mode = match &app.edit {
            Some(e) if e.mode == EditMode::Point => "Point",
            Some(e) if e.mode == EditMode::Enter => "Enter",
            Some(_) => "Edit",
            None => "Ready",
        };
        ui.label(egui::RichText::new(mode).color(theme::TEXT));
        if let Some(s) = &app.status {
            ui.add_space(12.0);
            ui.label(egui::RichText::new(s).color(theme::MUTED));
        }
        if !app.doc.wb.circular.is_empty() {
            ui.add_space(12.0);
            let (si, p) = app.doc.wb.circular[0];
            let name = app.doc.wb.sheets.get(si).map(|s| s.name.clone()).unwrap_or_default();
            ui.label(egui::RichText::new(format!("Circular References: {}!{}", name, p.to_a1())).color(egui::Color32::from_rgb(0xC0, 0x30, 0x30)));
        }
        if app.doc.wb.manual_calc {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Calculate (Manual)").color(theme::MUTED));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // zoom controls
            let mut z = (app.sheet_ref().zoom * 100.0).round();
            ui.label(format!("{:.0}%", z));
            if ui.small_button("+").clicked() {
                z = (z + 10.0).min(400.0);
                app.set_zoom(z / 100.0);
            }
            let mut zz = z;
            if ui.add(egui::Slider::new(&mut zz, 10.0..=400.0).show_value(false).logarithmic(true)).changed() {
                app.set_zoom(zz / 100.0);
            }
            if ui.small_button("−").clicked() {
                z = (z - 10.0).max(10.0);
                app.set_zoom(z / 100.0);
            }
            ui.add_space(16.0);
            // selection statistics
            if let Some(stats) = selection_stats(app) {
                let (count, ncount, sum, min, max) = stats;
                // Statistics use the active cell's number format, like Excel.
                let code = app.active_style().num_fmt;
                let fmt = |n: f64| fmt_with(n, &code);
                if ncount > 0 {
                    ui.label(format!("Sum: {}", fmt(sum)));
                    if ncount > 1 {
                        ui.label(format!("Max: {}", fmt(max)));
                        ui.label(format!("Min: {}", fmt(min)));
                        ui.label(format!("Numerical Count: {}", ncount));
                    }
                }
                ui.label(format!("Count: {}", count));
                if ncount > 0 {
                    ui.label(format!("Average: {}", fmt(sum / ncount as f64)));
                }
            }
        });
    });
}

fn fmt_with(n: f64, code: &str) -> String {
    let n = (n * 1e10).round() / 1e10;
    if engine::numfmt::is_general(code) || code == "@" {
        engine::value::format_number_general(n)
    } else {
        engine::numfmt::format_number(n, code).text
    }
}

/// (count, numeric count, sum, min, max) of the selection, if it spans several cells.
fn selection_stats(app: &App) -> Option<(usize, usize, f64, f64, f64)> {
    if app.sel.is_single_cell() || app.edit.is_some() {
        return None;
    }
    let s = app.sheet_ref();
    let mut seen = std::collections::HashSet::new();
    let (mut count, mut ncount, mut sum, mut min, mut max) = (0usize, 0usize, 0.0, f64::INFINITY, f64::NEG_INFINITY);
    let mut visit = |p: CellPos, v: Value| {
        if !seen.insert(p) || v.is_empty() || s.hidden_rows.contains(&p.row) {
            return;
        }
        count += 1;
        if let Value::Number(n) = v {
            ncount += 1;
            sum += n;
            min = f64::min(min, n);
            max = f64::max(max, n);
        }
    };
    for r in &app.sel.ranges {
        for (p, c) in s.cells_in(*r) {
            if !c.is_blank() {
                visit(*p, c.display_value());
            }
        }
        for (p, _) in s.spill_owner.iter() {
            if r.contains(*p) {
                visit(*p, s.value_at(*p));
            }
        }
    }
    if count < 2 && ncount < 1 {
        return if count == 0 { None } else { Some((count, ncount, sum, min, max)) };
    }
    Some((count, ncount, sum, min, max))
}
