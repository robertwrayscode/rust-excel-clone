//! Dynamic-array generators and LAMBDA helper functions (SEQUENCE, MAP, REDUCE...).

use std::cell::Cell;

use super::{Registry, VAR};
use crate::address::{MAX_COLS, MAX_ROWS};
use crate::ast::Expr;
use crate::eval::{Ctx, Lambda, Val};
use crate::parser::normalize_function_name;
use crate::value::{Array, ErrorKind, Value};

const CAT: &str = "Dynamic Array";

pub fn register(r: &mut Registry) {
    r.raw("SEQUENCE", 1, 4, sequence, CAT, "SEQUENCE(rows, [columns], [start], [step])", "Generates a list of sequential numbers in an array.");
    r.raw("RANDARRAY", 0, 5, randarray, CAT, "RANDARRAY([rows], [columns], [min], [max], [whole_number])", "Returns an array of random numbers.");
    r.raw("MAP", 2, VAR, map, CAT, "MAP(array1, [array2], ..., lambda)", "Returns an array formed by applying a LAMBDA to each value of the arrays.");
    r.raw("REDUCE", 2, 3, reduce, CAT, "REDUCE([initial_value], array, lambda)", "Reduces an array to an accumulated value by applying a LAMBDA to each value.");
    r.raw("SCAN", 2, 3, scan, CAT, "SCAN([initial_value], array, lambda)", "Scans an array applying a LAMBDA to each value and returns every intermediate value.");
    r.raw("BYROW", 2, 2, byrow, CAT, "BYROW(array, lambda)", "Applies a LAMBDA to each row and returns an array of the results.");
    r.raw("BYCOL", 2, 2, bycol, CAT, "BYCOL(array, lambda)", "Applies a LAMBDA to each column and returns an array of the results.");
    r.raw("MAKEARRAY", 3, 3, makearray, CAT, "MAKEARRAY(rows, cols, lambda)", "Returns an array of the given size computed by a LAMBDA of (row, column).");
    r.raw("ISOMITTED", 1, 1, isomitted, CAT, "ISOMITTED(argument)", "Checks whether a LAMBDA parameter was omitted.");
}

/// Resolve a function argument: an inline/named LAMBDA, or an eta-reduced
/// built-in function name such as `SUM` (wrapped in a lambda of `arity` params).
pub(crate) fn resolve_fn(ctx: &mut Ctx, e: &Expr, arity: usize) -> Option<Lambda> {
    if let Some(l) = ctx.get_lambda(e) {
        return Some(l);
    }
    let name = match e {
        Expr::Name(n) => normalize_function_name(n),
        Expr::Paren(inner) => return resolve_fn(ctx, inner, arity),
        _ => return None,
    };
    let def = super::lookup(&name)?;
    if arity < def.min || arity > def.max {
        return None;
    }
    let params: Vec<String> = (0..arity).map(|i| format!("__ETA{}", i)).collect();
    let body = Expr::Call(name, params.iter().map(|p| Expr::Name(p.clone())).collect());
    Some(Lambda { params, body })
}

/// A lambda result that must be a single value (nested arrays give #CALC!).
pub(crate) fn single(v: Value) -> Value {
    match v {
        Value::Array(a) => {
            if a.rows == 1 && a.cols == 1 {
                a.data.into_iter().next().unwrap()
            } else {
                Value::Error(ErrorKind::Calc)
            }
        }
        v => v,
    }
}

fn opt_int(ctx: &mut Ctx, args: &[Expr], i: usize, default: f64) -> Result<i64, ErrorKind> {
    Ok(ctx.opt_number(args, i, default)?.trunc() as i64)
}

fn sequence(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let rows = crate::tryv!(opt_int(ctx, args, 0, 1.0));
    let cols = crate::tryv!(opt_int(ctx, args, 1, 1.0));
    let start = crate::tryv!(ctx.opt_number(args, 2, 1.0));
    let step = crate::tryv!(ctx.opt_number(args, 3, 1.0));
    if rows < 0 || cols < 0 {
        return Value::Error(ErrorKind::Value);
    }
    if rows == 0 || cols == 0 {
        return Value::Error(ErrorKind::Calc);
    }
    if rows > MAX_ROWS as i64 || cols > MAX_COLS as i64 {
        return Value::Error(ErrorKind::Num);
    }
    let (rows, cols) = (rows as usize, cols as usize);
    let data = (0..rows * cols).map(|i| Value::number(start + step * i as f64)).collect();
    Value::from(Array::new(rows, cols, data))
}

thread_local! {
    static RNG: Cell<u64> = const { Cell::new(0) };
}

/// Uniform random number in [0, 1) (xorshift64*, seeded from the clock).
pub(crate) fn random() -> f64 {
    RNG.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15)
                | 1;
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn randarray(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let rows = crate::tryv!(opt_int(ctx, args, 0, 1.0));
    let cols = crate::tryv!(opt_int(ctx, args, 1, 1.0));
    let min = crate::tryv!(ctx.opt_number(args, 2, 0.0));
    let max = crate::tryv!(ctx.opt_number(args, 3, 1.0));
    let whole = crate::tryv!(ctx.opt_bool(args, 4, false));
    if rows < 0 || cols < 0 || min > max {
        return Value::Error(ErrorKind::Value);
    }
    if rows == 0 || cols == 0 {
        return Value::Error(ErrorKind::Calc);
    }
    if rows > MAX_ROWS as i64 || cols > MAX_COLS as i64 {
        return Value::Error(ErrorKind::Num);
    }
    if whole && (min.fract() != 0.0 || max.fract() != 0.0) {
        return Value::Error(ErrorKind::Value);
    }
    let n = rows as usize * cols as usize;
    let data = (0..n)
        .map(|_| {
            let r = random();
            if whole {
                Value::number((r * (max - min + 1.0)).floor() + min)
            } else {
                Value::number(min + r * (max - min))
            }
        })
        .collect();
    Value::from(Array::new(rows as usize, cols as usize, data))
}

fn map(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let (fexpr, arrs) = args.split_last().unwrap();
    let Some(l) = resolve_fn(ctx, fexpr, arrs.len()) else { return Value::Error(ErrorKind::Value) };
    if l.params.len() != arrs.len() {
        return Value::Error(ErrorKind::Value);
    }
    let mut arrays = Vec::with_capacity(arrs.len());
    for a in arrs {
        arrays.push(crate::tryv!(ctx.eval_array(a)));
    }
    let bd = |a: usize, b: usize| if a == 1 { b } else if b == 1 { a } else { a.max(b) };
    let rows = arrays.iter().fold(1, |acc, a| bd(acc, a.rows));
    let cols = arrays.iter().fold(1, |acc, a| bd(acc, a.cols));
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let vals: Vec<Val> = arrays.iter().map(|a| Val::V(a.get_broadcast(r, c))).collect();
            data.push(single(ctx.call_lambda(&l, vals)));
        }
    }
    Value::from(Array::new(rows, cols, data))
}

/// Shared argument handling for REDUCE and SCAN.
fn fold_args(ctx: &mut Ctx, args: &[Expr]) -> Result<(Val, Array, Lambda), ErrorKind> {
    let (init, arr, f) = if args.len() == 3 { (Some(&args[0]), &args[1], &args[2]) } else { (None, &args[0], &args[1]) };
    let l = resolve_fn(ctx, f, 2).ok_or(ErrorKind::Value)?;
    if l.params.len() != 2 {
        return Err(ErrorKind::Value);
    }
    let acc = match init {
        Some(e) => {
            let v = ctx.eval_value(e);
            Val::V(v)
        }
        None => Val::V(Value::Empty),
    };
    let a = ctx.eval_array(arr)?;
    Ok((acc, a, l))
}

fn reduce(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let (mut acc, a, l) = crate::tryv!(fold_args(ctx, args));
    for v in a.data.iter() {
        acc = Val::V(ctx.call_lambda(&l, vec![acc, Val::V(v.clone())]));
    }
    ctx.deref(acc)
}

fn scan(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let (mut acc, a, l) = crate::tryv!(fold_args(ctx, args));
    let mut data = Vec::with_capacity(a.data.len());
    for v in a.data.iter() {
        let r = ctx.call_lambda(&l, vec![acc, Val::V(v.clone())]);
        data.push(single(r.clone()));
        acc = Val::V(r);
    }
    Value::from(Array::new(a.rows, a.cols, data))
}

fn by(ctx: &mut Ctx, args: &[Expr], rows: bool) -> Value {
    let a = crate::tryv!(ctx.eval_array(&args[0]));
    let Some(l) = resolve_fn(ctx, &args[1], 1) else { return Value::Error(ErrorKind::Value) };
    if l.params.len() != 1 {
        return Value::Error(ErrorKind::Value);
    }
    let a = if rows { a } else { a.transpose() };
    let mut out = Vec::with_capacity(a.rows);
    for r in 0..a.rows {
        let row = Array::new(1, a.cols, a.row_values(r).to_vec());
        let arg = if rows { row } else { row.transpose() };
        out.push(single(ctx.call_lambda(&l, vec![Val::V(Value::from(arg))])));
    }
    let n = out.len();
    Value::from(if rows { Array::new(n, 1, out) } else { Array::new(1, n, out) })
}

fn byrow(ctx: &mut Ctx, args: &[Expr]) -> Value {
    by(ctx, args, true)
}

fn bycol(ctx: &mut Ctx, args: &[Expr]) -> Value {
    by(ctx, args, false)
}

fn makearray(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let rows = crate::tryv!(ctx.eval_number(&args[0])).trunc();
    let cols = crate::tryv!(ctx.eval_number(&args[1])).trunc();
    if rows < 1.0 || cols < 1.0 {
        return Value::Error(ErrorKind::Value);
    }
    if rows > MAX_ROWS as f64 || cols > MAX_COLS as f64 {
        return Value::Error(ErrorKind::Num);
    }
    let Some(l) = resolve_fn(ctx, &args[2], 2) else { return Value::Error(ErrorKind::Value) };
    if l.params.len() != 2 {
        return Value::Error(ErrorKind::Value);
    }
    let (rows, cols) = (rows as usize, cols as usize);
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let v = ctx.call_lambda(&l, vec![Val::V(Value::Number(r as f64 + 1.0)), Val::V(Value::Number(c as f64 + 1.0))]);
            data.push(single(v));
        }
    }
    Value::from(Array::new(rows, cols, data))
}

fn isomitted(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match &args[0] {
        Expr::Missing => Value::Bool(true),
        Expr::Name(_) => Value::Bool(matches!(ctx.eval(&args[0]), Val::V(Value::Empty))),
        _ => Value::Bool(false),
    }
}
