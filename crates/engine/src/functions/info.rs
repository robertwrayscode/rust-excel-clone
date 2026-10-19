//! Information functions.

use super::Registry;
use crate::address::{col_to_letters, CellPos};
use crate::ast::Expr;
use crate::eval::{Area, Ctx, Val};
use crate::style::HAlign;
use crate::tryv;
use crate::value::{ErrorKind, Value};

const CAT: &str = "Information";

pub fn register(r: &mut Registry) {
    r.raw("CELL", 1, 2, cell, CAT, "CELL(info_type, [reference])", "Returns information about the formatting, location, or contents of a cell.");
    r.scalar("ERROR.TYPE", 1, 1, error_type, CAT, "ERROR.TYPE(error_val)", "Returns a number corresponding to an error type.");
    r.raw("INFO", 1, 1, info, CAT, "INFO(type_text)", "Returns information about the current operating environment.");
    r.scalar("ISBLANK", 1, 1, |a| Value::Bool(a[0].is_empty()), CAT, "ISBLANK(value)", "Returns TRUE if the value is blank.");
    r.scalar("ISERR", 1, 1, |a| Value::Bool(matches!(a[0], Value::Error(e) if e != ErrorKind::NA)), CAT, "ISERR(value)", "Returns TRUE if the value is any error value except #N/A.");
    r.scalar("ISERROR", 1, 1, |a| Value::Bool(a[0].is_error()), CAT, "ISERROR(value)", "Returns TRUE if the value is any error value.");
    r.scalar("ISEVEN", 1, 1, iseven, CAT, "ISEVEN(number)", "Returns TRUE if the number is even.");
    r.raw("ISFORMULA", 1, 1, isformula, CAT, "ISFORMULA(reference)", "Returns TRUE if there is a reference to a cell that contains a formula.");
    r.scalar("ISLOGICAL", 1, 1, |a| Value::Bool(matches!(a[0], Value::Bool(_))), CAT, "ISLOGICAL(value)", "Returns TRUE if the value is a logical value.");
    r.scalar("ISNA", 1, 1, |a| Value::Bool(matches!(a[0], Value::Error(ErrorKind::NA))), CAT, "ISNA(value)", "Returns TRUE if the value is the #N/A error value.");
    r.scalar("ISNONTEXT", 1, 1, |a| Value::Bool(!matches!(a[0], Value::Text(_))), CAT, "ISNONTEXT(value)", "Returns TRUE if the value is not text.");
    r.scalar("ISNUMBER", 1, 1, |a| Value::Bool(matches!(a[0], Value::Number(_))), CAT, "ISNUMBER(value)", "Returns TRUE if the value is a number.");
    r.scalar("ISODD", 1, 1, isodd, CAT, "ISODD(number)", "Returns TRUE if the number is odd.");
    r.raw("ISREF", 1, 1, isref, CAT, "ISREF(value)", "Returns TRUE if the value is a reference.");
    r.scalar("ISTEXT", 1, 1, |a| Value::Bool(matches!(a[0], Value::Text(_))), CAT, "ISTEXT(value)", "Returns TRUE if the value is text.");
    r.scalar("N", 1, 1, n_fn, CAT, "N(value)", "Returns a value converted to a number.");
    r.scalar("NA", 0, 0, |_| Value::Error(ErrorKind::NA), CAT, "NA()", "Returns the error value #N/A.");
    r.raw("SHEET", 0, 1, sheet, CAT, "SHEET([value])", "Returns the sheet number of the referenced sheet.");
    r.raw("SHEETS", 0, 1, sheets, CAT, "SHEETS([reference])", "Returns the number of sheets in a reference.");
    r.raw("TYPE", 1, 1, type_fn, CAT, "TYPE(value)", "Returns a number indicating the data type of a value.");
}

fn error_type(args: &[Value]) -> Value {
    match &args[0] {
        Value::Error(e) => Value::Number(e.code() as f64),
        _ => Value::Error(ErrorKind::NA),
    }
}

fn parity(args: &[Value]) -> Result<bool, ErrorKind> {
    let n = match &args[0] {
        Value::Bool(_) => return Err(ErrorKind::Value),
        v => v.to_number()?,
    };
    Ok((n.trunc() as i64).rem_euclid(2) == 0)
}

fn iseven(args: &[Value]) -> Value {
    Value::Bool(tryv!(parity(args)))
}

fn isodd(args: &[Value]) -> Value {
    Value::Bool(!tryv!(parity(args)))
}

fn n_fn(args: &[Value]) -> Value {
    match &args[0] {
        Value::Number(n) => Value::Number(*n),
        Value::Bool(b) => Value::Number(if *b { 1.0 } else { 0.0 }),
        Value::Error(e) => Value::Error(*e),
        _ => Value::Number(0.0),
    }
}

fn isref(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::Bool(matches!(ctx.eval(&args[0]), Val::Refs(_)))
}

fn isformula(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let area = tryv!(ctx.eval_area(&args[0]));
    let p = area.rect.top_left();
    let f = ctx.wb.sheets.get(area.sheet).and_then(|s| s.cells.get(&p)).map(|c| c.formula.is_some()).unwrap_or(false);
    Value::Bool(f)
}

fn type_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = match ctx.eval(&args[0]) {
        Val::Refs(areas) => {
            if areas.len() != 1 || areas[0].rect.rows() != 1 || areas[0].rect.cols() != 1 {
                return Value::Number(64.0);
            }
            ctx.cell_value(areas[0].sheet, areas[0].rect.top_left())
        }
        Val::V(v) => v,
    };
    Value::Number(match v {
        Value::Empty | Value::Number(_) => 1.0,
        Value::Text(_) => 2.0,
        Value::Bool(_) => 4.0,
        Value::Error(_) => 16.0,
        Value::Array(_) => 64.0,
    })
}

fn sheet(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let Some(e) = args.first().filter(|e| !matches!(e, Expr::Missing)) else {
        return Value::Number(ctx.sheet as f64 + 1.0);
    };
    match ctx.eval(e) {
        Val::Refs(a) => Value::Number(a[0].sheet as f64 + 1.0),
        Val::V(Value::Text(s)) => match ctx.wb.sheet_index(&s) {
            Some(i) => Value::Number(i as f64 + 1.0),
            None => Value::Error(ErrorKind::NA),
        },
        Val::V(Value::Error(e)) => Value::Error(e),
        Val::V(_) => Value::Error(ErrorKind::NA),
    }
}

fn sheets(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let Some(e) = args.first().filter(|e| !matches!(e, Expr::Missing)) else {
        return Value::Number(ctx.wb.sheets.len() as f64);
    };
    match ctx.eval(e) {
        Val::Refs(a) => {
            let mut s: Vec<usize> = a.iter().map(|x| x.sheet).collect();
            s.sort_unstable();
            s.dedup();
            Value::Number(s.len() as f64)
        }
        Val::V(Value::Error(e)) => Value::Error(e),
        Val::V(_) => Value::Error(ErrorKind::NA),
    }
}

fn info(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let t = tryv!(ctx.eval_text(&args[0])).to_ascii_lowercase();
    match t.as_str() {
        "directory" => {
            let mut d = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
            if !d.ends_with(std::path::MAIN_SEPARATOR) {
                d.push(std::path::MAIN_SEPARATOR);
            }
            Value::Text(d)
        }
        "numfile" => Value::Number(ctx.wb.sheets.len() as f64),
        "origin" => Value::Text("$A:$A$1".into()),
        "osversion" => Value::Text(match std::env::consts::OS {
            "windows" => "Windows (64-bit) NT 10.00".into(),
            "macos" => "Macintosh (Intel) Version 14.0".into(),
            os => format!("{} (64-bit)", os),
        }),
        "recalc" => Value::Text(if ctx.wb.manual_calc { "Manual" } else { "Automatic" }.into()),
        "release" => Value::Text("16.0".into()),
        "system" => Value::Text(if cfg!(target_os = "macos") { "mac" } else { "pcdos" }.into()),
        _ => Value::Error(ErrorKind::Value),
    }
}

/// Quote a sheet name for use in an address if needed.
fn sheet_prefix(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
        format!("{}!", name)
    } else {
        format!("'{}'!", name.replace('\'', "''"))
    }
}

/// The CELL("format") code for a number format.
fn format_code(fmt: &str) -> String {
    let f = fmt.trim();
    if f.eq_ignore_ascii_case("general") || f.is_empty() {
        return "G".into();
    }
    let first = f.split(';').next().unwrap_or(f);
    let lower = first.to_ascii_lowercase();
    let neg_paren = f.split(';').nth(1).map(|s| s.contains('(')).unwrap_or(false);
    let neg_color = f.split(';').nth(1).map(|s| s.contains('[')).unwrap_or(false);
    let suffix = format!("{}{}", if neg_color { "-" } else { "" }, if neg_paren { "()" } else { "" });
    // Dates and times
    if crate::numfmt::is_date_format(first) && !lower.contains('0') {
        let clean: String = lower.chars().filter(|c| !matches!(c, '[' | ']' | '\\' | '"')).collect();
        let code = if clean.contains("am/pm") && clean.contains(":ss") {
            "D6"
        } else if clean.contains("am/pm") {
            "D7"
        } else if clean.contains("h:mm:ss") || clean.contains("mm:ss") {
            "D8"
        } else if clean.contains("h:mm") {
            "D9"
        } else if clean.contains("mmm") && clean.contains('d') && clean.contains('y') {
            "D1"
        } else if clean.contains("mmm") && clean.contains('d') {
            "D2"
        } else if clean.contains("mmm") {
            "D3"
        } else if clean.contains("d") && clean.contains('y') {
            "D4"
        } else {
            "D5"
        };
        return code.into();
    }
    let decimals = first.split('.').nth(1).map(|d| d.chars().take_while(|c| *c == '0' || *c == '#').count()).unwrap_or(0);
    let letter = if first.contains('%') {
        "P"
    } else if lower.contains("e+") || lower.contains("e-") {
        "S"
    } else if first.contains('$') || first.contains('€') || first.contains('£') || first.contains('¥') {
        "C"
    } else if first.contains(',') {
        ","
    } else {
        "F"
    };
    format!("{}{}{}", letter, decimals.min(9), suffix)
}

fn cell(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let t = tryv!(ctx.eval_text(&args[0])).to_ascii_lowercase();
    let area = match args.get(1) {
        None | Some(Expr::Missing) => Area { sheet: ctx.sheet, rect: crate::address::Rect::single(ctx.pos) },
        Some(e) => tryv!(ctx.eval_area(e)),
    };
    let p: CellPos = area.rect.top_left();
    let wb = ctx.wb;
    let Some(sheet) = wb.sheets.get(area.sheet) else { return Value::Error(ErrorKind::Ref) };
    let style = wb.cell_style(area.sheet, p);
    match t.as_str() {
        "address" => {
            let a = format!("${}${}", col_to_letters(p.col), p.row + 1);
            if area.sheet != ctx.sheet {
                Value::Text(format!("{}{}", sheet_prefix(&sheet.name), a))
            } else {
                Value::Text(a)
            }
        }
        "col" => Value::Number(p.col as f64 + 1.0),
        "row" => Value::Number(p.row as f64 + 1.0),
        "contents" => {
            let v = ctx.cell_value(area.sheet, p);
            if v.is_empty() {
                Value::Number(0.0)
            } else {
                v
            }
        }
        "type" => {
            let v = ctx.cell_value(area.sheet, p);
            Value::Text(
                match v {
                    Value::Empty => "b",
                    Value::Text(_) => "l",
                    _ => "v",
                }
                .into(),
            )
        }
        "width" => {
            let px = sheet.col_width(p.col) as f64;
            Value::Number(((px - 5.0).max(0.0) / 7.0).floor())
        }
        "format" => Value::Text(format_code(&style.num_fmt)),
        "color" => {
            let neg_color = style.num_fmt.split(';').nth(1).map(|s| s.contains('[')).unwrap_or(false);
            Value::Number(if neg_color { 1.0 } else { 0.0 })
        }
        "parentheses" => {
            let paren = style.num_fmt.split(';').next().map(|s| s.contains('(')).unwrap_or(false);
            Value::Number(if paren { 1.0 } else { 0.0 })
        }
        "prefix" => {
            let v = ctx.cell_value(area.sheet, p);
            if !matches!(v, Value::Text(_)) {
                return Value::Text(String::new());
            }
            Value::Text(
                match style.h_align {
                    HAlign::Left => "'",
                    HAlign::Center => "^",
                    HAlign::Right => "\"",
                    HAlign::Fill => "\\",
                    _ => "'",
                }
                .into(),
            )
        }
        "protect" => Value::Number(if style.locked { 1.0 } else { 0.0 }),
        "filename" => Value::Text(String::new()),
        "sheet" => Value::Text(sheet.name.clone()),
        _ => Value::Error(ErrorKind::Value),
    }
}
