//! Ribbon (tabs + command groups), File menu and the formula bar.

use egui::{pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, Ui};
use engine::style::{Color, HAlign, VAlign};
use engine::workbook::ChartKind;

use crate::actions::BorderPreset;
use crate::app::{bar_editor_id, App, EditMode, FontFlag, Pending, RibbonTab};
use crate::dialogs::{CfKind, Dialog};
use crate::numfmt_util;
use crate::theme;

const BODY_H: f32 = 78.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    // ---- tab row ----
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 2.0;
        file_menu(app, ui);
        ui.add_space(6.0);
        for (tab, name) in [(RibbonTab::Home, "Home"), (RibbonTab::Insert, "Insert"), (RibbonTab::Formulas, "Formulas"), (RibbonTab::Data, "Data"), (RibbonTab::View, "View")] {
            let sel = app.ribbon_tab == tab;
            let text = RichText::new(name).size(13.0).color(if sel { theme::ACCENT_DARK } else { theme::TEXT });
            let r = ui.add(egui::Button::new(if sel { text.strong() } else { text }).frame_when_inactive(false).min_size(vec2(58.0, 24.0)));
            if sel {
                ui.painter().hline(r.rect.x_range().shrink(10.0), r.rect.bottom() - 1.0, Stroke::new(3.0, theme::ACCENT));
            }
            if r.clicked() {
                app.ribbon_tab = tab;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            let undo_tip = app.doc.undo.redo_label().map(|l| format!("Redo {} (Ctrl+Y)", l)).unwrap_or_else(|| "Can't Redo".into());
            if ui.add_enabled(app.doc.undo.can_redo(), egui::Button::new("↷").frame_when_inactive(false)).on_hover_text(undo_tip).clicked() {
                app.redo();
            }
            let undo_tip = app.doc.undo.undo_label().map(|l| format!("Undo {} (Ctrl+Z)", l)).unwrap_or_else(|| "Can't Undo".into());
            if ui.add_enabled(app.doc.undo.can_undo(), egui::Button::new("↶").frame_when_inactive(false)).on_hover_text(undo_tip).clicked() {
                app.undo();
            }
            if ui.add(egui::Button::new("💾").frame_when_inactive(false)).on_hover_text("Save (Ctrl+S)").clicked() {
                app.save();
            }
        });
    });
    // ---- body ----
    let body = egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(egui::Margin::symmetric(6, 4)).corner_radius(4.0).outer_margin(egui::Margin { left: 6, right: 6, top: 0, bottom: 6 });
    body.show(ui, |ui| {
        ui.set_height(BODY_H);
        ui.set_width(ui.available_width());
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        combo_frame(ui, false);
        egui::ScrollArea::horizontal().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            ui.horizontal_top(|ui| match app.ribbon_tab {
                RibbonTab::Home => home(app, ui),
                RibbonTab::Insert => insert(app, ui),
                RibbonTab::Formulas => formulas(app, ui),
                RibbonTab::Data => data(app, ui),
                RibbonTab::View => view(app, ui),
            });
        });
    });
}

fn file_menu(app: &mut App, ui: &mut Ui) {
    let btn = egui::Button::new(RichText::new("  File  ").color(Color32::WHITE).size(13.0)).fill(theme::ACCENT).min_size(vec2(54.0, 24.0));
    let resp = ui.add(btn);
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(220.0);
        if ui.add(egui::Button::new("New").shortcut_text("Ctrl+N")).clicked() {
            app.request(Pending::New);
            ui.close();
        }
        if ui.add(egui::Button::new("Open...").shortcut_text("Ctrl+O")).clicked() {
            app.request(Pending::Open(None));
            ui.close();
        }
        let recent = app.recent.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button("Open Recent", |ui| {
                for p in recent {
                    let label = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    if ui.button(label).on_hover_text(p.display().to_string()).clicked() {
                        app.request(Pending::Open(Some(p.clone())));
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        if ui.add(egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
            app.save();
            ui.close();
        }
        if ui.add(egui::Button::new("Save As...").shortcut_text("Ctrl+Shift+S")).clicked() {
            app.save_as();
            ui.close();
        }
        ui.separator();
        if ui.add(egui::Button::new("Print...").shortcut_text("Ctrl+P")).clicked() {
            app.message("Print", "Printing is not available in RExcel yet.");
            ui.close();
        }
        if ui.button("About RExcel").clicked() {
            app.message("About RExcel", "RExcel — a spreadsheet application written in Rust.\nEngine: formulas, functions, number formats, xlsx/csv I/O.\nUI: egui.");
            ui.close();
        }
        ui.separator();
        if ui.add(egui::Button::new("Exit").shortcut_text("Alt+F4")).clicked() {
            app.request(Pending::Close);
            ui.close();
        }
    });
}

// ----------------------------------------------------------------------
// building blocks
// ----------------------------------------------------------------------

fn group(ui: &mut Ui, name: &str, add: impl FnOnce(&mut Ui)) {
    let top = ui.cursor().top();
    let inner = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        let content = ui.horizontal_top(|ui| add(ui)).response.rect;
        let label = ui.painter().layout_no_wrap(name.to_string(), FontId::proportional(11.0), theme::MUTED);
        let w = content.width().max(label.size().x + 8.0);
        let rest = (top + BODY_H - ui.cursor().top()).max(0.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, rest), Sense::hover());
        ui.painter().galley(pos2(content.left() + (w - label.size().x) / 2.0, r.bottom() - label.size().y), label, theme::MUTED);
    });
    let r = inner.response.rect;
    ui.add_space(5.0);
    ui.painter().vline(r.right() + 5.0, top + 4.0..=top + BODY_H - 4.0, Stroke::new(1.0, theme::BORDER));
    ui.add_space(6.0);
}

/// Ribbon buttons are frameless; combo boxes get a thin border.
fn combo_frame(ui: &mut Ui, on: bool) {
    ui.visuals_mut().widgets.inactive.bg_stroke = if on { Stroke::new(1.0, Color32::from_gray(180)) } else { Stroke::NONE };
}

/// A large icon button with a label under it.
fn big(ui: &mut Ui, icon: &str, label: &str, tip: &str) -> egui::Response {
    let w = (ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.0), theme::TEXT).size().x + 12.0).max(46.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BODY_H - 18.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect(rect, 3.0, Color32::from_gray(232), Stroke::new(1.0, Color32::from_gray(205)), egui::StrokeKind::Inside);
    }
    p.text(pos2(rect.center().x, rect.top() + 20.0), Align2::CENTER_CENTER, icon, FontId::proportional(24.0), theme::ACCENT_DARK);
    let g = p.layout(label.to_string(), FontId::proportional(12.0), theme::TEXT, rect.width());
    p.galley(pos2(rect.center().x - g.size().x / 2.0, rect.top() + 36.0), g, theme::TEXT);
    resp.on_hover_text(tip)
}

fn small(ui: &mut Ui, text: impl Into<egui::WidgetText>, tip: &str) -> egui::Response {
    ui.add(egui::Button::new(text).frame_when_inactive(false).min_size(vec2(24.0, 22.0))).on_hover_text(tip)
}

fn toggle(ui: &mut Ui, on: bool, text: impl Into<egui::WidgetText>, tip: &str) -> egui::Response {
    ui.add(egui::Button::selectable(on, text).min_size(vec2(24.0, 22.0))).on_hover_text(tip)
}

/// A toggle showing a painted horizontal-alignment icon (0 left, 1 center, 2 right).
fn align_toggle(ui: &mut Ui, on: bool, which: u8, tip: &str) -> egui::Response {
    let resp = ui.add(egui::Button::selectable(on, " ").min_size(vec2(24.0, 22.0))).on_hover_text(tip);
    let c = resp.rect.center();
    let stroke = Stroke::new(1.3, Color32::from_gray(60));
    for (i, w) in [12.0f32, 8.0, 12.0, 8.0].iter().enumerate() {
        let y = c.y - 4.5 + i as f32 * 3.0;
        let (x0, x1) = match which {
            0 => (c.x - 6.0, c.x - 6.0 + w),
            1 => (c.x - w / 2.0, c.x + w / 2.0),
            _ => (c.x + 6.0 - w, c.x + 6.0),
        };
        ui.painter().hline(x0..=x1, y, stroke);
    }
    resp
}

/// Office color palette; returns the picked color.
pub fn palette(ui: &mut Ui, auto_label: Option<&str>) -> Option<Color> {
    let mut picked = None;
    if let Some(l) = auto_label {
        if ui.button(l).clicked() {
            picked = Some(Color(0xFFFF_FFFF));
        }
    }
    ui.label(RichText::new("Theme Colors").small().color(theme::MUTED));
    for (ri, row) in theme::PALETTE.iter().enumerate() {
        if ri == 5 {
            ui.label(RichText::new("Standard Colors").small().color(theme::MUTED));
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for c in row {
                let (r, resp) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::click());
                let col = theme::to_c32(Color(*c));
                ui.painter().rect_filled(r, 0.0, col);
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, if resp.hovered() { Color32::from_rgb(0xF2, 0x94, 0x36) } else { Color32::from_gray(200) }), egui::StrokeKind::Inside);
                if resp.on_hover_text(format!("#{:06X}", c)).clicked() {
                    picked = Some(Color(*c));
                }
            }
        });
    }
    ui.add_space(4.0);
    let mut c32 = Color32::from_gray(128);
    ui.horizontal(|ui| {
        ui.label("More Colors:");
        if ui.color_edit_button_srgba(&mut c32).changed() {
            picked = Some(theme::from_c32(c32));
        }
    });
    picked
}

fn color_split(ui: &mut Ui, label: &str, current: Color, tip: &str, auto: &str) -> (bool, Option<Option<Color>>) {
    let mut apply = false;
    let mut pick: Option<Option<Color>> = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let r = ui.add(egui::Button::new(RichText::new(label).strong()).frame_when_inactive(false).min_size(vec2(24.0, 22.0))).on_hover_text(tip);
        ui.painter().rect_filled(Rect::from_min_max(pos2(r.rect.left() + 4.0, r.rect.bottom() - 5.0), pos2(r.rect.right() - 4.0, r.rect.bottom() - 2.0)), 0.0, theme::to_c32(current));
        if r.clicked() {
            apply = true;
        }
        ui.menu_button("▾", |ui| {
            if let Some(c) = palette(ui, Some(auto)) {
                pick = Some(if c.0 == 0xFFFF_FFFF { None } else { Some(c) });
                ui.close();
            }
        });
    });
    (apply, pick)
}

// ----------------------------------------------------------------------
// Home
// ----------------------------------------------------------------------

fn home(app: &mut App, ui: &mut Ui) {
    let st = app.active_style();
    group(ui, "Clipboard", |ui| {
        if big(ui, "📋", "Paste", "Paste (Ctrl+V)").clicked() {
            app.paste_text_or_clip(None);
        }
        ui.vertical(|ui| {
            if small(ui, "✂ Cut", "Cut (Ctrl+X)").clicked() {
                app.copy(true);
            }
            if small(ui, "⎘ Copy", "Copy (Ctrl+C)").clicked() {
                app.copy(false);
            }
            if toggle(ui, app.format_painter.is_some(), "🖊 Format Painter", "Copy formatting from one place and apply it to another").clicked() {
                if app.format_painter.is_some() {
                    app.format_painter = None;
                } else {
                    app.start_format_painter();
                }
            }
        });
    });
    group(ui, "Font", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let cur_font = st.font_name.clone().unwrap_or_else(|| engine::style::DEFAULT_FONT.into());
                combo_frame(ui, true);
                egui::ComboBox::from_id_salt("font-name").width(130.0).selected_text(&cur_font).show_ui(ui, |ui| {
                    for f in crate::fonts::FONT_NAMES {
                        if ui.selectable_label(f == cur_font, f).clicked() {
                            let name = if f == engine::style::DEFAULT_FONT { None } else { Some(f.to_string()) };
                            app.apply_style("Font", |s| s.font_name = name.clone());
                        }
                    }
                });
                let size = st.font_size();
                egui::ComboBox::from_id_salt("font-size").width(48.0).selected_text(format!("{}", size)).show_ui(ui, |ui| {
                    for s in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0] {
                        if ui.selectable_label(s == size, format!("{}", s)).clicked() {
                            app.apply_style("Font Size", |st| st.set_font_size(s));
                        }
                    }
                });
                combo_frame(ui, false);
                if small(ui, "A⁺", "Increase Font Size").clicked() {
                    app.change_font_size(1);
                }
                if small(ui, "A⁻", "Decrease Font Size").clicked() {
                    app.change_font_size(-1);
                }
            });
            ui.horizontal(|ui| {
                if toggle(ui, st.bold, RichText::new("B").strong(), "Bold (Ctrl+B)").clicked() {
                    app.toggle_font_flag(FontFlag::Bold);
                }
                if toggle(ui, st.italic, RichText::new("I").italics(), "Italic (Ctrl+I)").clicked() {
                    app.toggle_font_flag(FontFlag::Italic);
                }
                if toggle(ui, st.underline, RichText::new("U").underline(), "Underline (Ctrl+U)").clicked() {
                    app.toggle_font_flag(FontFlag::Underline);
                }
                if toggle(ui, st.strike, RichText::new("ab").strikethrough(), "Strikethrough (Ctrl+5)").clicked() {
                    app.toggle_font_flag(FontFlag::Strike);
                }
                borders_menu(app, ui);
                let (apply, pick) = color_split(ui, "🎨", app.fill_color, "Fill Color", "No Fill");
                if apply {
                    let c = app.fill_color;
                    app.apply_style("Fill Color", |s| s.fill = Some(c));
                }
                if let Some(c) = pick {
                    if let Some(c) = c {
                        app.fill_color = c;
                    }
                    app.apply_style("Fill Color", |s| s.fill = c);
                }
                let (apply, pick) = color_split(ui, "A", app.font_color, "Font Color", "Automatic");
                if apply {
                    let c = app.font_color;
                    app.apply_style("Font Color", |s| s.font_color = Some(c));
                }
                if let Some(c) = pick {
                    if let Some(c) = c {
                        app.font_color = c;
                    }
                    app.apply_style("Font Color", |s| s.font_color = c);
                }
            });
        });
    });
    group(ui, "Alignment", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if toggle(ui, st.v_align == VAlign::Top, "⬆", "Top Align").clicked() {
                    app.set_v_align(VAlign::Top);
                }
                if toggle(ui, st.v_align == VAlign::Center, "↕", "Middle Align").clicked() {
                    app.set_v_align(VAlign::Center);
                }
                if toggle(ui, st.v_align == VAlign::Bottom, "⬇", "Bottom Align").clicked() {
                    app.set_v_align(VAlign::Bottom);
                }
                ui.add_space(4.0);
                if toggle(ui, st.wrap, "↵ Wrap Text", "Wrap extra-long text into multiple lines").clicked() {
                    app.toggle_wrap();
                }
            });
            ui.horizontal(|ui| {
                if align_toggle(ui, st.h_align == HAlign::Left, 0, "Align Left").clicked() {
                    app.set_h_align(HAlign::Left);
                }
                if align_toggle(ui, st.h_align == HAlign::Center, 1, "Center").clicked() {
                    app.set_h_align(HAlign::Center);
                }
                if align_toggle(ui, st.h_align == HAlign::Right, 2, "Align Right").clicked() {
                    app.set_h_align(HAlign::Right);
                }
                if small(ui, "⇤", "Decrease Indent").clicked() {
                    app.change_indent(-1);
                }
                if small(ui, "⇥", "Increase Indent").clicked() {
                    app.change_indent(1);
                }
                let merged = app.is_merged_selection();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if toggle(ui, merged, "⊟ Merge & Center", "Merge & Center").clicked() {
                        if merged {
                            app.unmerge();
                        } else {
                            app.merge(true, false);
                        }
                    }
                    ui.menu_button("▾", |ui| {
                        if ui.button("Merge & Center").clicked() {
                            app.merge(true, false);
                            ui.close();
                        }
                        if ui.button("Merge Across").clicked() {
                            app.merge(false, true);
                            ui.close();
                        }
                        if ui.button("Merge Cells").clicked() {
                            app.merge(false, false);
                            ui.close();
                        }
                        if ui.button("Unmerge Cells").clicked() {
                            app.unmerge();
                            ui.close();
                        }
                    });
                });
            });
        });
    });
    group(ui, "Number", |ui| {
        ui.vertical(|ui| {
            let label = numfmt_util::RIBBON_FORMATS.iter().find(|(_, c)| *c == st.num_fmt).map(|(l, _)| l.to_string()).unwrap_or_else(|| numfmt_util::category_of(&st.num_fmt).to_string());
            combo_frame(ui, true);
            egui::ComboBox::from_id_salt("numfmt").width(140.0).selected_text(label).show_ui(ui, |ui| {
                let v = app.doc.wb.value(app.sheet, app.sel.active);
                for (l, c) in numfmt_util::RIBBON_FORMATS {
                    let sample = engine::numfmt::format_value(&if v.is_empty() { engine::Value::Number(1234.5) } else { v.clone() }, c).text;
                    if ui.selectable_label(st.num_fmt == c, format!("{:<12}  {}", l, sample)).clicked() {
                        app.set_number_format(c);
                    }
                }
                ui.separator();
                if ui.button("More Number Formats...").clicked() {
                    app.open_format_cells();
                }
            });
            combo_frame(ui, false);
            ui.horizontal(|ui| {
                if small(ui, "$", "Accounting Number Format").clicked() {
                    app.set_number_format(numfmt_util::ACCOUNTING);
                }
                if small(ui, "%", "Percent Style (Ctrl+Shift+%)").clicked() {
                    app.set_number_format("0%");
                }
                if small(ui, ",", "Comma Style").clicked() {
                    app.set_number_format(numfmt_util::COMMA);
                }
                if small(ui, ".0←", "Increase Decimal").clicked() {
                    app.change_decimals(1);
                }
                if small(ui, "→.0", "Decrease Decimal").clicked() {
                    app.change_decimals(-1);
                }
            });
        });
    });
    group(ui, "Styles", |ui| {
        ui.vertical(|ui| {
            ui.menu_button("Conditional Formatting ▾", |ui| cf_menu(app, ui));
            if small(ui, "Format Cells...", "Format Cells (Ctrl+1)").clicked() {
                app.open_format_cells();
            }
            ui.menu_button("Cell Styles ▾", |ui| {
                for (name, fill, font, bold) in [
                    ("Good", 0xC6EFCE, 0x006100, false),
                    ("Bad", 0xFFC7CE, 0x9C0006, false),
                    ("Neutral", 0xFFEB9C, 0x9C5700, false),
                    ("Heading", 0xFFFFFF, 0x44546A, true),
                    ("Accent", 0x4472C4, 0xFFFFFF, false),
                    ("Note", 0xFFFFCC, 0x000000, false),
                ] {
                    if ui.add(egui::Button::new(RichText::new(name).color(theme::to_c32(Color(font)))).fill(theme::to_c32(Color(fill)))).clicked() {
                        app.apply_style("Cell Style", |s| {
                            s.fill = if fill == 0xFFFFFF { None } else { Some(Color(fill)) };
                            s.font_color = Some(Color(font));
                            s.bold = bold;
                        });
                        ui.close();
                    }
                }
                if ui.button("Normal").clicked() {
                    app.apply_style("Cell Style", |s| *s = engine::style::Style { num_fmt: s.num_fmt.clone(), ..Default::default() });
                    ui.close();
                }
            });
        });
    });
    group(ui, "Cells", |ui| {
        ui.vertical(|ui| {
            ui.menu_button("⊕ Insert ▾", |ui| {
                if ui.button("Insert Sheet Rows").clicked() {
                    app.insert_rows();
                    ui.close();
                }
                if ui.button("Insert Sheet Columns").clicked() {
                    app.insert_cols();
                    ui.close();
                }
                if ui.button("Insert Sheet").clicked() {
                    app.insert_sheet();
                    ui.close();
                }
            });
            ui.menu_button("⊖ Delete ▾", |ui| {
                if ui.button("Delete Sheet Rows").clicked() {
                    app.delete_rows();
                    ui.close();
                }
                if ui.button("Delete Sheet Columns").clicked() {
                    app.delete_cols();
                    ui.close();
                }
                if ui.button("Delete Sheet").clicked() {
                    let s = app.sheet;
                    app.delete_sheet(s);
                    ui.close();
                }
            });
            ui.menu_button("▦ Format ▾", |ui| format_menu(app, ui));
        });
    });
    group(ui, "Editing", |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if small(ui, "∑ AutoSum", "AutoSum (Alt+=)").clicked() {
                    app.autosum("SUM");
                }
                ui.menu_button("▾", |ui| {
                    for (l, f) in [("Sum", "SUM"), ("Average", "AVERAGE"), ("Count Numbers", "COUNT"), ("Max", "MAX"), ("Min", "MIN")] {
                        if ui.button(l).clicked() {
                            app.autosum(f);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("More Functions...").clicked() {
                        app.open_insert_function();
                        ui.close();
                    }
                });
            });
            ui.menu_button("⬇ Fill ▾", |ui| {
                if ui.button("Down (Ctrl+D)").clicked() {
                    app.fill_down_right(true);
                    ui.close();
                }
                if ui.button("Right (Ctrl+R)").clicked() {
                    app.fill_down_right(false);
                    ui.close();
                }
            });
            ui.menu_button("⌫ Clear ▾", |ui| clear_menu(app, ui));
        });
        ui.vertical(|ui| {
            ui.menu_button("⇅ Sort & Filter ▾", |ui| sort_menu(app, ui));
            ui.menu_button("🔍 Find & Select ▾", |ui| {
                if ui.add(egui::Button::new("Find...").shortcut_text("Ctrl+F")).clicked() {
                    app.open_find(false);
                    ui.close();
                }
                if ui.add(egui::Button::new("Replace...").shortcut_text("Ctrl+H")).clicked() {
                    app.open_find(true);
                    ui.close();
                }
                if ui.add(egui::Button::new("Go To...").shortcut_text("Ctrl+G")).clicked() {
                    app.dialog = Some(Dialog::GoTo { text: String::new() });
                    ui.close();
                }
            });
        });
    });
}

fn borders_menu(app: &mut App, ui: &mut Ui) {
    ui.menu_button("⊞▾", |ui| {
        let black = Color::BLACK;
        for (label, preset) in [
            ("Bottom Border", BorderPreset::Bottom),
            ("Top Border", BorderPreset::Top),
            ("Left Border", BorderPreset::Left),
            ("Right Border", BorderPreset::Right),
            ("No Border", BorderPreset::None),
            ("All Borders", BorderPreset::All),
            ("Outside Borders", BorderPreset::Outside),
            ("Thick Box Border", BorderPreset::ThickOutside),
            ("Bottom Double Border", BorderPreset::DoubleBottom),
        ] {
            if ui.button(label).clicked() {
                app.apply_borders(preset, black);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("More Borders...").clicked() {
            app.open_format_cells();
            if let Some(Dialog::FormatCells(f)) = app.dialog.as_mut() {
                f.tab = crate::format_dialog::Tab::Border;
            }
            ui.close();
        }
    })
    .response
    .on_hover_text("Borders");
}

pub fn clear_menu(app: &mut App, ui: &mut Ui) {
    if ui.button("Clear All").clicked() {
        app.clear_selection(true, true);
        ui.close();
    }
    if ui.button("Clear Formats").clicked() {
        app.clear_selection(false, true);
        ui.close();
    }
    if ui.button("Clear Contents").clicked() {
        app.clear_selection(true, false);
        ui.close();
    }
    if ui.button("Clear Comments").clicked() {
        app.clear_comments();
        ui.close();
    }
    if ui.button("Clear Hyperlinks").clicked() {
        app.clear_hyperlinks();
        ui.close();
    }
}

pub fn sort_menu(app: &mut App, ui: &mut Ui) {
    if ui.button("Sort A to Z").clicked() {
        app.sort_simple(true);
        ui.close();
    }
    if ui.button("Sort Z to A").clicked() {
        app.sort_simple(false);
        ui.close();
    }
    if ui.button("Custom Sort...").clicked() {
        app.open_custom_sort();
        ui.close();
    }
    ui.separator();
    if ui.add(egui::Button::selectable(app.sheet_ref().autofilter.is_some(), "Filter").shortcut_text("Ctrl+Shift+L")).clicked() {
        app.toggle_autofilter();
        ui.close();
    }
}

fn format_menu(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new("Cell Size").small().color(theme::MUTED));
    if ui.button("Row Height...").clicked() {
        let h = app.sheet_ref().row_height(app.sel.active.row);
        app.dialog = Some(Dialog::Size { rows: true, value: format!("{}", h * 0.75) });
        ui.close();
    }
    if ui.button("AutoFit Row Height").clicked() {
        let r = app.sel.primary();
        let rows: Vec<u32> = (r.top..=r.bottom.min(r.top + 5000)).collect();
        crate::grid::autofit_rows(app, ui, rows);
        ui.close();
    }
    if ui.button("Column Width...").clicked() {
        let w = app.sheet_ref().col_width(app.sel.active.col);
        app.dialog = Some(Dialog::Size { rows: false, value: format!("{:.2}", w / 7.0) });
        ui.close();
    }
    if ui.button("AutoFit Column Width").clicked() {
        let r = app.sel.primary();
        let cols: Vec<u32> = (r.left..=r.right.min(r.left + 500)).collect();
        crate::grid::autofit_cols(app, ui, cols);
        ui.close();
    }
    ui.separator();
    ui.label(RichText::new("Visibility").small().color(theme::MUTED));
    ui.menu_button("Hide & Unhide", |ui| {
        if ui.button("Hide Rows").clicked() {
            app.hide_rows_cols(true, true);
            ui.close();
        }
        if ui.button("Hide Columns").clicked() {
            app.hide_rows_cols(false, true);
            ui.close();
        }
        if ui.button("Unhide Rows").clicked() {
            app.hide_rows_cols(true, false);
            ui.close();
        }
        if ui.button("Unhide Columns").clicked() {
            app.hide_rows_cols(false, false);
            ui.close();
        }
    });
    ui.separator();
    ui.label(RichText::new("Organize Sheets").small().color(theme::MUTED));
    if ui.button("Rename Sheet").clicked() {
        let i = app.sheet;
        app.tabs.rename = Some((i, app.sheet_ref().name.clone(), true));
        ui.close();
    }
    if ui.button("Move or Copy Sheet...").clicked() {
        let i = app.sheet;
        app.duplicate_sheet(i);
        ui.close();
    }
    ui.separator();
    if ui.button("Format Cells...").clicked() {
        app.open_format_cells();
        ui.close();
    }
}

pub fn cf_menu(app: &mut App, ui: &mut Ui) {
    ui.menu_button("Highlight Cells Rules", |ui| {
        for (l, k) in [
            ("Greater Than...", CfKind::Greater),
            ("Less Than...", CfKind::Less),
            ("Between...", CfKind::Between),
            ("Equal To...", CfKind::Equal),
            ("Text that Contains...", CfKind::TextContains),
            ("Duplicate Values...", CfKind::Duplicates),
        ] {
            if ui.button(l).clicked() {
                app.dialog = Some(Dialog::cond_format(k));
                ui.close();
            }
        }
    });
    ui.menu_button("Top/Bottom Rules", |ui| {
        for (l, k) in [("Top 10 Items...", CfKind::Top10), ("Bottom 10 Items...", CfKind::Bottom10), ("Above Average...", CfKind::AboveAverage), ("Below Average...", CfKind::BelowAverage)] {
            if ui.button(l).clicked() {
                app.dialog = Some(Dialog::cond_format(k));
                ui.close();
            }
        }
    });
    ui.menu_button("Data Bars", |ui| {
        for (name, c) in [("Blue", 0x638EC6), ("Green", 0x63BE7B), ("Red", 0xFF555A), ("Orange", 0xFFB628), ("Purple", 0xD6007B)] {
            if ui.button(name).clicked() {
                app.add_cf(engine::workbook::CfRule::DataBar { color: Color(c) });
                ui.close();
            }
        }
    });
    ui.menu_button("Color Scales", |ui| {
        for (name, a, m, b) in [
            ("Green - Yellow - Red", 0x63BE7B, Some(0xFFEB84), 0xF8696B),
            ("Red - Yellow - Green", 0xF8696B, Some(0xFFEB84), 0x63BE7B),
            ("Green - White", 0xFFFFFF, None, 0x63BE7B),
            ("White - Red", 0xFFFFFF, None, 0xF8696B),
            ("Blue - White - Red", 0x5A8AC6, Some(0xFCFCFF), 0xF8696B),
        ] {
            if ui.button(name).clicked() {
                app.add_cf(engine::workbook::CfRule::ColorScale { min: Color(a), mid: m.map(Color), max: Color(b) });
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button("New Rule (formula)...").clicked() {
        app.dialog = Some(Dialog::cond_format(CfKind::Formula));
        ui.close();
    }
    ui.menu_button("Clear Rules", |ui| {
        if ui.button("Clear Rules from Selected Cells").clicked() {
            let rects = app.sel.ranges.clone();
            app.modify("Clear Rules", engine::ops::Scope::Sheet(app.sheet), |wb, s| wb.sheets[s].cond_formats.retain(|c| !rects.iter().any(|r| r.intersects(&c.range))));
            ui.close();
        }
        if ui.button("Clear Rules from Entire Sheet").clicked() {
            app.modify("Clear Rules", engine::ops::Scope::Sheet(app.sheet), |wb, s| wb.sheets[s].cond_formats.clear());
            ui.close();
        }
    });
    if ui.button("Manage Rules...").clicked() {
        app.dialog = Some(Dialog::CfManager { selected: None });
        ui.close();
    }
}

// ----------------------------------------------------------------------
// Insert / Formulas / Data / View
// ----------------------------------------------------------------------

fn insert(app: &mut App, ui: &mut Ui) {
    group(ui, "Tables", |ui| {
        if big(ui, "⛃", "Filter", "Turn on filtering for the selected cells").clicked() {
            app.toggle_autofilter();
        }
    });
    group(ui, "Charts", |ui| {
        for (icon, label, kind) in [("📊", "Column", ChartKind::Column), ("☰", "Bar", ChartKind::Bar), ("📈", "Line", ChartKind::Line), ("◭", "Area", ChartKind::Area), ("◔", "Pie", ChartKind::Pie), ("⁘", "Scatter", ChartKind::Scatter)] {
            if big(ui, icon, label, &format!("Insert {} Chart", label)).clicked() {
                app.insert_chart(kind);
            }
        }
    });
    group(ui, "Links", |ui| {
        if big(ui, "🔗", "Link", "Insert Link (Ctrl+K)").clicked() {
            app.open_hyperlink();
        }
    });
    group(ui, "Comments", |ui| {
        if big(ui, "💬", "Comment", "Insert Comment (Shift+F2)").clicked() {
            app.edit_comment();
        }
    });
    group(ui, "Symbols", |ui| {
        if big(ui, "ƒx", "Function", "Insert Function (Shift+F3)").clicked() {
            app.open_insert_function();
        }
    });
}

fn formulas(app: &mut App, ui: &mut Ui) {
    group(ui, "Function Library", |ui| {
        if big(ui, "ƒx", "Insert Function", "Insert Function (Shift+F3)").clicked() {
            app.open_insert_function();
        }
        if big(ui, "∑", "AutoSum", "AutoSum (Alt+=)").clicked() {
            app.autosum("SUM");
        }
        ui.vertical(|ui| {
            let mut cats: Vec<&str> = app.functions.iter().map(|f| f.category).collect();
            cats.sort();
            cats.dedup();
            let per_row = cats.len().div_ceil(3).max(1);
            for chunk in cats.chunks(per_row) {
                ui.horizontal(|ui| {
                    for c in chunk {
                        ui.menu_button(format!("{} ▾", c), |ui| {
                            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                                let fns: Vec<&'static engine::functions::FnDef> = app.functions.iter().copied().filter(|f| f.category == *c).collect();
                                for f in fns {
                                    if ui.button(f.name).on_hover_text(format!("{}\n{}", f.syntax, f.description)).clicked() {
                                        app.insert_function_text(f.name);
                                        ui.close();
                                    }
                                }
                            });
                        });
                    }
                });
            }
        });
    });
    group(ui, "Defined Names", |ui| {
        if big(ui, "🏷", "Name Manager", "Name Manager (Ctrl+F3)").clicked() {
            app.dialog = Some(Dialog::name_manager());
        }
        ui.vertical(|ui| {
            if small(ui, "Define Name...", "Define a name for the selection").clicked() {
                app.dialog = Some(Dialog::define_name(app));
            }
        });
    });
    group(ui, "Formula Auditing", |ui| {
        if big(ui, "ƒ", "Show Formulas", "Show Formulas (Ctrl+`)").clicked() {
            app.show_formulas = !app.show_formulas;
        }
    });
    group(ui, "Calculation", |ui| {
        ui.vertical(|ui| {
            let manual = app.doc.wb.manual_calc;
            ui.menu_button(format!("Calculation Options: {} ▾", if manual { "Manual" } else { "Automatic" }), |ui| {
                if ui.radio(!manual, "Automatic").clicked() {
                    app.set_calc_manual(false);
                    ui.close();
                }
                if ui.radio(manual, "Manual").clicked() {
                    app.set_calc_manual(true);
                    ui.close();
                }
            });
            if small(ui, "⟳ Calculate Now", "Calculate Now (F9)").clicked() {
                app.doc.wb.recalculate();
                app.touch();
            }
        });
    });
}

fn data(app: &mut App, ui: &mut Ui) {
    group(ui, "Sort & Filter", |ui| {
        ui.vertical(|ui| {
            if small(ui, "A→Z", "Sort A to Z").clicked() {
                app.sort_simple(true);
            }
            if small(ui, "Z→A", "Sort Z to A").clicked() {
                app.sort_simple(false);
            }
        });
        if big(ui, "⇅", "Sort", "Custom Sort").clicked() {
            app.open_custom_sort();
        }
        let on = app.sheet_ref().autofilter.is_some();
        if big(ui, "⛃", if on { "Filter ✓" } else { "Filter" }, "Filter (Ctrl+Shift+L)").clicked() {
            app.toggle_autofilter();
        }
        if on && small(ui, "Clear", "Clear the filter").clicked() {
            app.clear_filter();
        }
    });
    group(ui, "Data Tools", |ui| {
        if big(ui, "⊞", "Text to Columns", "Split text into columns").clicked() {
            app.dialog = Some(Dialog::TextToColumns { tab: true, comma: true, semicolon: false, space: false, other: String::new(), consecutive: false });
        }
        if big(ui, "🗗", "Remove Duplicates", "Delete duplicate rows").clicked() {
            app.dialog = Some(Dialog::remove_duplicates(app));
        }
        if big(ui, "✔", "Data Validation", "Restrict the data that can be entered").clicked() {
            app.open_validation();
        }
    });
    group(ui, "Forecast", |ui| {
        if big(ui, "🎯", "Goal Seek", "What-If Analysis: Goal Seek").clicked() {
            app.dialog = Some(Dialog::GoalSeek { set_cell: app.sel.active.to_a1(), to_value: String::new(), changing: String::new() });
        }
    });
}

fn view(app: &mut App, ui: &mut Ui) {
    group(ui, "Show", |ui| {
        ui.vertical(|ui| {
            let mut g = app.sheet_ref().show_gridlines;
            if ui.checkbox(&mut g, "Gridlines").changed() {
                app.doc.wb.sheets[app.sheet].show_gridlines = g;
                app.doc.dirty = true;
            }
            ui.checkbox(&mut app.show_formula_bar, "Formula Bar");
            ui.checkbox(&mut app.show_headings, "Headings");
        });
    });
    group(ui, "Zoom", |ui| {
        if big(ui, "🔍", "Zoom", "Zoom").clicked() {
            app.dialog = Some(Dialog::Zoom { value: (app.sheet_ref().zoom * 100.0).round() as u32 });
        }
        if big(ui, "1:1", "100%", "Zoom to 100%").clicked() {
            app.set_zoom(1.0);
        }
        for pair in [[50, 75], [150, 200]] {
            ui.vertical(|ui| {
                for z in pair {
                    if small(ui, format!("{}%", z), &format!("Zoom to {}%", z)).clicked() {
                        app.set_zoom(z as f32 / 100.0);
                    }
                }
            });
        }
    });
    group(ui, "Window", |ui| {
        ui.menu_button("❄ Freeze Panes ▾", |ui| {
            if ui.button("Freeze Panes").on_hover_text("Keep rows and columns above and left of the selection visible").clicked() {
                app.freeze_at_selection();
                ui.close();
            }
            if ui.button("Freeze Top Row").clicked() {
                app.freeze(1, 0);
                ui.close();
            }
            if ui.button("Freeze First Column").clicked() {
                app.freeze(0, 1);
                ui.close();
            }
            if ui.button("Unfreeze Panes").clicked() {
                app.freeze(0, 0);
                ui.close();
            }
        });
    });
    group(ui, "Formulas", |ui| {
        ui.vertical(|ui| {
            ui.checkbox(&mut app.show_formulas, "Show Formulas");
        });
    });
}

// ----------------------------------------------------------------------
// Formula bar
// ----------------------------------------------------------------------

pub fn formula_bar(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // Name box
        let nb_id = egui::Id::unique("rexcel-namebox");
        let focused = ui.memory(|m| m.has_focus(nb_id));
        if !focused {
            app.name_box = name_box_text(app);
        }
        let resp = ui.add(egui::TextEdit::singleline(&mut app.name_box).id(nb_id).desired_width(96.0).font(FontId::proportional(13.0)));
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let t = app.name_box.clone();
            if !app.goto(&t) {
                // define a new name for the selection
                let r = app.sel.primary();
                let refers = format!("={}!{}", engine::address::quote_sheet_name(&app.sheet_ref().name), abs_ref(r));
                let mut wb2 = app.doc.wb.clone();
                match wb2.define_name(t.trim(), None, &refers) {
                    Ok(()) => {
                        let name = t.trim().to_string();
                        app.modify("Define Name", engine::ops::Scope::Workbook, |wb, _| {
                            let _ = wb.define_name(&name, None, &refers);
                        });
                    }
                    Err(_) => app.message("RExcel", "Reference isn't valid."),
                }
            }
        }
        ui.menu_button("▾", |ui| {
            let names: Vec<String> = app.doc.wb.names.iter().map(|n| n.name.clone()).collect();
            if names.is_empty() {
                ui.label(RichText::new("(no defined names)").color(theme::MUTED));
            }
            for n in names {
                if ui.button(&n).clicked() {
                    app.goto(&n);
                    ui.close();
                }
            }
        });
        ui.add_space(4.0);
        let editing = app.edit.is_some();
        if ui.add_enabled(editing, egui::Button::new("✕").frame_when_inactive(false)).on_hover_text("Cancel").clicked() {
            app.cancel_edit();
        }
        if ui.add_enabled(editing, egui::Button::new("✔").frame_when_inactive(false)).on_hover_text("Enter").clicked() {
            app.commit_edit(0, 0, false);
        }
        if ui.add(egui::Button::new(RichText::new("ƒx").italics()).frame_when_inactive(false)).on_hover_text("Insert Function").clicked() {
            app.open_insert_function();
        }
        ui.add_space(2.0);
        // formula text
        let id = bar_editor_id();
        let font = FontId::proportional(14.0);
        let width = ui.available_width() - 4.0;
        let f2 = font.clone();
        let mut layouter = move |ui: &Ui, buf: &dyn egui::TextBuffer, w: f32| ui.painter().layout_job(crate::grid::formula_job(buf.as_str(), f2.clone(), w));
        let lines = app.edit.as_ref().map(|e| e.text.lines().count()).unwrap_or(1).clamp(1, 4);
        let out = if let Some(e) = app.edit.as_mut() {
            let out = egui::TextEdit::multiline(&mut e.text)
                .id(id)
                .font(font)
                .desired_width(width)
                .desired_rows(lines)
                .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::Enter)))
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui);
            if out.response.has_focus() {
                if let Some(cr) = out.cursor_range {
                    e.cursor = cr.primary.index.0;
                }
                e.in_bar = true;
            }
            if out.response.changed() {
                e.point = None;
                e.ac_index = 0;
            }
            Some(out)
        } else {
            let mut text = app.doc.wb.input_text(app.sheet, app.sel.active);
            let out = egui::TextEdit::multiline(&mut text).id(id).font(font).desired_width(width).desired_rows(1).layouter(&mut layouter).show(ui);
            if out.response.gained_focus() || out.response.has_focus() {
                let cursor = out.cursor_range.map(|c| c.primary.index.0);
                app.begin_edit(None, EditMode::Edit, true);
                if let Some(e) = app.edit.as_mut() {
                    e.set_cursor = cursor;
                    e.request_focus = false;
                    e.in_bar = true;
                }
            }
            None
        };
        if let Some(out) = out {
            if app.edit.as_ref().map(|e| e.in_bar).unwrap_or(false) {
                let anchor = pos2(out.response.rect.left() + 8.0, out.response.rect.bottom() + 2.0);
                crate::grid::autocomplete_popup(app, ui.ctx(), anchor);
            }
        }
    });
}

fn abs_ref(r: engine::Rect) -> String {
    let a = |p: engine::CellPos| format!("${}${}", engine::address::col_to_letters(p.col), p.row + 1);
    if r.cell_count() == 1 {
        a(r.top_left())
    } else {
        format!("{}:{}", a(r.top_left()), a(r.bottom_right()))
    }
}

fn name_box_text(app: &App) -> String {
    if let Some(crate::grid::Drag::Cells { .. }) = &app.grid.drag {
        let r = app.sel.primary();
        if r.cell_count() > 1 {
            return format!("{}R x {}C", r.rows(), r.cols());
        }
    }
    // a defined name exactly matching the selection
    let r = app.sel.primary();
    for n in &app.doc.wb.names {
        if let engine::ast::Expr::Ref(rf) = &n.formula {
            let same_sheet = rf.sheet.as_deref().map(|s| s.eq_ignore_ascii_case(&app.sheet_ref().name)).unwrap_or(true);
            if same_sheet && rf.rect() == r && r.cell_count() > 1 {
                return n.name.clone();
            }
        }
    }
    if let Some(m) = app.sheet_ref().merge_at(app.sel.active) {
        if m == r {
            return m.top_left().to_a1();
        }
    }
    app.sel.active.to_a1()
}
