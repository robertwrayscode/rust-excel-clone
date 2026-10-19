//! Formula abstract syntax tree, printer and reference transforms.

use crate::address::{col_to_letters, quote_sheet_name, CellPos, Rect, MAX_COLS, MAX_ROWS};
use crate::value::ErrorKind;

/// One end of a reference. `row`/`col` of `None` means "entire" (for A:A or 1:1).
#[derive(Debug, Clone, PartialEq)]
pub struct RefPart {
    pub row: Option<u32>,
    pub col: Option<u32>,
    pub row_abs: bool,
    pub col_abs: bool,
}

impl RefPart {
    pub fn cell(row: u32, col: u32, row_abs: bool, col_abs: bool) -> RefPart {
        RefPart { row: Some(row), col: Some(col), row_abs, col_abs }
    }
    fn fmt_into(&self, out: &mut String) {
        if let Some(c) = self.col {
            if self.col_abs {
                out.push('$');
            }
            out.push_str(&col_to_letters(c));
        }
        if let Some(r) = self.row {
            if self.row_abs {
                out.push('$');
            }
            out.push_str(&(r + 1).to_string());
        }
    }
}

/// A cell or area reference, optionally on another sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub sheet: Option<String>,
    pub start: RefPart,
    /// `None` for a single-cell reference.
    pub end: Option<RefPart>,
}

impl Reference {
    /// The absolute rectangle this reference covers.
    pub fn rect(&self) -> Rect {
        let s = &self.start;
        let e = self.end.as_ref().unwrap_or(&self.start);
        let top = s.row.unwrap_or(0).min(e.row.unwrap_or(0));
        let bottom = match (s.row, e.row) {
            (Some(a), Some(b)) => a.max(b),
            _ => MAX_ROWS - 1,
        };
        let left = s.col.unwrap_or(0).min(e.col.unwrap_or(0));
        let right = match (s.col, e.col) {
            (Some(a), Some(b)) => a.max(b),
            _ => MAX_COLS - 1,
        };
        let top = if s.row.is_none() || e.row.is_none() { 0 } else { top };
        let left = if s.col.is_none() || e.col.is_none() { 0 } else { left };
        Rect { top, left, bottom, right }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    Percent,
    /// `@` implicit intersection operator
    Implicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `:` between non-literal references, e.g. `A1:INDEX(...)`
    Range,
    /// space operator
    Intersect,
    /// `,` inside parentheses: (A1:A3,C1:C3)
    Union,
}

impl BinaryOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Pow => "^",
            BinaryOp::Concat => "&",
            BinaryOp::Eq => "=",
            BinaryOp::Ne => "<>",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::Range => ":",
            BinaryOp::Intersect => " ",
            BinaryOp::Union => ",",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(ErrorKind),
    Ref(Reference),
    /// A spilled-range reference, `A1#`
    SpillRef(Reference),
    /// Defined name or LET/LAMBDA variable
    Name(String),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// Function call; name is upper-cased with `_xlfn.` stripped.
    Call(String, Vec<Expr>),
    /// Call of an expression result, e.g. `LAMBDA(x,x+1)(3)`
    CallExpr(Box<Expr>, Vec<Expr>),
    Array(Vec<Vec<Expr>>),
    Paren(Box<Expr>),
    /// An omitted argument, e.g. the middle of `IF(A1,,2)`
    Missing,
}

impl Expr {
    /// Print as formula text (without leading `=`).
    pub fn to_formula(&self) -> String {
        let mut s = String::new();
        self.fmt_into(&mut s);
        s
    }

    fn fmt_into(&self, out: &mut String) {
        match self {
            Expr::Number(n) => out.push_str(&crate::value::format_number_general(*n)),
            Expr::Text(t) => {
                out.push('"');
                out.push_str(&t.replace('"', "\"\""));
                out.push('"');
            }
            Expr::Bool(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
            Expr::Error(e) => out.push_str(e.as_str()),
            Expr::Ref(r) => fmt_ref(r, out),
            Expr::SpillRef(r) => {
                fmt_ref(r, out);
                out.push('#');
            }
            Expr::Name(n) => out.push_str(n),
            Expr::Unary(op, e) => match op {
                UnaryOp::Neg => {
                    out.push('-');
                    e.fmt_into(out)
                }
                UnaryOp::Plus => {
                    out.push('+');
                    e.fmt_into(out)
                }
                UnaryOp::Implicit => {
                    out.push('@');
                    e.fmt_into(out)
                }
                UnaryOp::Percent => {
                    e.fmt_into(out);
                    out.push('%')
                }
            },
            Expr::Binary(op, a, b) => {
                a.fmt_into(out);
                out.push_str(op.symbol());
                b.fmt_into(out);
            }
            Expr::Call(name, args) => {
                out.push_str(name);
                out.push('(');
                fmt_args(args, out);
                out.push(')');
            }
            Expr::CallExpr(f, args) => {
                f.fmt_into(out);
                out.push('(');
                fmt_args(args, out);
                out.push(')');
            }
            Expr::Array(rows) => {
                out.push('{');
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        out.push(';');
                    }
                    for (j, e) in row.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        e.fmt_into(out);
                    }
                }
                out.push('}');
            }
            Expr::Paren(e) => {
                out.push('(');
                e.fmt_into(out);
                out.push(')');
            }
            Expr::Missing => {}
        }
    }

    /// Visit every reference in the expression mutably.
    pub fn walk_refs_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        match self {
            Expr::Ref(_) | Expr::SpillRef(_) => f(self),
            Expr::Unary(_, e) | Expr::Paren(e) => e.walk_refs_mut(f),
            Expr::Binary(_, a, b) => {
                a.walk_refs_mut(f);
                b.walk_refs_mut(f);
            }
            Expr::Call(_, args) => args.iter_mut().for_each(|a| a.walk_refs_mut(f)),
            Expr::CallExpr(g, args) => {
                g.walk_refs_mut(f);
                args.iter_mut().for_each(|a| a.walk_refs_mut(f));
            }
            Expr::Array(rows) => rows.iter_mut().flatten().for_each(|a| a.walk_refs_mut(f)),
            _ => {}
        }
    }

    /// Visit every reference immutably.
    pub fn walk_refs(&self, f: &mut dyn FnMut(&Reference)) {
        match self {
            Expr::Ref(r) | Expr::SpillRef(r) => f(r),
            Expr::Unary(_, e) | Expr::Paren(e) => e.walk_refs(f),
            Expr::Binary(_, a, b) => {
                a.walk_refs(f);
                b.walk_refs(f);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.walk_refs(f)),
            Expr::CallExpr(g, args) => {
                g.walk_refs(f);
                args.iter().for_each(|a| a.walk_refs(f));
            }
            Expr::Array(rows) => rows.iter().flatten().for_each(|a| a.walk_refs(f)),
            _ => {}
        }
    }

    /// Does this formula contain a volatile function (NOW, RAND, OFFSET, INDIRECT...)?
    pub fn is_volatile(&self) -> bool {
        match self {
            Expr::Call(name, args) => {
                matches!(
                    name.as_str(),
                    "NOW" | "TODAY" | "RAND" | "RANDBETWEEN" | "RANDARRAY" | "OFFSET" | "INDIRECT" | "INFO" | "CELL"
                ) || args.iter().any(|a| a.is_volatile())
            }
            Expr::CallExpr(g, args) => g.is_volatile() || args.iter().any(|a| a.is_volatile()),
            Expr::Unary(_, e) | Expr::Paren(e) => e.is_volatile(),
            Expr::Binary(_, a, b) => a.is_volatile() || b.is_volatile(),
            Expr::Array(rows) => rows.iter().flatten().any(|a| a.is_volatile()),
            _ => false,
        }
    }

    /// Shift relative parts of references by (drow, dcol), as when copying a
    /// formula. References that go off-sheet become #REF!.
    pub fn shift_relative(&mut self, drow: i64, dcol: i64) {
        self.walk_refs_mut(&mut |e| {
            let r = match e {
                Expr::Ref(r) | Expr::SpillRef(r) => r,
                _ => return,
            };
            let mut ok = true;
            for part in std::iter::once(&mut r.start).chain(r.end.iter_mut()) {
                if let Some(row) = part.row {
                    if !part.row_abs {
                        let n = row as i64 + drow;
                        if n < 0 || n >= MAX_ROWS as i64 {
                            ok = false;
                        } else {
                            part.row = Some(n as u32);
                        }
                    }
                }
                if let Some(col) = part.col {
                    if !part.col_abs {
                        let n = col as i64 + dcol;
                        if n < 0 || n >= MAX_COLS as i64 {
                            ok = false;
                        } else {
                            part.col = Some(n as u32);
                        }
                    }
                }
            }
            if !ok {
                *e = Expr::Error(ErrorKind::Ref);
            }
        });
    }

    /// Adjust references for inserted/deleted rows or columns on `sheet`.
    /// `formula_sheet` is the sheet the formula lives on (used for unqualified refs).
    /// `at` is the first affected index, `count` positive = insert, negative = delete.
    pub fn adjust_for_insert_delete(
        &mut self,
        formula_sheet: &str,
        target_sheet: &str,
        is_row: bool,
        at: u32,
        count: i64,
    ) {
        self.walk_refs_mut(&mut |e| {
            let r = match e {
                Expr::Ref(r) | Expr::SpillRef(r) => r,
                _ => return,
            };
            let sheet = r.sheet.as_deref().unwrap_or(formula_sheet);
            if !sheet.eq_ignore_ascii_case(target_sheet) {
                return;
            }
            let max = if is_row { MAX_ROWS } else { MAX_COLS } as i64;
            let get = |p: &RefPart| if is_row { p.row } else { p.col };
            let is_area = r.end.is_some();
            let start_v = get(&r.start);
            let end_v = r.end.as_ref().and_then(get);
            if start_v.is_none() {
                return; // whole row/col in the other dimension
            }
            let (s, en) = match (start_v, end_v) {
                (Some(a), Some(b)) => (a.min(b) as i64, a.max(b) as i64),
                (Some(a), None) => (a as i64, a as i64),
                _ => return,
            };
            let at = at as i64;
            let (ns, ne) = if count > 0 {
                let ns = if s >= at { s + count } else { s };
                let ne = if en >= at { en + count } else { en };
                (ns, ne)
            } else {
                let del_end = at - count - 1; // inclusive
                let n = -count;
                let map = |v: i64| -> Option<i64> {
                    if v < at {
                        Some(v)
                    } else if v > del_end {
                        Some(v - n)
                    } else {
                        None
                    }
                };
                if is_area {
                    // shrink areas
                    let ns = if s < at { s } else if s > del_end { s - n } else { at };
                    let ne = if en < at { en } else if en > del_end { en - n } else { at - 1 };
                    if ne < ns {
                        *e = Expr::Error(ErrorKind::Ref);
                        return;
                    }
                    (ns, ne)
                } else {
                    match map(s) {
                        Some(v) => (v, v),
                        None => {
                            *e = Expr::Error(ErrorKind::Ref);
                            return;
                        }
                    }
                }
            };
            if ne >= max || ns >= max {
                *e = Expr::Error(ErrorKind::Ref);
                return;
            }
            let set = |p: &mut RefPart, v: i64| {
                if is_row {
                    p.row = Some(v as u32)
                } else {
                    p.col = Some(v as u32)
                }
            };
            if let Some(end) = r.end.as_mut() {
                // keep orientation: start is the smaller unless user wrote reversed
                let start_smaller = get(&r.start) <= get(end);
                if start_smaller {
                    set(&mut r.start, ns);
                    set(end, ne);
                } else {
                    set(&mut r.start, ne);
                    set(end, ns);
                }
            } else {
                set(&mut r.start, ns);
            }
        });
    }

    /// Rename sheet references.
    pub fn rename_sheet(&mut self, old: &str, new: &str) {
        self.walk_refs_mut(&mut |e| {
            if let Expr::Ref(r) | Expr::SpillRef(r) = e {
                if let Some(s) = &r.sheet {
                    if s.eq_ignore_ascii_case(old) {
                        r.sheet = Some(new.to_string());
                    }
                }
            }
        });
    }

    /// Replace references to a deleted sheet with #REF!.
    pub fn invalidate_sheet(&mut self, name: &str) {
        self.walk_refs_mut(&mut |e| {
            if let Expr::Ref(r) | Expr::SpillRef(r) = e {
                if let Some(s) = &r.sheet {
                    if s.eq_ignore_ascii_case(name) {
                        *e = Expr::Error(ErrorKind::Ref);
                    }
                }
            }
        });
    }

    /// Rewrite references that point into `from` (on `sheet`) to the same
    /// relative spot in a rectangle moved by (drow, dcol). Used by cut/paste.
    pub fn move_refs(&mut self, formula_sheet: &str, sheet: &str, from: Rect, drow: i64, dcol: i64) {
        self.walk_refs_mut(&mut |e| {
            if let Expr::Ref(r) | Expr::SpillRef(r) = e {
                let s = r.sheet.as_deref().unwrap_or(formula_sheet);
                if !s.eq_ignore_ascii_case(sheet) {
                    return;
                }
                let rr = r.rect();
                if from.contains(rr.top_left()) && from.contains(rr.bottom_right()) {
                    for part in std::iter::once(&mut r.start).chain(r.end.iter_mut()) {
                        if let Some(row) = part.row.as_mut() {
                            *row = (*row as i64 + drow).clamp(0, MAX_ROWS as i64 - 1) as u32;
                        }
                        if let Some(col) = part.col.as_mut() {
                            *col = (*col as i64 + dcol).clamp(0, MAX_COLS as i64 - 1) as u32;
                        }
                    }
                }
            }
        });
    }
}

fn fmt_args(args: &[Expr], out: &mut String) {
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        a.fmt_into(out);
    }
}

fn fmt_ref(r: &Reference, out: &mut String) {
    if let Some(s) = &r.sheet {
        out.push_str(&quote_sheet_name(s));
        out.push('!');
    }
    r.start.fmt_into(out);
    if let Some(e) = &r.end {
        out.push(':');
        e.fmt_into(out);
    }
}

/// Build a simple cell reference expression.
pub fn cell_ref(sheet: Option<&str>, pos: CellPos) -> Expr {
    Expr::Ref(Reference {
        sheet: sheet.map(|s| s.to_string()),
        start: RefPart::cell(pos.row, pos.col, false, false),
        end: None,
    })
}
