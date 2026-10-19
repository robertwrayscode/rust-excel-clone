use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use engine::address::Rect;
use engine::style::{BorderKind, BorderLine, Borders, Color, HAlign, VAlign};
use engine::workbook::{
    AutoFilter, CfOperator, CfRule, CfStyle, Chart, ChartKind, ConditionalFormat, DataValidation, ValidationRule,
};
use engine::{io, CellPos, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}

fn r(a1: &str) -> Rect {
    Rect::parse(a1).unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("engine_io_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn build() -> Workbook {
    let mut wb = Workbook::new();
    wb.rename_sheet(0, "Data").unwrap();
    let s2 = wb.add_sheet(Some("Other Sheet"));
    // values
    wb.set_input(0, p("A1"), "Item").unwrap();
    wb.set_input(0, p("B1"), "Q1").unwrap();
    wb.set_input(0, p("C1"), "Q2").unwrap();
    let rows = [("Apples", 10.0, 12.5), ("Pears", 20.0, 7.25), ("Plums", 30.0, 1.0)];
    for (i, (name, a, b)) in rows.iter().enumerate() {
        let row = i as u32 + 1;
        wb.set_value(0, CellPos::new(row, 0), Value::Text(name.to_string()));
        wb.set_value(0, CellPos::new(row, 1), Value::Number(*a));
        wb.set_value(0, CellPos::new(row, 2), Value::Number(*b));
    }
    wb.set_value(0, p("D2"), Value::Bool(true));
    wb.set_value(0, p("D3"), Value::Error(engine::ErrorKind::NA));
    wb.set_value(0, p("D4"), Value::Text("multi\nline & <xml> \"quoted\"".into()));
    // formulas
    wb.set_formula(0, p("B5"), "SUM(B2:B4)").unwrap();
    wb.set_formula(0, p("C5"), "SUM(C2:C4)*Rate").unwrap();
    wb.set_formula(0, p("E2"), "A2&\"-\"&B2").unwrap();
    wb.set_formula(0, p("E3"), "IFS(B3>15,\"big\",TRUE,\"small\")").unwrap();
    wb.set_formula(0, p("E4"), "CHOOSE(3,B2,B3,B4)").unwrap();
    // newer functions (need _xlfn. prefixes in the file)
    wb.set_formula(0, p("E5"), "CONCAT(A2,\"-\",TEXTJOIN(\",\",TRUE,B2:C2))").unwrap();
    wb.set_formula(0, p("E6"), "XLOOKUP(\"Plums\",A2:A4,B2:B4)").unwrap();
    wb.set_formula(0, p("E7"), "SEQUENCE(3)").unwrap();
    // spilled array
    wb.set_formula(0, p("F1"), "B2:B4*2").unwrap();
    wb.set_formula(0, p("G1"), "'Other Sheet'!A1*2").unwrap();
    wb.set_value(s2, p("A1"), Value::Number(21.0));
    // date
    wb.set_input(0, p("H1"), "2024-01-15").unwrap();
    // names
    wb.define_name("Rate", None, "Data!$B$2/10").unwrap();
    wb.define_name("LocalName", Some(1), "'Other Sheet'!$A$1").unwrap();
    // styles
    wb.update_style(0, r("A1:C1"), |s| {
        s.bold = true;
        s.italic = true;
        s.underline = true;
        s.fill = Some(Color(0x4472C4));
        s.font_color = Some(Color(0xFFFFFF));
        s.h_align = HAlign::Center;
        s.v_align = VAlign::Center;
        s.set_font_size(14.0);
        s.font_name = Some("Arial".into());
        s.borders = Borders {
            bottom: Some(BorderLine { kind: BorderKind::Double, color: Color(0xFF0000) }),
            top: Some(BorderLine { kind: BorderKind::Thin, color: Color::BLACK }),
            ..Default::default()
        };
    });
    wb.update_style(0, r("B2:C5"), |s| s.num_fmt = "#,##0.00".into());
    wb.update_style(0, r("D4"), |s| {
        s.wrap = true;
        s.strike = true;
        s.indent = 2;
        s.h_align = HAlign::Left;
    });
    wb.update_style(0, r("A4"), |s| s.rotation = 45);
    wb.update_style(0, r("A3"), |s| s.rotation = -30);
    wb.update_style(0, r("A9"), |s| s.fill = Some(Color(0xFFFF00)));
    wb.update_style(0, r("B9"), |s| s.num_fmt = "0.0%;[Red]-0.0%".into());
    wb.set_value(0, p("B9"), Value::Number(-0.125));
    // sheet features
    {
        let sh = &mut wb.sheets[0];
        sh.col_widths.insert(0, 120.0);
        sh.col_widths.insert(3, 200.0);
        sh.row_heights.insert(0, 30.0);
        sh.hidden_rows.insert(7);
        sh.hidden_rows.insert(3); // "Plums" is filtered out by the autofilter below
        sh.hidden_cols.insert(9);
        sh.merges.push(r("A7:C7"));
        sh.freeze_rows = 1;
        sh.freeze_cols = 1;
        sh.tab_color = Some(Color(0x00B050));
        sh.zoom = 1.25;
        sh.show_gridlines = false;
        sh.comments.insert(p("B2"), "A note\nwith two lines".into());
        sh.hyperlinks.insert(p("A10"), "https://example.com/".into());
        sh.hyperlinks.insert(p("A11"), "#'Other Sheet'!A1".into());
        let mut allowed = HashMap::new();
        allowed.insert(0u32, ["Apples".to_string(), "Pears".to_string()].into_iter().collect::<HashSet<_>>());
        sh.autofilter = Some(AutoFilter { range: r("A1:C4"), allowed });
        sh.cond_formats.push(ConditionalFormat {
            range: r("B2:B4"),
            rule: CfRule::CellValue {
                op: CfOperator::Greater,
                a: "15".into(),
                b: String::new(),
                style: CfStyle { font_color: Some(Color(0x9C0006)), fill: Some(Color(0xFFC7CE)), bold: true, italic: false },
            },
        });
        sh.cond_formats.push(ConditionalFormat {
            range: r("A2:A4"),
            rule: CfRule::Expression {
                formula: "$B2>15".into(),
                style: CfStyle { font_color: None, fill: Some(Color(0xC6EFCE)), bold: false, italic: true },
            },
        });
        sh.cond_formats.push(ConditionalFormat {
            range: r("C2:C4"),
            rule: CfRule::ColorScale { min: Color(0xF8696B), mid: Some(Color(0xFFEB84)), max: Color(0x63BE7B) },
        });
        sh.cond_formats.push(ConditionalFormat { range: r("B2:B4"), rule: CfRule::DataBar { color: Color(0x638EC6) } });
        sh.cond_formats.push(ConditionalFormat {
            range: r("C2:C4"),
            rule: CfRule::TopBottom { top: true, rank: 1, percent: false, style: CfStyle { bold: true, ..Default::default() } },
        });
        sh.validations.push(DataValidation {
            range: r("I2:I4"),
            rule: ValidationRule::List(vec!["Yes".into(), "No".into(), "Maybe".into()]),
        });
        sh.validations.push(DataValidation { range: r("J2:J4"), rule: ValidationRule::ListRange(r("A2:A4")) });
        sh.validations.push(DataValidation { range: r("K2"), rule: ValidationRule::WholeNumber { min: 1.0, max: 10.0 } });
        sh.charts.push(Chart {
            kind: ChartKind::Column,
            title: "Sales".into(),
            data: r("A1:C4"),
            has_header: true,
            series_in_columns: true,
            anchor: p("E10"),
            width: 480.0,
            height: 288.0,
        });
        sh.charts.push(Chart {
            kind: ChartKind::Line,
            title: String::new(),
            data: r("A1:C4"),
            has_header: true,
            series_in_columns: false,
            anchor: p("M10"),
            width: 400.0,
            height: 250.0,
        });
    }
    wb.sheets[1].hidden = false;
    let s3 = wb.add_sheet(Some("Hidden"));
    wb.sheets[s3].hidden = true;
    wb.recalculate();
    wb
}

#[test]
fn xlsx_roundtrip() {
    let wb = build();
    assert_eq!(wb.value(0, p("B5")), Value::Number(60.0));
    let path = tmp("roundtrip.xlsx");
    io::write_xlsx(&wb, &path).unwrap();
    let mut back = io::read_xlsx(&path).unwrap();

    // sheets
    let names: Vec<&str> = back.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Data", "Other Sheet", "Hidden"]);
    assert!(back.sheets[2].hidden);
    assert!(!back.sheets[0].hidden);

    // cached values survive before recalculation
    assert_eq!(back.value(0, p("B5")), Value::Number(60.0));
    // values
    assert_eq!(back.value(0, p("A2")), Value::Text("Apples".into()));
    assert_eq!(back.value(0, p("C3")), Value::Number(7.25));
    assert_eq!(back.value(0, p("D2")), Value::Bool(true));
    assert_eq!(back.value(0, p("D3")), Value::Error(engine::ErrorKind::NA));
    assert_eq!(back.value(0, p("D4")), Value::Text("multi\nline & <xml> \"quoted\"".into()));
    assert_eq!(back.value(0, p("H1")), wb.value(0, p("H1")));

    // formulas
    for a in ["B5", "C5", "E2", "E3", "E4", "E5", "E6", "E7", "F1", "G1"] {
        assert_eq!(back.input_text(0, p(a)), wb.input_text(0, p(a)), "formula at {}", a);
    }
    // recalc
    back.set_value(0, p("B2"), Value::Number(100.0));
    back.recalculate();
    assert_eq!(back.value(0, p("B5")), Value::Number(150.0));
    assert_eq!(back.value(0, p("C5")), Value::Number(20.75 * 10.0));
    assert_eq!(back.value(0, p("E2")), Value::Text("Apples-100".into()));
    assert_eq!(back.value(0, p("E3")), Value::Text("big".into()));
    assert_eq!(back.value(0, p("E4")), Value::Number(30.0));
    assert_eq!(back.value(0, p("F1")), Value::Number(200.0));
    assert_eq!(back.value(0, p("F3")), Value::Number(60.0));
    assert_eq!(back.value(0, p("G1")), Value::Number(42.0));

    // styles
    for a in ["A1", "B2", "C5", "D4", "A3", "A4", "A9", "B9", "H1"] {
        assert_eq!(back.cell_style(0, p(a)), wb.cell_style(0, p(a)), "style at {}", a);
    }
    assert_eq!(back.cell_style(0, p("H1")).num_fmt, "m/d/yyyy");

    // sheet features
    let (a, b) = (&wb.sheets[0], &back.sheets[0]);
    assert_eq!(b.col_widths.get(&0), Some(&120.0));
    assert_eq!(b.col_widths.get(&3), Some(&200.0));
    assert_eq!(b.row_heights.get(&0), Some(&30.0));
    assert_eq!(b.hidden_rows, a.hidden_rows);
    assert_eq!(b.hidden_cols, a.hidden_cols);
    assert_eq!(b.merges, a.merges);
    assert_eq!((b.freeze_rows, b.freeze_cols), (1, 1));
    assert_eq!(b.tab_color, a.tab_color);
    assert_eq!(b.zoom, 1.25);
    assert!(!b.show_gridlines);
    assert_eq!(b.comments, a.comments);
    assert_eq!(b.hyperlinks, a.hyperlinks);
    assert_eq!(b.autofilter.as_ref().unwrap().range, r("A1:C4"));
    assert_eq!(b.autofilter.as_ref().unwrap().allowed, a.autofilter.as_ref().unwrap().allowed);
    // rust_xlsxwriter groups rules by range, so only compare membership.
    assert_eq!(b.cond_formats.len(), a.cond_formats.len());
    for cf in &a.cond_formats {
        assert!(b.cond_formats.contains(cf), "missing {:?}", cf);
    }
    assert_eq!(b.validations, a.validations);
    assert_eq!(b.charts, a.charts);

    // names
    assert_eq!(back.names.len(), 2);
    let rate = back.find_name("Rate", 0).unwrap();
    assert_eq!(rate.scope, None);
    assert_eq!(rate.formula.to_formula(), "Data!$B$2/10");
    let local = back.find_name("LocalName", 1).unwrap();
    assert_eq!(local.scope, Some(1));
    assert!(back.find_name("LocalName", 0).is_none());

    // write again: second-generation file equals first in content
    let path2 = tmp("roundtrip2.xlsx");
    io::write_xlsx(&back, &path2).unwrap();
    let again = io::open(&path2).unwrap();
    assert_eq!(again.sheets[0].charts, back.sheets[0].charts);
    assert_eq!(again.value(0, p("B5")), Value::Number(150.0));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&path2);
}

#[test]
fn open_and_save_dispatch() {
    let wb = build();
    let path = tmp("dispatch.xlsx");
    io::save(&wb, 0, &path).unwrap();
    let back = io::open(&path).unwrap();
    assert_eq!(back.value(0, p("B5")), Value::Number(60.0));
    assert!(io::save(&wb, 0, &tmp("x.xlsm")).is_err());
    assert!(io::open(&tmp("missing.xlsx")).is_err());
    assert!(io::open(&tmp("file.unknown")).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn csv_roundtrip() {
    let mut wb = Workbook::new();
    wb.set_input(0, p("A1"), "Name").unwrap();
    wb.set_input(0, p("B1"), "Amount").unwrap();
    wb.set_input(0, p("C1"), "When").unwrap();
    wb.set_input(0, p("A2"), "Comma, inc").unwrap();
    wb.set_input(0, p("B2"), "1234.5").unwrap();
    wb.set_input(0, p("C2"), "1/15/2024").unwrap();
    wb.set_input(0, p("A3"), "Quote \"q\"").unwrap();
    wb.set_input(0, p("B3"), "=B2*2").unwrap();
    wb.set_input(0, p("D3"), "50%").unwrap();
    wb.update_style(0, r("B2:B3"), |s| s.num_fmt = "#,##0.00".into());
    wb.recalculate();

    let path = tmp("roundtrip.csv");
    io::save(&wb, 0, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("Name,Amount,When,\r\n"), "{}", text);
    assert!(text.contains("\"Comma, inc\",\"1,234.50\",1/15/2024,\r\n"), "{}", text);
    assert!(text.contains("\"Quote \"\"q\"\"\",\"2,469.00\",,50%"), "{}", text);

    let back = io::open(&path).unwrap();
    assert_eq!(back.sheets[0].name, "roundtrip");
    assert_eq!(back.value(0, p("A2")), Value::Text("Comma, inc".into()));
    assert_eq!(back.value(0, p("B2")), Value::Number(1234.5));
    assert_eq!(back.value(0, p("C2")), Value::Number(45306.0));
    assert_eq!(back.cell_style(0, p("C2")).num_fmt, "m/d/yyyy");
    assert_eq!(back.value(0, p("B3")), Value::Number(2469.0));
    assert_eq!(back.value(0, p("D3")), Value::Number(0.5));
    assert_eq!(back.value(0, p("A3")), Value::Text("Quote \"q\"".into()));

    // TSV
    let tpath = tmp("roundtrip.tsv");
    io::save(&wb, 0, &tpath).unwrap();
    let t = std::fs::read_to_string(&tpath).unwrap();
    assert!(t.starts_with("Name\tAmount\tWhen\t\r\n"));
    let back = io::open(&tpath).unwrap();
    assert_eq!(back.value(0, p("A2")), Value::Text("Comma, inc".into()));
    assert_eq!(back.value(0, p("B2")), Value::Number(1234.5));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&tpath);
}

#[test]
fn xlsx_written_by_writer_has_xlfn_prefixes() {
    let wb = build();
    let path = tmp("xlfn.xlsx");
    io::write_xlsx(&wb, &path).unwrap();
    let f = std::fs::File::open(&path).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut s = String::new();
    use std::io::Read;
    z.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut s).unwrap();
    assert!(s.contains("_xlfn.CONCAT("), "{}", s);
    assert!(s.contains("_xlfn.TEXTJOIN("));
    assert!(s.contains("_xlfn.IFS("));
    assert!(s.contains("_xlfn.XLOOKUP("));
    assert!(s.contains("_xlfn.SEQUENCE("));
    let _ = std::fs::remove_file(&path);
}

fn close(a: Color, b: Color) -> bool {
    (a.r() as i32 - b.r() as i32).abs() <= 2 && (a.g() as i32 - b.g() as i32).abs() <= 2 && (a.b() as i32 - b.b() as i32).abs() <= 2
}

/// A hand-written package resembling what Excel itself produces: theme colors,
/// shared formulas, rich shared strings, built-in number formats, a dynamic
/// array with cached spill values, legacy comments and scoped names.
#[test]
fn reads_excel_style_package() {
    use std::io::Write;
    let path = tmp("excel_style.xlsx");
    let files: Vec<(&str, &str)> = vec![
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/></Types>"#),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#),
        ("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="First" sheetId="1" r:id="rId1"/><sheet name="Second" sheetId="2" state="hidden" r:id="rId2"/></sheets><definedNames><definedName name="_xlnm._FilterDatabase" localSheetId="0" hidden="1">First!$A$1:$B$3</definedName><definedName name="Total" localSheetId="1">First!$C$1</definedName><definedName name="Base">First!$A$1</definedName></definedNames></workbook>"#),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="/xl/worksheets/sheet2.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#),
        ("xl/theme/theme1.xml", r#"<?xml version="1.0" encoding="UTF-8"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme></a:themeElements></a:theme>"#),
        ("xl/styles.xml", r#"<?xml version="1.0" encoding="UTF-8"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="&quot;$&quot;#,##0.00"/></numFmts><fonts count="2"><font><sz val="11"/><color theme="1"/><name val="Calibri"/><family val="2"/><scheme val="minor"/></font><font><b/><sz val="11"/><color theme="4"/><name val="Calibri"/></font></fonts><fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor theme="5" tint="0.79998168889431442"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="2"><border><left/><right/><top/><bottom/><diagonal/></border><border><left style="thin"><color indexed="64"/></left><right/><top/><bottom style="medium"><color rgb="FFFF0000"/></bottom><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="4"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="164" fontId="1" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1"><alignment horizontal="center" vertical="top" wrapText="1" textRotation="135"/></xf><xf numFmtId="10" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><dxfs count="1"><dxf><font><color rgb="FF9C0006"/></font><fill><patternFill><bgColor rgb="FFFFC7CE"/></patternFill></fill></dxf></dxfs></styleSheet>"#),
        ("xl/sharedStrings.xml", r#"<?xml version="1.0" encoding="UTF-8"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="3"><si><t>Header</t></si><si><r><rPr><b/></rPr><t>Rich </t></r><r><t xml:space="preserve">text</t></r><rPh sb="0" eb="1"><t>ignored</t></rPh></si><si><t>Line_x000D_break</t></si></sst>"#),
        ("xl/worksheets/sheet1.xml", r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetPr><tabColor theme="4"/></sheetPr><sheetViews><sheetView tabSelected="1" workbookViewId="0"><pane xSplit="1" ySplit="2" topLeftCell="B3" activePane="bottomRight" state="frozen"/></sheetView></sheetViews><sheetFormatPr defaultRowHeight="15"/><cols><col min="1" max="2" width="20.7109375" customWidth="1"/><col min="4" max="4" width="9.140625" hidden="1"/></cols><sheetData><row r="1" ht="24" customHeight="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s" s="2"><v>1</v></c><c r="C1"><f>SUM(A2:A4)</f><v>6</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><f t="shared" ref="B2:B4" si="0">A2*10</f><v>10</v></c><c r="D2" t="inlineStr"><is><t>inline</t></is></c><c r="E2" s="1"><v>45292</v></c><c r="F2" t="b"><v>1</v></c><c r="G2" t="e"><v>#DIV/0!</v></c><c r="H2" s="3"><v>0.25</v></c></row><row r="3" hidden="1"><c r="A3"><v>2</v></c><c r="B3"><f t="shared" si="0"/><v>20</v></c><c r="D3" t="s"><v>2</v></c></row><row r="4"><c r="A4"><v>3</v></c><c r="B4"><f t="shared" si="0"/><v>30</v></c><c r="C4" cm="1"><f t="array" ref="C4:C6">A2:A4*2</f><v>2</v></c></row><row r="5"><c r="C5"><v>4</v></c></row><row r="6"><c r="C6"><v>6</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="D5:E6"/></mergeCells><conditionalFormatting sqref="A2:A4 C2"><cfRule type="cellIs" dxfId="0" priority="1" operator="between"><formula>1</formula><formula>2</formula></cfRule></conditionalFormatting><dataValidations count="1"><dataValidation type="list" allowBlank="1" showInputMessage="1" showErrorMessage="1" sqref="F3:F5"><formula1>"a,b,c"</formula1></dataValidation></dataValidations><hyperlinks><hyperlink ref="D2" r:id="rId9"/><hyperlink ref="D3" location="Second!A1" display="go"/></hyperlinks><legacyDrawing r:id="rId2"/></worksheet>"#),
        ("xl/worksheets/_rels/sheet1.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.org/x?a=1&amp;b=2" TargetMode="External"/><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/></Relationships>"#),
        ("xl/comments1.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors><author>Jane</author></authors><commentList><comment ref=\"A2\" authorId=\"0\"><text><r><rPr><b/></rPr><t>Jane:</t></r><r><t xml:space=\"preserve\">\nCheck this</t></r></text></comment></commentList></comments>"),
        ("xl/worksheets/sheet2.xml", r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f>First!C1+Total</f><v>12</v></c></row></sheetData></worksheet>"#),
    ];
    {
        let f = std::fs::File::create(&path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in &files {
            z.start_file(*name, opts).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    let wb = io::read_xlsx(&path).unwrap();
    assert_eq!(wb.sheets.len(), 2);
    assert!(wb.sheets[1].hidden);
    let s = &wb.sheets[0];
    assert_eq!(s.tab_color, Some(Color(0x4472C4)));
    assert_eq!((s.freeze_rows, s.freeze_cols), (2, 1));
    assert_eq!(s.col_widths.get(&0), Some(&145.0));
    assert_eq!(s.col_widths.get(&1), Some(&145.0));
    assert!(s.hidden_cols.contains(&3));
    assert!(s.hidden_rows.contains(&2));
    assert_eq!(s.row_heights.get(&0), Some(&32.0));
    assert_eq!(s.merges, vec![r("D5:E6")]);
    assert_eq!(wb.value(0, p("A1")), Value::Text("Header".into()));
    assert_eq!(wb.value(0, p("B1")), Value::Text("Rich text".into()));
    assert_eq!(wb.value(0, p("D3")), Value::Text("Line\rbreak".into()));
    assert_eq!(wb.value(0, p("D2")), Value::Text("inline".into()));
    assert_eq!(wb.value(0, p("F2")), Value::Bool(true));
    assert_eq!(wb.value(0, p("G2")), Value::Error(engine::ErrorKind::Div0));
    assert_eq!(wb.input_text(0, p("B3")), "=A3*10");
    assert_eq!(wb.input_text(0, p("B4")), "=A4*10");
    assert_eq!(wb.input_text(0, p("C4")), "=A2:A4*2");
    // cached spill values are dropped so the engine can spill again
    assert!(wb.cell(0, p("C5")).is_none());
    // styles
    assert_eq!(wb.cell_style(0, p("E2")).num_fmt, "m/d/yyyy");
    assert_eq!(wb.cell_style(0, p("H2")).num_fmt, "0.00%");
    let st = wb.cell_style(0, p("B1"));
    assert!(st.bold);
    assert_eq!(st.font_color, Some(Color(0x4472C4)));
    assert!(close(st.fill.unwrap(), Color(0xFBE4D5)), "{:06X}", st.fill.unwrap().0);
    assert_eq!(st.num_fmt, "\"$\"#,##0.00");
    assert_eq!(st.h_align, HAlign::Center);
    assert_eq!(st.v_align, VAlign::Top);
    assert!(st.wrap);
    assert_eq!(st.rotation, -45);
    assert_eq!(st.borders.left, Some(BorderLine { kind: BorderKind::Thin, color: Color::BLACK }));
    assert_eq!(st.borders.bottom, Some(BorderLine { kind: BorderKind::Medium, color: Color(0xFF0000) }));
    assert_eq!(wb.cell_style(0, p("A2")), &engine::style::Style::default());
    // features
    assert_eq!(s.comments.get(&p("A2")).map(|s| s.as_str()), Some("Check this"));
    assert_eq!(s.hyperlinks.get(&p("D2")).map(|s| s.as_str()), Some("https://example.org/x?a=1&b=2"));
    assert_eq!(s.hyperlinks.get(&p("D3")).map(|s| s.as_str()), Some("#Second!A1"));
    assert_eq!(s.cond_formats.len(), 2);
    assert_eq!(s.cond_formats[1].range, r("C2"));
    assert_eq!(
        s.validations,
        vec![DataValidation { range: r("F3:F5"), rule: ValidationRule::List(vec!["a".into(), "b".into(), "c".into()]) }]
    );
    assert_eq!(wb.names.len(), 2);
    assert_eq!(wb.find_name("Total", 1).unwrap().scope, Some(1));

    let mut wb = wb;
    wb.recalculate();
    assert_eq!(wb.value(0, p("C1")), Value::Number(6.0));
    assert_eq!(wb.value(0, p("B4")), Value::Number(30.0));
    assert_eq!(wb.value(0, p("C6")), Value::Number(6.0));
    assert_eq!(wb.value(1, p("A1")), Value::Number(12.0));
    let _ = std::fs::remove_file(&path);
}
