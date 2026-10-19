//! Database functions (DSUM, DAVERAGE, DGET, ...).
//!
//! A database is a range whose first row holds field names. The criteria range
//! has a header row of field names; each following row is a set of AND-ed
//! conditions, and the rows are OR-ed together.

use super::Registry;
use crate::ast::Expr;
use crate::criteria::Criteria;
use crate::eval::Ctx;
use crate::tryv;
use crate::value::{parse_number_text, Array, ErrorKind, Value};

const CAT: &str = "Database";

pub fn register(r: &mut Registry) {
    r.raw("DAVERAGE", 3, 3, daverage, CAT, "DAVERAGE(database, field, criteria)", "Averages the values in a field of records that match the criteria.");
    r.raw("DCOUNT", 2, 3, dcount, CAT, "DCOUNT(database, [field], criteria)", "Counts the cells containing numbers in a field of records that match the criteria.");
    r.raw("DCOUNTA", 2, 3, dcounta, CAT, "DCOUNTA(database, [field], criteria)", "Counts the nonblank cells in a field of records that match the criteria.");
    r.raw("DGET", 3, 3, dget, CAT, "DGET(database, field, criteria)", "Extracts the single record that matches the criteria.");
    r.raw("DMAX", 3, 3, dmax, CAT, "DMAX(database, field, criteria)", "Returns the maximum value in a field of records that match the criteria.");
    r.raw("DMIN", 3, 3, dmin, CAT, "DMIN(database, field, criteria)", "Returns the minimum value in a field of records that match the criteria.");
    r.raw("DPRODUCT", 3, 3, dproduct, CAT, "DPRODUCT(database, field, criteria)", "Multiplies the values in a field of records that match the criteria.");
    r.raw("DSTDEV", 3, 3, dstdev, CAT, "DSTDEV(database, field, criteria)", "Estimates the standard deviation based on a sample of matching records.");
    r.raw("DSTDEVP", 3, 3, dstdevp, CAT, "DSTDEVP(database, field, criteria)", "Calculates the standard deviation based on the entire population of matching records.");
    r.raw("DSUM", 3, 3, dsum, CAT, "DSUM(database, field, criteria)", "Adds the numbers in a field of records that match the criteria.");
    r.raw("DVAR", 3, 3, dvar, CAT, "DVAR(database, field, criteria)", "Estimates variance based on a sample of matching records.");
    r.raw("DVARP", 3, 3, dvarp, CAT, "DVARP(database, field, criteria)", "Calculates variance based on the entire population of matching records.");
}

/// A criteria cell as the D-functions read it: plain text means "begins with".
fn db_criterion(v: &Value) -> Criteria {
    if let Value::Text(s) = v {
        let has_op = s.starts_with(['<', '>', '=']);
        let special = parse_number_text(s).is_some() || s.eq_ignore_ascii_case("TRUE") || s.eq_ignore_ascii_case("FALSE");
        if !has_op && !special && !s.is_empty() {
            return Criteria::new(&Value::Text(format!("{}*", s)));
        }
    }
    Criteria::new(v)
}

fn header_name(v: &Value) -> String {
    v.to_text().unwrap_or_default().trim().to_lowercase()
}

/// Column of `field` (name or 1-based number) in the database header.
fn field_index(db: &Array, field: &Value) -> Result<usize, ErrorKind> {
    match field {
        Value::Error(e) => Err(*e),
        Value::Number(n) => {
            let i = n.trunc() as i64;
            if i < 1 || i as usize > db.cols {
                Err(ErrorKind::Value)
            } else {
                Ok(i as usize - 1)
            }
        }
        Value::Text(t) => {
            let t = t.trim().to_lowercase();
            (0..db.cols).find(|&c| header_name(db.get(0, c)) == t).ok_or(ErrorKind::Value)
        }
        _ => Err(ErrorKind::Value),
    }
}

/// Values of the field for matching records (`None` field: one marker per record).
fn matching(ctx: &mut Ctx, args: &[Expr]) -> Result<Vec<Value>, ErrorKind> {
    let (db_e, field_e, crit_e) = if args.len() == 3 { (&args[0], Some(&args[1]), &args[2]) } else { (&args[0], None, &args[1]) };
    let db = ctx.eval_array(db_e)?;
    let field = match field_e {
        None | Some(Expr::Missing) => None,
        Some(e) => Some(field_index(&db, &ctx.eval_scalar(e))?),
    };
    let crit = ctx.eval_array(crit_e)?;
    // Compile criteria rows: each condition is (database column or None, criterion).
    let mut rows: Vec<Vec<(Option<usize>, Criteria)>> = Vec::new();
    for r in 1..crit.rows {
        let mut conds = Vec::new();
        for c in 0..crit.cols {
            let v = crit.get(r, c);
            if v.is_empty() {
                continue;
            }
            let name = header_name(crit.get(0, c));
            let col = (0..db.cols).find(|&k| header_name(db.get(0, k)) == name);
            conds.push((col, db_criterion(v)));
        }
        rows.push(conds);
    }
    let mut out = Vec::new();
    for r in 1..db.rows {
        let ok = rows.is_empty()
            || rows.iter().any(|conds| conds.iter().all(|(col, c)| col.map(|k| c.matches(db.get(r, k))).unwrap_or(false)));
        if ok {
            out.push(match field {
                Some(f) => db.get(r, f).clone(),
                None => Value::Bool(true),
            });
        }
    }
    Ok(out)
}

fn numbers(ctx: &mut Ctx, args: &[Expr]) -> Result<Vec<f64>, ErrorKind> {
    let mut out = Vec::new();
    for v in matching(ctx, args)? {
        match v {
            Value::Number(n) => out.push(n),
            Value::Error(e) => return Err(e),
            _ => {}
        }
    }
    Ok(out)
}

fn dsum(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::number(tryv!(numbers(ctx, args)).iter().sum())
}

fn daverage(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = tryv!(numbers(ctx, args));
    if v.is_empty() {
        return Value::Error(ErrorKind::Div0);
    }
    Value::number(v.iter().sum::<f64>() / v.len() as f64)
}

fn dcount(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let field_given = args.len() == 3 && !matches!(args[1], Expr::Missing);
    let v = tryv!(matching(ctx, args));
    let n = if field_given { v.iter().filter(|x| matches!(x, Value::Number(_))).count() } else { v.len() };
    Value::Number(n as f64)
}

fn dcounta(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = tryv!(matching(ctx, args));
    Value::Number(v.iter().filter(|x| !x.is_empty()).count() as f64)
}

fn dget(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = tryv!(matching(ctx, args));
    match v.len() {
        0 => Value::Error(ErrorKind::Value),
        1 => v.into_iter().next().unwrap(),
        _ => Value::Error(ErrorKind::Num),
    }
}

fn dmax(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = tryv!(numbers(ctx, args));
    Value::number(v.iter().copied().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x)))).unwrap_or(0.0))
}

fn dmin(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = tryv!(numbers(ctx, args));
    Value::number(v.iter().copied().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x)))).unwrap_or(0.0))
}

fn dproduct(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let v = tryv!(numbers(ctx, args));
    if v.is_empty() {
        return Value::Number(0.0);
    }
    Value::number(v.iter().product())
}

fn variance(v: &[f64], sample: bool) -> Result<f64, ErrorKind> {
    let n = v.len() as f64;
    if (sample && v.len() < 2) || v.is_empty() {
        return Err(ErrorKind::Div0);
    }
    let mean = v.iter().sum::<f64>() / n;
    let ss: f64 = v.iter().map(|x| (x - mean) * (x - mean)).sum();
    Ok(ss / if sample { n - 1.0 } else { n })
}

fn dvar(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::number(tryv!(variance(&tryv!(numbers(ctx, args)), true)))
}

fn dvarp(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::number(tryv!(variance(&tryv!(numbers(ctx, args)), false)))
}

fn dstdev(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::number(tryv!(variance(&tryv!(numbers(ctx, args)), true)).sqrt())
}

fn dstdevp(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::number(tryv!(variance(&tryv!(numbers(ctx, args)), false)).sqrt())
}
