//! Spell checking (Review > Spelling, F7), using a bundled en_US Hunspell dictionary.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use egui::{vec2, Color32, Context, RichText};
use engine::address::{CellPos, Rect};
use engine::ops::Scope;
use engine::{Value, Workbook};
use spellbook::Dictionary;

use crate::app::App;
use crate::theme;

const AFF: &str = include_str!("../assets/dict/en_US.aff");
const DIC: &str = include_str!("../assets/dict/en_US.dic");

fn dict() -> &'static Mutex<Dictionary> {
    static D: OnceLock<Mutex<Dictionary>> = OnceLock::new();
    D.get_or_init(|| {
        let mut d = Dictionary::new(AFF, DIC).expect("bundled dictionary parses");
        for w in load_custom_words() {
            let _ = d.add(&w);
        }
        Mutex::new(d)
    })
}

/// Start loading the dictionary in the background so the first F7 is instant.
pub fn preload() {
    std::thread::spawn(|| {
        let _ = dict();
    });
}

fn custom_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rexcel").join("custom.dic"))
}

fn load_custom_words() -> Vec<String> {
    custom_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

/// Add a word to the user's custom dictionary (saved across sessions).
fn add_to_custom(word: &str) {
    let _ = dict().lock().map(|mut d| d.add(word));
    if let Some(p) = custom_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut words = load_custom_words();
        if !words.iter().any(|w| w == word) {
            words.push(word.to_string());
            let _ = std::fs::write(p, words.join("\n") + "\n");
        }
    }
}

pub fn check_word(word: &str) -> bool {
    dict().lock().map(|d| d.check(word)).unwrap_or(true)
}

pub fn suggest(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(d) = dict().lock() {
        d.suggest(word, &mut out);
    }
    out.truncate(8);
    out
}

/// Byte ranges of the words in `text` that should be spell checked.
/// Skips web/e-mail addresses, words with digits, and words in UPPERCASE (like Excel's defaults).
pub fn words(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < text.len() {
        // whitespace-separated token
        while i < text.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let tok_start = i;
        while i < text.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let tok = &text[tok_start..i];
        if tok.is_empty() {
            continue;
        }
        let lower = tok.to_ascii_lowercase();
        if tok.contains('@') || lower.contains("://") || lower.starts_with("www.") || tok.contains('\\') || tok.contains('/') && tok.contains('.') {
            continue;
        }
        // runs of letters, digits and inner apostrophes
        let mut run_start: Option<usize> = None;
        let mut iter = tok.char_indices().peekable();
        while let Some((off, c)) = iter.next() {
            let is_word_char = c.is_alphanumeric() || ((c == '\'' || c == '’') && run_start.is_some() && iter.peek().is_some_and(|(_, n)| n.is_alphabetic()));
            if is_word_char {
                if run_start.is_none() {
                    run_start = Some(off);
                }
            }
            let at_end = iter.peek().is_none();
            if let Some(s) = run_start {
                if !is_word_char || at_end {
                    let e = if is_word_char { off + c.len_utf8() } else { off };
                    push_word(&mut out, text, tok_start + s, tok_start + e);
                    run_start = None;
                }
            }
        }
    }
    out
}

fn push_word(out: &mut Vec<(usize, usize)>, text: &str, s: usize, e: usize) {
    let w = &text[s..e];
    if w.chars().any(|c| c.is_numeric()) {
        return;
    }
    let letters = w.chars().filter(|c| c.is_alphabetic()).count();
    if letters == 0 {
        return;
    }
    if letters > 1 && w.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
        return;
    }
    out.push((s, e));
}

/// A word the dictionary doesn't know.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub pos: CellPos,
    pub start: usize,
    pub end: usize,
    pub word: String,
}

/// Text cells to check, in the order Excel checks them: from the active cell to the end of
/// the sheet, then from the top back to the active cell. With a multi-cell selection only
/// the selection is checked.
pub fn cell_order(wb: &Workbook, sheet: usize, from: CellPos, only: Option<Rect>) -> Vec<CellPos> {
    let Some(s) = wb.sheets.get(sheet) else { return vec![] };
    let mut cells: Vec<CellPos> = s
        .cells
        .iter()
        .filter(|(p, c)| c.formula.is_none() && matches!(c.value, Value::Text(_)) && only.map(|r| r.contains(**p)).unwrap_or(true))
        .map(|(p, _)| *p)
        .collect();
    cells.sort();
    let split = cells.partition_point(|p| *p < from);
    cells.rotate_left(split);
    cells
}

/// Find the next unknown word, starting at `cells[idx]` byte offset `offset`.
pub fn next_issue(
    wb: &Workbook,
    sheet: usize,
    cells: &[CellPos],
    mut idx: usize,
    mut offset: usize,
    ignore: &HashSet<String>,
    check: &dyn Fn(&str) -> bool,
) -> Option<(usize, Issue)> {
    while idx < cells.len() {
        let p = cells[idx];
        if let Some(Value::Text(t)) = wb.cell(sheet, p).map(|c| &c.value) {
            for (s, e) in words(t) {
                if s < offset {
                    continue;
                }
                let w = &t[s..e];
                if ignore.contains(&w.to_lowercase()) || check(w) {
                    continue;
                }
                return Some((idx, Issue { pos: p, start: s, end: e, word: w.to_string() }));
            }
        }
        idx += 1;
        offset = 0;
    }
    None
}

/// State of the Spelling dialog.
pub struct SpellState {
    sheet: usize,
    cells: Vec<CellPos>,
    idx: usize,
    offset: usize,
    current: Option<Issue>,
    suggestions: Vec<String>,
    selected: Option<usize>,
    change_to: String,
    ignore_all: HashSet<String>,
    change_all: HashMap<String, String>,
    changes: usize,
}

impl SpellState {
    fn advance(&mut self, wb: &Workbook) {
        loop {
            let found = next_issue(wb, self.sheet, &self.cells, self.idx, self.offset, &self.ignore_all, &check_word);
            match found {
                Some((idx, issue)) => {
                    self.idx = idx;
                    self.offset = issue.end;
                    if self.change_all.contains_key(&issue.word.to_lowercase()) {
                        // handled by the caller (needs a mutable workbook)
                        self.current = Some(issue);
                        return;
                    }
                    self.suggestions = suggest(&issue.word);
                    self.selected = if self.suggestions.is_empty() { None } else { Some(0) };
                    self.change_to = self.suggestions.first().cloned().unwrap_or_else(|| issue.word.clone());
                    self.current = Some(issue);
                    return;
                }
                None => {
                    self.current = None;
                    return;
                }
            }
        }
    }
}

impl App {
    /// Review > Spelling (F7).
    pub fn start_spelling(&mut self) {
        if self.commit_if_editing() {
            // committed an open edit first
        }
        let r = self.sel.primary();
        let only = if r.cell_count() > 1 { Some(r) } else { None };
        let from = only.map(|r| r.top_left()).unwrap_or(self.sel.active);
        let cells = cell_order(&self.doc.wb, self.sheet, from, only);
        let mut st = SpellState {
            sheet: self.sheet,
            cells,
            idx: 0,
            offset: 0,
            current: None,
            suggestions: vec![],
            selected: None,
            change_to: String::new(),
            ignore_all: HashSet::new(),
            change_all: HashMap::new(),
            changes: 0,
        };
        self.set_status("Checking spelling...");
        self.spell_step(&mut st);
        if st.current.is_some() {
            self.spell = Some(st);
        } else {
            self.spell_done(&st);
        }
    }

    fn spell_done(&mut self, st: &SpellState) {
        self.set_status("Ready");
        let text = if st.changes > 0 {
            format!("Spell check complete. You're good to go!\n\n{} change(s) made.", st.changes)
        } else {
            "Spell check complete. You're good to go!".to_string()
        };
        self.message("Spelling", text);
    }

    /// Move to the next issue, applying any "Change All" replacements on the way.
    fn spell_step(&mut self, st: &mut SpellState) {
        loop {
            st.advance(&self.doc.wb);
            let Some(issue) = st.current.clone() else { return };
            if let Some(rep) = st.change_all.get(&issue.word.to_lowercase()).cloned() {
                self.spell_replace(st, &issue, &rep);
                continue;
            }
            self.select_cell(issue.pos);
            return;
        }
    }

    fn spell_replace(&mut self, st: &mut SpellState, issue: &Issue, with: &str) {
        let Some(Value::Text(t)) = self.doc.wb.cell(st.sheet, issue.pos).map(|c| c.value.clone()) else { return };
        if t.get(issue.start..issue.end) != Some(issue.word.as_str()) {
            return;
        }
        let with = match_case(&issue.word, with);
        let new_text = format!("{}{}{}", &t[..issue.start], with, &t[issue.end..]);
        let pos = issue.pos;
        self.modify("Spelling", Scope::Cells { sheet: st.sheet, rects: vec![Rect::single(pos)] }, |wb, sh| {
            wb.set_value(sh, pos, Value::Text(new_text));
        });
        st.offset = issue.start + with.len();
        st.changes += 1;
    }
}

/// Keep the capitalisation of the original word ("Teh" -> "The").
fn match_case(orig: &str, rep: &str) -> String {
    let first_upper = orig.chars().next().is_some_and(|c| c.is_uppercase());
    let rep_first_upper = rep.chars().next().is_some_and(|c| c.is_uppercase());
    if first_upper && !rep_first_upper {
        let mut c = rep.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    } else {
        rep.to_string()
    }
}

/// Draw the Spelling dialog.
pub fn show(app: &mut App, ctx: &Context) {
    let Some(mut st) = app.spell.take() else { return };
    let Some(issue) = st.current.clone() else { return };
    let mut open = true;
    let mut action = "";
    egui::Window::new("Spelling: English (United States)")
        .id(egui::Id::unique("rexcel-spelling"))
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - vec2(230.0, 160.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(460.0);
            ui.label("Not in Dictionary:");
            ui.horizontal_top(|ui| {
                let text = match app.doc.wb.cell(st.sheet, issue.pos).map(|c| &c.value) {
                    Some(Value::Text(t)) => t.clone(),
                    _ => issue.word.clone(),
                };
                egui::Frame::new().stroke(egui::Stroke::new(1.0, theme::BORDER)).inner_margin(6).show(ui, |ui| {
                    ui.set_width(330.0);
                    ui.set_min_height(48.0);
                    let mut job = egui::text::LayoutJob::default();
                    let fmt = |c: Color32| egui::TextFormat { color: c, font_id: egui::FontId::proportional(14.0), ..Default::default() };
                    let (s, e) = (issue.start.min(text.len()), issue.end.min(text.len()));
                    job.append(&text[..s], 0.0, fmt(theme::TEXT));
                    job.append(&text[s..e], 0.0, egui::TextFormat { color: Color32::from_rgb(0xC0, 0x00, 0x00), underline: egui::Stroke::new(1.5, Color32::from_rgb(0xC0, 0x00, 0x00)), ..fmt(theme::TEXT) });
                    job.append(&text[e..], 0.0, fmt(theme::TEXT));
                    job.wrap.max_width = 320.0;
                    ui.label(job);
                });
                ui.vertical(|ui| {
                    if ui.add(egui::Button::new("Ignore Once").min_size(vec2(110.0, 24.0))).clicked() {
                        action = "ignore";
                    }
                    if ui.add(egui::Button::new("Ignore All").min_size(vec2(110.0, 24.0))).clicked() {
                        action = "ignore_all";
                    }
                    if ui.add(egui::Button::new("Add to Dictionary").min_size(vec2(110.0, 24.0))).clicked() {
                        action = "add";
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Change to:");
                ui.add(egui::TextEdit::singleline(&mut st.change_to).desired_width(262.0));
            });
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label("Suggestions:");
                    egui::Frame::new().stroke(egui::Stroke::new(1.0, theme::BORDER)).inner_margin(4).show(ui, |ui| {
                        ui.set_width(330.0);
                        ui.set_height(110.0);
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if st.suggestions.is_empty() {
                                ui.label(RichText::new("(No Suggestions)").color(theme::MUTED));
                            }
                            for (i, s) in st.suggestions.clone().iter().enumerate() {
                                let r = ui.selectable_label(st.selected == Some(i), s);
                                if r.clicked() {
                                    st.selected = Some(i);
                                    st.change_to = s.clone();
                                }
                                if r.double_clicked() {
                                    action = "change";
                                }
                            }
                        });
                    });
                });
                ui.vertical(|ui| {
                    ui.add_space(18.0);
                    let can = !st.change_to.trim().is_empty();
                    if ui.add_enabled(can, egui::Button::new(RichText::new("Change").color(Color32::WHITE)).fill(theme::ACCENT).min_size(vec2(110.0, 24.0))).clicked() {
                        action = "change";
                    }
                    if ui.add_enabled(can, egui::Button::new("Change All").min_size(vec2(110.0, 24.0))).clicked() {
                        action = "change_all";
                    }
                    ui.add_space(30.0);
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(110.0, 24.0))).clicked() {
                        action = "cancel";
                    }
                });
            });
            ui.label(RichText::new(format!("Cell {}", issue.pos.to_a1())).color(theme::MUTED).small());
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    match action {
        "ignore" => app.spell_step(&mut st),
        "ignore_all" => {
            st.ignore_all.insert(issue.word.to_lowercase());
            app.spell_step(&mut st);
        }
        "add" => {
            add_to_custom(&issue.word);
            st.ignore_all.insert(issue.word.to_lowercase());
            app.spell_step(&mut st);
        }
        "change" | "change_all" => {
            let with = st.change_to.trim().to_string();
            if action == "change_all" {
                st.change_all.insert(issue.word.to_lowercase(), with.clone());
            }
            app.spell_replace(&mut st, &issue, &with);
            app.spell_step(&mut st);
        }
        "cancel" => open = false,
        _ => {}
    }
    if !open {
        app.set_status("Ready");
        return;
    }
    if st.current.is_some() {
        app.spell = Some(st);
    } else {
        app.spell_done(&st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(t: &str) -> Vec<&str> {
        words(t).into_iter().map(|(s, e)| &t[s..e]).collect()
    }

    #[test]
    fn tokenizing() {
        assert_eq!(ws("Hello, wrold! It's fine."), vec!["Hello", "wrold", "It's", "fine"]);
        assert_eq!(ws("Q1 sales NASA abc123 x"), vec!["sales", "x"]);
        assert_eq!(ws("see www.example.com or bob@example.com"), vec!["see", "or"]);
        assert_eq!(ws("'quoted' words"), vec!["quoted", "words"]);
        assert_eq!(ws("café naïve"), vec!["café", "naïve"]);
    }

    #[test]
    fn dictionary() {
        assert!(check_word("spreadsheet"));
        assert!(check_word("Hello"));
        assert!(check_word("doesn't"));
        assert!(!check_word("wrold"));
        assert!(suggest("wrold").iter().any(|s| s == "world"));
        assert!(suggest("recieve").iter().any(|s| s == "receive"));
    }

    #[test]
    fn scanning() {
        let mut wb = Workbook::new();
        wb.set_input(0, CellPos::new(0, 0), "This is fine").unwrap();
        wb.set_input(0, CellPos::new(1, 0), "Teh qick fox").unwrap();
        wb.set_input(0, CellPos::new(2, 0), "=\"formulas are skippd\"").unwrap();
        wb.set_input(0, CellPos::new(3, 0), "42").unwrap();
        let cells = cell_order(&wb, 0, CellPos::new(0, 0), None);
        assert_eq!(cells.len(), 2);
        let ignore = HashSet::new();
        let (idx, i) = next_issue(&wb, 0, &cells, 0, 0, &ignore, &check_word).unwrap();
        assert_eq!((i.pos, i.word.as_str()), (CellPos::new(1, 0), "Teh"));
        let (_, i2) = next_issue(&wb, 0, &cells, idx, i.end, &ignore, &check_word).unwrap();
        assert_eq!(i2.word, "qick");
        // starting from the second cell wraps around to the first
        let cells = cell_order(&wb, 0, CellPos::new(1, 0), None);
        assert_eq!(cells[0], CellPos::new(1, 0));
        assert_eq!(match_case("Teh", "the"), "The");
    }
}
