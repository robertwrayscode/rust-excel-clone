//! A demo workbook (`--demo`) that exercises formulas, formatting,
//! conditional formats, charts and validation.

use engine::address::{CellPos, Rect};
use engine::style::{BorderKind, BorderLine, Color, HAlign};
use engine::workbook::{CfOperator, CfRule, CfStyle, Chart, ChartKind, ConditionalFormat, DataValidation, ValidationRule, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).expect("valid address")
}

fn r(a1: &str) -> Rect {
    Rect::parse(a1).expect("valid range")
}

pub fn build() -> Workbook {
    let mut wb = Workbook::new();
    wb.sheets[0].name = "Sales".into();
    let s = 0;
    let set = |wb: &mut Workbook, a: &str, t: &str| {
        let _ = wb.set_input(s, p(a), t);
    };

    set(&mut wb, "A1", "Quarterly Sales Report");
    wb.update_style(s, r("A1:G1"), |st| {
        st.bold = true;
        st.set_font_size(16.0);
        st.font_color = Some(Color(0x1F4E78));
    });
    wb.sheets[s].merges.push(r("A1:G1"));
    wb.update_style(s, r("A1:G1"), |st| st.h_align = HAlign::Center);

    let header = ["Region", "Q1", "Q2", "Q3", "Q4", "Total", "Growth"];
    for (i, h) in header.iter().enumerate() {
        let _ = wb.set_input(s, CellPos::new(2, i as u32), h);
    }
    wb.update_style(s, r("A3:G3"), |st| {
        st.bold = true;
        st.fill = Some(Color(0x217346));
        st.font_color = Some(Color::WHITE);
        st.h_align = HAlign::Center;
    });

    let data: [(&str, [f64; 4]); 6] = [
        ("North", [12500.0, 14200.0, 15900.0, 17300.0]),
        ("South", [9800.0, 9400.0, 10100.0, 11800.0]),
        ("East", [15200.0, 16800.0, 15100.0, 19400.0]),
        ("West", [7600.0, 8900.0, 9700.0, 10200.0]),
        ("Central", [11300.0, 10800.0, 12600.0, 13900.0]),
        ("Online", [5400.0, 8700.0, 12300.0, 16800.0]),
    ];
    for (i, (name, q)) in data.iter().enumerate() {
        let row = 3 + i as u32;
        let _ = wb.set_input(s, CellPos::new(row, 0), name);
        for (j, v) in q.iter().enumerate() {
            wb.set_value(s, CellPos::new(row, 1 + j as u32), engine::Value::Number(*v));
        }
        let n = row + 1;
        let _ = wb.set_input(s, CellPos::new(row, 5), &format!("=SUM(B{n}:E{n})"));
        let _ = wb.set_input(s, CellPos::new(row, 6), &format!("=E{n}/B{n}-1"));
    }
    set(&mut wb, "A10", "Total");
    for (c, l) in ["B", "C", "D", "E", "F"].iter().enumerate() {
        let _ = wb.set_input(s, CellPos::new(9, 1 + c as u32), &format!("=SUM({l}4:{l}9)"));
    }
    set(&mut wb, "G10", "=E10/B10-1");
    set(&mut wb, "A11", "Average");
    for (c, l) in ["B", "C", "D", "E", "F"].iter().enumerate() {
        let _ = wb.set_input(s, CellPos::new(10, 1 + c as u32), &format!("=AVERAGE({l}4:{l}9)"));
    }
    wb.update_style(s, r("B4:F11"), |st| st.num_fmt = "$#,##0".into());
    wb.update_style(s, r("G4:G10"), |st| st.num_fmt = "0.0%".into());
    wb.update_style(s, r("A10:G11"), |st| st.bold = true);
    let thin = Some(BorderLine { kind: BorderKind::Thin, color: Color(0xA6A6A6) });
    wb.update_style(s, r("A3:G11"), |st| {
        st.borders.left = thin;
        st.borders.right = thin;
        st.borders.top = thin;
        st.borders.bottom = thin;
    });
    let dbl = Some(BorderLine { kind: BorderKind::Double, color: Color::BLACK });
    wb.update_style(s, r("A9:G9"), |st| st.borders.bottom = dbl);
    wb.update_style(s, r("A10:G10"), |st| st.borders.top = dbl);

    // Notes and info block
    set(&mut wb, "I3", "Target");
    set(&mut wb, "J3", "60000");
    wb.update_style(s, r("J3"), |st| st.num_fmt = "$#,##0".into());
    set(&mut wb, "I4", "Regions on target");
    set(&mut wb, "J4", "=COUNTIF(F4:F9,\">=\"&J3)");
    set(&mut wb, "I5", "Best region");
    set(&mut wb, "J5", "=INDEX(A4:A9,MATCH(MAX(F4:F9),F4:F9,0))");
    set(&mut wb, "I6", "Report date");
    set(&mut wb, "J6", "3/31/2026");
    set(&mut wb, "I7", "Status");
    set(&mut wb, "J7", "Final");
    set(&mut wb, "I8", "Note");
    set(&mut wb, "J8", "Online sales more than tripled over the year, driven by the new web store.");
    wb.update_style(s, r("I3:I8"), |st| st.bold = true);
    wb.sheets[s].validations.push(DataValidation {
        range: r("J7"),
        rule: ValidationRule::List(vec!["Draft".into(), "Review".into(), "Final".into()]),
    });
    wb.sheets[s].comments.insert(p("J3"), "Annual target per region.".into());
    let _ = wb.define_name("Target", None, "Sales!$J$3");

    // Conditional formats (formulas stored without '=').
    let sheet = &mut wb.sheets[s];
    sheet.cond_formats.push(ConditionalFormat { range: r("F4:F9"), rule: CfRule::DataBar { color: Color(0x638EC6) } });
    sheet.cond_formats.push(ConditionalFormat {
        range: r("B4:E9"),
        rule: CfRule::ColorScale { min: Color(0xF8696B), mid: Some(Color(0xFFEB84)), max: Color(0x63BE7B) },
    });
    sheet.cond_formats.push(ConditionalFormat {
        range: r("G4:G9"),
        rule: CfRule::CellValue {
            op: CfOperator::Greater,
            a: "0.5".into(),
            b: String::new(),
            style: CfStyle { font_color: Some(Color(0x006100)), fill: Some(Color(0xC6EFCE)), bold: true, italic: false },
        },
    });
    sheet.cond_formats.push(ConditionalFormat {
        range: r("A4:A9"),
        rule: CfRule::Expression {
            formula: "$F4>=Target".into(),
            style: CfStyle { font_color: Some(Color(0x9C5700)), fill: Some(Color(0xFFEB9C)), bold: false, italic: false },
        },
    });

    sheet.charts.push(Chart {
        kind: ChartKind::Column,
        title: "Sales by Quarter".into(),
        data: r("A3:E9"),
        has_header: true,
        series_in_columns: false,
        anchor: p("A13"),
        width: 520.0,
        height: 280.0,
    });
    sheet.charts.push(Chart {
        kind: ChartKind::Pie,
        title: "Q1 Share by Region".into(),
        data: r("A3:B9"),
        has_header: true,
        series_in_columns: true,
        anchor: p("I13"),
        width: 360.0,
        height: 280.0,
    });

    sheet.col_widths.insert(0, 90.0);
    sheet.col_widths.insert(8, 120.0);
    sheet.col_widths.insert(9, 110.0);
    sheet.row_heights.insert(0, 30.0);
    sheet.freeze_rows = 3;
    sheet.tab_color = Some(Color(0x217346));

    // Second sheet: a simple list with dates and text.
    let s2 = wb.add_sheet(Some("Orders"));
    for (i, h) in ["Order", "Date", "Customer", "Amount", "Paid"].iter().enumerate() {
        let _ = wb.set_input(s2, CellPos::new(0, i as u32), h);
    }
    wb.update_style(s2, r("A1:E1"), |st| {
        st.bold = true;
        st.borders.bottom = Some(BorderLine { kind: BorderKind::Medium, color: Color::BLACK });
    });
    let customers = ["Acme Corp", "Globex", "Initech", "Umbrella", "Hooli", "Stark Industries", "Wayne Enterprises", "Acme Corp"];
    for (i, c) in customers.iter().enumerate() {
        let row = 1 + i as u32;
        let _ = wb.set_input(s2, CellPos::new(row, 0), &format!("{}", 1001 + i));
        let _ = wb.set_input(s2, CellPos::new(row, 1), &format!("1/{}/2026", 3 + i * 3));
        let _ = wb.set_input(s2, CellPos::new(row, 2), c);
        let _ = wb.set_input(s2, CellPos::new(row, 3), &format!("{}", 250.0 + (i as f64 * 137.5) % 900.0));
        let _ = wb.set_input(s2, CellPos::new(row, 4), if i % 3 == 0 { "No" } else { "Yes" });
    }
    let _ = wb.set_input(s2, p("C11"), "Total");
    let _ = wb.set_input(s2, p("D11"), "=SUM(D2:D9)");
    wb.update_style(s2, r("D2:D11"), |st| st.num_fmt = "#,##0.00".into());
    wb.update_style(s2, r("C11:D11"), |st| st.bold = true);
    let sh2 = &mut wb.sheets[s2];
    sh2.col_widths.insert(2, 130.0);
    sh2.cond_formats.push(ConditionalFormat {
        range: r("E2:E9"),
        rule: CfRule::TextContains { text: "No".into(), style: CfStyle { font_color: Some(Color(0x9C0006)), fill: Some(Color(0xFFC7CE)), bold: false, italic: false } },
    });
    sh2.cond_formats.push(ConditionalFormat {
        range: r("C2:C9"),
        rule: CfRule::Duplicates { unique: false, style: CfStyle { font_color: None, fill: Some(Color(0xFCE4D6)), bold: false, italic: true } },
    });
    sh2.validations.push(DataValidation { range: r("E2:E100"), rule: ValidationRule::List(vec!["Yes".into(), "No".into()]) });
    sh2.hyperlinks.insert(p("G1"), "#Sales!A1".into());
    let _ = wb.set_input(s2, p("G1"), "Back to Sales");

    wb.recalculate();
    wb
}
