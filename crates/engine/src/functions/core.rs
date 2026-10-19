//! Logical functions and language constructs (IF, LET, LAMBDA...).

use super::{Registry, VAR};
use crate::ast::Expr;
use crate::eval::{broadcast2, make_lambda, Ctx, Val};
use crate::value::{Array, ErrorKind, Value};

const CAT: &str = "Logical";

pub fn register(r: &mut Registry) {
    r.refraw("IF", 1, 3, if_fn, CAT, "IF(logical_test, [value_if_true], [value_if_false])", "Returns one value if a condition is TRUE and another if FALSE.");
    r.refraw("IFERROR", 2, 2, iferror, CAT, "IFERROR(value, value_if_error)", "Returns value_if_error if the expression is an error, otherwise the expression.");
    r.refraw("IFNA", 2, 2, ifna, CAT, "IFNA(value, value_if_na)", "Returns value_if_na if the expression is #N/A, otherwise the expression.");
    r.refraw("IFS", 2, VAR, ifs, CAT, "IFS(logical_test1, value1, [logical_test2, value2], ...)", "Returns the value for the first TRUE condition.");
    r.refraw("SWITCH", 3, VAR, switch, CAT, "SWITCH(expression, value1, result1, [default_or_value2, result2], ...)", "Matches an expression against a list of values.");
    r.refraw("CHOOSE", 2, VAR, choose, CAT, "CHOOSE(index_num, value1, [value2], ...)", "Chooses a value from a list based on an index.");
    r.raw("AND", 1, VAR, and, CAT, "AND(logical1, [logical2], ...)", "TRUE if all arguments are TRUE.");
    r.raw("OR", 1, VAR, or, CAT, "OR(logical1, [logical2], ...)", "TRUE if any argument is TRUE.");
    r.raw("XOR", 1, VAR, xor, CAT, "XOR(logical1, [logical2], ...)", "TRUE if an odd number of arguments are TRUE.");
    r.scalar("NOT", 1, 1, not, CAT, "NOT(logical)", "Reverses a logical value.");
    r.scalar("TRUE", 0, 0, |_| Value::Bool(true), CAT, "TRUE()", "The logical value TRUE.");
    r.scalar("FALSE", 0, 0, |_| Value::Bool(false), CAT, "FALSE()", "The logical value FALSE.");
    r.refraw("LET", 3, VAR, let_fn, CAT, "LET(name1, value1, [name2, value2], ..., calculation)", "Assigns names to calculation results.");
    r.raw("LAMBDA", 1, VAR, lambda, CAT, "LAMBDA([parameter1, ...], calculation)", "Creates a custom, reusable function.");
}

fn if_fn(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let cond = ctx.eval_value(&args[0]);
    let branch = |ctx: &mut Ctx, i: usize, default: Value| -> Val {
        match args.get(i) {
            None => Val::V(default),
            Some(Expr::Missing) => Val::V(Value::Number(0.0)),
            Some(e) => ctx.eval(e),
        }
    };
    match cond {
        Value::Array(a) => {
            let t = branch(ctx, 1, Value::Bool(true));
            let t = ctx.deref(t);
            let f = branch(ctx, 2, Value::Bool(false));
            let f = ctx.deref(f);
            let dims = |v: &Value| match v {
                Value::Array(x) => (x.rows, x.cols),
                _ => (1, 1),
            };
            let bd = |a: usize, b: usize| if a == 1 { b } else if b == 1 { a } else { a.max(b) };
            let (tr, tc) = dims(&t);
            let (fr, fc) = dims(&f);
            let rows = bd(bd(a.rows, tr), fr);
            let cols = bd(bd(a.cols, tc), fc);
            let get = |v: &Value, r: usize, c: usize| match v {
                Value::Array(x) => x.get_broadcast(r, c),
                v => v.clone(),
            };
            let mut data = Vec::with_capacity(rows * cols);
            for r in 0..rows {
                for c in 0..cols {
                    data.push(match a.get_broadcast(r, c).to_bool() {
                        Ok(true) => get(&t, r, c),
                        Ok(false) => get(&f, r, c),
                        Err(e) => Value::Error(e),
                    });
                }
            }
            Val::V(Value::from(Array::new(rows, cols, data)))
        }
        c => match c.to_bool() {
            Ok(true) => branch(ctx, 1, Value::Bool(true)),
            Ok(false) => branch(ctx, 2, Value::Bool(false)),
            Err(e) => Val::V(Value::Error(e)),
        },
    }
}

fn iferror(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let v = ctx.eval(&args[0]);
    match v {
        Val::V(Value::Error(_)) => ctx.eval(&args[1]),
        Val::V(Value::Array(a)) => {
            let fallback = ctx.eval_value(&args[1]);
            let arr = Value::Array(a);
            Val::V(broadcast2(&arr, &fallback, |x, f| if x.is_error() { f.clone() } else { x.clone() }))
        }
        Val::Refs(r) => {
            let v = ctx.deref(Val::Refs(r.clone()));
            match v {
                Value::Error(_) => ctx.eval(&args[1]),
                Value::Array(a) => {
                    let fallback = ctx.eval_value(&args[1]);
                    Val::V(broadcast2(&Value::Array(a), &fallback, |x, f| if x.is_error() { f.clone() } else { x.clone() }))
                }
                _ => Val::Refs(r),
            }
        }
        v => v,
    }
}

fn ifna(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let v = ctx.eval_value(&args[0]);
    match v {
        Value::Error(ErrorKind::NA) => ctx.eval(&args[1]),
        Value::Array(a) => {
            let fallback = ctx.eval_value(&args[1]);
            Val::V(broadcast2(&Value::Array(a), &fallback, |x, f| {
                if matches!(x, Value::Error(ErrorKind::NA)) {
                    f.clone()
                } else {
                    x.clone()
                }
            }))
        }
        v => Val::V(v),
    }
}

fn ifs(ctx: &mut Ctx, args: &[Expr]) -> Val {
    if args.len() % 2 != 0 {
        return Val::V(Value::Error(ErrorKind::Value));
    }
    for pair in args.chunks(2) {
        match ctx.eval_scalar(&pair[0]).to_bool() {
            Ok(true) => return ctx.eval(&pair[1]),
            Ok(false) => {}
            Err(e) => return Val::V(Value::Error(e)),
        }
    }
    Val::V(Value::Error(ErrorKind::NA))
}

fn switch(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let v = ctx.eval_scalar(&args[0]);
    if let Value::Error(e) = v {
        return Val::V(Value::Error(e));
    }
    let rest = &args[1..];
    let pairs = rest.len() / 2;
    for i in 0..pairs {
        let candidate = ctx.eval_scalar(&rest[i * 2]);
        if candidate.excel_eq(&v) && candidate.type_rank() == v.type_rank() {
            return ctx.eval(&rest[i * 2 + 1]);
        }
    }
    if rest.len() % 2 == 1 {
        return ctx.eval(&rest[rest.len() - 1]);
    }
    Val::V(Value::Error(ErrorKind::NA))
}

fn choose(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let idx = ctx.eval_value(&args[0]);
    if let Value::Array(a) = idx {
        let mut data = Vec::with_capacity(a.data.len());
        for v in a.data.iter() {
            data.push(match v.to_number() {
                Ok(n) => {
                    let i = n.trunc() as usize;
                    if i < 1 || i >= args.len() {
                        Value::Error(ErrorKind::Value)
                    } else {
                        ctx.eval_scalar(&args[i])
                    }
                }
                Err(e) => Value::Error(e),
            });
        }
        return Val::V(Value::from(Array::new(a.rows, a.cols, data)));
    }
    let n = match idx.to_number() {
        Ok(n) => n.trunc() as i64,
        Err(e) => return Val::V(Value::Error(e)),
    };
    if n < 1 || n as usize >= args.len() {
        return Val::V(Value::Error(ErrorKind::Value));
    }
    ctx.eval(&args[n as usize])
}

fn logical_values(ctx: &mut Ctx, args: &[Expr]) -> Result<Vec<bool>, ErrorKind> {
    let mut out = Vec::new();
    ctx.for_each_value(args, &mut |v, from_ref| {
        match v {
            Value::Bool(b) => out.push(*b),
            Value::Number(n) => out.push(*n != 0.0),
            Value::Error(e) => return Err(*e),
            Value::Text(_) if !from_ref => out.push(v.to_bool()?),
            _ => {}
        }
        Ok(())
    })?;
    if out.is_empty() {
        return Err(ErrorKind::Value);
    }
    Ok(out)
}

fn and(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match logical_values(ctx, args) {
        Ok(v) => Value::Bool(v.iter().all(|b| *b)),
        Err(e) => Value::Error(e),
    }
}

fn or(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match logical_values(ctx, args) {
        Ok(v) => Value::Bool(v.iter().any(|b| *b)),
        Err(e) => Value::Error(e),
    }
}

fn xor(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match logical_values(ctx, args) {
        Ok(v) => Value::Bool(v.iter().filter(|b| **b).count() % 2 == 1),
        Err(e) => Value::Error(e),
    }
}

fn not(args: &[Value]) -> Value {
    match args[0].to_bool() {
        Ok(b) => Value::Bool(!b),
        Err(e) => Value::Error(e),
    }
}

fn let_fn(ctx: &mut Ctx, args: &[Expr]) -> Val {
    if args.len() % 2 == 0 {
        return Val::V(Value::Error(ErrorKind::Value));
    }
    ctx.push_scope();
    let (calc, pairs) = args.split_last().unwrap();
    for pair in pairs.chunks(2) {
        let Expr::Name(name) = &pair[0] else {
            ctx.pop_scope();
            return Val::V(Value::Error(ErrorKind::Value));
        };
        if let Expr::Call(f, largs) = &pair[1] {
            if f == "LAMBDA" {
                if let Some(l) = make_lambda(largs) {
                    ctx.bind_lambda(name, l);
                    continue;
                }
            }
        }
        let v = ctx.eval(&pair[1]);
        ctx.bind_value(name, v);
    }
    let r = ctx.eval(calc);
    // References computed inside LET may refer to bound names only through values,
    // so it's safe to return them as-is.
    ctx.pop_scope();
    r
}

fn lambda(_ctx: &mut Ctx, _args: &[Expr]) -> Value {
    // A LAMBDA that is not called evaluates to #CALC!
    Value::Error(ErrorKind::Calc)
}
