use engine::condfmt;
use engine::ops::{self, FindOptions, PasteOptions, PasteWhat, Scope, SortKey, UndoStack};
use engine::style::Color;
use engine::workbook::{CfOperator, CfRule, CfStyle, ConditionalFormat, DataValidation, ValidationRule};
use engine::{CellPos, Rect, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}
fn r(a1: &str) -> Rect {
    Rect::parse(a1).unwrap()
}
fn set(wb: &mut Workbook, a1: &str, t: &str) {
    wb.set_input(0, p(a1), t).unwrap();
}
fn val(wb: &Workbook, a1: &str) -> Value {
    wb.value(0, p(a1))
}
fn num(n: f64) -> Value {
    Value::Number(n)
}
fn text(s: &str) -> Value {
    Value::Text(s.into())
}

#[test]
fn undo_redo_cells_and_structure() {
    let mut wb = Workbook::new();
    let mut u = UndoStack::new();
    u.record(&wb, Scope::Cells { sheet: 0, rects: vec![r("A1")] }, "Typing", 0, r("A1"));
    set(&mut wb, "A1", "5");
    u.record(&wb, Scope::Cells { sheet: 0, rects: vec![r("A2")] }, "Typing", 0, r("A2"));
    set(&mut wb, "A2", "=A1*2");
    wb.recalculate();
    assert_eq!(val(&wb, "A2"), num(10.0));

    u.record(&wb, Scope::Workbook, "Insert rows", 0, r("A1"));
    wb.insert_rows(0, 0, 2);
    assert_eq!(wb.input_text(0, p("A4")), "=A3*2");

    u.undo(&mut wb).unwrap();
    assert_eq!(wb.input_text(0, p("A2")), "=A1*2");
    let res = u.undo(&mut wb).unwrap();
    assert_eq!(res.selection, r("A2"));
    assert!(wb.cell(0, p("A2")).is_none());
    u.undo(&mut wb).unwrap();
    assert!(wb.cell(0, p("A1")).is_none());
    assert!(!u.can_undo());

    u.redo(&mut wb).unwrap();
    u.redo(&mut wb).unwrap();
    assert_eq!(val(&wb, "A2"), num(10.0));
    u.redo(&mut wb).unwrap();
    assert_eq!(val(&wb, "A4"), num(10.0));
    assert!(!u.can_redo());
}

#[test]
fn undo_sheet_scope() {
    let mut wb = Workbook::new();
    let mut u = UndoStack::new();
    u.record(&wb, Scope::Sheet(0), "Merge", 0, r("A1:B2"));
    wb.sheets[0].merges.push(r("A1:B2"));
    u.undo(&mut wb);
    assert!(wb.sheets[0].merges.is_empty());
    u.redo(&mut wb);
    assert_eq!(wb.sheets[0].merges.len(), 1);
}

#[test]
fn copy_paste_shifts_relative_refs() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "2");
    set(&mut wb, "B1", "=A1*10+$A$1");
    wb.recalculate();
    let clip = ops::copy(&wb, 0, r("B1"), false);
    let area = ops::paste(&mut wb, 0, r("C2"), &clip, PasteOptions::default());
    assert_eq!(area, r("C2"));
    assert_eq!(wb.input_text(0, p("C2")), "=B2*10+$A$1");
    // tiling into a larger destination
    let area = ops::paste(&mut wb, 0, r("B2:B3"), &clip, PasteOptions::default());
    assert_eq!(area, r("B2:B3"));
    assert_eq!(wb.input_text(0, p("B3")), "=A3*10+$A$1");
    wb.recalculate();
    assert_eq!(val(&wb, "B2"), num(21.0));
}

#[test]
fn paste_special_values_transpose_and_ops() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "2");
    set(&mut wb, "A3", "=A1+A2");
    wb.recalculate();
    let clip = ops::copy(&wb, 0, r("A1:A3"), false);
    let opts = PasteOptions { what: PasteWhat::Values, transpose: true, ..Default::default() };
    let area = ops::paste(&mut wb, 0, r("C1"), &clip, opts);
    assert_eq!(area, r("C1:E1"));
    assert_eq!(val(&wb, "E1"), num(3.0));
    assert!(wb.cell(0, p("E1")).unwrap().formula.is_none());

    set(&mut wb, "G1", "10");
    set(&mut wb, "G2", "10");
    let clip = ops::copy(&wb, 0, r("A1:A2"), false);
    let opts = PasteOptions { what: PasteWhat::Values, op: ops::PasteOp::Multiply, ..Default::default() };
    ops::paste(&mut wb, 0, r("G1"), &clip, opts);
    assert_eq!(val(&wb, "G1"), num(10.0));
    assert_eq!(val(&wb, "G2"), num(20.0));
}

#[test]
fn cut_paste_moves_and_rewrites_refs() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "7");
    set(&mut wb, "B1", "=A1+1");
    wb.recalculate();
    let clip = ops::copy(&wb, 0, r("A1"), true);
    ops::cut_paste(&mut wb, &clip, 0, p("D5"));
    wb.recalculate();
    assert!(wb.cell(0, p("A1")).is_none());
    assert_eq!(val(&wb, "D5"), num(7.0));
    assert_eq!(wb.input_text(0, p("B1")), "=D5+1");
    assert_eq!(val(&wb, "B1"), num(8.0));
}

#[test]
fn tsv_roundtrip() {
    let rows = ops::parse_tsv("a\tb\n1\t\"x\ty\"\n");
    assert_eq!(rows, vec![vec!["a".to_string(), "b".into()], vec!["1".into(), "x\ty".into()]]);
    let mut wb = Workbook::new();
    let area = ops::paste_text(&mut wb, 0, p("B2"), "1\t2\n3\thello");
    assert_eq!(area, r("B2:C3"));
    assert_eq!(val(&wb, "C3"), text("hello"));
    assert_eq!(val(&wb, "B3"), num(3.0));
    assert_eq!(ops::to_tsv(&wb, 0, r("B2:C3")), "1\t2\n3\thello\n");
}

#[test]
fn fill_series_detection() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "3");
    ops::fill(&mut wb, 0, r("A1:A2"), r("A1:A5"));
    assert_eq!(val(&wb, "A5"), num(9.0));

    set(&mut wb, "B1", "Item 1");
    ops::fill(&mut wb, 0, r("B1"), r("B1:B3"));
    assert_eq!(val(&wb, "B3"), text("Item 3"));

    set(&mut wb, "C1", "Mon");
    ops::fill(&mut wb, 0, r("C1"), r("C1:C8"));
    assert_eq!(val(&wb, "C2"), text("Tue"));
    assert_eq!(val(&wb, "C8"), text("Mon"));

    set(&mut wb, "D1", "January");
    ops::fill(&mut wb, 0, r("D1"), r("D1:F1"));
    assert_eq!(val(&wb, "F1"), text("March"));

    // single number copies
    set(&mut wb, "E1", "5");
    ops::fill(&mut wb, 0, r("E1"), r("E1:E3"));
    assert_eq!(val(&wb, "E3"), num(5.0));

    // formulas shift
    set(&mut wb, "G1", "=A1*2");
    ops::fill(&mut wb, 0, r("G1"), r("G1:G4"));
    assert_eq!(wb.input_text(0, p("G4")), "=A4*2");

    // fill upward continues backwards
    set(&mut wb, "H5", "10");
    set(&mut wb, "H6", "20");
    ops::fill(&mut wb, 0, r("H5:H6"), r("H3:H6"));
    assert_eq!(val(&wb, "H3"), num(-10.0));

    // dates step by a day
    set(&mut wb, "I1", "1/30/2024");
    ops::fill(&mut wb, 0, r("I1"), r("I1:I3"));
    let a = val(&wb, "I1").to_number().unwrap();
    assert_eq!(val(&wb, "I3"), num(a + 2.0));
}

#[test]
fn fill_down_and_right() {
    let mut wb = Workbook::new();
    set(&mut wb, "Z1", "1");
    set(&mut wb, "Z2", "2");
    set(&mut wb, "Z3", "3");
    set(&mut wb, "A1", "=Z1");
    ops::fill_down(&mut wb, 0, r("A1:A3"));
    wb.recalculate();
    assert_eq!(val(&wb, "A3"), num(3.0));
    set(&mut wb, "B1", "x");
    ops::fill_right(&mut wb, 0, r("B1:D1"));
    assert_eq!(val(&wb, "D1"), text("x"));
}

#[test]
fn sort_with_header() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "Name");
    set(&mut wb, "B1", "Score");
    set(&mut wb, "A2", "Bob");
    set(&mut wb, "B2", "7");
    set(&mut wb, "A3", "alice");
    set(&mut wb, "B3", "9");
    set(&mut wb, "A4", "Carl");
    set(&mut wb, "B4", "");
    set(&mut wb, "C2", "=B2*2");
    wb.recalculate();
    assert!(ops::guess_header(&wb, 0, r("A1:C4")));
    ops::sort(&mut wb, 0, r("A1:C4"), &[SortKey { col: 1, ascending: false }], true);
    wb.recalculate();
    assert_eq!(val(&wb, "A1"), text("Name"));
    assert_eq!(val(&wb, "A2"), text("alice"));
    assert_eq!(val(&wb, "A3"), text("Bob"));
    assert_eq!(val(&wb, "A4"), text("Carl")); // blanks last
    assert_eq!(wb.input_text(0, p("C3")), "=B3*2");
    assert_eq!(val(&wb, "C3"), num(14.0));
    ops::sort(&mut wb, 0, r("A1:C4"), &[SortKey { col: 0, ascending: true }], true);
    assert_eq!(val(&wb, "A2"), text("alice"));
    assert_eq!(val(&wb, "A4"), text("Carl"));
}

#[test]
fn find_and_replace() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "Hello world");
    set(&mut wb, "A2", "hello");
    set(&mut wb, "A3", "=IF(TRUE,\"hello\",\"x\")");
    wb.recalculate();
    let mut o = FindOptions { query: "hello".into(), ..Default::default() };
    assert_eq!(ops::find_all(&wb, &o).len(), 3);
    o.in_formulas = true;
    assert_eq!(ops::find_all(&wb, &o).len(), 3);
    o.in_formulas = false;
    o.match_case = true;
    assert_eq!(ops::find_all(&wb, &o), vec![(0, p("A2")), (0, p("A3"))]);
    o.match_case = false;
    o.entire_cell = true;
    assert_eq!(ops::find_all(&wb, &o), vec![(0, p("A2")), (0, p("A3"))]);
    o.entire_cell = false;
    let wild = FindOptions { query: "h*d".into(), ..Default::default() };
    assert_eq!(ops::find_all(&wb, &wild), vec![(0, p("A1"))]);

    let n = ops::replace_all(&mut wb, &FindOptions { query: "hello".into(), ..Default::default() }, "bye");
    assert_eq!(n, 3);
    wb.recalculate();
    assert_eq!(val(&wb, "A1"), text("bye world"));
    assert_eq!(val(&wb, "A3"), text("bye"));
    let next = ops::find_next(&wb, &FindOptions { query: "bye".into(), ..Default::default() }, (0, p("A1")));
    assert_eq!(next, Some((0, p("A2"))));
}

#[test]
fn data_tools() {
    let mut wb = Workbook::new();
    for (i, t) in ["x", "a", "b", "a", "c", "b"].iter().enumerate() {
        set(&mut wb, &format!("A{}", i + 1), t);
    }
    let n = ops::remove_duplicates(&mut wb, 0, r("A1:A6"), &[0], true);
    assert_eq!(n, 2);
    assert_eq!(val(&wb, "A4"), text("c"));
    assert!(wb.cell(0, p("A5")).is_none());

    set(&mut wb, "C1", "a,b,3");
    ops::text_to_columns(&mut wb, 0, r("C1"), &[','], false);
    assert_eq!(val(&wb, "E1"), num(3.0));

    set(&mut wb, "F1", "2");
    set(&mut wb, "G1", "=F1*F1+1");
    wb.recalculate();
    let x = ops::goal_seek(&mut wb, 0, p("G1"), 26.0, p("F1")).unwrap();
    assert!((x - 5.0).abs() < 1e-6, "{x}");

    set(&mut wb, "H1", "Header");
    set(&mut wb, "H2", "1");
    set(&mut wb, "H3", "2");
    assert_eq!(ops::autosum_range(&wb, 0, p("H4")), Some(r("H2:H3")));
    assert_eq!(ops::current_region(&wb, 0, p("H2")), r("C1:H3"));
}

#[test]
fn validation() {
    let mut wb = Workbook::new();
    wb.sheets[0].validations.push(DataValidation { range: r("A1:A5"), rule: ValidationRule::List(vec!["Yes".into(), "No".into()]) });
    wb.sheets[0].validations.push(DataValidation { range: r("B1"), rule: ValidationRule::WholeNumber { min: 1.0, max: 10.0 } });
    assert!(ops::validate_input(&wb, 0, p("A1"), "yes").is_ok());
    assert!(ops::validate_input(&wb, 0, p("A1"), "maybe").is_err());
    assert!(ops::validate_input(&wb, 0, p("B1"), "5").is_ok());
    assert!(ops::validate_input(&wb, 0, p("B1"), "5.5").is_err());
    assert!(ops::validate_input(&wb, 0, p("C1"), "anything").is_ok());
}

#[test]
fn navigation_edges() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "1");
    set(&mut wb, "A3", "1");
    set(&mut wb, "A10", "1");
    assert_eq!(ops::data_edge(&wb, 0, p("A1"), 1, 0), p("A3"));
    assert_eq!(ops::data_edge(&wb, 0, p("A3"), 1, 0), p("A10"));
    assert_eq!(ops::data_edge(&wb, 0, p("A10"), 1, 0), CellPos::new(engine::address::MAX_ROWS - 1, 0));
    assert_eq!(ops::data_edge(&wb, 0, p("C5"), 0, -1), p("A5"));
}

#[test]
fn conditional_formats() {
    let mut wb = Workbook::new();
    for i in 1..=5 {
        set(&mut wb, &format!("A{}", i), &i.to_string());
    }
    set(&mut wb, "B1", "apple");
    set(&mut wb, "B2", "pear");
    set(&mut wb, "B3", "apple");
    wb.recalculate();
    let red = CfStyle { fill: Some(Color::RED), ..Default::default() };
    let s = &mut wb.sheets[0];
    s.cond_formats.push(ConditionalFormat {
        range: r("A1:A5"),
        rule: CfRule::CellValue { op: CfOperator::Greater, a: "3".into(), b: String::new(), style: red.clone() },
    });
    s.cond_formats.push(ConditionalFormat { range: r("A1:A5"), rule: CfRule::DataBar { color: Color::BLUE } });
    s.cond_formats.push(ConditionalFormat { range: r("B1:B3"), rule: CfRule::Duplicates { unique: false, style: red.clone() } });
    s.cond_formats.push(ConditionalFormat {
        range: r("A1:A5"),
        rule: CfRule::TopBottom { top: true, rank: 1, percent: false, style: CfStyle { bold: true, ..Default::default() } },
    });
    s.cond_formats.push(ConditionalFormat {
        range: r("A1:A5"),
        rule: CfRule::Expression { formula: "A1=2".into(), style: CfStyle { italic: true, ..Default::default() } },
    });
    s.cond_formats.push(ConditionalFormat {
        range: r("A1:A5"),
        rule: CfRule::ColorScale { min: Color::WHITE, mid: None, max: Color::GREEN },
    });
    let f = condfmt::evaluate(&wb, 0, r("A1:Z100"));
    assert_eq!(f[&p("A4")].fill, Some(Color::RED));
    assert_eq!(f[&p("A1")].fill, Some(Color::WHITE)); // color scale only
    assert!(f[&p("A5")].bold);
    assert!(!f[&p("A4")].bold);
    assert!(f[&p("A2")].italic);
    assert_eq!(f[&p("A5")].data_bar.unwrap().0, 1.0);
    assert_eq!(f[&p("B1")].fill, Some(Color::RED));
    assert!(!f.contains_key(&p("B2")));
}
