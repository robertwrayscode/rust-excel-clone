//! Formula evaluation and workbook recalculation.

use std::collections::HashMap;
use std::rc::Rc;

use crate::address::{CellPos, Rect, MAX_COLS, MAX_ROWS};
use crate::ast::{BinaryOp, Expr, Reference, UnaryOp};
use crate::functions::{self, FnKind};
use crate::value::{Array, ErrorKind, Value};
use crate::workbook::Workbook;

/// A rectangular area on a specific sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub sheet: usize,
    pub rect: Rect,
}

/// Result of evaluating an expression: a value or a reference.
#[derive(Debug, Clone)]
pub enum Val {
    V(Value),
    Refs(Vec<Area>),
}

impl From<Value> for Val {
    fn from(v: Value) -> Self {
        Val::V(v)
    }
}

/// A user-defined function created with LAMBDA.
#[derive(Debug, Clone)]
pub struct Lambda {
    pub params: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
enum Binding {
    Val(Val),
    Lambda(Rc<Lambda>),
}

enum Slot {
    Computing,
    Done(Rc<Value>),
}

/// Evaluation context passed to functions.
pub struct Ctx<'a> {
    pub wb: &'a Workbook,
    memo: HashMap<(usize, CellPos), Slot>,
    /// Sheet of the cell being evaluated.
    pub sheet: usize,
    /// Position of the cell being evaluated.
    pub pos: CellPos,
    scopes: Vec<HashMap<String, Binding>>,
    pub circular: Vec<(usize, CellPos)>,
    depth: u32,
    /// Read stored values instead of recomputing formulas.
    readonly: bool,
}

const MAX_DEPTH: u32 = 200_000;

impl<'a> Ctx<'a> {
    pub fn new(wb: &'a Workbook) -> Ctx<'a> {
        Ctx {
            wb,
            memo: HashMap::new(),
            sheet: 0,
            pos: CellPos::default(),
            scopes: Vec::new(),
            circular: Vec::new(),
            depth: 0,
            readonly: false,
        }
    }

    /// A context that reads already-computed cell values (no recalculation).
    pub fn readonly(wb: &'a Workbook, sheet: usize, pos: CellPos) -> Ctx<'a> {
        let mut c = Ctx::new(wb);
        c.readonly = true;
        c.sheet = sheet;
        c.pos = pos;
        c
    }

    // ----- cell access --------------------------------------------------

    /// Fully computed value of a formula cell (may be an array).
    fn compute(&mut self, sheet: usize, pos: CellPos) -> Rc<Value> {
        let key = (sheet, pos);
        match self.memo.get(&key) {
            Some(Slot::Done(v)) => return v.clone(),
            Some(Slot::Computing) => {
                if !self.circular.contains(&key) {
                    self.circular.push(key);
                }
                return Rc::new(Value::Number(0.0));
            }
            None => {}
        }
        let wb = self.wb;
        let Some(cell) = wb.sheets.get(sheet).and_then(|s| s.cells.get(&pos)) else {
            return Rc::new(Value::Empty);
        };
        let Some(expr) = cell.formula.as_deref() else {
            return Rc::new(cell.value.clone());
        };
        if self.readonly {
            return Rc::new(cell.value.clone());
        }
        if self.depth > MAX_DEPTH {
            return Rc::new(Value::Error(ErrorKind::Calc));
        }
        self.memo.insert(key, Slot::Computing);
        let saved = (self.sheet, self.pos, std::mem::take(&mut self.scopes));
        self.sheet = sheet;
        self.pos = pos;
        self.depth += 1;
        let v = self.eval_value(expr);
        self.depth -= 1;
        self.sheet = saved.0;
        self.pos = saved.1;
        self.scopes = saved.2;
        let v = finalize_result(v);
        let rc = Rc::new(v);
        self.memo.insert(key, Slot::Done(rc.clone()));
        rc
    }

    /// The value of a single cell (top-left element for array formulas, spilled values included).
    pub fn cell_value(&mut self, sheet: usize, pos: CellPos) -> Value {
        let wb = self.wb;
        let Some(s) = wb.sheets.get(sheet) else { return Value::Error(ErrorKind::Ref) };
        match s.cells.get(&pos) {
            Some(c) if c.formula.is_some() => {
                let v = self.compute(sheet, pos);
                match &*v {
                    Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
                    v => v.clone(),
                }
            }
            Some(c) if !c.value.is_empty() => c.value.clone(),
            _ => {
                if let Some(anchor) = s.spill_owner.get(&pos) {
                    let v = self.compute(sheet, *anchor);
                    if let Value::Array(a) = &*v {
                        let r = (pos.row - anchor.row) as usize;
                        let c = (pos.col - anchor.col) as usize;
                        if r < a.rows && c < a.cols {
                            return a.get(r, c).clone();
                        }
                    }
                }
                Value::Empty
            }
        }
    }

    /// Clip whole-row/column areas to the sheet's data extent.
    pub fn clip_area(&self, area: Area) -> Rect {
        let mut r = area.rect;
        if r.bottom == MAX_ROWS - 1 || r.right == MAX_COLS - 1 {
            let ext = self.wb.sheets.get(area.sheet).and_then(|s| s.data_extent());
            match ext {
                Some(e) => {
                    if r.bottom == MAX_ROWS - 1 {
                        r.bottom = e.bottom.max(r.top);
                    }
                    if r.right == MAX_COLS - 1 {
                        r.right = e.right.max(r.left);
                    }
                }
                None => {
                    if r.bottom == MAX_ROWS - 1 {
                        r.bottom = r.top;
                    }
                    if r.right == MAX_COLS - 1 {
                        r.right = r.left;
                    }
                }
            }
        }
        r
    }

    /// Dense array of values in an area (whole rows/columns are clipped to the data).
    pub fn area_array(&mut self, area: Area) -> Array {
        let r = self.clip_area(area);
        let rows = r.rows() as usize;
        let cols = r.cols() as usize;
        let mut data = vec![Value::Empty; rows * cols];
        for (p, v) in self.area_values(area) {
            if r.contains(p) {
                data[(p.row - r.top) as usize * cols + (p.col - r.left) as usize] = v;
            }
        }
        Array::new(rows, cols, data)
    }

    /// Non-empty values in an area, row-major order.
    pub fn area_values(&mut self, area: Area) -> Vec<(CellPos, Value)> {
        let wb = self.wb;
        let Some(sheet) = wb.sheets.get(area.sheet) else { return vec![] };
        let rect = area.rect;
        let mut out = Vec::new();
        for (p, c) in sheet.cells_in(rect) {
            if c.formula.is_some() {
                let v = self.cell_value(area.sheet, *p);
                out.push((*p, v));
            } else if !c.value.is_empty() {
                out.push((*p, c.value.clone()));
            }
        }
        let mut spilled = false;
        for (anchor, sa) in &sheet.spill_areas {
            if let Some(inter) = sa.intersection(&rect) {
                for p in inter.iter() {
                    if p == *anchor {
                        continue;
                    }
                    if sheet.cells.get(&p).map(|c| !c.is_blank()).unwrap_or(false) {
                        continue;
                    }
                    let v = self.cell_value(area.sheet, p);
                    if !v.is_empty() {
                        out.push((p, v));
                        spilled = true;
                    }
                }
            }
        }
        if spilled {
            out.sort_by_key(|(p, _)| *p);
        }
        out
    }

    // ----- scopes -------------------------------------------------------

    fn lookup_binding(&self, name: &str) -> Option<&Binding> {
        let up = name.to_ascii_uppercase();
        self.scopes.iter().rev().find_map(|s| s.get(&up))
    }

    /// Push a scope binding names to values (used by LET and LAMBDA).
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }
    pub fn bind_value(&mut self, name: &str, v: Val) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_ascii_uppercase(), Binding::Val(v));
        }
    }
    pub fn bind_lambda(&mut self, name: &str, l: Lambda) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_ascii_uppercase(), Binding::Lambda(Rc::new(l)));
        }
    }

    /// Interpret an argument as a lambda: an inline `LAMBDA(...)`, a LET-bound
    /// lambda, or a defined name holding a LAMBDA.
    pub fn get_lambda(&mut self, e: &Expr) -> Option<Lambda> {
        match e {
            Expr::Call(n, args) if n == "LAMBDA" => make_lambda(args),
            Expr::Paren(inner) => self.get_lambda(inner),
            Expr::Name(n) => {
                if let Some(Binding::Lambda(l)) = self.lookup_binding(n) {
                    return Some((**l).clone());
                }
                let f = self.wb.find_name(n, self.sheet)?.formula.clone();
                self.get_lambda(&f)
            }
            Expr::Call(n, args) if args.is_empty() => {
                // ETA reduced function name, e.g. MAP(A1:A3, SUM) is not supported; treat as name
                let f = self.wb.find_name(n, self.sheet)?.formula.clone();
                self.get_lambda(&f)
            }
            _ => None,
        }
    }

    /// Call a lambda with argument values.
    pub fn call_lambda(&mut self, l: &Lambda, args: Vec<Val>) -> Value {
        if args.len() > l.params.len() {
            return Value::Error(ErrorKind::Value);
        }
        self.push_scope();
        for (i, p) in l.params.iter().enumerate() {
            let v = args.get(i).cloned().unwrap_or(Val::V(Value::Empty));
            self.bind_value(p, v);
        }
        self.depth += 1;
        let r = if self.depth > MAX_DEPTH { Value::Error(ErrorKind::Calc) } else { self.eval_value(&l.body) };
        self.depth -= 1;
        self.pop_scope();
        r
    }

    // ----- evaluation ---------------------------------------------------

    pub fn resolve_ref(&self, r: &Reference) -> Result<Area, ErrorKind> {
        let sheet = match &r.sheet {
            None => self.sheet,
            Some(n) => self.wb.sheet_index(n).ok_or(ErrorKind::Ref)?,
        };
        Ok(Area { sheet, rect: r.rect() })
    }

    /// Evaluate to a value or reference.
    pub fn eval(&mut self, e: &Expr) -> Val {
        match e {
            Expr::Number(n) => Val::V(Value::Number(*n)),
            Expr::Text(s) => Val::V(Value::Text(s.clone())),
            Expr::Bool(b) => Val::V(Value::Bool(*b)),
            Expr::Error(k) => Val::V(Value::Error(*k)),
            Expr::Missing => Val::V(Value::Empty),
            Expr::Paren(inner) => self.eval(inner),
            Expr::Ref(r) => match self.resolve_ref(r) {
                Ok(a) => Val::Refs(vec![a]),
                Err(e) => Val::V(Value::Error(e)),
            },
            Expr::SpillRef(r) => {
                let a = match self.resolve_ref(r) {
                    Ok(a) => a,
                    Err(e) => return Val::V(Value::Error(e)),
                };
                let anchor = a.rect.top_left();
                let v = self.compute(a.sheet, anchor);
                match &*v {
                    Value::Array(arr) => Val::Refs(vec![Area {
                        sheet: a.sheet,
                        rect: Rect {
                            top: anchor.row,
                            left: anchor.col,
                            bottom: anchor.row + arr.rows as u32 - 1,
                            right: anchor.col + arr.cols as u32 - 1,
                        },
                    }]),
                    Value::Error(e) => Val::V(Value::Error(*e)),
                    _ => Val::Refs(vec![Area { sheet: a.sheet, rect: Rect::single(anchor) }]),
                }
            }
            Expr::Name(n) => self.eval_name(n),
            Expr::Unary(op, inner) => match op {
                UnaryOp::Implicit => Val::V(self.eval_scalar(inner)),
                UnaryOp::Neg => Val::V(map_numbers(self.eval_value(inner), |x| -x)),
                UnaryOp::Plus => Val::V(self.eval_value(inner)),
                UnaryOp::Percent => Val::V(map_numbers(self.eval_value(inner), |x| x / 100.0)),
            },
            Expr::Binary(op, a, b) => self.eval_binary(*op, a, b),
            Expr::Call(name, args) => self.eval_call(name, args),
            Expr::CallExpr(f, args) => {
                let Some(l) = self.get_lambda(f) else { return Val::V(Value::Error(ErrorKind::Value)) };
                let vals: Vec<Val> = args.iter().map(|a| self.eval(a)).collect();
                Val::V(self.call_lambda(&l, vals))
            }
            Expr::Array(rows) => {
                let rows: Vec<Vec<Value>> =
                    rows.iter().map(|r| r.iter().map(|e| self.eval_scalar(e)).collect()).collect();
                Val::V(Value::from(Array::from_rows(rows)))
            }
        }
    }

    fn eval_name(&mut self, n: &str) -> Val {
        if let Some(b) = self.lookup_binding(n) {
            return match b {
                Binding::Val(v) => v.clone(),
                Binding::Lambda(_) => Val::V(Value::Error(ErrorKind::Calc)),
            };
        }
        // Sheet-qualified name: Sheet1!Name
        let (sheet, name) = match n.split_once('!') {
            Some((s, nm)) => match self.wb.sheet_index(s.trim_matches('\'')) {
                Some(i) => (i, nm),
                None => return Val::V(Value::Error(ErrorKind::Ref)),
            },
            None => (self.sheet, n),
        };
        let Some(def) = self.wb.find_name(name, sheet) else {
            return Val::V(Value::Error(ErrorKind::Name));
        };
        if matches!(&def.formula, Expr::Call(f, _) if f == "LAMBDA") {
            return Val::V(Value::Error(ErrorKind::Calc));
        }
        self.depth += 1;
        if self.depth > 10_000 {
            self.depth -= 1;
            return Val::V(Value::Error(ErrorKind::Calc));
        }
        let saved_scopes = std::mem::take(&mut self.scopes);
        let r = self.eval(&def.formula.clone());
        self.scopes = saved_scopes;
        self.depth -= 1;
        r
    }

    fn eval_call(&mut self, name: &str, args: &[Expr]) -> Val {
        if let Some(Binding::Lambda(l)) = self.lookup_binding(name) {
            let l = l.clone();
            let vals: Vec<Val> = args.iter().map(|a| self.eval(a)).collect();
            return Val::V(self.call_lambda(&l, vals));
        }
        if let Some(def) = functions::lookup(name) {
            if args.len() < def.min || args.len() > def.max {
                return Val::V(Value::Error(ErrorKind::Value));
            }
            return match def.kind {
                FnKind::Scalar(f) => {
                    let vals: Vec<Value> = args.iter().map(|a| self.eval_value(a)).collect();
                    Val::V(lift(&vals, f))
                }
                FnKind::Raw(f) => Val::V(f(self, args)),
                FnKind::RefRaw(f) => f(self, args),
            };
        }
        // User-defined LAMBDA in a defined name
        if let Some(def) = self.wb.find_name(name, self.sheet) {
            if let Expr::Call(f, largs) = &def.formula {
                if f == "LAMBDA" {
                    if let Some(l) = make_lambda(largs) {
                        let vals: Vec<Val> = args.iter().map(|a| self.eval(a)).collect();
                        return Val::V(self.call_lambda(&l, vals));
                    }
                }
            }
        }
        Val::V(Value::Error(ErrorKind::Name))
    }

    fn eval_binary(&mut self, op: BinaryOp, a: &Expr, b: &Expr) -> Val {
        match op {
            BinaryOp::Range => {
                let (x, y) = (self.eval(a), self.eval(b));
                match (x, y) {
                    (Val::Refs(x), Val::Refs(y)) if x.len() == 1 && y.len() == 1 && x[0].sheet == y[0].sheet => {
                        let r1 = x[0].rect;
                        let r2 = y[0].rect;
                        Val::Refs(vec![Area {
                            sheet: x[0].sheet,
                            rect: Rect {
                                top: r1.top.min(r2.top),
                                left: r1.left.min(r2.left),
                                bottom: r1.bottom.max(r2.bottom),
                                right: r1.right.max(r2.right),
                            },
                        }])
                    }
                    (Val::V(Value::Error(e)), _) | (_, Val::V(Value::Error(e))) => Val::V(Value::Error(e)),
                    _ => Val::V(Value::Error(ErrorKind::Value)),
                }
            }
            BinaryOp::Union => {
                let (x, y) = (self.eval(a), self.eval(b));
                match (x, y) {
                    (Val::Refs(mut x), Val::Refs(y)) => {
                        x.extend(y);
                        Val::Refs(x)
                    }
                    (Val::V(Value::Error(e)), _) | (_, Val::V(Value::Error(e))) => Val::V(Value::Error(e)),
                    _ => Val::V(Value::Error(ErrorKind::Value)),
                }
            }
            BinaryOp::Intersect => {
                let (x, y) = (self.eval(a), self.eval(b));
                match (x, y) {
                    (Val::Refs(x), Val::Refs(y)) => {
                        let mut out = vec![];
                        for p in &x {
                            for q in &y {
                                if p.sheet == q.sheet {
                                    if let Some(r) = p.rect.intersection(&q.rect) {
                                        out.push(Area { sheet: p.sheet, rect: r });
                                    }
                                }
                            }
                        }
                        if out.is_empty() {
                            Val::V(Value::Error(ErrorKind::Null))
                        } else {
                            Val::Refs(out)
                        }
                    }
                    (Val::V(Value::Error(e)), _) | (_, Val::V(Value::Error(e))) => Val::V(Value::Error(e)),
                    _ => Val::V(Value::Error(ErrorKind::Value)),
                }
            }
            _ => {
                let x = self.eval_value(a);
                let y = self.eval_value(b);
                Val::V(broadcast2(&x, &y, |p, q| binary_scalar(op, p, q)))
            }
        }
    }

    /// Evaluate to a value; references become arrays (or a scalar for one cell).
    pub fn eval_value(&mut self, e: &Expr) -> Value {
        let v = self.eval(e);
        self.deref(v)
    }

    /// Turn a Val into a Value (references become arrays).
    pub fn deref(&mut self, v: Val) -> Value {
        match v {
            Val::V(v) => v,
            Val::Refs(areas) => {
                if areas.len() != 1 {
                    return Value::Error(ErrorKind::Value);
                }
                let a = areas[0];
                if a.rect.rows() == 1 && a.rect.cols() == 1 {
                    self.cell_value(a.sheet, a.rect.top_left())
                } else {
                    Value::from(self.area_array(a))
                }
            }
        }
    }

    /// Evaluate to a single value (implicit intersection for ranges, first element for arrays).
    pub fn eval_scalar(&mut self, e: &Expr) -> Value {
        let v = self.eval(e);
        self.to_scalar(v)
    }

    pub fn to_scalar(&mut self, v: Val) -> Value {
        match v {
            Val::V(Value::Array(a)) => a.data.into_iter().next().unwrap_or(Value::Error(ErrorKind::Calc)),
            Val::V(v) => v,
            Val::Refs(areas) => {
                if areas.len() != 1 {
                    return Value::Error(ErrorKind::Value);
                }
                let a = areas[0];
                let r = a.rect;
                if r.rows() == 1 && r.cols() == 1 {
                    return self.cell_value(a.sheet, r.top_left());
                }
                if r.cols() == 1 && self.pos.row >= r.top && self.pos.row <= r.bottom {
                    return self.cell_value(a.sheet, CellPos::new(self.pos.row, r.left));
                }
                if r.rows() == 1 && self.pos.col >= r.left && self.pos.col <= r.right {
                    return self.cell_value(a.sheet, CellPos::new(r.top, self.pos.col));
                }
                Value::Error(ErrorKind::Value)
            }
        }
    }

    // ----- helpers for function implementations -----------------------

    pub fn eval_number(&mut self, e: &Expr) -> Result<f64, ErrorKind> {
        self.eval_scalar(e).to_number()
    }
    pub fn eval_text(&mut self, e: &Expr) -> Result<String, ErrorKind> {
        self.eval_scalar(e).to_text()
    }
    pub fn eval_bool(&mut self, e: &Expr) -> Result<bool, ErrorKind> {
        self.eval_scalar(e).to_bool()
    }
    /// Optional numeric argument with a default (also used when the argument is omitted).
    pub fn opt_number(&mut self, args: &[Expr], i: usize, default: f64) -> Result<f64, ErrorKind> {
        match args.get(i) {
            None | Some(Expr::Missing) => Ok(default),
            Some(e) => self.eval_number(e),
        }
    }
    pub fn opt_bool(&mut self, args: &[Expr], i: usize, default: bool) -> Result<bool, ErrorKind> {
        match args.get(i) {
            None | Some(Expr::Missing) => Ok(default),
            Some(e) => self.eval_bool(e),
        }
    }

    /// Evaluate an argument that must be a reference.
    pub fn eval_area(&mut self, e: &Expr) -> Result<Area, ErrorKind> {
        match self.eval(e) {
            Val::Refs(a) if a.len() == 1 => Ok(a[0]),
            Val::Refs(_) => Err(ErrorKind::Value),
            Val::V(Value::Error(e)) => Err(e),
            Val::V(_) => Err(ErrorKind::Value),
        }
    }

    /// Evaluate an argument to a 2D array (scalars become 1x1).
    pub fn eval_array(&mut self, e: &Expr) -> Result<Array, ErrorKind> {
        match self.eval_value(e) {
            Value::Array(a) => Ok(*a),
            Value::Error(e) => Err(e),
            v => Ok(Array::new(1, 1, vec![v])),
        }
    }

    /// Visit every value in the arguments. `from_ref` is true for values that
    /// came from references or arrays (where Excel aggregate functions ignore
    /// text and logical values). Empty cells in references are skipped.
    pub fn for_each_value(
        &mut self,
        args: &[Expr],
        f: &mut dyn FnMut(&Value, bool) -> Result<(), ErrorKind>,
    ) -> Result<(), ErrorKind> {
        for a in args {
            if matches!(a, Expr::Missing) {
                continue;
            }
            match self.eval(a) {
                Val::V(Value::Array(arr)) => {
                    for v in arr.data.iter() {
                        f(v, true)?;
                    }
                }
                Val::V(v) => f(&v, false)?,
                Val::Refs(areas) => {
                    for area in areas {
                        for (_, v) in self.area_values(area) {
                            f(&v, true)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Numbers from the arguments with SUM semantics: direct text/logical args are
    /// converted, text/logical in references are ignored, errors propagate.
    pub fn collect_numbers(&mut self, args: &[Expr]) -> Result<Vec<f64>, ErrorKind> {
        let mut out = Vec::new();
        self.for_each_value(args, &mut |v, from_ref| {
            match v {
                Value::Number(n) => out.push(*n),
                Value::Error(e) => return Err(*e),
                Value::Bool(b) if !from_ref => out.push(if *b { 1.0 } else { 0.0 }),
                Value::Text(_) if !from_ref => out.push(v.to_number()?),
                _ => {}
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Numbers with AVERAGEA/COUNTA semantics: text in references counts as 0,
    /// logical values count as 1/0.
    pub fn collect_numbers_a(&mut self, args: &[Expr]) -> Result<Vec<f64>, ErrorKind> {
        let mut out = Vec::new();
        self.for_each_value(args, &mut |v, from_ref| {
            match v {
                Value::Number(n) => out.push(*n),
                Value::Error(e) => return Err(*e),
                Value::Bool(b) => out.push(if *b { 1.0 } else { 0.0 }),
                Value::Text(_) => {
                    if from_ref {
                        out.push(0.0)
                    } else {
                        out.push(v.to_number()?)
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Evaluate a formula expression in the context of a given cell (used by
    /// conditional formatting and data validation).
    pub fn eval_at(&mut self, sheet: usize, pos: CellPos, e: &Expr) -> Value {
        let saved = (self.sheet, self.pos);
        self.sheet = sheet;
        self.pos = pos;
        let v = self.eval_scalar(e);
        self.sheet = saved.0;
        self.pos = saved.1;
        v
    }
}

/// Build a lambda from LAMBDA's argument list.
pub fn make_lambda(args: &[Expr]) -> Option<Lambda> {
    let (body, params) = args.split_last()?;
    let mut names = Vec::new();
    for p in params {
        match p {
            Expr::Name(n) => names.push(n.to_ascii_uppercase()),
            _ => return None,
        }
    }
    Some(Lambda { params: names, body: body.clone() })
}

fn finalize_result(v: Value) -> Value {
    match v {
        Value::Empty => Value::Number(0.0),
        Value::Array(a) => {
            if a.rows == 1 && a.cols == 1 {
                finalize_result(a.data.into_iter().next().unwrap())
            } else if a.data.is_empty() {
                Value::Error(ErrorKind::Calc)
            } else {
                let data = a.data.into_iter().map(|v| if v.is_empty() { Value::Number(0.0) } else { v }).collect();
                Value::from(Array::new(a.rows, a.cols, data))
            }
        }
        v => v,
    }
}

/// Apply a numeric function to a value or each element of an array.
pub fn map_numbers(v: Value, f: impl Fn(f64) -> f64) -> Value {
    match v {
        Value::Array(a) => {
            let data = a
                .data
                .iter()
                .map(|x| match x.to_number() {
                    Ok(n) => Value::number(f(n)),
                    Err(e) => Value::Error(e),
                })
                .collect();
            Value::from(Array::new(a.rows, a.cols, data))
        }
        v => match v.to_number() {
            Ok(n) => Value::number(f(n)),
            Err(e) => Value::Error(e),
        },
    }
}

/// Apply a binary function element-wise with Excel's array broadcasting.
pub fn broadcast2(a: &Value, b: &Value, f: impl Fn(&Value, &Value) -> Value) -> Value {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            let rows = bdim(x.rows, y.rows);
            let cols = bdim(x.cols, y.cols);
            let mut data = Vec::with_capacity(rows * cols);
            for r in 0..rows {
                for c in 0..cols {
                    let p = x.get_broadcast(r, c);
                    let q = y.get_broadcast(r, c);
                    if matches!(p, Value::Error(ErrorKind::NA)) && (r >= x.rows && x.rows != 1 || c >= x.cols && x.cols != 1) {
                        data.push(Value::Error(ErrorKind::NA));
                        continue;
                    }
                    data.push(f(&p, &q));
                }
            }
            Value::from(Array::new(rows, cols, data))
        }
        (Value::Array(x), s) => {
            let data = x.data.iter().map(|p| f(p, s)).collect();
            Value::from(Array::new(x.rows, x.cols, data))
        }
        (s, Value::Array(y)) => {
            let data = y.data.iter().map(|q| f(s, q)).collect();
            Value::from(Array::new(y.rows, y.cols, data))
        }
        (p, q) => f(p, q),
    }
}

fn bdim(a: usize, b: usize) -> usize {
    if a == 1 {
        b
    } else if b == 1 {
        a
    } else {
        a.max(b)
    }
}

/// Lift a scalar function over array arguments.
pub fn lift(args: &[Value], f: fn(&[Value]) -> Value) -> Value {
    let mut rows = 1;
    let mut cols = 1;
    let mut any = false;
    for a in args {
        if let Value::Array(x) = a {
            any = true;
            rows = bdim(rows, x.rows);
            cols = bdim(cols, x.cols);
        }
    }
    if !any {
        return f(args);
    }
    let mut data = Vec::with_capacity(rows * cols);
    let mut buf: Vec<Value> = Vec::with_capacity(args.len());
    for r in 0..rows {
        for c in 0..cols {
            buf.clear();
            for a in args {
                buf.push(match a {
                    Value::Array(x) => x.get_broadcast(r, c),
                    v => v.clone(),
                });
            }
            data.push(f(&buf));
        }
    }
    Value::from(Array::new(rows, cols, data))
}

fn binary_scalar(op: BinaryOp, a: &Value, b: &Value) -> Value {
    use BinaryOp::*;
    match op {
        Add | Sub | Mul | Div | Pow => {
            let x = match a.to_number() {
                Ok(x) => x,
                Err(e) => return Value::Error(e),
            };
            let y = match b.to_number() {
                Ok(y) => y,
                Err(e) => return Value::Error(e),
            };
            match op {
                Add => Value::number(x + y),
                Sub => Value::number(x - y),
                Mul => Value::number(x * y),
                Div => {
                    if y == 0.0 {
                        Value::Error(ErrorKind::Div0)
                    } else {
                        Value::number(x / y)
                    }
                }
                Pow => {
                    if x == 0.0 && y == 0.0 {
                        Value::Error(ErrorKind::Num)
                    } else if x == 0.0 && y < 0.0 {
                        Value::Error(ErrorKind::Div0)
                    } else {
                        Value::number(x.powf(y))
                    }
                }
                _ => unreachable!(),
            }
        }
        Concat => {
            let x = match a.to_text() {
                Ok(x) => x,
                Err(e) => return Value::Error(e),
            };
            let y = match b.to_text() {
                Ok(y) => y,
                Err(e) => return Value::Error(e),
            };
            Value::Text(x + &y)
        }
        Eq | Ne | Lt | Le | Gt | Ge => {
            if let Value::Error(e) = a {
                return Value::Error(*e);
            }
            if let Value::Error(e) = b {
                return Value::Error(*e);
            }
            let ord = a.compare(b);
            use std::cmp::Ordering::*;
            Value::Bool(match op {
                Eq => ord == Equal,
                Ne => ord != Equal,
                Lt => ord == Less,
                Le => ord != Greater,
                Gt => ord == Greater,
                Ge => ord != Less,
                _ => unreachable!(),
            })
        }
        _ => Value::Error(ErrorKind::Value),
    }
}

/// Evaluate a standalone formula against the workbook's current values.
pub fn eval_formula(wb: &Workbook, sheet: usize, pos: CellPos, e: &Expr) -> Value {
    let mut ctx = Ctx::readonly(wb, sheet, pos);
    let v = ctx.eval_value(e);
    finalize_result(v)
}

type Results = (Vec<((usize, CellPos), Value)>, Vec<(usize, CellPos)>);

fn compute_all(wb: &Workbook) -> Results {
    let mut ctx = Ctx::new(wb);
    let mut keys = Vec::new();
    for (si, s) in wb.sheets.iter().enumerate() {
        for (p, c) in &s.cells {
            if c.formula.is_some() {
                keys.push((si, *p));
            }
        }
    }
    for (s, p) in &keys {
        ctx.compute(*s, *p);
    }
    let circular = std::mem::take(&mut ctx.circular);
    let memo = std::mem::take(&mut ctx.memo);
    drop(ctx);
    let mut out = Vec::with_capacity(memo.len());
    for (k, slot) in memo {
        if let Slot::Done(rc) = slot {
            let v = Rc::try_unwrap(rc).unwrap_or_else(|rc| (*rc).clone());
            out.push((k, v));
        }
    }
    (out, circular)
}

/// Recalculate every formula, then lay out spilled arrays.
pub fn recalculate(wb: &mut Workbook) {
    for _ in 0..4 {
        let (results, circular) = std::thread::scope(|s| {
            let wbr: &Workbook = wb;
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, move || compute_all(wbr))
                .expect("spawn calc thread")
                .join()
                .unwrap_or_else(|_| (Vec::new(), Vec::new()))
        });
        wb.circular = circular;
        for ((s, p), v) in results {
            if let Some(c) = wb.sheets[s].cells.get_mut(&p) {
                if c.formula.is_some() {
                    c.value = v;
                }
            }
        }
        if !layout_spills(wb) {
            break;
        }
    }
}

/// Recompute spill areas. Returns true if they changed.
fn layout_spills(wb: &mut Workbook) -> bool {
    let mut changed = false;
    for sheet in wb.sheets.iter_mut() {
        let mut owner: HashMap<CellPos, CellPos> = HashMap::new();
        let mut areas: HashMap<CellPos, Rect> = HashMap::new();
        let anchors: Vec<(CellPos, usize, usize)> = sheet
            .cells
            .iter()
            .filter_map(|(p, c)| match (&c.formula, &c.value) {
                (Some(_), Value::Array(a)) => Some((*p, a.rows, a.cols)),
                _ => None,
            })
            .collect();
        let mut spill_errors = Vec::new();
        for (anchor, rows, cols) in anchors {
            let bottom = anchor.row as u64 + rows as u64 - 1;
            let right = anchor.col as u64 + cols as u64 - 1;
            if bottom >= MAX_ROWS as u64 || right >= MAX_COLS as u64 {
                spill_errors.push(anchor);
                continue;
            }
            let rect = Rect { top: anchor.row, left: anchor.col, bottom: bottom as u32, right: right as u32 };
            let blocked = sheet.cells_in(rect).any(|(p, c)| *p != anchor && !c.is_blank())
                || rect.iter().any(|p| owner.contains_key(&p) || areas.contains_key(&p) && p != anchor);
            if blocked {
                spill_errors.push(anchor);
                continue;
            }
            for p in rect.iter() {
                if p != anchor {
                    owner.insert(p, anchor);
                }
            }
            areas.insert(anchor, rect);
        }
        for a in spill_errors {
            if let Some(c) = sheet.cells.get_mut(&a) {
                c.value = Value::Error(ErrorKind::Spill);
            }
            if sheet.spill_areas.contains_key(&a) {
                changed = true;
            }
        }
        if areas != sheet.spill_areas {
            changed = true;
        }
        sheet.spill_owner = owner;
        sheet.spill_areas = areas;
    }
    changed
}
