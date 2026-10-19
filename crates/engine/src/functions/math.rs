//! Math and trigonometry functions.

use super::{Registry, VAR};
use crate::ast::Expr;
use crate::eval::Ctx;
use crate::value::Value;

const CAT: &str = "Math & Trig";

pub fn register(r: &mut Registry) {
    r.raw("SUM", 1, VAR, sum, CAT, "SUM(number1, [number2], ...)", "Adds all the numbers.");
}

fn sum(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match ctx.collect_numbers(args) {
        Ok(v) => Value::number(v.iter().sum()),
        Err(e) => Value::Error(e),
    }
}
