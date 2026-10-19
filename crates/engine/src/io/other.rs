//! Read-only import of .xls / .xlsb / .ods via calamine (values and formulas).

use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use crate::address::{CellPos, Rect};
use crate::value::{ErrorKind, Value};
use crate::workbook::Workbook;

fn error_kind(e: &calamine::CellErrorType) -> ErrorKind {
    use calamine::CellErrorType as C;
    match e {
        C::Div0 => ErrorKind::Div0,
        C::NA => ErrorKind::NA,
        C::Name => ErrorKind::Name,
        C::Null => ErrorKind::Null,
        C::Num => ErrorKind::Num,
        C::Ref => ErrorKind::Ref,
        C::GettingData => ErrorKind::GettingData,
        _ => ErrorKind::Value,
    }
}

/// Read .xls/.xlsb/.ods (values, date formats and formulas only). Does not recalculate.
pub fn read_other(path: &Path) -> Result<Workbook, String> {
    let mut book = open_workbook_auto(path).map_err(|e| format!("Cannot open {}: {}", path.display(), e))?;
    let mut wb = Workbook::empty();
    for name in book.sheet_names() {
        let idx = wb.add_sheet(Some(&name));
        if let Ok(range) = book.worksheet_range(&name) {
            let (r0, c0) = range.start().unwrap_or((0, 0));
            for (r, c, d) in range.used_cells() {
                let p = CellPos::new(r0 + r as u32, c0 + c as u32);
                let v = match d {
                    Data::Int(i) => Value::Number(*i as f64),
                    Data::Float(f) => Value::Number(*f),
                    Data::String(s) => Value::Text(s.clone()),
                    Data::Bool(b) => Value::Bool(*b),
                    Data::DateTime(dt) => {
                        let fmt = if dt.as_f64().fract() == 0.0 { "m/d/yyyy" } else { "m/d/yyyy h:mm" };
                        wb.update_style(idx, Rect::single(p), |s| s.num_fmt = fmt.to_string());
                        Value::Number(dt.as_f64())
                    }
                    Data::DateTimeIso(s) | Data::DurationIso(s) => {
                        crate::datetime::parse_datetime_text(s).map(Value::Number).unwrap_or_else(|| Value::Text(s.clone()))
                    }
                    Data::Error(e) => Value::Error(error_kind(e)),
                    Data::Empty => continue,
                };
                wb.set_value(idx, p, v);
            }
        }
        if let Ok(formulas) = book.worksheet_formula(&name) {
            let (r0, c0) = formulas.start().unwrap_or((0, 0));
            for (r, c, f) in formulas.used_cells() {
                if f.is_empty() {
                    continue;
                }
                let p = CellPos::new(r0 + r as u32, c0 + c as u32);
                if let Ok(e) = crate::parser::parse_formula(f.trim_start_matches('=')) {
                    let cell = wb.sheets[idx].cells.entry(p).or_default();
                    cell.formula = Some(Box::new(e));
                }
            }
        }
    }
    if wb.sheets.is_empty() {
        wb.add_sheet(None);
    }
    Ok(wb)
}
