//! Built-in worksheet function registry.
//!
//! Each category module exposes `register(&mut Registry)`. There are three kinds
//! of functions:
//! * [`FnKind::Scalar`]: arguments are evaluated to values and arrays are
//!   lifted element-wise automatically (like ABS, LEN, DATE).
//! * [`FnKind::Raw`]: the function receives the unevaluated argument
//!   expressions and the [`Ctx`] (needed for ranges, short-circuiting, etc).
//! * [`FnKind::RefRaw`]: like Raw, but may return a reference (OFFSET, INDEX...).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ast::Expr;
use crate::eval::{Ctx, Val};
use crate::value::{ErrorKind, Value};

pub mod core;
pub mod math;
pub mod stats;
pub mod text;
pub mod date;
pub mod lookup;
pub mod info;
pub mod financial;
pub mod engineering;
pub mod array;
pub mod database;

pub type ScalarFn = fn(&[Value]) -> Value;
pub type RawFn = fn(&mut Ctx, &[Expr]) -> Value;
pub type RefRawFn = fn(&mut Ctx, &[Expr]) -> Val;

#[derive(Clone, Copy)]
pub enum FnKind {
    Scalar(ScalarFn),
    Raw(RawFn),
    RefRaw(RefRawFn),
}

/// Unlimited argument count.
pub const VAR: usize = 255;

#[derive(Clone, Copy)]
pub struct FnDef {
    pub name: &'static str,
    pub min: usize,
    pub max: usize,
    pub kind: FnKind,
    pub category: &'static str,
    /// e.g. "SUM(number1, [number2], ...)"
    pub syntax: &'static str,
    pub description: &'static str,
}

#[derive(Default)]
pub struct Registry {
    pub map: HashMap<&'static str, FnDef>,
}

impl Registry {
    pub fn add(&mut self, def: FnDef) {
        self.map.insert(def.name, def);
    }
    pub fn scalar(&mut self, name: &'static str, min: usize, max: usize, f: ScalarFn, category: &'static str, syntax: &'static str, description: &'static str) {
        self.add(FnDef { name, min, max, kind: FnKind::Scalar(f), category, syntax, description });
    }
    pub fn raw(&mut self, name: &'static str, min: usize, max: usize, f: RawFn, category: &'static str, syntax: &'static str, description: &'static str) {
        self.add(FnDef { name, min, max, kind: FnKind::Raw(f), category, syntax, description });
    }
    pub fn refraw(&mut self, name: &'static str, min: usize, max: usize, f: RefRawFn, category: &'static str, syntax: &'static str, description: &'static str) {
        self.add(FnDef { name, min, max, kind: FnKind::RefRaw(f), category, syntax, description });
    }
}

fn registry() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| {
        let mut r = Registry::default();
        core::register(&mut r);
        math::register(&mut r);
        stats::register(&mut r);
        text::register(&mut r);
        date::register(&mut r);
        lookup::register(&mut r);
        info::register(&mut r);
        financial::register(&mut r);
        engineering::register(&mut r);
        array::register(&mut r);
        database::register(&mut r);
        r
    })
}

/// Find a function by (upper-case) name.
pub fn lookup(name: &str) -> Option<&'static FnDef> {
    registry().map.get(name)
}

/// All registered functions, sorted by name.
pub fn all() -> Vec<&'static FnDef> {
    let mut v: Vec<&FnDef> = registry().map.values().collect();
    v.sort_by_key(|d| d.name);
    v
}

// ----- helpers shared by function modules ----------------------------------

/// Return early with an error value.
#[macro_export]
macro_rules! tryv {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return $crate::value::Value::Error(e),
        }
    };
}

/// Numeric argument `i` (Empty/missing -> 0).
pub fn num(args: &[Value], i: usize) -> Result<f64, ErrorKind> {
    args.get(i).map(|v| v.to_number()).unwrap_or(Ok(0.0))
}

/// Optional numeric argument with default when missing or empty.
pub fn num_or(args: &[Value], i: usize, default: f64) -> Result<f64, ErrorKind> {
    match args.get(i) {
        None | Some(Value::Empty) => Ok(default),
        Some(v) => v.to_number(),
    }
}

/// Text argument `i` (missing -> "").
pub fn text(args: &[Value], i: usize) -> Result<String, ErrorKind> {
    args.get(i).map(|v| v.to_text()).unwrap_or(Ok(String::new()))
}

/// Boolean argument with default.
pub fn bool_or(args: &[Value], i: usize, default: bool) -> Result<bool, ErrorKind> {
    match args.get(i) {
        None | Some(Value::Empty) => Ok(default),
        Some(v) => v.to_bool(),
    }
}

/// Truncate toward zero to an integer (Excel's usual integer argument handling).
pub fn int(args: &[Value], i: usize) -> Result<i64, ErrorKind> {
    Ok(num(args, i)?.trunc() as i64)
}
