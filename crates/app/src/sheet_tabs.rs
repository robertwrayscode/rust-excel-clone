//! Sheet tabs at the bottom of the grid.

use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind};

use crate::app::App;
use crate::theme;

#[derive(Default)]
pub struct TabsState {
    pub first: usize,
    pub rename: Option<(usize, String, bool)>,
    pub drag_from: Option<usize>,
}

pub fn show(app: &mut App, ui: &mut egui::Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    let font = FontId::proportional(13.0);
    let mut x = rect.left() + 4.0;
    let y0 = rect.top() + 1.0;
    let h = rect.height() - 4.0;

    // navigation arrows
    for (i, label) in ["◀", "▶"].iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, y0 + 2.0), vec2(20.0, h - 2.0));
        let resp = ui.interact(r, egui::Id::unique(("rexcel-tabnav", i)), Sense::click());
        painter.text(r.center(), Align2::CENTER_CENTER, *label, FontId::proportional(10.0), if resp.hovered() { theme::ACCENT } else { Color32::from_gray(110) });
        if resp.clicked() {
            let n = app.doc.wb.sheets.len();
            if i == 0 {
                app.tabs.first = app.tabs.first.saturating_sub(1);
            } else if app.tabs.first + 1 < n {
                app.tabs.first += 1;
            }
        }
        x += 22.0;
    }
    x += 4.0;

    let n = app.doc.wb.sheets.len();
    app.tabs.first = app.tabs.first.min(n.saturating_sub(1));
    let mut tab_rects: Vec<(usize, Rect)> = Vec::new();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let limit = rect.right() - 30.0;
    for i in app.tabs.first..n {
        let s = &app.doc.wb.sheets[i];
        if s.hidden {
            continue;
        }
        let active = i == app.sheet;
        let name = s.name.clone();
        let galley = painter.layout_no_wrap(name.clone(), font.clone(), theme::TEXT);
        let w = galley.size().x + 26.0;
        if x + w > limit {
            break;
        }
        let r = Rect::from_min_size(pos2(x, y0), vec2(w, h));
        tab_rects.push((i, r));
        let resp = ui.interact(r, egui::Id::unique(("rexcel-tab", i)), Sense::click_and_drag());
        // renaming in place
        if let Some((ri, text, first)) = app.tabs.rename.as_mut() {
            if *ri == i {
                let te = egui::TextEdit::singleline(text).font(font.clone()).desired_width(w.max(80.0)).margin(egui::Margin::symmetric(4, 2));
                let out = ui.put(Rect::from_min_size(r.min, vec2(w.max(90.0), h)), te);
                if *first {
                    out.request_focus();
                    *first = false;
                }
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if esc {
                    app.tabs.rename = None;
                } else if out.lost_focus() || enter {
                    let t = text.clone();
                    app.tabs.rename = None;
                    app.rename_sheet(i, &t);
                }
                x += w.max(90.0) + 2.0;
                continue;
            }
        }
        let bg = if active { Color32::WHITE } else if resp.hovered() { Color32::from_gray(230) } else { theme::RIBBON_BG };
        painter.rect_filled(r, 0.0, bg);
        if let Some(c) = s.tab_color {
            painter.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 4.0), r.max), 0.0, theme::to_c32(c));
        }
        if active {
            painter.hline(r.left() + 4.0..=r.right() - 4.0, r.bottom() - 2.0, Stroke::new(2.5, theme::ACCENT));
            painter.vline(r.left(), r.y_range(), Stroke::new(1.0, theme::BORDER));
            painter.vline(r.right(), r.y_range(), Stroke::new(1.0, theme::BORDER));
        } else {
            painter.vline(r.right(), r.top() + 5.0..=r.bottom() - 5.0, Stroke::new(1.0, Color32::from_gray(200)));
        }
        let tc = if active { theme::ACCENT_DARK } else { theme::TEXT };
        let f = if active { FontId::proportional(13.0) } else { font.clone() };
        painter.text(r.center(), Align2::CENTER_CENTER, &name, f.clone(), tc);
        if active {
            painter.text(r.center() + vec2(0.4, 0.0), Align2::CENTER_CENTER, &name, f, tc);
        }
        if resp.clicked() {
            // While editing a formula, clicking another sheet allows pointing at it.
            if app.pointing_possible() {
                app.set_sheet(i);
            } else if app.commit_if_editing() {
                app.set_sheet(i);
            }
        }
        if resp.double_clicked() {
            app.tabs.rename = Some((i, name.clone(), true));
        }
        if resp.drag_started() {
            app.tabs.drag_from = Some(i);
        }
        resp.context_menu(|ui| tab_menu(app, ui, i));
        x += w + 1.0;
    }
    // drag to reorder
    if let Some(from) = app.tabs.drag_from {
        if let Some(pp) = pointer {
            if let Some((_, r)) = tab_rects.iter().find(|(_, r)| r.x_range().contains(pp.x)) {
                painter.vline(r.left(), r.y_range(), Stroke::new(2.0, theme::ACCENT));
            }
        }
        if ui.input(|i| i.pointer.primary_released()) {
            if let Some(pp) = pointer {
                if let Some((to, _)) = tab_rects.iter().find(|(_, r)| r.x_range().contains(pp.x)) {
                    if *to != from {
                        app.move_sheet(from, *to);
                    }
                }
            }
            app.tabs.drag_from = None;
        }
    }
    // add-sheet button
    let plus = Rect::from_center_size(pos2(x + 14.0, rect.center().y), vec2(22.0, 22.0));
    if plus.right() < rect.right() + 4.0 {
        let resp = ui.interact(plus, egui::Id::unique("rexcel-addsheet"), Sense::click()).on_hover_text("New sheet (Shift+F11)");
        if resp.hovered() {
            painter.circle_filled(plus.center(), 11.0, Color32::from_gray(225));
        }
        painter.circle_stroke(plus.center(), 9.0, Stroke::new(1.0, Color32::from_gray(120)));
        painter.text(plus.center(), Align2::CENTER_CENTER, "+", FontId::proportional(16.0), Color32::from_gray(80));
        if resp.clicked() && app.commit_if_editing() {
            app.add_sheet_end();
        }
    }
    let _ = StrokeKind::Inside;
}

fn tab_menu(app: &mut App, ui: &mut egui::Ui, i: usize) {
    if ui.button("Insert").clicked() {
        app.set_sheet(i);
        app.insert_sheet();
        ui.close();
    }
    if ui.button("Delete").clicked() {
        app.delete_sheet(i);
        ui.close();
    }
    if ui.button("Rename").clicked() {
        app.tabs.rename = Some((i, app.doc.wb.sheets[i].name.clone(), true));
        ui.close();
    }
    if ui.button("Move or Copy... (Duplicate)").clicked() {
        app.duplicate_sheet(i);
        ui.close();
    }
    ui.separator();
    if ui.add_enabled(i > 0, egui::Button::new("Move Left")).clicked() {
        app.move_sheet(i, i - 1);
        ui.close();
    }
    if ui.add_enabled(i + 1 < app.doc.wb.sheets.len(), egui::Button::new("Move Right")).clicked() {
        app.move_sheet(i, i + 1);
        ui.close();
    }
    ui.separator();
    ui.menu_button("Tab Color", |ui| {
        if crate::ribbon::palette(ui, None).map(|c| {
            app.modify("Tab Color", engine::ops::Scope::Workbook, |wb, _| wb.sheets[i].tab_color = Some(c));
        }).is_some() {
            ui.close();
        }
        if ui.button("No Color").clicked() {
            app.modify("Tab Color", engine::ops::Scope::Workbook, |wb, _| wb.sheets[i].tab_color = None);
            ui.close();
        }
    });
    if ui.button("Hide").clicked() {
        app.set_sheet_hidden(i, true);
        ui.close();
    }
    let hidden: Vec<(usize, String)> = app.doc.wb.sheets.iter().enumerate().filter(|(_, s)| s.hidden).map(|(i, s)| (i, s.name.clone())).collect();
    ui.add_enabled_ui(!hidden.is_empty(), |ui| {
        ui.menu_button("Unhide", |ui| {
            for (hi, name) in hidden {
                if ui.button(name).clicked() {
                    app.set_sheet_hidden(hi, false);
                    ui.close();
                }
            }
        });
    });
}
