//! Dialog windows, the Find & Replace panel, autofilter popup and context menus.

use std::collections::{BTreeSet, HashSet};

use egui::{vec2, Align2, Color32, Context, RichText, Ui};
use engine::address::{CellPos, Rect, MAX_COLS, MAX_ROWS};
use engine::ops::{self, FindOptions, PasteOp, PasteOptions, PasteWhat, Scope, SortKey};
use engine::style::Color;
use engine::workbook::{CfOperator, CfRule, CfStyle, Chart, ChartKind, ConditionalFormat, DataValidation, ValidationRule};

use crate::app::{App, Pending};
use crate::grid::CtxTarget;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfKind {
    Greater,
    Less,
    Between,
    Equal,
    TextContains,
    Duplicates,
    Top10,
    Bottom10,
    AboveAverage,
    BelowAverage,
    Formula,
}

/// Preset formats offered by the highlight-rules dialogs.
pub const CF_PRESETS: [(&str, Option<u32>, Option<u32>); 6] = [
    ("Light Red Fill with Dark Red Text", Some(0xFFC7CE), Some(0x9C0006)),
    ("Yellow Fill with Dark Yellow Text", Some(0xFFEB9C), Some(0x9C5700)),
    ("Green Fill with Dark Green Text", Some(0xC6EFCE), Some(0x006100)),
    ("Light Red Fill", Some(0xFFC7CE), None),
    ("Red Text", None, Some(0x9C0006)),
    ("Bold Text", None, None),
];

pub enum Dialog {
    Message { title: String, text: String },
    SaveChanges(Pending),
    FormatCells(Box<crate::format_dialog::FormatState>),
    PasteSpecial(PasteOptions),
    GoTo { text: String },
    InsertFunction { search: String, category: String, selected: Option<&'static str> },
    NameManager { selected: Option<usize>, name: String, refers: String, error: String },
    Sort { range: Rect, header: bool, keys: Vec<(u32, bool)> },
    Validation { range: Rect, kind: usize, list: String, min: String, max: String },
    TextToColumns { tab: bool, comma: bool, semicolon: bool, space: bool, other: String, consecutive: bool },
    RemoveDuplicates { range: Rect, header: bool, cols: Vec<(u32, bool)> },
    GoalSeek { set_cell: String, to_value: String, changing: String },
    CondFormat { kind: CfKind, a: String, b: String, preset: usize },
    CfManager { selected: Option<usize> },
    Chart { idx: usize, kind: ChartKind, title: String, range: String, header: bool, by_cols: bool },
    Comment { cell: CellPos, text: String },
    Hyperlink { cell: CellPos, url: String, display: String },
    Size { rows: bool, value: String },
    Zoom { value: u32 },
}

impl Dialog {
    pub fn is_modal(&self) -> bool {
        true
    }
    pub fn name_manager() -> Dialog {
        Dialog::NameManager { selected: None, name: String::new(), refers: String::new(), error: String::new() }
    }
    pub fn define_name(app: &App) -> Dialog {
        let r = app.sel.primary();
        let refers = format!("={}!{}", engine::address::quote_sheet_name(&app.sheet_ref().name), abs(r));
        Dialog::NameManager { selected: None, name: String::new(), refers, error: String::new() }
    }
    pub fn cond_format(kind: CfKind) -> Dialog {
        let (a, b) = match kind {
            CfKind::Top10 | CfKind::Bottom10 => ("10".to_string(), String::new()),
            _ => (String::new(), String::new()),
        };
        Dialog::CondFormat { kind, a, b, preset: 0 }
    }
    pub fn validation(range: Rect, existing: Option<ValidationRule>) -> Dialog {
        let fmt = |x: f64| if x.is_finite() { engine::value::format_number_general(x) } else { String::new() };
        match existing {
            Some(ValidationRule::List(v)) => Dialog::Validation { range, kind: 1, list: v.join(","), min: String::new(), max: String::new() },
            Some(ValidationRule::ListRange(r)) => Dialog::Validation { range, kind: 1, list: format!("={}", abs(r)), min: String::new(), max: String::new() },
            Some(ValidationRule::WholeNumber { min, max }) => Dialog::Validation { range, kind: 2, list: String::new(), min: fmt(min), max: fmt(max) },
            Some(ValidationRule::Decimal { min, max }) => Dialog::Validation { range, kind: 3, list: String::new(), min: fmt(min), max: fmt(max) },
            Some(ValidationRule::TextLength { min, max }) => Dialog::Validation { range, kind: 4, list: String::new(), min: fmt(min), max: fmt(max) },
            None => Dialog::Validation { range, kind: 0, list: String::new(), min: String::new(), max: String::new() },
        }
    }
    pub fn remove_duplicates(app: &App) -> Dialog {
        let r = app.data_range();
        let header = ops::guess_header(&app.doc.wb, app.sheet, r);
        Dialog::RemoveDuplicates { range: r, header, cols: (r.left..=r.right.min(r.left + 50)).map(|c| (c, true)).collect() }
    }
    pub fn chart(idx: usize, c: &Chart) -> Dialog {
        Dialog::Chart { idx, kind: c.kind, title: c.title.clone(), range: c.data.to_a1(), header: c.has_header, by_cols: c.series_in_columns }
    }
}

fn abs(r: Rect) -> String {
    let a = |p: CellPos| format!("${}${}", engine::address::col_to_letters(p.col), p.row + 1);
    if r.cell_count() == 1 {
        a(r.top_left())
    } else {
        format!("{}:{}", a(r.top_left()), a(r.bottom_right()))
    }
}

#[derive(Default)]
pub struct FindState {
    pub query: String,
    pub replace: String,
    pub match_case: bool,
    pub entire: bool,
    pub in_formulas: bool,
    pub workbook: bool,
    pub replace_mode: bool,
    pub results: Vec<(usize, CellPos)>,
    pub focus: bool,
    pub message: String,
}

impl FindState {
    fn options(&self, sheet: usize) -> FindOptions {
        FindOptions { query: self.query.clone(), match_case: self.match_case, entire_cell: self.entire, in_formulas: self.in_formulas, sheet: if self.workbook { None } else { Some(sheet) } }
    }
}

pub struct FilterPopup {
    pub col: u32,
    pub pos: egui::Pos2,
    pub values: Vec<(String, bool)>,
    pub search: String,
}

impl FilterPopup {
    pub fn new(app: &App, col: u32, pos: egui::Pos2) -> FilterPopup {
        let af = app.sheet_ref().autofilter.clone().unwrap();
        let off = col - af.range.left;
        let allowed = af.allowed.get(&off);
        let bottom = app.sheet_ref().data_extent().map(|e| e.bottom.max(af.range.bottom)).unwrap_or(af.range.bottom);
        let mut set = BTreeSet::new();
        for r in af.range.top + 1..=bottom.min(af.range.top + 100_000) {
            set.insert(crate::grid::display_text(&app.doc.wb, app.sheet, CellPos::new(r, col)));
        }
        let values = set.into_iter().map(|v| {
            let on = allowed.map(|a| a.contains(&v)).unwrap_or(true);
            (v, on)
        }).collect();
        FilterPopup { col, pos, values, search: String::new() }
    }
}

impl App {
    pub fn add_cf(&mut self, rule: CfRule) {
        let rects = self.sel_rects_bounded();
        self.modify("Conditional Formatting", Scope::Sheet(self.sheet), |wb, s| {
            for r in rects {
                wb.sheets[s].cond_formats.push(ConditionalFormat { range: r, rule: rule.clone() });
            }
        });
    }

    pub fn clear_filter(&mut self) {
        self.modify("Clear Filter", Scope::Sheet(self.sheet), |wb, s| {
            let sh = &mut wb.sheets[s];
            if let Some(af) = sh.autofilter.as_mut() {
                af.allowed.clear();
                for r in af.range.top..=af.range.bottom {
                    sh.hidden_rows.remove(&r);
                }
            }
        });
    }

    fn apply_filter_now(&mut self, col_off: u32, allowed: Option<HashSet<String>>) {
        let sheet = self.sheet;
        self.modify("Filter", Scope::Sheet(sheet), |wb, s| {
            let bottom = wb.sheets[s].data_extent().map(|e| e.bottom);
            {
                let sh = &mut wb.sheets[s];
                let Some(af) = sh.autofilter.as_mut() else { return };
                if let Some(b) = bottom {
                    af.range.bottom = af.range.bottom.max(b);
                }
                match allowed {
                    Some(a) => {
                        af.allowed.insert(col_off, a);
                    }
                    None => {
                        af.allowed.remove(&col_off);
                    }
                }
            }
            let wbc = wb.clone();
            let f = move |p: CellPos| crate::grid::display_text(&wbc, s, p);
            App::apply_filter(&mut wb.sheets[s], &f);
        });
    }
}

// ======================================================================
// entry point
// ======================================================================

pub fn show(app: &mut App, ctx: &Context) {
    if let Some(mut d) = app.dialog.take() {
        let keep = show_dialog(app, ctx, &mut d);
        if keep && app.dialog.is_none() {
            app.dialog = Some(d);
        }
    }
    find_dialog(app, ctx);
    crate::spell::show(app, ctx);
    filter_popup(app, ctx);
}

pub(crate) fn window(ctx: &Context, title: &str, width: f32, add: impl FnOnce(&mut Ui)) -> bool {
    let mut open = true;
    let center = ctx.content_rect().center();
    egui::Window::new(title)
        .id(egui::Id::unique(("rexcel-dialog", title)))
        .collapsible(false)
        .resizable(false)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(center)
        .default_width(width)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(width);
            add(ui);
        });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    open && !esc
}

pub(crate) fn ok_cancel(ui: &mut Ui) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.add_space(6.0);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() {
            cancel = true;
        }
        if ui.add(egui::Button::new(RichText::new("OK").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(72.0, 24.0))).clicked() {
            ok = true;
        }
    });
    if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ui.memory(|m| m.focused().is_some_and(|f| f == egui::Id::unique("multiline-dialog"))) {
        ok = true;
    }
    (ok, cancel)
}

fn parse_cell(app: &App, t: &str) -> Option<CellPos> {
    crate::editor::parse_goto(t).map(|(_, r)| r.top_left()).or_else(|| {
        app.doc.wb.find_name(t.trim(), app.sheet).and_then(|n| if let engine::ast::Expr::Ref(r) = &n.formula { Some(r.rect().top_left()) } else { None })
    })
}

fn show_dialog(app: &mut App, ctx: &Context, d: &mut Dialog) -> bool {
    match d {
        Dialog::Message { title, text } => {
            let t = text.clone();
            let mut done = false;
            let open = window(ctx, &title.clone(), 300.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ℹ").size(28.0).color(Color32::from_rgb(0x2B, 0x6C, 0xC4)));
                    ui.label(t);
                });
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("OK").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(72.0, 24.0))).clicked() {
                        done = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = true;
                }
            });
            if !open || done {
                if let Some(e) = app.edit.as_mut() {
                    e.request_focus = true;
                }
                return false;
            }
            true
        }
        Dialog::SaveChanges(p) => {
            let mut choice = 0;
            let name = app.doc.title();
            let open = window(ctx, "RExcel", 360.0, |ui| {
                ui.label(RichText::new(format!("Want to save your changes to '{}'?", name)).size(15.0));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Save").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(80.0, 26.0))).clicked() {
                        choice = 1;
                    }
                    if ui.add(egui::Button::new("Don't Save").min_size(vec2(80.0, 26.0))).clicked() {
                        choice = 2;
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(80.0, 26.0))).clicked() {
                        choice = 3;
                    }
                });
            });
            match choice {
                1 => {
                    if app.save() {
                        app.doc.dirty = false;
                        app.do_pending(p.clone());
                    }
                    false
                }
                2 => {
                    app.doc.dirty = false;
                    app.do_pending(p.clone());
                    false
                }
                3 => false,
                _ => open,
            }
        }
        Dialog::FormatCells(st) => crate::format_dialog::show(app, ctx, st),
        Dialog::PasteSpecial(o) => {
            let mut res = (false, false);
            let open = window(ctx, "Paste Special", 360.0, |ui| {
                ui.columns(2, |cols| {
                    cols[0].label(RichText::new("Paste").strong());
                    for (l, w) in [("All", PasteWhat::All), ("Formulas", PasteWhat::Formulas), ("Values", PasteWhat::Values), ("Formats", PasteWhat::Formats), ("Values and number formats", PasteWhat::ValuesAndFormats), ("All except borders", PasteWhat::AllExceptBorders)] {
                        cols[0].radio_value(&mut o.what, w, l);
                    }
                    cols[1].label(RichText::new("Operation").strong());
                    for (l, w) in [("None", PasteOp::None), ("Add", PasteOp::Add), ("Subtract", PasteOp::Subtract), ("Multiply", PasteOp::Multiply), ("Divide", PasteOp::Divide)] {
                        cols[1].radio_value(&mut o.op, w, l);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut o.skip_blanks, "Skip blanks");
                    ui.checkbox(&mut o.transpose, "Transpose");
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                if app.clip.as_ref().map(|c| c.cut).unwrap_or(false) {
                    app.message("Paste Special", "Paste Special isn't available after Cut. Use Copy instead.");
                } else if app.clip.is_some() {
                    app.paste_clip(*o);
                } else {
                    app.message("Paste Special", "Copy something first.");
                }
                return false;
            }
            open && !res.1
        }
        Dialog::GoTo { text } => {
            let mut res = (false, false);
            let names: Vec<String> = app.doc.wb.names.iter().map(|n| n.name.clone()).collect();
            let open = window(ctx, "Go To", 280.0, |ui| {
                ui.label("Go to:");
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    for n in &names {
                        if ui.selectable_label(text == n, n).clicked() {
                            *text = n.clone();
                        }
                    }
                });
                ui.label("Reference:");
                let r = ui.text_edit_singleline(text);
                if !r.has_focus() && text.is_empty() {
                    r.request_focus();
                }
                res = ok_cancel(ui);
            });
            if res.0 {
                let t = text.clone();
                if !app.goto(&t) {
                    app.message("Go To", "Reference isn't valid.");
                }
                return false;
            }
            open && !res.1
        }
        Dialog::InsertFunction { search, category, selected } => {
            let mut res = (false, false);
            let mut cats: Vec<&str> = app.functions.iter().map(|f| f.category).collect();
            cats.sort();
            cats.dedup();
            let fns = app.functions.clone();
            let open = window(ctx, "Insert Function", 440.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search for a function:");
                    ui.text_edit_singleline(search);
                });
                ui.horizontal(|ui| {
                    ui.label("Or select a category:");
                    egui::ComboBox::from_id_salt("if-cat").selected_text(category.as_str()).width(180.0).show_ui(ui, |ui| {
                        for c in std::iter::once("All").chain(cats.iter().copied()) {
                            if ui.selectable_label(category == c, c).clicked() {
                                *category = c.to_string();
                            }
                        }
                    });
                });
                ui.label("Select a function:");
                let q = search.to_uppercase();
                let list: Vec<&'static engine::functions::FnDef> = fns
                    .iter()
                    .copied()
                    .filter(|f| (category == "All" || f.category == category) && (q.is_empty() || f.name.contains(&q) || f.description.to_uppercase().contains(&q)))
                    .collect();
                egui::Frame::new().stroke(egui::Stroke::new(1.0, theme::BORDER)).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(200.0).auto_shrink([false, false]).show(ui, |ui| {
                        for f in &list {
                            let r = ui.selectable_label(*selected == Some(f.name), f.name);
                            if r.clicked() {
                                *selected = Some(f.name);
                            }
                            if r.double_clicked() {
                                *selected = Some(f.name);
                                res.0 = true;
                            }
                        }
                    });
                });
                if let Some(f) = selected.and_then(engine::functions::lookup) {
                    ui.label(RichText::new(f.syntax).strong());
                    ui.label(f.description);
                } else if let Some(f) = list.first() {
                    if selected.is_none() {
                        ui.label(RichText::new(f.syntax).strong().color(theme::MUTED));
                    }
                }
                let r = ok_cancel(ui);
                res.0 |= r.0;
                res.1 = r.1;
            });
            if res.0 {
                if let Some(n) = *selected {
                    app.insert_function_text(n);
                }
                return false;
            }
            open && !res.1
        }
        Dialog::NameManager { selected, name, refers, error } => {
            let mut close = false;
            let sheet_names: Vec<String> = app.doc.wb.sheets.iter().map(|s| s.name.clone()).collect();
            let rows: Vec<(String, String, String)> = app
                .doc
                .wb
                .names
                .iter()
                .map(|n| (n.name.clone(), format!("={}", n.formula.to_formula()), n.scope.map(|s| sheet_names.get(s).cloned().unwrap_or_default()).unwrap_or_else(|| "Workbook".into())))
                .collect();
            let mut action: Option<u8> = None;
            let open = window(ctx, "Name Manager", 520.0, |ui| {
                egui::Grid::new("names").striped(true).num_columns(3).min_col_width(120.0).show(ui, |ui| {
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Refers To").strong());
                    ui.label(RichText::new("Scope").strong());
                    ui.end_row();
                    for (i, (n, f, s)) in rows.iter().enumerate() {
                        if ui.selectable_label(*selected == Some(i), n).clicked() {
                            *selected = Some(i);
                            *name = n.clone();
                            *refers = f.clone();
                        }
                        ui.label(f);
                        ui.label(s);
                        ui.end_row();
                    }
                });
                if rows.is_empty() {
                    ui.label(RichText::new("No names defined yet.").color(theme::MUTED));
                }
                ui.separator();
                egui::Grid::new("name-edit").num_columns(2).show(ui, |ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(name);
                    ui.end_row();
                    ui.label("Refers to:");
                    ui.add(egui::TextEdit::singleline(refers).desired_width(320.0));
                    ui.end_row();
                });
                if !error.is_empty() {
                    ui.colored_label(Color32::from_rgb(0xC0, 0x30, 0x30), error.as_str());
                }
                ui.horizontal(|ui| {
                    if ui.button(if selected.is_some() { "Update" } else { "Add" }).clicked() {
                        action = Some(1);
                    }
                    if ui.button("New").clicked() {
                        action = Some(3);
                    }
                    if ui.add_enabled(selected.is_some(), egui::Button::new("Delete")).clicked() {
                        action = Some(2);
                    }
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
            match action {
                Some(1) => {
                    let mut test = app.doc.wb.clone();
                    if let Some(i) = *selected {
                        if i < test.names.len() {
                            test.names.remove(i);
                        }
                    }
                    match test.define_name(name.trim(), None, refers) {
                        Ok(()) => {
                            let (n, r, sel) = (name.trim().to_string(), refers.clone(), *selected);
                            app.modify("Define Name", Scope::Workbook, |wb, _| {
                                if let Some(i) = sel {
                                    if i < wb.names.len() {
                                        wb.names.remove(i);
                                    }
                                }
                                let _ = wb.define_name(&n, None, &r);
                            });
                            *selected = None;
                            name.clear();
                            refers.clear();
                            error.clear();
                        }
                        Err(e) => *error = e,
                    }
                }
                Some(2) => {
                    if let Some(i) = selected.take() {
                        app.modify("Delete Name", Scope::Workbook, |wb, _| {
                            if i < wb.names.len() {
                                wb.names.remove(i);
                            }
                        });
                    }
                    name.clear();
                    refers.clear();
                }
                Some(3) => {
                    *selected = None;
                    name.clear();
                    *refers = format!("={}!{}", engine::address::quote_sheet_name(&app.sheet_ref().name), abs(app.sel.primary()));
                }
                _ => {}
            }
            open && !close
        }
        Dialog::Sort { range, header, keys } => {
            let mut res = (false, false);
            let r = *range;
            let col_name = |app: &App, c: u32, header: bool| -> String {
                if header {
                    let t = crate::grid::display_text(&app.doc.wb, app.sheet, CellPos::new(r.top, c));
                    if !t.is_empty() {
                        return t;
                    }
                }
                format!("Column {}", engine::address::col_to_letters(c))
            };
            let names: Vec<(u32, String)> = (r.left..=r.right.min(r.left + 100)).map(|c| (c, col_name(app, c, *header))).collect();
            let open = window(ctx, "Sort", 460.0, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("+ Add Level").clicked() {
                        keys.push((r.left, true));
                    }
                    if ui.add_enabled(keys.len() > 1, egui::Button::new("− Delete Level")).clicked() {
                        keys.pop();
                    }
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(header, "My data has headers");
                    });
                });
                ui.separator();
                for (i, (c, asc)) in keys.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(if i == 0 { "Sort by    " } else { "Then by   " });
                        let cur = names.iter().find(|(x, _)| x == c).map(|x| x.1.clone()).unwrap_or_default();
                        egui::ComboBox::from_id_salt(("sortcol", i)).selected_text(cur).width(180.0).show_ui(ui, |ui| {
                            for (x, n) in &names {
                                ui.selectable_value(c, *x, n);
                            }
                        });
                        egui::ComboBox::from_id_salt(("sortord", i)).selected_text(if *asc { "A to Z" } else { "Z to A" }).width(100.0).show_ui(ui, |ui| {
                            ui.selectable_value(asc, true, "A to Z / Smallest to Largest");
                            ui.selectable_value(asc, false, "Z to A / Largest to Smallest");
                        });
                    });
                }
                res = ok_cancel(ui);
            });
            if res.0 {
                let k: Vec<SortKey> = keys.iter().map(|(c, a)| SortKey { col: *c, ascending: *a }).collect();
                let h = *header;
                app.modify("Sort", Scope::Sheet(app.sheet), |wb, s| ops::sort(wb, s, r, &k, h));
                return false;
            }
            open && !res.1
        }
        Dialog::Validation { range, kind, list, min, max } => {
            let mut res = (false, false);
            let mut clear = false;
            let open = window(ctx, "Data Validation", 380.0, |ui| {
                ui.label(format!("Apply to: {}", range.to_a1()));
                ui.horizontal(|ui| {
                    ui.label("Allow:");
                    let kinds = ["Any value", "List", "Whole number", "Decimal", "Text length"];
                    egui::ComboBox::from_id_salt("dv-kind").selected_text(kinds[*kind]).show_ui(ui, |ui| {
                        for (i, k) in kinds.iter().enumerate() {
                            ui.selectable_value(kind, i, *k);
                        }
                    });
                });
                match *kind {
                    1 => {
                        ui.label("Source (comma separated, or =A1:A5):");
                        ui.text_edit_singleline(list);
                    }
                    2..=4 => {
                        egui::Grid::new("dv-mm").show(ui, |ui| {
                            ui.label("Minimum:");
                            ui.text_edit_singleline(min);
                            ui.end_row();
                            ui.label("Maximum:");
                            ui.text_edit_singleline(max);
                            ui.end_row();
                        });
                    }
                    _ => {}
                }
                ui.horizontal(|ui| {
                    if ui.button("Clear All").clicked() {
                        clear = true;
                    }
                });
                res = ok_cancel(ui);
            });
            let r = *range;
            if clear || (res.0 && *kind == 0) {
                app.modify("Data Validation", Scope::Sheet(app.sheet), |wb, s| wb.sheets[s].validations.retain(|v| !v.range.intersects(&r)));
                return false;
            }
            if res.0 {
                let num = |s: &str, d: f64| s.trim().parse::<f64>().unwrap_or(d);
                let rule = match *kind {
                    1 => {
                        let t = list.trim();
                        if let Some(rng) = t.strip_prefix('=').and_then(|x| crate::editor::parse_goto(x)) {
                            ValidationRule::ListRange(rng.1)
                        } else {
                            ValidationRule::List(t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                        }
                    }
                    2 => ValidationRule::WholeNumber { min: num(min, f64::NEG_INFINITY), max: num(max, f64::INFINITY) },
                    3 => ValidationRule::Decimal { min: num(min, f64::NEG_INFINITY), max: num(max, f64::INFINITY) },
                    _ => ValidationRule::TextLength { min: num(min, 0.0), max: num(max, f64::INFINITY) },
                };
                app.modify("Data Validation", Scope::Sheet(app.sheet), |wb, s| {
                    wb.sheets[s].validations.retain(|v| !(v.range == r));
                    wb.sheets[s].validations.push(DataValidation { range: r, rule });
                });
                return false;
            }
            open && !res.1
        }
        Dialog::TextToColumns { tab, comma, semicolon, space, other, consecutive } => {
            let mut res = (false, false);
            let open = window(ctx, "Convert Text to Columns", 340.0, |ui| {
                ui.label("Delimiters:");
                ui.checkbox(tab, "Tab");
                ui.checkbox(semicolon, "Semicolon");
                ui.checkbox(comma, "Comma");
                ui.checkbox(space, "Space");
                ui.horizontal(|ui| {
                    ui.label("Other:");
                    ui.add(egui::TextEdit::singleline(other).desired_width(30.0));
                });
                ui.checkbox(consecutive, "Treat consecutive delimiters as one");
                res = ok_cancel(ui);
            });
            if res.0 {
                let mut d = Vec::new();
                if *tab {
                    d.push('\t');
                }
                if *comma {
                    d.push(',');
                }
                if *semicolon {
                    d.push(';');
                }
                if *space {
                    d.push(' ');
                }
                d.extend(other.chars());
                let r = Rect { left: app.sel.primary().left, right: app.sel.primary().left, ..app.sel.primary() };
                let ext = ops::text_to_columns_extent(&app.doc.wb, app.sheet, r, &d);
                let c = *consecutive;
                app.modify("Text to Columns", Scope::Cells { sheet: app.sheet, rects: vec![ext] }, |wb, s| {
                    ops::text_to_columns(wb, s, r, &d, c);
                });
                return false;
            }
            open && !res.1
        }
        Dialog::RemoveDuplicates { range, header, cols } => {
            let mut res = (false, false);
            let r = *range;
            let open = window(ctx, "Remove Duplicates", 320.0, |ui| {
                ui.label("Select one or more columns that contain duplicates.");
                ui.checkbox(header, "My data has headers");
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for (c, on) in cols.iter_mut() {
                        let name = if *header { crate::grid::display_text(&app.doc.wb, app.sheet, CellPos::new(r.top, *c)) } else { String::new() };
                        let label = if name.is_empty() { format!("Column {}", engine::address::col_to_letters(*c)) } else { name };
                        ui.checkbox(on, label);
                    }
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                let cs: Vec<u32> = cols.iter().filter(|(_, on)| *on).map(|(c, _)| *c).collect();
                let h = *header;
                let mut removed = 0;
                app.modify("Remove Duplicates", Scope::Sheet(app.sheet), |wb, s| removed = ops::remove_duplicates(wb, s, r, &cs, h));
                app.message("Remove Duplicates", if removed == 0 { "No duplicate values found.".to_string() } else { format!("{} duplicate values found and removed.", removed) });
                return false;
            }
            open && !res.1
        }
        Dialog::GoalSeek { set_cell, to_value, changing } => {
            let mut res = (false, false);
            let open = window(ctx, "Goal Seek", 280.0, |ui| {
                egui::Grid::new("gs").num_columns(2).show(ui, |ui| {
                    ui.label("Set cell:");
                    ui.text_edit_singleline(set_cell);
                    ui.end_row();
                    ui.label("To value:");
                    ui.text_edit_singleline(to_value);
                    ui.end_row();
                    ui.label("By changing cell:");
                    ui.text_edit_singleline(changing);
                    ui.end_row();
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                let (Some(sc), Some(ch), Ok(target)) = (parse_cell(app, set_cell), parse_cell(app, changing), to_value.trim().parse::<f64>()) else {
                    app.message("Goal Seek", "Enter a set cell, a numeric target value and a changing cell.");
                    return false;
                };
                let sheet = app.sheet;
                let sel = app.sel.primary();
                app.doc.undo.record(&app.doc.wb, Scope::Cells { sheet, rects: vec![Rect::single(ch)] }, "Goal Seek", sheet, sel);
                match ops::goal_seek(&mut app.doc.wb, sheet, sc, target, ch) {
                    Ok(x) => {
                        app.after_change();
                        app.message("Goal Seek Status", format!("Goal Seeking with Cell {} found a solution.\n\nTarget value: {}\nCurrent value: {}\nBy changing {} to {}", sc.to_a1(), target, crate::grid::display_text(&app.doc.wb, sheet, sc), ch.to_a1(), engine::value::format_number_general(x)));
                    }
                    Err(e) => {
                        app.doc.undo.discard_last();
                        app.message("Goal Seek Status", e);
                    }
                }
                return false;
            }
            open && !res.1
        }
        Dialog::CondFormat { kind, a, b, preset } => {
            let mut res = (false, false);
            let k = *kind;
            let title = match k {
                CfKind::Greater => "Greater Than",
                CfKind::Less => "Less Than",
                CfKind::Between => "Between",
                CfKind::Equal => "Equal To",
                CfKind::TextContains => "Text That Contains",
                CfKind::Duplicates => "Duplicate Values",
                CfKind::Top10 => "Top 10 Items",
                CfKind::Bottom10 => "Bottom 10 Items",
                CfKind::AboveAverage => "Above Average",
                CfKind::BelowAverage => "Below Average",
                CfKind::Formula => "New Formatting Rule",
            };
            let open = window(ctx, title, 380.0, |ui| {
                match k {
                    CfKind::Greater | CfKind::Less | CfKind::Equal => {
                        ui.label(format!("Format cells that are {}:", title.to_uppercase()));
                        ui.text_edit_singleline(a);
                    }
                    CfKind::Between => {
                        ui.label("Format cells that are BETWEEN:");
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(a).desired_width(100.0));
                            ui.label("and");
                            ui.add(egui::TextEdit::singleline(b).desired_width(100.0));
                        });
                    }
                    CfKind::TextContains => {
                        ui.label("Format cells that contain the text:");
                        ui.text_edit_singleline(a);
                    }
                    CfKind::Top10 | CfKind::Bottom10 => {
                        ui.horizontal(|ui| {
                            ui.label(if k == CfKind::Top10 { "Format cells that rank in the TOP:" } else { "Format cells that rank in the BOTTOM:" });
                            ui.add(egui::TextEdit::singleline(a).desired_width(40.0));
                        });
                    }
                    CfKind::Duplicates => {
                        ui.label("Format cells that contain duplicate values.");
                    }
                    CfKind::AboveAverage | CfKind::BelowAverage => {
                        ui.label(format!("Format cells that are {} AVERAGE.", if k == CfKind::AboveAverage { "ABOVE" } else { "BELOW" }));
                    }
                    CfKind::Formula => {
                        ui.label("Format values where this formula is true (relative to the top-left cell):");
                        ui.text_edit_singleline(a);
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("with");
                    egui::ComboBox::from_id_salt("cf-preset").selected_text(CF_PRESETS[*preset].0).width(240.0).show_ui(ui, |ui| {
                        for (i, (n, _, _)) in CF_PRESETS.iter().enumerate() {
                            ui.selectable_value(preset, i, *n);
                        }
                    });
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                let (_, fill, font) = CF_PRESETS[*preset];
                let style = CfStyle { fill: fill.map(Color), font_color: font.map(Color), bold: *preset == 5, italic: false };
                let strip = |s: &str| s.trim().trim_start_matches('=').to_string();
                let rule = match k {
                    CfKind::Greater => CfRule::CellValue { op: CfOperator::Greater, a: strip(a), b: String::new(), style },
                    CfKind::Less => CfRule::CellValue { op: CfOperator::Less, a: strip(a), b: String::new(), style },
                    CfKind::Equal => CfRule::CellValue { op: CfOperator::Equal, a: strip(a), b: String::new(), style },
                    CfKind::Between => CfRule::CellValue { op: CfOperator::Between, a: strip(a), b: strip(b), style },
                    CfKind::TextContains => CfRule::TextContains { text: a.clone(), style },
                    CfKind::Duplicates => CfRule::Duplicates { unique: false, style },
                    CfKind::Top10 | CfKind::Bottom10 => CfRule::TopBottom { top: k == CfKind::Top10, rank: a.trim().parse().unwrap_or(10), percent: false, style },
                    CfKind::AboveAverage | CfKind::BelowAverage => CfRule::AboveAverage { above: k == CfKind::AboveAverage, style },
                    CfKind::Formula => CfRule::Expression { formula: strip(a), style },
                };
                app.add_cf(rule);
                return false;
            }
            open && !res.1
        }
        Dialog::CfManager { selected } => {
            let mut close = false;
            let mut delete = None;
            let rules: Vec<(String, String)> = app.sheet_ref().cond_formats.iter().map(|c| (engine::condfmt::describe(&c.rule), c.range.to_a1())).collect();
            let open = window(ctx, "Conditional Formatting Rules Manager", 440.0, |ui| {
                egui::Grid::new("cfm").striped(true).num_columns(2).min_col_width(160.0).show(ui, |ui| {
                    ui.label(RichText::new("Rule (applied in order shown)").strong());
                    ui.label(RichText::new("Applies to").strong());
                    ui.end_row();
                    for (i, (d, r)) in rules.iter().enumerate() {
                        if ui.selectable_label(*selected == Some(i), d).clicked() {
                            *selected = Some(i);
                        }
                        ui.label(r);
                        ui.end_row();
                    }
                });
                if rules.is_empty() {
                    ui.label(RichText::new("No rules on this sheet.").color(theme::MUTED));
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(selected.is_some(), egui::Button::new("Delete Rule")).clicked() {
                        delete = selected.take();
                    }
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
            if let Some(i) = delete {
                app.modify("Delete Rule", Scope::Sheet(app.sheet), |wb, s| {
                    if i < wb.sheets[s].cond_formats.len() {
                        wb.sheets[s].cond_formats.remove(i);
                    }
                });
            }
            open && !close
        }
        Dialog::Chart { idx, kind, title, range, header, by_cols } => {
            let mut res = (false, false);
            let open = window(ctx, "Chart", 340.0, |ui| {
                egui::Grid::new("chart").num_columns(2).show(ui, |ui| {
                    ui.label("Chart type:");
                    egui::ComboBox::from_id_salt("chart-kind").selected_text(kind.name()).show_ui(ui, |ui| {
                        for k in ChartKind::ALL {
                            ui.selectable_value(kind, k, k.name());
                        }
                    });
                    ui.end_row();
                    ui.label("Title:");
                    ui.text_edit_singleline(title);
                    ui.end_row();
                    ui.label("Data range:");
                    ui.text_edit_singleline(range);
                    ui.end_row();
                });
                ui.checkbox(header, "First row/column contains labels");
                ui.horizontal(|ui| {
                    ui.label("Series in:");
                    ui.radio_value(by_cols, true, "Columns");
                    ui.radio_value(by_cols, false, "Rows");
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                let i = *idx;
                let (k, t, h, bc) = (*kind, title.clone(), *header, *by_cols);
                let rng = Rect::parse(&range.replace('$', ""));
                app.modify("Edit Chart", Scope::Sheet(app.sheet), |wb, s| {
                    if let Some(c) = wb.sheets[s].charts.get_mut(i) {
                        c.kind = k;
                        c.title = t;
                        c.has_header = h;
                        c.series_in_columns = bc;
                        if let Some(r) = rng {
                            c.data = r;
                        }
                    }
                });
                return false;
            }
            open && !res.1
        }
        Dialog::Comment { cell, text } => {
            let mut res = (false, false);
            let mut delete = false;
            let open = window(ctx, &format!("Comment - {}", cell.to_a1()), 320.0, |ui| {
                ui.add(egui::TextEdit::multiline(text).id(egui::Id::unique("multiline-dialog")).desired_rows(5).desired_width(300.0));
                if ui.button("Delete Comment").clicked() {
                    delete = true;
                }
                res = ok_cancel(ui);
            });
            let p = *cell;
            if delete || (res.0 && text.trim().is_empty()) {
                app.modify("Delete Comment", Scope::Cells { sheet: app.sheet, rects: vec![Rect::single(p)] }, |wb, s| {
                    wb.sheets[s].comments.remove(&p);
                });
                return false;
            }
            if res.0 {
                let t = text.clone();
                app.modify("Edit Comment", Scope::Cells { sheet: app.sheet, rects: vec![Rect::single(p)] }, |wb, s| {
                    wb.sheets[s].comments.insert(p, t);
                });
                return false;
            }
            open && !res.1
        }
        Dialog::Hyperlink { cell, url, display } => {
            let mut res = (false, false);
            let mut remove = false;
            let open = window(ctx, "Insert Hyperlink", 380.0, |ui| {
                egui::Grid::new("hl").num_columns(2).show(ui, |ui| {
                    ui.label("Text to display:");
                    ui.text_edit_singleline(display);
                    ui.end_row();
                    ui.label("Address:");
                    ui.add(egui::TextEdit::singleline(url).desired_width(260.0).hint_text("https://… or #Sheet1!A1"));
                    ui.end_row();
                });
                if ui.button("Remove Link").clicked() {
                    remove = true;
                }
                res = ok_cancel(ui);
            });
            let p = *cell;
            if remove {
                app.modify("Remove Hyperlink", Scope::Cells { sheet: app.sheet, rects: vec![Rect::single(p)] }, |wb, s| {
                    wb.sheets[s].hyperlinks.remove(&p);
                });
                return false;
            }
            if res.0 {
                let (u, d) = (url.trim().to_string(), display.clone());
                app.modify("Insert Hyperlink", Scope::Cells { sheet: app.sheet, rects: vec![Rect::single(p)] }, |wb, s| {
                    if !u.is_empty() {
                        wb.sheets[s].hyperlinks.insert(p, u.clone());
                    }
                    let shown = if d.is_empty() { u.clone() } else { d };
                    if wb.value(s, p).is_empty() || crate::grid::display_text(wb, s, p) != shown {
                        wb.set_value(s, p, engine::Value::Text(shown));
                    }
                });
                return false;
            }
            open && !res.1
        }
        Dialog::Size { rows, value } => {
            let mut res = (false, false);
            let title = if *rows { "Row Height" } else { "Column Width" };
            let open = window(ctx, title, 220.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{}:", title));
                    ui.add(egui::TextEdit::singleline(value).desired_width(60.0));
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                if let Ok(v) = value.trim().parse::<f32>() {
                    if *rows {
                        app.set_row_height((v / 0.75).clamp(0.0, 545.0));
                    } else {
                        app.set_col_width((v * 7.0).clamp(0.0, 1785.0).round());
                    }
                }
                return false;
            }
            open && !res.1
        }
        Dialog::Zoom { value } => {
            let mut res = (false, false);
            let open = window(ctx, "Zoom", 200.0, |ui| {
                for z in [200, 100, 75, 50, 25] {
                    ui.radio_value(value, z, format!("{}%", z));
                }
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    ui.add(egui::DragValue::new(value).range(10..=400).suffix("%"));
                });
                res = ok_cancel(ui);
            });
            if res.0 {
                app.set_zoom(*value as f32 / 100.0);
                return false;
            }
            open && !res.1
        }
    }
}

// ======================================================================
// Find & Replace
// ======================================================================

fn find_dialog(app: &mut App, ctx: &Context) {
    let Some(mut st) = app.find.take() else { return };
    let mut open = true;
    let mut action = 0;
    let title = if st.replace_mode { "Find and Replace" } else { "Find" };
    egui::Window::new("Find and Replace")
        .id(egui::Id::unique("rexcel-find"))
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - vec2(200.0, 150.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(420.0);
            ui.horizontal(|ui| {
                if ui.selectable_label(!st.replace_mode, "Find").clicked() {
                    st.replace_mode = false;
                }
                if ui.selectable_label(st.replace_mode, "Replace").clicked() {
                    st.replace_mode = true;
                }
            });
            ui.separator();
            egui::Grid::new("find-grid").num_columns(2).show(ui, |ui| {
                ui.label("Find what:");
                let r = ui.add(egui::TextEdit::singleline(&mut st.query).desired_width(300.0));
                if st.focus {
                    r.request_focus();
                    st.focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = 1;
                }
                ui.end_row();
                if st.replace_mode {
                    ui.label("Replace with:");
                    ui.add(egui::TextEdit::singleline(&mut st.replace).desired_width(300.0));
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Within:");
                ui.radio_value(&mut st.workbook, false, "Sheet");
                ui.radio_value(&mut st.workbook, true, "Workbook");
                ui.add_space(8.0);
                ui.label("Look in:");
                ui.radio_value(&mut st.in_formulas, true, "Formulas");
                ui.radio_value(&mut st.in_formulas, false, "Values");
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.match_case, "Match case");
                ui.checkbox(&mut st.entire, "Match entire cell contents");
            });
            ui.horizontal(|ui| {
                if st.replace_mode {
                    if ui.button("Replace All").clicked() {
                        action = 3;
                    }
                    if ui.button("Replace").clicked() {
                        action = 4;
                    }
                }
                if ui.button("Find All").clicked() {
                    action = 2;
                }
                if ui.add(egui::Button::new(RichText::new("Find Next").color(Color32::WHITE)).fill(theme::ACCENT)).clicked() {
                    action = 1;
                }
            });
            if !st.message.is_empty() {
                ui.label(RichText::new(&st.message).color(theme::MUTED));
            }
            if !st.results.is_empty() {
                ui.separator();
                let mut goto = None;
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    egui::Grid::new("find-results").striped(true).num_columns(3).show(ui, |ui| {
                        ui.label(RichText::new("Sheet").strong());
                        ui.label(RichText::new("Cell").strong());
                        ui.label(RichText::new("Value").strong());
                        ui.end_row();
                        for (s, p) in st.results.iter().take(1000) {
                            let name = app.doc.wb.sheets.get(*s).map(|x| x.name.clone()).unwrap_or_default();
                            if ui.link(&name).clicked() {
                                goto = Some((*s, *p));
                            }
                            if ui.link(p.to_a1()).clicked() {
                                goto = Some((*s, *p));
                            }
                            ui.label(crate::grid::display_text(&app.doc.wb, *s, *p));
                            ui.end_row();
                        }
                    });
                });
                if let Some((s, p)) = goto {
                    app.set_sheet(s);
                    app.select_cell(p);
                }
            }
        });
    let _ = title;
    let opts = st.options(app.sheet);
    match action {
        1 => match ops::find_next(&app.doc.wb, &opts, (app.sheet, app.sel.active)) {
            Some((s, p)) => {
                app.set_sheet(s);
                app.select_cell(p);
                st.message.clear();
            }
            None => st.message = "We couldn't find what you were looking for.".into(),
        },
        2 => {
            st.results = ops::find_all(&app.doc.wb, &opts);
            st.message = format!("{} cell(s) found", st.results.len());
        }
        3 => {
            let with = st.replace.clone();
            let mut n = 0;
            app.modify("Replace All", Scope::Workbook, |wb, _| n = ops::replace_all(wb, &opts, &with));
            st.message = format!("All done. We made {} replacements.", n);
            if n == 0 {
                app.doc.undo.discard_last();
            }
        }
        4 => {
            let (s, p) = (app.sheet, app.sel.active);
            let with = st.replace.clone();
            if ops::cell_matches(&app.doc.wb, s, p, &FindOptions { in_formulas: true, ..opts.clone() }) || ops::cell_matches(&app.doc.wb, s, p, &opts) {
                app.modify("Replace", Scope::Cells { sheet: s, rects: vec![Rect::single(p)] }, |wb, sh| {
                    ops::replace_in_cell(wb, sh, p, &opts, &with);
                });
            }
            if let Some((s, p)) = ops::find_next(&app.doc.wb, &opts, (s, p)) {
                app.set_sheet(s);
                app.select_cell(p);
            }
        }
        _ => {}
    }
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if open && !esc {
        app.find = Some(st);
    }
}

// ======================================================================
// AutoFilter popup
// ======================================================================

fn filter_popup(app: &mut App, ctx: &Context) {
    let Some(mut fp) = app.filter_popup.take() else { return };
    let mut close = false;
    let mut apply = false;
    let mut sort: Option<bool> = None;
    let area = egui::Area::new(egui::Id::unique("rexcel-filter-popup")).order(egui::Order::Foreground).fixed_pos(fp.pos).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(230.0);
            if ui.add(egui::Button::new("↑ Sort A to Z").frame_when_inactive(false)).clicked() {
                sort = Some(true);
            }
            if ui.add(egui::Button::new("↓ Sort Z to A").frame_when_inactive(false)).clicked() {
                sort = Some(false);
            }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut fp.search).hint_text("Search"));
            let all = fp.values.iter().all(|v| v.1);
            let mut all2 = all;
            if ui.checkbox(&mut all2, "(Select All)").changed() {
                for v in fp.values.iter_mut() {
                    v.1 = all2;
                }
            }
            let q = fp.search.to_lowercase();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for (v, on) in fp.values.iter_mut() {
                    if !q.is_empty() && !v.to_lowercase().contains(&q) {
                        continue;
                    }
                    let label = if v.is_empty() { "(Blanks)".to_string() } else { v.clone() };
                    ui.checkbox(on, label);
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("OK").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(64.0, 22.0))).clicked() {
                    apply = true;
                }
                if ui.add(egui::Button::new("Cancel").min_size(vec2(64.0, 22.0))).clicked() {
                    close = true;
                }
            });
        });
    });
    if ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer() && !area.response.hovered() {
        let p = ctx.input(|i| i.pointer.interact_pos());
        if p.map(|p| !area.response.rect.contains(p)).unwrap_or(true) {
            close = true;
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    let Some(af) = app.sheet_ref().autofilter.clone() else { return };
    if let Some(asc) = sort {
        let r = Rect { bottom: app.sheet_ref().data_extent().map(|e| e.bottom.max(af.range.bottom)).unwrap_or(af.range.bottom), ..af.range };
        let col = fp.col;
        app.modify("Sort", Scope::Sheet(app.sheet), |wb, s| ops::sort(wb, s, r, &[SortKey { col, ascending: asc }], true));
        return;
    }
    if apply {
        let off = fp.col - af.range.left;
        let allowed: HashSet<String> = fp.values.iter().filter(|v| v.1).map(|v| v.0.clone()).collect();
        let all = allowed.len() == fp.values.len();
        app.apply_filter_now(off, if all { None } else { Some(allowed) });
        return;
    }
    if !close {
        app.filter_popup = Some(fp);
    }
}

// ======================================================================
// Context menus
// ======================================================================

pub fn grid_context_menu(app: &mut App, ui: &mut Ui) {
    ui.set_min_width(200.0);
    let target = app.grid.ctx_target;
    if ui.add(egui::Button::new("✂ Cut").shortcut_text("Ctrl+X")).clicked() {
        app.copy(true);
        ui.close();
    }
    if ui.add(egui::Button::new("⎘ Copy").shortcut_text("Ctrl+C")).clicked() {
        app.copy(false);
        ui.close();
    }
    if ui.add(egui::Button::new("📋 Paste").shortcut_text("Ctrl+V")).clicked() {
        app.paste_text_or_clip(None);
        ui.close();
    }
    if ui.add_enabled(app.clip.is_some(), egui::Button::new("Paste Special...")).clicked() {
        app.dialog = Some(Dialog::PasteSpecial(Default::default()));
        ui.close();
    }
    ui.separator();
    match target {
        CtxTarget::Rows => {
            if ui.button("Insert").clicked() {
                app.insert_rows();
                ui.close();
            }
            if ui.button("Delete").clicked() {
                app.delete_rows();
                ui.close();
            }
            if ui.button("Clear Contents").clicked() {
                app.clear_selection(true, false);
                ui.close();
            }
            ui.separator();
            if ui.button("Format Cells...").clicked() {
                app.open_format_cells();
                ui.close();
            }
            if ui.button("Row Height...").clicked() {
                let h = app.sheet_ref().row_height(app.sel.active.row);
                app.dialog = Some(Dialog::Size { rows: true, value: format!("{}", h * 0.75) });
                ui.close();
            }
            if ui.button("Hide").clicked() {
                app.hide_rows_cols(true, true);
                ui.close();
            }
            if ui.button("Unhide").clicked() {
                app.hide_rows_cols(true, false);
                ui.close();
            }
        }
        CtxTarget::Cols => {
            if ui.button("Insert").clicked() {
                app.insert_cols();
                ui.close();
            }
            if ui.button("Delete").clicked() {
                app.delete_cols();
                ui.close();
            }
            if ui.button("Clear Contents").clicked() {
                app.clear_selection(true, false);
                ui.close();
            }
            ui.separator();
            if ui.button("Format Cells...").clicked() {
                app.open_format_cells();
                ui.close();
            }
            if ui.button("Column Width...").clicked() {
                let w = app.sheet_ref().col_width(app.sel.active.col);
                app.dialog = Some(Dialog::Size { rows: false, value: format!("{:.2}", w / 7.0) });
                ui.close();
            }
            if ui.button("Hide").clicked() {
                app.hide_rows_cols(false, true);
                ui.close();
            }
            if ui.button("Unhide").clicked() {
                app.hide_rows_cols(false, false);
                ui.close();
            }
        }
        CtxTarget::Cells => {
            ui.menu_button("Insert", |ui| {
                if ui.button("Entire Row").clicked() {
                    app.insert_rows();
                    ui.close();
                }
                if ui.button("Entire Column").clicked() {
                    app.insert_cols();
                    ui.close();
                }
            });
            ui.menu_button("Delete", |ui| {
                if ui.button("Entire Row").clicked() {
                    app.delete_rows();
                    ui.close();
                }
                if ui.button("Entire Column").clicked() {
                    app.delete_cols();
                    ui.close();
                }
            });
            if ui.button("Clear Contents").clicked() {
                app.clear_selection(true, false);
                ui.close();
            }
            ui.separator();
            ui.menu_button("Filter", |ui| {
                if ui.button(if app.sheet_ref().autofilter.is_some() { "Remove Filter" } else { "Filter" }).clicked() {
                    app.toggle_autofilter();
                    ui.close();
                }
                if ui.button("Filter by Selected Cell's Value").clicked() {
                    if app.sheet_ref().autofilter.is_none() {
                        app.toggle_autofilter();
                    }
                    if let Some(af) = app.sheet_ref().autofilter.clone() {
                        let p = app.sel.active;
                        if af.range.left <= p.col && p.col <= af.range.right {
                            let v = crate::grid::display_text(&app.doc.wb, app.sheet, p);
                            app.apply_filter_now(p.col - af.range.left, Some([v].into_iter().collect()));
                        }
                    }
                    ui.close();
                }
            });
            ui.menu_button("Sort", |ui| crate::ribbon::sort_menu(app, ui));
            ui.separator();
            let has_comment = app.sheet_ref().comments.contains_key(&app.sel.active);
            if ui.button(if has_comment { "Edit Comment" } else { "Insert Comment" }).clicked() {
                app.edit_comment();
                ui.close();
            }
            if has_comment && ui.button("Delete Comment").clicked() {
                app.clear_comments();
                ui.close();
            }
            ui.separator();
            if ui.add(egui::Button::new("Format Cells...").shortcut_text("Ctrl+1")).clicked() {
                app.open_format_cells();
                ui.close();
            }
            if ui.button("Pick From Drop-down List...").clicked() {
                app.validation_popup = Some(app.sel.active);
                ui.close();
            }
            if ui.button("Define Name...").clicked() {
                app.dialog = Some(Dialog::define_name(app));
                ui.close();
            }
            if ui.add(egui::Button::new("🔗 Link").shortcut_text("Ctrl+K")).clicked() {
                app.open_hyperlink();
                ui.close();
            }
        }
    }
    let _ = (MAX_ROWS, MAX_COLS);
}
