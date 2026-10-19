//! The Format Cells dialog (Number, Alignment, Font, Border, Fill, Protection).

use egui::{pos2, vec2, Align2, Color32, Context, Rect as ERect, RichText, Sense, Stroke, Ui};
use engine::address::Rect;
use engine::style::{BorderKind, BorderLine, Color, HAlign, Style, VAlign};
use engine::Value;

use crate::app::App;
use crate::numfmt_util as nf;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Number,
    Alignment,
    Font,
    Border,
    Fill,
    Protection,
}

const TABS: [(Tab, &str); 6] = [
    (Tab::Number, "Number"),
    (Tab::Alignment, "Alignment"),
    (Tab::Font, "Font"),
    (Tab::Border, "Border"),
    (Tab::Fill, "Fill"),
    (Tab::Protection, "Protection"),
];

const SYMBOLS: [&str; 5] = ["$", "€", "£", "¥", ""];

/// Edge indices for the border editor.
const TOP: usize = 0;
const BOTTOM: usize = 1;
const LEFT: usize = 2;
const RIGHT: usize = 3;
const INNER_H: usize = 4;
const INNER_V: usize = 5;

pub struct FormatState {
    pub tab: Tab,
    init: Style,
    st: Style,
    sample: Value,
    category: &'static str,
    decimals: u32,
    thousands: bool,
    symbol: &'static str,
    red_negative: bool,
    /// Border edges as shown in the preview, and whether the user changed them.
    edges: [Option<BorderLine>; 6],
    touched: [bool; 6],
    line_kind: BorderKind,
    line_color: Color,
    merge: Option<bool>,
}

impl FormatState {
    pub fn new(st: Style, sample: Value) -> FormatState {
        let code = st.num_fmt.clone();
        let symbol = SYMBOLS.iter().copied().find(|s| !s.is_empty() && code.contains(s)).unwrap_or("$");
        let b = st.borders;
        FormatState {
            tab: Tab::Number,
            category: nf::category_of(&code),
            decimals: nf::decimals_of(&code),
            thousands: code.contains(','),
            symbol,
            red_negative: code.contains("[Red]"),
            edges: [b.top, b.bottom, b.left, b.right, None, None],
            touched: [false; 6],
            line_kind: BorderKind::Thin,
            line_color: Color::BLACK,
            merge: None,
            init: st.clone(),
            st,
            sample,
        }
    }

    fn rebuild_number(&mut self) {
        self.st.num_fmt = match self.category {
            "General" => "General".into(),
            "Text" => "@".into(),
            "Number" | "Currency" | "Accounting" | "Percentage" | "Scientific" => {
                nf::build_number(self.category, self.decimals, self.thousands, self.symbol, self.red_negative)
            }
            "Date" => nf::DATE_FORMATS[0].into(),
            "Time" => nf::TIME_FORMATS[0].into(),
            "Fraction" => nf::FRACTION_FORMATS[0].into(),
            "Special" => nf::SPECIAL_FORMATS[0].1.into(),
            _ => self.st.num_fmt.clone(),
        };
    }
}

/// Show the dialog. Returns whether it stays open.
pub fn show(app: &mut App, ctx: &Context, st: &mut Box<FormatState>) -> bool {
    if st.merge.is_none() {
        st.merge = Some(app.is_merged_selection());
    }
    let mut ok = false;
    let mut cancel = false;
    let open = crate::dialogs::window(ctx, "Format Cells", 520.0, |ui| {
        ui.horizontal(|ui| {
            for (t, label) in TABS {
                if ui.add(egui::Button::selectable(st.tab == t, label)).clicked() {
                    st.tab = t;
                }
            }
        });
        ui.separator();
        ui.allocate_ui(vec2(520.0, 330.0), |ui| {
            ui.set_min_size(vec2(520.0, 330.0));
            match st.tab {
                Tab::Number => number_tab(ui, st),
                Tab::Alignment => alignment_tab(ui, st),
                Tab::Font => font_tab(ui, st),
                Tab::Border => border_tab(ui, st),
                Tab::Fill => fill_tab(ui, st),
                Tab::Protection => protection_tab(ui, st),
            }
        });
        ui.separator();
        let (o, c) = crate::dialogs::ok_cancel(ui);
        ok = o;
        cancel = c;
    });
    if ok {
        apply(app, st);
        return false;
    }
    open && !cancel
}

// ----------------------------------------------------------------------
// tabs
// ----------------------------------------------------------------------

fn number_tab(ui: &mut Ui, st: &mut FormatState) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label("Category:");
            egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(2).show(ui, |ui| {
                ui.set_width(130.0);
                for c in nf::CATEGORIES {
                    if ui.add_sized(vec2(126.0, 18.0), egui::Button::selectable(st.category == c, c)).clicked() && st.category != c {
                        st.category = c;
                        st.rebuild_number();
                    }
                }
            });
        });
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.set_width(360.0);
            egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(6).show(ui, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new("Sample").small().color(theme::MUTED));
                let f = engine::numfmt::format_value(&st.sample, &st.st.num_fmt);
                let color = f.color.map(theme::to_c32).unwrap_or(theme::TEXT);
                let text = if f.text.is_empty() { " ".to_string() } else { f.text };
                ui.label(RichText::new(text).color(color));
            });
            ui.add_space(6.0);
            let mut changed = false;
            match st.category {
                "General" => {
                    ui.label("General format cells have no specific number format.");
                }
                "Number" | "Currency" | "Accounting" | "Percentage" | "Scientific" => {
                    ui.horizontal(|ui| {
                        ui.label("Decimal places:");
                        changed |= ui.add(egui::DragValue::new(&mut st.decimals).range(0..=30)).changed();
                    });
                    if st.category == "Number" {
                        changed |= ui.checkbox(&mut st.thousands, "Use 1000 Separator (,)").changed();
                    }
                    if st.category == "Currency" || st.category == "Accounting" {
                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            egui::ComboBox::from_id_salt("fc-symbol")
                                .selected_text(if st.symbol.is_empty() { "None" } else { st.symbol })
                                .show_ui(ui, |ui| {
                                    for s in SYMBOLS {
                                        changed |= ui.selectable_value(&mut st.symbol, s, if s.is_empty() { "None" } else { s }).changed();
                                    }
                                });
                        });
                    }
                    if st.category == "Number" || st.category == "Currency" {
                        changed |= ui.checkbox(&mut st.red_negative, "Show negative numbers in red").changed();
                    }
                    if changed {
                        st.rebuild_number();
                    }
                }
                "Date" => code_list(ui, st, &nf::DATE_FORMATS.map(|c| (c, c))),
                "Time" => code_list(ui, st, &nf::TIME_FORMATS.map(|c| (c, c))),
                "Fraction" => code_list(ui, st, &nf::FRACTION_FORMATS.map(|c| (c, c))),
                "Special" => code_list(ui, st, &nf::SPECIAL_FORMATS.map(|(l, c)| (l, c))),
                "Text" => {
                    ui.label("Text format cells are treated as text even when a number is in the cell.\nThe cell is displayed exactly as entered.");
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        ui.add(egui::TextEdit::singleline(&mut st.st.num_fmt).desired_width(280.0));
                    });
                    let list: Vec<(&str, &str)> = nf::CUSTOM_FORMATS.iter().map(|c| (*c, *c)).collect();
                    code_list(ui, st, &list);
                }
            }
        });
    });
}

fn code_list(ui: &mut Ui, st: &mut FormatState, items: &[(&str, &str)]) {
    ui.label("Type:");
    egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(2).show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(190.0).id_salt("fc-codes").show(ui, |ui| {
            ui.set_width(336.0);
            for (label, code) in items {
                let shown = if label == code {
                    let f = engine::numfmt::format_value(&st.sample, code);
                    if matches!(st.sample, Value::Number(_)) && st.category != "Custom" { f.text } else { code.to_string() }
                } else {
                    label.to_string()
                };
                if ui.add_sized(vec2(332.0, 18.0), egui::Button::selectable(st.st.num_fmt == *code, shown)).clicked() {
                    st.st.num_fmt = code.to_string();
                }
            }
        });
    });
}

fn alignment_tab(ui: &mut Ui, st: &mut FormatState) {
    let h_opts = [
        (HAlign::General, "General"),
        (HAlign::Left, "Left (Indent)"),
        (HAlign::Center, "Center"),
        (HAlign::Right, "Right (Indent)"),
        (HAlign::Fill, "Fill"),
        (HAlign::Justify, "Justify"),
        (HAlign::CenterAcross, "Center Across Selection"),
    ];
    let v_opts = [(VAlign::Top, "Top"), (VAlign::Center, "Center"), (VAlign::Bottom, "Bottom")];
    ui.label(RichText::new("Text alignment").strong());
    egui::Grid::new("fc-align").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
        ui.label("Horizontal:");
        egui::ComboBox::from_id_salt("fc-h")
            .width(200.0)
            .selected_text(h_opts.iter().find(|(h, _)| *h == st.st.h_align).map(|(_, l)| *l).unwrap_or(""))
            .show_ui(ui, |ui| {
                for (h, l) in h_opts {
                    ui.selectable_value(&mut st.st.h_align, h, l);
                }
            });
        ui.end_row();
        ui.label("Vertical:");
        egui::ComboBox::from_id_salt("fc-v")
            .width(200.0)
            .selected_text(v_opts.iter().find(|(v, _)| *v == st.st.v_align).map(|(_, l)| *l).unwrap_or(""))
            .show_ui(ui, |ui| {
                for (v, l) in v_opts {
                    ui.selectable_value(&mut st.st.v_align, v, l);
                }
            });
        ui.end_row();
        ui.label("Indent:");
        ui.add(egui::DragValue::new(&mut st.st.indent).range(0..=15));
        ui.end_row();
        ui.label("Orientation (degrees):");
        ui.add(egui::Slider::new(&mut st.st.rotation, -90..=90));
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.label(RichText::new("Text control").strong());
    ui.checkbox(&mut st.st.wrap, "Wrap text");
    if let Some(m) = st.merge.as_mut() {
        ui.checkbox(m, "Merge cells");
    }
}

fn font_tab(ui: &mut Ui, st: &mut FormatState) {
    let styles = ["Regular", "Italic", "Bold", "Bold Italic"];
    let cur_style = (st.st.bold as usize) * 2 + st.st.italic as usize;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label("Font:");
            let name = st.st.font_name.clone().unwrap_or_else(|| engine::style::DEFAULT_FONT.to_string());
            egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(2).show(ui, |ui| {
                ui.set_width(180.0);
                for f in crate::fonts::FONT_NAMES {
                    if ui.add_sized(vec2(176.0, 18.0), egui::Button::selectable(name.eq_ignore_ascii_case(f), f)).clicked() {
                        st.st.font_name = if f == engine::style::DEFAULT_FONT { None } else { Some(f.to_string()) };
                    }
                }
            });
        });
        ui.vertical(|ui| {
            ui.label("Font style:");
            egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(2).show(ui, |ui| {
                ui.set_width(110.0);
                for (i, s) in styles.iter().enumerate() {
                    if ui.add_sized(vec2(106.0, 18.0), egui::Button::selectable(cur_style == i, *s)).clicked() {
                        st.st.bold = i >= 2;
                        st.st.italic = i % 2 == 1;
                    }
                }
            });
        });
        ui.vertical(|ui| {
            ui.label("Size:");
            let mut size = st.st.font_size();
            if ui.add(egui::DragValue::new(&mut size).range(1.0..=409.0).speed(0.5)).changed() {
                st.st.set_font_size(size);
            }
            egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(2).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(140.0).id_salt("fc-sizes").show(ui, |ui| {
                    ui.set_width(60.0);
                    for s in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0] {
                        if ui.add_sized(vec2(56.0, 18.0), egui::Button::selectable((st.st.font_size() - s).abs() < 0.05, format!("{}", s))).clicked() {
                            st.st.set_font_size(s);
                        }
                    }
                });
            });
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut st.st.underline, "Underline");
        ui.checkbox(&mut st.st.strike, "Strikethrough");
        ui.add_space(20.0);
        ui.label("Color:");
        let cur = st.st.font_color.unwrap_or(Color::BLACK);
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, theme::to_c32(cur));
        ui.menu_button("▾", |ui| {
            if let Some(c) = crate::ribbon::palette(ui, Some("Automatic")) {
                st.st.font_color = if c.0 == 0xFFFF_FFFF { None } else { Some(c) };
                ui.close();
            }
        });
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Preview").small().color(theme::MUTED));
    let (r, _) = ui.allocate_exact_size(vec2(500.0, 60.0), Sense::hover());
    let p = ui.painter_at(r);
    p.rect(r, 0.0, Color32::WHITE, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    let (font, fake_bold) = crate::fonts::cell_font(st.st.font_name.as_deref(), st.st.bold, st.st.font_size() * 1.333);
    let color = st.st.font_color.map(theme::to_c32).unwrap_or(Color32::BLACK);
    let name = st.st.font_name.clone().unwrap_or_else(|| engine::style::DEFAULT_FONT.to_string());
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &name,
        0.0,
        egui::TextFormat {
            font_id: font,
            color,
            italics: st.st.italic,
            underline: if st.st.underline { Stroke::new(1.0, color) } else { Stroke::NONE },
            strikethrough: if st.st.strike { Stroke::new(1.0, color) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    let g = p.layout_job(job);
    let pos = r.center() - g.size() / 2.0;
    p.galley(pos, g.clone(), color);
    if fake_bold {
        p.galley(pos + vec2(0.6, 0.0), g, color);
    }
}

fn kind_stroke(kind: BorderKind) -> f32 {
    match kind {
        BorderKind::Thin | BorderKind::Dashed | BorderKind::Dotted => 1.0,
        BorderKind::Medium | BorderKind::Double => 2.0,
        BorderKind::Thick => 3.0,
    }
}

fn paint_line(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, line: BorderLine) {
    let c = theme::to_c32(line.color);
    let s = Stroke::new(kind_stroke(line.kind), c);
    match line.kind {
        BorderKind::Dashed => {
            p.extend(egui::Shape::dashed_line(&[a, b], s, 5.0, 3.0));
        }
        BorderKind::Dotted => {
            p.extend(egui::Shape::dashed_line(&[a, b], s, 1.5, 2.0));
        }
        BorderKind::Double => {
            let d = if a.x == b.x { vec2(1.5, 0.0) } else { vec2(0.0, 1.5) };
            p.line_segment([a - d, b - d], Stroke::new(1.0, c));
            p.line_segment([a + d, b + d], Stroke::new(1.0, c));
        }
        _ => {
            p.line_segment([a, b], s);
        }
    }
}

fn border_tab(ui: &mut Ui, st: &mut FormatState) {
    let kinds = [
        (BorderKind::Thin, "Thin"),
        (BorderKind::Medium, "Medium"),
        (BorderKind::Thick, "Thick"),
        (BorderKind::Dashed, "Dashed"),
        (BorderKind::Dotted, "Dotted"),
        (BorderKind::Double, "Double"),
    ];
    let line = BorderLine { kind: st.line_kind, color: st.line_color };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(130.0);
            ui.label(RichText::new("Line").strong());
            ui.label("Style:");
            for (k, l) in kinds {
                let (r, resp) = ui.allocate_exact_size(vec2(120.0, 20.0), Sense::click());
                if st.line_kind == k {
                    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, theme::ACCENT_LIGHT);
                }
                paint_line(ui.painter(), pos2(r.left() + 6.0, r.center().y), pos2(r.right() - 6.0, r.center().y), BorderLine { kind: k, color: Color::BLACK });
                if resp.on_hover_text(l).clicked() {
                    st.line_kind = k;
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Color:");
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, theme::to_c32(st.line_color));
                ui.menu_button("▾", |ui| {
                    if let Some(c) = crate::ribbon::palette(ui, Some("Automatic")) {
                        st.line_color = if c.0 == 0xFFFF_FFFF { Color::BLACK } else { c };
                        ui.close();
                    }
                });
            });
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Presets").strong());
            ui.horizontal(|ui| {
                if ui.button("None").clicked() {
                    for i in 0..6 {
                        st.edges[i] = None;
                        st.touched[i] = true;
                    }
                }
                if ui.button("Outline").clicked() {
                    for i in [TOP, BOTTOM, LEFT, RIGHT] {
                        st.edges[i] = Some(line);
                        st.touched[i] = true;
                    }
                }
                if ui.button("Inside").clicked() {
                    for i in [INNER_H, INNER_V] {
                        st.edges[i] = Some(line);
                        st.touched[i] = true;
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Border").strong());
            ui.label(RichText::new("Click the preview edges or the buttons to apply borders.").small().color(theme::MUTED));
            ui.add_space(4.0);
            let (area, resp) = ui.allocate_exact_size(vec2(240.0, 160.0), Sense::click());
            let p = ui.painter_at(area.expand(4.0));
            let r = area.shrink(20.0);
            p.rect_filled(r, 0.0, Color32::WHITE);
            // corner guides
            for c in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
                p.circle_filled(c, 1.5, Color32::from_gray(160));
            }
            p.text(r.center() - vec2(r.width() / 4.0, 0.0), Align2::CENTER_CENTER, "Text", egui::FontId::proportional(12.0), Color32::from_gray(120));
            p.text(r.center() + vec2(r.width() / 4.0, 0.0), Align2::CENTER_CENTER, "Text", egui::FontId::proportional(12.0), Color32::from_gray(120));
            let segs: [(usize, egui::Pos2, egui::Pos2); 6] = [
                (TOP, r.left_top(), r.right_top()),
                (BOTTOM, r.left_bottom(), r.right_bottom()),
                (LEFT, r.left_top(), r.left_bottom()),
                (RIGHT, r.right_top(), r.right_bottom()),
                (INNER_H, r.left_center(), r.right_center()),
                (INNER_V, r.center_top(), r.center_bottom()),
            ];
            for (i, a, b) in segs {
                if let Some(l) = st.edges[i] {
                    paint_line(&p, a, b, l);
                }
            }
            if resp.clicked() {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let near = |a: egui::Pos2, b: egui::Pos2| {
                        let seg = ERect::from_two_pos(a, b).expand(8.0);
                        seg.contains(pp)
                    };
                    if let Some((i, _, _)) = segs.iter().find(|(_, a, b)| near(*a, *b)) {
                        st.edges[*i] = if st.edges[*i] == Some(line) { None } else { Some(line) };
                        st.touched[*i] = true;
                    }
                }
            }
            ui.horizontal(|ui| {
                for (i, label) in [(TOP, "Top"), (INNER_H, "Middle"), (BOTTOM, "Bottom"), (LEFT, "Left"), (INNER_V, "Center"), (RIGHT, "Right")] {
                    if ui.add(egui::Button::selectable(st.edges[i].is_some(), label)).clicked() {
                        st.edges[i] = if st.edges[i].is_some() { None } else { Some(line) };
                        st.touched[i] = true;
                    }
                }
            });
        });
    });
}

fn fill_tab(ui: &mut Ui, st: &mut FormatState) {
    ui.label(RichText::new("Background Color:").strong());
    if ui.button("No Color").clicked() {
        st.st.fill = None;
    }
    if let Some(c) = crate::ribbon::palette(ui, None) {
        st.st.fill = Some(c);
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Sample").small().color(theme::MUTED));
    let (r, _) = ui.allocate_exact_size(vec2(300.0, 40.0), Sense::hover());
    ui.painter().rect(r, 0.0, st.st.fill.map(theme::to_c32).unwrap_or(Color32::WHITE), Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
}

fn protection_tab(ui: &mut Ui, st: &mut FormatState) {
    ui.checkbox(&mut st.st.locked, "Locked");
    ui.add_space(8.0);
    ui.label(
        RichText::new("Locking cells has no effect until you protect the worksheet (Review tab, Protect Sheet).")
            .color(theme::MUTED),
    );
}

// ----------------------------------------------------------------------
// apply
// ----------------------------------------------------------------------

/// Apply only the properties the user changed, so a multi-cell selection with
/// mixed formatting keeps everything that was left alone.
fn apply(app: &mut App, st: &FormatState) {
    let a = &st.init;
    let b = st.st.clone();
    let font_changed = a.font_name != b.font_name;
    let size_changed = a.font_size_tenths != b.font_size_tenths;
    let style_changed = a.bold != b.bold || a.italic != b.italic;
    let any_style = a != &b;
    let any_border = st.touched.iter().any(|t| *t);
    let merge_change = st.merge.is_some_and(|m| m != app.is_merged_selection());
    if !any_style && !any_border && !merge_change {
        return;
    }
    if any_style || any_border {
        let rects = app.sel_rects_bounded();
        let scope = app.cells_scope();
        let init = a.clone();
        let edges = st.edges;
        let touched = st.touched;
        app.modify("Format Cells", scope, |wb, s| {
            for r in &rects {
                if any_style {
                    wb.update_style(s, *r, |x| {
                        if init.num_fmt != b.num_fmt {
                            x.num_fmt = b.num_fmt.clone();
                        }
                        if init.h_align != b.h_align {
                            x.h_align = b.h_align;
                        }
                        if init.v_align != b.v_align {
                            x.v_align = b.v_align;
                        }
                        if init.wrap != b.wrap {
                            x.wrap = b.wrap;
                        }
                        if init.indent != b.indent {
                            x.indent = b.indent;
                        }
                        if init.rotation != b.rotation {
                            x.rotation = b.rotation;
                        }
                        if font_changed {
                            x.font_name = b.font_name.clone();
                        }
                        if size_changed {
                            x.font_size_tenths = b.font_size_tenths;
                        }
                        if style_changed {
                            x.bold = b.bold;
                            x.italic = b.italic;
                        }
                        if init.underline != b.underline {
                            x.underline = b.underline;
                        }
                        if init.strike != b.strike {
                            x.strike = b.strike;
                        }
                        if init.font_color != b.font_color {
                            x.font_color = b.font_color;
                        }
                        if init.fill != b.fill {
                            x.fill = b.fill;
                        }
                        if init.locked != b.locked {
                            x.locked = b.locked;
                        }
                    });
                }
                if any_border {
                    apply_edges(wb, s, *r, &edges, &touched);
                }
            }
        });
    }
    if merge_change {
        if st.merge == Some(true) {
            app.merge(false, false);
        } else {
            app.unmerge();
        }
    }
}

fn apply_edges(wb: &mut engine::workbook::Workbook, s: usize, r: Rect, edges: &[Option<BorderLine>; 6], touched: &[bool; 6]) {
    let row = |row: u32| Rect { top: row, bottom: row, ..r };
    let col = |col: u32| Rect { left: col, right: col, ..r };
    if touched[TOP] {
        let e = edges[TOP];
        wb.update_style(s, row(r.top), |x| x.borders.top = e);
    }
    if touched[BOTTOM] {
        let e = edges[BOTTOM];
        wb.update_style(s, row(r.bottom), |x| x.borders.bottom = e);
    }
    if touched[LEFT] {
        let e = edges[LEFT];
        wb.update_style(s, col(r.left), |x| x.borders.left = e);
    }
    if touched[RIGHT] {
        let e = edges[RIGHT];
        wb.update_style(s, col(r.right), |x| x.borders.right = e);
    }
    if touched[INNER_H] && r.bottom > r.top {
        let e = edges[INNER_H];
        wb.update_style(s, Rect { bottom: r.bottom - 1, ..r }, |x| x.borders.bottom = e);
        wb.update_style(s, Rect { top: r.top + 1, ..r }, |x| x.borders.top = e);
    }
    if touched[INNER_V] && r.right > r.left {
        let e = edges[INNER_V];
        wb.update_style(s, Rect { right: r.right - 1, ..r }, |x| x.borders.right = e);
        wb.update_style(s, Rect { left: r.left + 1, ..r }, |x| x.borders.left = e);
    }
}
