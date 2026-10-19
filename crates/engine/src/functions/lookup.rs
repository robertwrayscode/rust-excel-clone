//! Lookup & Reference functions (VLOOKUP, INDEX, MATCH, XLOOKUP, OFFSET,
//! INDIRECT, FILTER, SORT, UNIQUE, ...).

use std::cmp::Ordering;
use std::collections::HashMap;

use super::{bool_or, int, num_or, Registry, VAR};
use crate::address::{col_to_letters, quote_sheet_name, CellPos, Rect, MAX_COLS, MAX_ROWS};
use crate::ast::{BinaryOp, Expr};
use crate::criteria::wildcard_match;
use crate::eval::{broadcast2, Area, Ctx, Val};
use crate::functions::array::{resolve_fn, single};
use crate::parser::parse_formula;
use crate::tryv;
use crate::value::{compare_text, Array, ErrorKind, Value};

const CAT: &str = "Lookup & Reference";

pub fn register(r: &mut Registry) {
    r.scalar("ADDRESS", 2, 5, address, CAT, "ADDRESS(row_num, column_num, [abs_num], [a1], [sheet_text])", "Returns a reference as text to a single cell.");
    r.raw("AREAS", 1, 1, areas, CAT, "AREAS(reference)", "Returns the number of areas in a reference.");
    r.raw("CHOOSECOLS", 2, VAR, choosecols, CAT, "CHOOSECOLS(array, col_num1, [col_num2], ...)", "Returns the specified columns from an array.");
    r.raw("CHOOSEROWS", 2, VAR, chooserows, CAT, "CHOOSEROWS(array, row_num1, [row_num2], ...)", "Returns the specified rows from an array.");
    r.raw("COLUMN", 0, 1, column, CAT, "COLUMN([reference])", "Returns the column number of a reference.");
    r.raw("COLUMNS", 1, 1, columns, CAT, "COLUMNS(array)", "Returns the number of columns in a reference or array.");
    r.raw("DROP", 2, 3, drop, CAT, "DROP(array, rows, [columns])", "Excludes a number of rows or columns from the start or end of an array.");
    r.raw("EXPAND", 2, 4, expand, CAT, "EXPAND(array, rows, [columns], [pad_with])", "Expands an array to the specified dimensions.");
    r.raw("FILTER", 2, 3, filter, CAT, "FILTER(array, include, [if_empty])", "Filters a range of data based on criteria you define.");
    r.raw("FORMULATEXT", 1, 1, formulatext, CAT, "FORMULATEXT(reference)", "Returns the formula at the given reference as text.");
    r.raw("GETPIVOTDATA", 2, VAR, getpivotdata, CAT, "GETPIVOTDATA(data_field, pivot_table, [field1, item1], ...)", "Returns data stored in a PivotTable report.");
    r.raw("HLOOKUP", 3, 4, hlookup, CAT, "HLOOKUP(lookup_value, table_array, row_index_num, [range_lookup])", "Looks in the top row of a table and returns the value in the indicated row.");
    r.raw("HSTACK", 1, VAR, hstack, CAT, "HSTACK(array1, [array2], ...)", "Appends arrays horizontally and in sequence to return a larger array.");
    r.scalar("HYPERLINK", 1, 2, hyperlink, CAT, "HYPERLINK(link_location, [friendly_name])", "Creates a shortcut that opens a document or web page.");
    r.refraw("INDEX", 2, 4, index, CAT, "INDEX(array, row_num, [column_num], [area_num])", "Returns a value or reference of the cell at the intersection of a row and column.");
    r.refraw("INDIRECT", 1, 2, indirect, CAT, "INDIRECT(ref_text, [a1])", "Returns the reference specified by a text string.");
    r.raw("LOOKUP", 2, 3, lookup_fn, CAT, "LOOKUP(lookup_value, lookup_vector, [result_vector])", "Looks up a value in a vector or array.");
    r.raw("MATCH", 2, 3, match_fn, CAT, "MATCH(lookup_value, lookup_array, [match_type])", "Returns the relative position of an item in an array.");
    r.refraw("OFFSET", 3, 5, offset, CAT, "OFFSET(reference, rows, cols, [height], [width])", "Returns a reference offset from a given reference.");
    r.raw("ROW", 0, 1, row, CAT, "ROW([reference])", "Returns the row number of a reference.");
    r.raw("ROWS", 1, 1, rows, CAT, "ROWS(array)", "Returns the number of rows in a reference or array.");
    r.raw("RTD", 3, VAR, rtd, CAT, "RTD(progID, server, topic1, [topic2], ...)", "Retrieves real-time data from a program that supports COM automation.");
    r.raw("SORT", 1, 4, sort, CAT, "SORT(array, [sort_index], [sort_order], [by_col])", "Sorts the contents of a range or array.");
    r.raw("SORTBY", 2, VAR, sortby, CAT, "SORTBY(array, by_array1, [sort_order1], [by_array2, sort_order2], ...)", "Sorts a range or array based on the values in corresponding ranges or arrays.");
    r.raw("TAKE", 2, 3, take, CAT, "TAKE(array, rows, [columns])", "Returns a number of rows or columns from the start or end of an array.");
    r.raw("TOCOL", 1, 3, tocol, CAT, "TOCOL(array, [ignore], [scan_by_column])", "Returns the array in a single column.");
    r.raw("TOROW", 1, 3, torow, CAT, "TOROW(array, [ignore], [scan_by_column])", "Returns the array in a single row.");
    r.raw("TRANSPOSE", 1, 1, transpose, CAT, "TRANSPOSE(array)", "Returns the transpose of an array.");
    r.raw("UNIQUE", 1, 3, unique, CAT, "UNIQUE(array, [by_col], [exactly_once])", "Returns the unique values from a range or array.");
    r.raw("VLOOKUP", 3, 4, vlookup, CAT, "VLOOKUP(lookup_value, table_array, col_index_num, [range_lookup])", "Looks in the first column of a table and returns the value in the indicated column.");
    r.raw("VSTACK", 1, VAR, vstack, CAT, "VSTACK(array1, [array2], ...)", "Appends arrays vertically and in sequence to return a larger array.");
    r.raw("WRAPCOLS", 2, 3, wrapcols, CAT, "WRAPCOLS(vector, wrap_count, [pad_with])", "Wraps a row or column of values by columns after a specified number of elements.");
    r.raw("WRAPROWS", 2, 3, wraprows, CAT, "WRAPROWS(vector, wrap_count, [pad_with])", "Wraps a row or column of values by rows after a specified number of elements.");
    r.refraw("XLOOKUP", 3, 6, xlookup, CAT, "XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode])", "Searches a range or array and returns the item corresponding to the first match.");
    r.raw("XMATCH", 2, 4, xmatch, CAT, "XMATCH(lookup_value, lookup_array, [match_mode], [search_mode])", "Returns the relative position of an item in an array or range.");
    r.refraw("TRIMRANGE", 1, 3, trimrange, CAT, "TRIMRANGE(range, [trim_rows], [trim_cols])", "Excludes empty rows and columns from the outer edges of a range or array.");
    r.raw("GROUPBY", 3, 8, groupby, CAT, "GROUPBY(row_fields, values, function, [field_headers], [total_depth], [sort_order], [filter_array], [field_relationship])", "Groups rows by the row fields and aggregates the values.");
    r.raw("PIVOTBY", 4, 11, pivotby, CAT, "PIVOTBY(row_fields, col_fields, values, function, [field_headers], [row_total_depth], [row_sort_order], [col_total_depth], [col_sort_order], [filter_array], [relative_to])", "Groups values by row and column fields and aggregates them.");
}

// ----- shared helpers ---------------------------------------------------

fn err<T>(e: ErrorKind) -> Result<T, ErrorKind> {
    Err(e)
}

fn verr(e: ErrorKind) -> Val {
    Val::V(Value::Error(e))
}

fn is_missing(args: &[Expr], i: usize) -> bool {
    matches!(args.get(i), None | Some(Expr::Missing))
}

/// Optional integer argument (truncated) or `None` when omitted.
fn opt_i64(ctx: &mut Ctx, args: &[Expr], i: usize) -> Result<Option<i64>, ErrorKind> {
    if is_missing(args, i) {
        return Ok(None);
    }
    Ok(Some(ctx.eval_number(&args[i])?.trunc() as i64))
}

fn to_value(a: Array) -> Value {
    if a.rows == 1 && a.cols == 1 {
        a.data.into_iter().next().unwrap()
    } else if a.data.is_empty() {
        Value::Error(ErrorKind::Calc)
    } else {
        Value::from(a)
    }
}

/// Values of a one-dimensional array (a row or a column).
fn vector(a: &Array) -> Option<Vec<Value>> {
    if a.cols == 1 || a.rows == 1 {
        Some(a.data.clone())
    } else {
        None
    }
}

/// Sub-array of rows `r0..r1` and columns `c0..c1` (exclusive ends).
fn sub(a: &Array, r0: usize, r1: usize, c0: usize, c1: usize) -> Array {
    let mut data = Vec::with_capacity((r1 - r0) * (c1 - c0));
    for r in r0..r1 {
        for c in c0..c1 {
            data.push(a.get(r, c).clone());
        }
    }
    Array::new(r1 - r0, c1 - c0, data)
}

/// Compare a candidate with a lookup key when both are of the same type.
fn lk_cmp(cand: &Value, key: &Value) -> Option<Ordering> {
    match (cand, key) {
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
        (Value::Text(a), Value::Text(b)) => Some(compare_text(a, b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Empty, Value::Empty) => Some(Ordering::Equal),
        _ => None,
    }
}

/// Exact lookup equality (case-insensitive; optional wildcards for text keys).
fn lk_eq(cand: &Value, key: &Value, wildcard: bool) -> bool {
    if wildcard {
        if let (Value::Text(p), Value::Text(s)) = (key, cand) {
            if p.contains(['*', '?', '~']) {
                return wildcard_match(p, s);
            }
        }
    }
    lk_cmp(cand, key) == Some(Ordering::Equal)
}

/// Excel-style binary search: the last index whose (comparable) value
/// satisfies `left(ordering of value vs key)`. Values of other types are skipped.
fn bsearch_last(vals: &[Value], key: &Value, left: impl Fn(Ordering) -> bool) -> Option<usize> {
    let (mut lo, mut hi) = (0usize, vals.len());
    let mut best = None;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let mut m = mid;
        while m < hi && lk_cmp(&vals[m], key).is_none() {
            m += 1;
        }
        if m == hi {
            hi = mid;
            continue;
        }
        if left(lk_cmp(&vals[m], key).unwrap()) {
            best = Some(m);
            lo = m + 1;
        } else {
            hi = mid;
        }
    }
    best
}

/// First comparable index after `after` (or from the start when `None`).
fn next_comparable(vals: &[Value], key: &Value, after: Option<usize>) -> Option<usize> {
    let start = after.map(|i| i + 1).unwrap_or(0);
    (start..vals.len()).find(|&i| lk_cmp(&vals[i], key).is_some())
}

/// Apply `f` to a lookup key, or to each element when the key is an array.
fn map_key(key: &Value, mut f: impl FnMut(&Value) -> Value) -> Value {
    match key {
        Value::Array(a) => {
            let data = a.data.iter().map(|k| single(f(k))).collect();
            Value::from(Array::new(a.rows, a.cols, data))
        }
        k => f(k),
    }
}

/// MATCH/VLOOKUP-style search. `mode`: 0 exact (wildcards), 1 largest <= key
/// (ascending data), -1 smallest >= key (descending data).
fn legacy_search(vals: &[Value], key: &Value, mode: i32) -> Option<usize> {
    if key.is_empty() {
        return None;
    }
    match mode {
        0 => vals.iter().position(|c| lk_eq(c, key, true)),
        1 => bsearch_last(vals, key, |o| o != Ordering::Greater),
        _ => bsearch_last(vals, key, |o| o != Ordering::Less),
    }
}

/// XMATCH/XLOOKUP search. Returns the zero-based index found.
fn xsearch(vals: &[Value], key: &Value, mode: i64, search: i64) -> Option<usize> {
    match search {
        2 => {
            let le = bsearch_last(vals, key, |o| o != Ordering::Greater);
            if let Some(i) = le {
                if lk_cmp(&vals[i], key) == Some(Ordering::Equal) {
                    return Some(i);
                }
            }
            match mode {
                -1 => le,
                1 => {
                    let lt = bsearch_last(vals, key, |o| o == Ordering::Less);
                    next_comparable(vals, key, lt)
                }
                _ => None,
            }
        }
        -2 => {
            let ge = bsearch_last(vals, key, |o| o != Ordering::Less);
            if let Some(i) = ge {
                if lk_cmp(&vals[i], key) == Some(Ordering::Equal) {
                    return Some(i);
                }
            }
            match mode {
                1 => ge,
                -1 => {
                    let gt = bsearch_last(vals, key, |o| o == Ordering::Greater);
                    next_comparable(vals, key, gt)
                }
                _ => None,
            }
        }
        _ => {
            let order: Vec<usize> = if search == -1 { (0..vals.len()).rev().collect() } else { (0..vals.len()).collect() };
            let wildcard = mode == 2;
            if let Some(&i) = order.iter().find(|&&i| lk_eq(&vals[i], key, wildcard)) {
                return Some(i);
            }
            if mode != -1 && mode != 1 {
                return None;
            }
            let mut best: Option<usize> = None;
            for &i in &order {
                let Some(o) = lk_cmp(&vals[i], key) else { continue };
                let ok = if mode == -1 { o == Ordering::Less } else { o == Ordering::Greater };
                if !ok {
                    continue;
                }
                best = match best {
                    None => Some(i),
                    Some(b) => {
                        let ob = lk_cmp(&vals[i], &vals[b]).unwrap_or(Ordering::Equal);
                        let better = if mode == -1 { ob == Ordering::Greater } else { ob == Ordering::Less };
                        if better {
                            Some(i)
                        } else {
                            Some(b)
                        }
                    }
                };
            }
            best
        }
    }
}

/// Evaluate an argument to a value or (single-area) reference, returning the
/// array of values and the logical (unclipped) dimensions.
fn array_with_dims(ctx: &mut Ctx, e: &Expr) -> Result<(Array, usize, usize, Option<Area>), ErrorKind> {
    match ctx.eval(e) {
        Val::Refs(a) if a.len() == 1 => {
            let area = a[0];
            let arr = ctx.area_array(area);
            Ok((arr, area.rect.rows() as usize, area.rect.cols() as usize, Some(area)))
        }
        Val::Refs(_) => err(ErrorKind::Value),
        Val::V(Value::Error(e)) => Err(e),
        Val::V(Value::Array(a)) => {
            let (r, c) = (a.rows, a.cols);
            Ok((*a, r, c, None))
        }
        Val::V(v) => Ok((Array::new(1, 1, vec![v]), 1, 1, None)),
    }
}

/// Value ordering used by SORT/SORTBY: numbers < text < logicals < errors < blanks.
fn sort_cmp(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Text(x), Value::Text(y)) => compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Error(x), Value::Error(y)) => x.code().cmp(&y.code()),
        _ => a.type_rank().cmp(&b.type_rank()),
    }
}

/// Case-insensitive identity key for UNIQUE/GROUPBY.
fn vkey(v: &Value) -> String {
    match v {
        Value::Number(n) => format!("n{}", if *n == 0.0 { 0.0 } else { *n }),
        Value::Text(s) => format!("t{}", s.to_lowercase()),
        Value::Bool(b) => format!("b{}", b),
        Value::Error(e) => format!("e{}", e.as_str()),
        Value::Empty => "z".into(),
        Value::Array(_) => "a".into(),
    }
}

fn row_key(vals: &[Value]) -> String {
    vals.iter().map(vkey).collect::<Vec<_>>().join("\u{1}")
}

// ----- ADDRESS / AREAS / ROW / COLUMN -------------------------------------

fn address(args: &[Value]) -> Value {
    let row = tryv!(int(args, 0));
    let col = tryv!(int(args, 1));
    let abs = tryv!(num_or(args, 2, 1.0)).trunc() as i64;
    let a1 = tryv!(bool_or(args, 3, true));
    if row < 1 || row > MAX_ROWS as i64 || col < 1 || col > MAX_COLS as i64 || !(1..=4).contains(&abs) {
        return Value::Error(ErrorKind::Value);
    }
    let (row_abs, col_abs) = match abs {
        1 => (true, true),
        2 => (true, false),
        3 => (false, true),
        _ => (false, false),
    };
    let cell = if a1 {
        format!("{}{}{}{}", if col_abs { "$" } else { "" }, col_to_letters(col as u32 - 1), if row_abs { "$" } else { "" }, row)
    } else {
        let r = if row_abs { format!("R{}", row) } else { format!("R[{}]", row) };
        let c = if col_abs { format!("C{}", col) } else { format!("C[{}]", col) };
        format!("{}{}", r, c)
    };
    match args.get(4) {
        None | Some(Value::Empty) => Value::Text(cell),
        Some(v) => {
            let s = tryv!(v.to_text());
            if s.is_empty() {
                Value::Text(format!("!{}", cell))
            } else {
                Value::Text(format!("{}!{}", quote_sheet_name(&s), cell))
            }
        }
    }
}

fn areas(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match ctx.eval(&args[0]) {
        Val::Refs(a) => Value::Number(a.len() as f64),
        Val::V(Value::Error(e)) => Value::Error(e),
        Val::V(_) => Value::Error(ErrorKind::Value),
    }
}

fn row_or_col(ctx: &mut Ctx, args: &[Expr], is_row: bool) -> Value {
    if is_missing(args, 0) {
        return Value::Number(if is_row { ctx.pos.row } else { ctx.pos.col } as f64 + 1.0);
    }
    let area = match ctx.eval(&args[0]) {
        Val::Refs(a) if !a.is_empty() => a[0],
        Val::Refs(_) => return Value::Error(ErrorKind::Value),
        Val::V(Value::Error(e)) => return Value::Error(e),
        Val::V(_) => return Value::Error(ErrorKind::Value),
    };
    let r = if (is_row && area.rect.rows() > 1) || (!is_row && area.rect.cols() > 1) { ctx.clip_area(area) } else { area.rect };
    let (a, b) = if is_row { (r.top, r.bottom) } else { (r.left, r.right) };
    if a == b {
        return Value::Number(a as f64 + 1.0);
    }
    let vals: Vec<Value> = (a..=b).map(|i| Value::Number(i as f64 + 1.0)).collect();
    Value::from(if is_row { Array::column(vals) } else { Array::row(vals) })
}

fn row(ctx: &mut Ctx, args: &[Expr]) -> Value {
    row_or_col(ctx, args, true)
}

fn column(ctx: &mut Ctx, args: &[Expr]) -> Value {
    row_or_col(ctx, args, false)
}

fn dims(ctx: &mut Ctx, e: &Expr) -> Result<(usize, usize), ErrorKind> {
    match ctx.eval(e) {
        Val::Refs(a) if a.len() == 1 => Ok((a[0].rect.rows() as usize, a[0].rect.cols() as usize)),
        Val::Refs(_) => Err(ErrorKind::Ref),
        Val::V(Value::Error(e)) => Err(e),
        Val::V(Value::Array(a)) => Ok((a.rows, a.cols)),
        Val::V(_) => Ok((1, 1)),
    }
}

fn rows(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::Number(tryv!(dims(ctx, &args[0])).0 as f64)
}

fn columns(ctx: &mut Ctx, args: &[Expr]) -> Value {
    Value::Number(tryv!(dims(ctx, &args[0])).1 as f64)
}

// ----- CHOOSECOLS / CHOOSEROWS / DROP / TAKE / EXPAND -------------------

fn choose_indices(ctx: &mut Ctx, args: &[Expr], n: usize) -> Result<Vec<usize>, ErrorKind> {
    let mut out = Vec::new();
    for e in args {
        let v = ctx.eval_value(e);
        let vals = match v {
            Value::Array(a) => a.data,
            v => vec![v],
        };
        for v in vals {
            let k = v.to_number()?.trunc() as i64;
            let idx = if k > 0 { k - 1 } else { n as i64 + k };
            if k == 0 || idx < 0 || idx >= n as i64 {
                return Err(ErrorKind::Value);
            }
            out.push(idx as usize);
        }
    }
    Ok(out)
}

fn choosecols(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let idx = tryv!(choose_indices(ctx, &args[1..], a.cols));
    let mut data = Vec::with_capacity(a.rows * idx.len());
    for r in 0..a.rows {
        for &c in &idx {
            data.push(a.get(r, c).clone());
        }
    }
    to_value(Array::new(a.rows, idx.len(), data))
}

fn chooserows(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let idx = tryv!(choose_indices(ctx, &args[1..], a.rows));
    let mut data = Vec::with_capacity(a.cols * idx.len());
    for &r in &idx {
        data.extend_from_slice(a.row_values(r));
    }
    to_value(Array::new(idx.len(), a.cols, data))
}

/// Range kept by TAKE (`take`) or DROP along one dimension of size `n`.
fn keep_range(k: Option<i64>, n: usize, take: bool) -> Result<(usize, usize), ErrorKind> {
    let Some(k) = k else { return Ok((0, n)) };
    let m = (k.unsigned_abs() as usize).min(n);
    let r = if take {
        if k == 0 {
            return Err(ErrorKind::Calc);
        }
        if k > 0 {
            (0, m)
        } else {
            (n - m, n)
        }
    } else if k >= 0 {
        (m, n)
    } else {
        (0, n - m)
    };
    if r.0 >= r.1 {
        return Err(ErrorKind::Calc);
    }
    Ok(r)
}

fn take_drop(ctx: &mut Ctx, args: &[Expr], take: bool) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let rk = tryv!(opt_i64(ctx, args, 1));
    let ck = tryv!(opt_i64(ctx, args, 2));
    let (r0, r1) = tryv!(keep_range(rk, a.rows, take));
    let (c0, c1) = tryv!(keep_range(ck, a.cols, take));
    to_value(sub(&a, r0, r1, c0, c1))
}

fn take(ctx: &mut Ctx, args: &[Expr]) -> Value {
    take_drop(ctx, args, true)
}

fn drop(ctx: &mut Ctx, args: &[Expr]) -> Value {
    take_drop(ctx, args, false)
}

fn pad_arg(ctx: &mut Ctx, args: &[Expr], i: usize) -> Value {
    if is_missing(args, i) {
        Value::Error(ErrorKind::NA)
    } else {
        ctx.eval_scalar(&args[i])
    }
}

fn expand(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let rows = tryv!(opt_i64(ctx, args, 1)).unwrap_or(a.rows as i64);
    let cols = tryv!(opt_i64(ctx, args, 2)).unwrap_or(a.cols as i64);
    let pad = pad_arg(ctx, args, 3);
    if rows < a.rows as i64 || cols < a.cols as i64 {
        return Value::Error(ErrorKind::Value);
    }
    if rows > MAX_ROWS as i64 || cols > MAX_COLS as i64 {
        return Value::Error(ErrorKind::Num);
    }
    let (rows, cols) = (rows as usize, cols as usize);
    let mut out = Array::filled(rows, cols, pad);
    for r in 0..a.rows {
        for c in 0..a.cols {
            *out.get_mut(r, c) = a.get(r, c).clone();
        }
    }
    to_value(out)
}

// ----- FILTER / SORT / SORTBY / UNIQUE ------------------------------------

fn filter(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let inc = tryv!(ctx.eval_array(&args[1]));
    let by_rows = if inc.cols == 1 && inc.rows == a.rows {
        true
    } else if inc.rows == 1 && inc.cols == a.cols {
        false
    } else {
        return Value::Error(ErrorKind::Value);
    };
    let mut keep = Vec::with_capacity(inc.data.len());
    for v in inc.data.iter() {
        match v {
            Value::Error(e) => return Value::Error(*e),
            Value::Text(_) => match v.to_bool() {
                Ok(b) => keep.push(b),
                Err(_) => return Value::Error(ErrorKind::Value),
            },
            v => keep.push(v.to_bool().unwrap_or(false)),
        }
    }
    let n = keep.iter().filter(|b| **b).count();
    if n == 0 {
        return if is_missing(args, 2) { Value::Error(ErrorKind::Calc) } else { ctx.eval_value(&args[2]) };
    }
    if by_rows {
        let mut data = Vec::with_capacity(n * a.cols);
        for (r, k) in keep.iter().enumerate() {
            if *k {
                data.extend_from_slice(a.row_values(r));
            }
        }
        to_value(Array::new(n, a.cols, data))
    } else {
        let mut data = Vec::with_capacity(n * a.rows);
        for r in 0..a.rows {
            for (c, k) in keep.iter().enumerate() {
                if *k {
                    data.push(a.get(r, c).clone());
                }
            }
        }
        to_value(Array::new(a.rows, n, data))
    }
}

/// Sort the rows of `a` by the given keys (one value per row, ascending flag).
fn sort_rows(a: &Array, keys: &[(Vec<Value>, bool)]) -> Array {
    let mut idx: Vec<usize> = (0..a.rows).collect();
    idx.sort_by(|&x, &y| {
        for (k, asc) in keys {
            let o = sort_cmp(&k[x], &k[y]);
            let o = if *asc { o } else { o.reverse() };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    let mut data = Vec::with_capacity(a.data.len());
    for r in idx {
        data.extend_from_slice(a.row_values(r));
    }
    Array::new(a.rows, a.cols, data)
}

fn num_list(ctx: &mut Ctx, args: &[Expr], i: usize, default: f64) -> Result<Vec<i64>, ErrorKind> {
    if is_missing(args, i) {
        return Ok(vec![default as i64]);
    }
    match ctx.eval_value(&args[i]) {
        Value::Array(a) => a.data.iter().map(|v| v.to_number().map(|n| n.trunc() as i64)).collect(),
        v => Ok(vec![v.to_number()?.trunc() as i64]),
    }
}

fn sort(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let idx = tryv!(num_list(ctx, args, 1, 1.0));
    let ord = tryv!(num_list(ctx, args, 2, 1.0));
    let by_col = tryv!(ctx.opt_bool(args, 3, false));
    let work = if by_col { a.transpose() } else { a };
    if ord.len() != 1 && ord.len() != idx.len() {
        return Value::Error(ErrorKind::Value);
    }
    let mut keys = Vec::with_capacity(idx.len());
    for (i, &k) in idx.iter().enumerate() {
        let o = if ord.len() == 1 { ord[0] } else { ord[i] };
        if k < 1 || k as usize > work.cols || (o != 1 && o != -1) {
            return Value::Error(ErrorKind::Value);
        }
        let col: Vec<Value> = (0..work.rows).map(|r| work.get(r, k as usize - 1).clone()).collect();
        keys.push((col, o == 1));
    }
    let s = sort_rows(&work, &keys);
    to_value(if by_col { s.transpose() } else { s })
}

fn sortby(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let mut keys = Vec::new();
    let mut by_rows: Option<bool> = None;
    let mut i = 1;
    while i < args.len() {
        let by = tryv!(ctx.eval_array(&args[i]));
        let o = tryv!(ctx.opt_number(args, i + 1, 1.0)).trunc() as i64;
        if o != 1 && o != -1 {
            return Value::Error(ErrorKind::Value);
        }
        let rows_dir = if by.cols == 1 && by.rows == a.rows {
            true
        } else if by.rows == 1 && by.cols == a.cols {
            false
        } else {
            return Value::Error(ErrorKind::Value);
        };
        match by_rows {
            Some(d) if d != rows_dir => return Value::Error(ErrorKind::Value),
            _ => by_rows = Some(rows_dir),
        }
        keys.push((by.data, o == 1));
        i += 2;
    }
    if by_rows.unwrap_or(true) {
        to_value(sort_rows(&a, &keys))
    } else {
        to_value(sort_rows(&a.transpose(), &keys).transpose())
    }
}

fn unique(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let by_col = tryv!(ctx.opt_bool(args, 1, false));
    let once = tryv!(ctx.opt_bool(args, 2, false));
    let work = if by_col { a.transpose() } else { a };
    let mut order: Vec<usize> = Vec::new();
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for r in 0..work.rows {
        let k = row_key(work.row_values(r));
        let e = counts.entry(k).or_insert_with(|| {
            order.push(r);
            (r, 0)
        });
        e.1 += 1;
    }
    let mut data = Vec::new();
    let mut n = 0;
    for &r in &order {
        let k = row_key(work.row_values(r));
        if once && counts[&k].1 != 1 {
            continue;
        }
        data.extend_from_slice(work.row_values(r));
        n += 1;
    }
    if n == 0 {
        return Value::Error(ErrorKind::Calc);
    }
    let out = Array::new(n, work.cols, data);
    to_value(if by_col { out.transpose() } else { out })
}

// ----- stacking and reshaping ---------------------------------------------

fn stack(ctx: &mut Ctx, args: &[Expr], horizontal: bool) -> Value {
    let mut arrays = Vec::with_capacity(args.len());
    for e in args {
        let a = tryv!(ctx.eval_array(e));
        arrays.push(if horizontal { a.transpose() } else { a });
    }
    let cols = arrays.iter().map(|a| a.cols).max().unwrap_or(0);
    let rows: usize = arrays.iter().map(|a| a.rows).sum();
    let mut data = Vec::with_capacity(rows * cols);
    for a in &arrays {
        for r in 0..a.rows {
            for c in 0..cols {
                data.push(if c < a.cols { a.get(r, c).clone() } else { Value::Error(ErrorKind::NA) });
            }
        }
    }
    let out = Array::new(rows, cols, data);
    to_value(if horizontal { out.transpose() } else { out })
}

fn hstack(ctx: &mut Ctx, args: &[Expr]) -> Value {
    stack(ctx, args, true)
}

fn vstack(ctx: &mut Ctx, args: &[Expr]) -> Value {
    stack(ctx, args, false)
}

fn to_line(ctx: &mut Ctx, args: &[Expr], as_col: bool) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let ignore = tryv!(ctx.opt_number(args, 1, 0.0)).trunc() as i64;
    let by_col = tryv!(ctx.opt_bool(args, 2, false));
    if !(0..=3).contains(&ignore) {
        return Value::Error(ErrorKind::Value);
    }
    let src = if by_col { a.transpose() } else { a };
    let vals: Vec<Value> = src
        .data
        .into_iter()
        .filter(|v| !((ignore & 1 != 0 && v.is_empty()) || (ignore & 2 != 0 && v.is_error())))
        .collect();
    if vals.is_empty() {
        return Value::Error(ErrorKind::Calc);
    }
    to_value(if as_col { Array::column(vals) } else { Array::row(vals) })
}

fn tocol(ctx: &mut Ctx, args: &[Expr]) -> Value {
    to_line(ctx, args, true)
}

fn torow(ctx: &mut Ctx, args: &[Expr]) -> Value {
    to_line(ctx, args, false)
}

fn transpose(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    to_value(a.transpose())
}

fn wrap(ctx: &mut Ctx, args: &[Expr], by_rows: bool) -> Value {
    let a = tryv!(ctx.eval_array(&args[0]));
    let Some(v) = vector(&a) else { return Value::Error(ErrorKind::Value) };
    let wc = tryv!(ctx.eval_number(&args[1])).trunc();
    if wc < 1.0 {
        return Value::Error(ErrorKind::Num);
    }
    let pad = pad_arg(ctx, args, 2);
    let wc = (wc as usize).min(v.len().max(1));
    let lines = v.len().div_ceil(wc);
    // Build as rows of `wc` values, then transpose for WRAPCOLS.
    let mut data = Vec::with_capacity(lines * wc);
    for i in 0..lines * wc {
        data.push(v.get(i).cloned().unwrap_or_else(|| pad.clone()));
    }
    let out = Array::new(lines, wc, data);
    to_value(if by_rows { out } else { out.transpose() })
}

fn wrapcols(ctx: &mut Ctx, args: &[Expr]) -> Value {
    wrap(ctx, args, false)
}

fn wraprows(ctx: &mut Ctx, args: &[Expr]) -> Value {
    wrap(ctx, args, true)
}

// ----- misc ----------------------------------------------------------------

fn formulatext(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let area = tryv!(ctx.eval_area(&args[0]));
    match ctx.wb.cell(area.sheet, area.rect.top_left()).and_then(|c| c.formula.as_ref()) {
        Some(f) => Value::Text(format!("={}", f.to_formula())),
        None => Value::Error(ErrorKind::NA),
    }
}

fn getpivotdata(_ctx: &mut Ctx, _args: &[Expr]) -> Value {
    Value::Error(ErrorKind::Ref)
}

fn rtd(_ctx: &mut Ctx, _args: &[Expr]) -> Value {
    Value::Error(ErrorKind::NA)
}

fn hyperlink(args: &[Value]) -> Value {
    match args.get(1) {
        Some(v) if !v.is_empty() => v.clone(),
        _ => args[0].clone(),
    }
}

// ----- VLOOKUP / HLOOKUP / LOOKUP / MATCH ---------------------------------

fn vh_lookup(ctx: &mut Ctx, args: &[Expr], vertical: bool) -> Value {
    let key = ctx.eval_value(&args[0]);
    let table = tryv!(ctx.eval_array(&args[1]));
    let idx = ctx.eval_value(&args[2]);
    let approx = tryv!(ctx.opt_bool(args, 3, true));
    let t = if vertical { table } else { table.transpose() };
    let first: Vec<Value> = (0..t.rows).map(|r| t.get(r, 0).clone()).collect();
    broadcast2(&key, &idx, |k, i| {
        if let Value::Error(e) = k {
            return Value::Error(*e);
        }
        let i = match i.to_number() {
            Ok(n) => n.trunc() as i64,
            Err(e) => return Value::Error(e),
        };
        if i < 1 {
            return Value::Error(ErrorKind::Value);
        }
        if i as usize > t.cols {
            return Value::Error(ErrorKind::Ref);
        }
        match legacy_search(&first, k, if approx { 1 } else { 0 }) {
            Some(r) => t.get(r, i as usize - 1).clone(),
            None => Value::Error(ErrorKind::NA),
        }
    })
}

fn vlookup(ctx: &mut Ctx, args: &[Expr]) -> Value {
    vh_lookup(ctx, args, true)
}

fn hlookup(ctx: &mut Ctx, args: &[Expr]) -> Value {
    vh_lookup(ctx, args, false)
}

fn lookup_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let key = ctx.eval_value(&args[0]);
    let a = tryv!(ctx.eval_array(&args[1]));
    let (search, result): (Vec<Value>, Vec<Value>) = if args.len() == 3 {
        let r = tryv!(ctx.eval_array(&args[2]));
        let Some(s) = vector(&a) else { return Value::Error(ErrorKind::NA) };
        (s, r.data)
    } else if a.cols > a.rows {
        (a.row_values(0).to_vec(), a.row_values(a.rows - 1).to_vec())
    } else {
        ((0..a.rows).map(|r| a.get(r, 0).clone()).collect(), (0..a.rows).map(|r| a.get(r, a.cols - 1).clone()).collect())
    };
    map_key(&key, |k| {
        if let Value::Error(e) = k {
            return Value::Error(*e);
        }
        match legacy_search(&search, k, 1) {
            Some(i) => result.get(i).cloned().unwrap_or(Value::Error(ErrorKind::NA)),
            None => Value::Error(ErrorKind::NA),
        }
    })
}

fn match_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let key = ctx.eval_value(&args[0]);
    let a = tryv!(ctx.eval_array(&args[1]));
    let mt = tryv!(ctx.opt_number(args, 2, 1.0));
    let mode = if mt > 0.0 {
        1
    } else if mt < 0.0 {
        -1
    } else {
        0
    };
    let Some(vals) = vector(&a) else { return Value::Error(ErrorKind::NA) };
    map_key(&key, |k| {
        if let Value::Error(e) = k {
            return Value::Error(*e);
        }
        match legacy_search(&vals, k, mode) {
            Some(i) => Value::Number(i as f64 + 1.0),
            None => Value::Error(ErrorKind::NA),
        }
    })
}

// ----- XLOOKUP / XMATCH -----------------------------------------------------

fn x_modes(ctx: &mut Ctx, args: &[Expr], mi: usize) -> Result<(i64, i64), ErrorKind> {
    let mode = ctx.opt_number(args, mi, 0.0)?.trunc() as i64;
    let search = ctx.opt_number(args, mi + 1, 1.0)?.trunc() as i64;
    if !(-1..=2).contains(&mode) || ![1, -1, 2, -2].contains(&search) {
        return Err(ErrorKind::Value);
    }
    Ok((mode, search))
}

fn xmatch(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let key = ctx.eval_value(&args[0]);
    let a = tryv!(ctx.eval_array(&args[1]));
    let (mode, search) = tryv!(x_modes(ctx, args, 2));
    let Some(vals) = vector(&a) else { return Value::Error(ErrorKind::Value) };
    map_key(&key, |k| {
        if let Value::Error(e) = k {
            return Value::Error(*e);
        }
        match xsearch(&vals, k, mode, search) {
            Some(i) => Value::Number(i as f64 + 1.0),
            None => Value::Error(ErrorKind::NA),
        }
    })
}

fn xlookup(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let key = ctx.eval_value(&args[0]);
    let (la, lrows, lcols, _) = match array_with_dims(ctx, &args[1]) {
        Ok(x) => x,
        Err(e) => return verr(e),
    };
    if lrows != 1 && lcols != 1 {
        return verr(ErrorKind::Value);
    }
    let (ra, rrows, rcols, rarea) = match array_with_dims(ctx, &args[2]) {
        Ok(x) => x,
        Err(e) => return verr(e),
    };
    let (mode, search) = match x_modes(ctx, args, 4) {
        Ok(x) => x,
        Err(e) => return verr(e),
    };
    let n = lrows.max(lcols);
    let vertical = if lcols == 1 && rrows == n {
        true
    } else if lrows == 1 && rcols == n {
        false
    } else {
        return verr(ErrorKind::Value);
    };
    let vals = la.data;
    // Result for a found index: a reference slice or an array slice.
    let result_at = |i: usize| -> Val {
        if let Some(area) = rarea {
            let r = area.rect;
            let rect = if vertical {
                Rect { top: r.top + i as u32, bottom: r.top + i as u32, left: r.left, right: r.right }
            } else {
                Rect { top: r.top, bottom: r.bottom, left: r.left + i as u32, right: r.left + i as u32 }
            };
            Val::Refs(vec![Area { sheet: area.sheet, rect }])
        } else if vertical {
            if i < ra.rows {
                Val::V(to_value(sub(&ra, i, i + 1, 0, ra.cols)))
            } else {
                Val::V(Value::Empty)
            }
        } else if i < ra.cols {
            Val::V(to_value(sub(&ra, 0, ra.rows, i, i + 1)))
        } else {
            Val::V(Value::Empty)
        }
    };
    match key {
        Value::Error(e) => verr(e),
        Value::Array(keys) => {
            let mut data = Vec::with_capacity(keys.data.len());
            let mut fallback: Option<Value> = None;
            for k in keys.data.iter() {
                let v = match xsearch(&vals, k, mode, search) {
                    Some(i) => {
                        let r = result_at(i);
                        ctx.deref(r)
                    }
                    None => {
                        if is_missing(args, 3) {
                            Value::Error(ErrorKind::NA)
                        } else {
                            if fallback.is_none() {
                                fallback = Some(ctx.eval_value(&args[3]));
                            }
                            fallback.clone().unwrap()
                        }
                    }
                };
                data.push(match v {
                    Value::Array(a) => a.data.into_iter().next().unwrap_or(Value::Empty),
                    v => v,
                });
            }
            Val::V(Value::from(Array::new(keys.rows, keys.cols, data)))
        }
        k => match xsearch(&vals, &k, mode, search) {
            Some(i) => result_at(i),
            None => {
                if is_missing(args, 3) {
                    verr(ErrorKind::NA)
                } else {
                    ctx.eval(&args[3])
                }
            }
        },
    }
}

// ----- INDEX / OFFSET / INDIRECT / TRIMRANGE -------------------------------

/// Resolve INDEX row/column numbers against an area of `nrows` x `ncols`.
/// Returns zero-based inclusive row and column bounds.
fn index_bounds(nrows: usize, ncols: usize, row: Option<i64>, col: Option<i64>) -> Result<(usize, usize, usize, usize), ErrorKind> {
    let (r, c) = match (row, col) {
        (Some(r), None) if nrows == 1 && ncols > 1 => (1, r),
        (r, c) => (r.unwrap_or(if nrows == 1 { 1 } else { 0 }), c.unwrap_or(if ncols == 1 { 1 } else { 0 })),
    };
    if r < 0 || c < 0 {
        return Err(ErrorKind::Value);
    }
    if r as usize > nrows || c as usize > ncols {
        return Err(ErrorKind::Ref);
    }
    let (r0, r1) = if r == 0 { (0, nrows - 1) } else { (r as usize - 1, r as usize - 1) };
    let (c0, c1) = if c == 0 { (0, ncols - 1) } else { (c as usize - 1, c as usize - 1) };
    Ok((r0, r1, c0, c1))
}

fn index(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let first = ctx.eval(&args[0]);
    if let Val::V(Value::Error(e)) = first {
        return verr(e);
    }
    let rv = if is_missing(args, 1) { None } else { Some(ctx.eval_value(&args[1])) };
    let cv = if is_missing(args, 2) { None } else { Some(ctx.eval_value(&args[2])) };
    let area_num = match ctx.opt_number(args, 3, 1.0) {
        Ok(n) => n.trunc() as i64,
        Err(e) => return verr(e),
    };
    let as_int = |v: &Option<Value>| -> Result<Option<i64>, ErrorKind> {
        match v {
            None => Ok(None),
            Some(v) => Ok(Some(v.to_number()?.trunc() as i64)),
        }
    };
    let is_arr = |v: &Option<Value>| matches!(v, Some(Value::Array(_)));
    // Array-valued row/column numbers: return the selected values element-wise.
    if is_arr(&rv) || is_arr(&cv) {
        let arr = match first {
            Val::Refs(a) => {
                if area_num < 1 || area_num as usize > a.len() {
                    return verr(ErrorKind::Ref);
                }
                ctx.area_array(a[area_num as usize - 1])
            }
            Val::V(Value::Array(a)) => *a,
            Val::V(v) => Array::new(1, 1, vec![v]),
        };
        let rr = rv.clone().unwrap_or(Value::Empty);
        let cc = cv.clone().unwrap_or(Value::Empty);
        let (hr, hc) = (rv.is_some(), cv.is_some());
        return Val::V(broadcast2(&rr, &cc, |r, c| {
            let r = if hr { Some(r) } else { None };
            let c = if hc { Some(c) } else { None };
            let ri = match r.map(|v| v.to_number()).transpose() {
                Ok(x) => x.map(|n| n.trunc() as i64),
                Err(e) => return Value::Error(e),
            };
            let ci = match c.map(|v| v.to_number()).transpose() {
                Ok(x) => x.map(|n| n.trunc() as i64),
                Err(e) => return Value::Error(e),
            };
            match index_bounds(arr.rows, arr.cols, ri, ci) {
                Ok((r0, _, c0, _)) => arr.get(r0, c0).clone(),
                Err(e) => Value::Error(e),
            }
        }));
    }
    let (ri, ci) = match (as_int(&rv), as_int(&cv)) {
        (Ok(r), Ok(c)) => (r, c),
        (Err(e), _) | (_, Err(e)) => return verr(e),
    };
    match first {
        Val::Refs(a) => {
            if area_num < 1 || area_num as usize > a.len() {
                return verr(ErrorKind::Ref);
            }
            let area = a[area_num as usize - 1];
            let rect = area.rect;
            match index_bounds(rect.rows() as usize, rect.cols() as usize, ri, ci) {
                Ok((r0, r1, c0, c1)) => Val::Refs(vec![Area {
                    sheet: area.sheet,
                    rect: Rect {
                        top: rect.top + r0 as u32,
                        bottom: rect.top + r1 as u32,
                        left: rect.left + c0 as u32,
                        right: rect.left + c1 as u32,
                    },
                }]),
                Err(e) => verr(e),
            }
        }
        Val::V(v) => {
            if area_num != 1 {
                return verr(ErrorKind::Ref);
            }
            let arr = match v {
                Value::Array(a) => *a,
                v => Array::new(1, 1, vec![v]),
            };
            match index_bounds(arr.rows, arr.cols, ri, ci) {
                Ok((r0, r1, c0, c1)) => Val::V(to_value(sub(&arr, r0, r1 + 1, c0, c1 + 1))),
                Err(e) => verr(e),
            }
        }
    }
}

fn offset(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let base = match ctx.eval(&args[0]) {
        Val::Refs(a) if a.len() == 1 => a[0],
        Val::V(Value::Error(e)) => return verr(e),
        _ => return verr(ErrorKind::Value),
    };
    let geti = |ctx: &mut Ctx, i: usize, d: i64| -> Result<i64, ErrorKind> { Ok(ctx.opt_number(args, i, d as f64)?.trunc() as i64) };
    let (dr, dc, h, w) = match (|| -> Result<_, ErrorKind> {
        Ok((geti(ctx, 1, 0)?, geti(ctx, 2, 0)?, geti(ctx, 3, base.rect.rows() as i64)?, geti(ctx, 4, base.rect.cols() as i64)?))
    })() {
        Ok(x) => x,
        Err(e) => return verr(e),
    };
    if h == 0 || w == 0 {
        return verr(ErrorKind::Ref);
    }
    let top = base.rect.top as i64 + dr;
    let left = base.rect.left as i64 + dc;
    let (t, b) = if h > 0 { (top, top + h - 1) } else { (top + h + 1, top) };
    let (l, r) = if w > 0 { (left, left + w - 1) } else { (left + w + 1, left) };
    if t < 0 || l < 0 || b >= MAX_ROWS as i64 || r >= MAX_COLS as i64 {
        return verr(ErrorKind::Ref);
    }
    Val::Refs(vec![Area { sheet: base.sheet, rect: Rect { top: t as u32, left: l as u32, bottom: b as u32, right: r as u32 } }])
}

/// Is this parsed expression purely a reference (cells, names, range operators)?
fn is_ref_expr(e: &Expr) -> bool {
    match e {
        Expr::Ref(_) | Expr::SpillRef(_) | Expr::Name(_) => true,
        Expr::Paren(i) => is_ref_expr(i),
        Expr::Binary(BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersect, a, b) => is_ref_expr(a) && is_ref_expr(b),
        _ => false,
    }
}

/// One end of an R1C1 reference: (row, col), `None` meaning the whole row/column.
fn r1c1_part(s: &str, base: CellPos) -> Option<(Option<u32>, Option<u32>)> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    // Parse a coordinate after R or C: absolute N, relative [N], or bare (current).
    let coord = |i: &mut usize, cur: u32, max: u32| -> Option<u32> {
        let v: i64 = if *i < chars.len() && chars[*i] == '[' {
            let end = chars[*i..].iter().position(|&c| c == ']')? + *i;
            let n: i64 = chars[*i + 1..end].iter().collect::<String>().parse().ok()?;
            *i = end + 1;
            cur as i64 + n
        } else {
            let start = *i;
            while *i < chars.len() && chars[*i].is_ascii_digit() {
                *i += 1;
            }
            if start == *i {
                cur as i64
            } else {
                chars[start..*i].iter().collect::<String>().parse::<i64>().ok()? - 1
            }
        };
        if v < 0 || v >= max as i64 {
            None
        } else {
            Some(v as u32)
        }
    };
    let mut row = None;
    let mut col = None;
    if i < chars.len() && chars[i].eq_ignore_ascii_case(&'R') {
        i += 1;
        row = Some(coord(&mut i, base.row, MAX_ROWS)?);
    }
    if i < chars.len() && chars[i].eq_ignore_ascii_case(&'C') {
        i += 1;
        col = Some(coord(&mut i, base.col, MAX_COLS)?);
    }
    if i != chars.len() || (row.is_none() && col.is_none()) {
        return None;
    }
    Some((row, col))
}

fn parse_r1c1(text: &str, base: CellPos) -> Option<(Option<String>, Rect)> {
    let (sheet, rest) = match text.rfind('!') {
        Some(i) => {
            let s = &text[..i];
            let s = if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') { s[1..s.len() - 1].replace("''", "'") } else { s.to_string() };
            (Some(s), &text[i + 1..])
        }
        None => (None, text),
    };
    let mut parts = rest.split(':');
    let a = r1c1_part(parts.next()?.trim(), base)?;
    let b = match parts.next() {
        Some(p) => r1c1_part(p.trim(), base)?,
        None => a,
    };
    if parts.next().is_some() {
        return None;
    }
    let rows = match (a.0, b.0) {
        (Some(x), Some(y)) => (x.min(y), x.max(y)),
        _ => (0, MAX_ROWS - 1),
    };
    let cols = match (a.1, b.1) {
        (Some(x), Some(y)) => (x.min(y), x.max(y)),
        _ => (0, MAX_COLS - 1),
    };
    Some((sheet, Rect { top: rows.0, bottom: rows.1, left: cols.0, right: cols.1 }))
}

fn indirect(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let text = match ctx.eval_text(&args[0]) {
        Ok(t) => t,
        Err(e) => return verr(e),
    };
    let a1 = match ctx.opt_bool(args, 1, true) {
        Ok(b) => b,
        Err(e) => return verr(e),
    };
    let text = text.trim();
    if text.is_empty() {
        return verr(ErrorKind::Ref);
    }
    if !a1 {
        let Some((sheet, rect)) = parse_r1c1(text, ctx.pos) else { return verr(ErrorKind::Ref) };
        let sheet = match sheet {
            None => ctx.sheet,
            Some(n) => match ctx.wb.sheet_index(&n) {
                Some(i) => i,
                None => return verr(ErrorKind::Ref),
            },
        };
        return Val::Refs(vec![Area { sheet, rect }]);
    }
    let Ok(e) = parse_formula(text) else { return verr(ErrorKind::Ref) };
    if !is_ref_expr(&e) {
        return verr(ErrorKind::Ref);
    }
    match ctx.eval(&e) {
        Val::Refs(a) => Val::Refs(a),
        _ => verr(ErrorKind::Ref),
    }
}

/// Bounds kept after trimming empty leading/trailing lines (`mode` 0..3).
fn trim_bounds(nonempty: &[bool], mode: i64) -> (usize, usize) {
    let n = nonempty.len();
    let first = nonempty.iter().position(|b| *b);
    let last = nonempty.iter().rposition(|b| *b);
    let (f, l) = match (first, last) {
        (Some(f), Some(l)) => (f, l + 1),
        _ => (0, 0),
    };
    let start = if mode & 1 != 0 { f } else { 0 };
    let end = if mode & 2 != 0 { l } else { n };
    (start, end.max(start))
}

fn trimrange(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let (tr, tc) = match (ctx.opt_number(args, 1, 3.0), ctx.opt_number(args, 2, 3.0)) {
        (Ok(a), Ok(b)) => (a.trunc() as i64, b.trunc() as i64),
        (Err(e), _) | (_, Err(e)) => return verr(e),
    };
    if !(0..=3).contains(&tr) || !(0..=3).contains(&tc) {
        return verr(ErrorKind::Value);
    }
    let v = ctx.eval(&args[0]);
    match v {
        Val::Refs(a) if a.len() == 1 => {
            let area = a[0];
            let r = ctx.clip_area(area);
            let clipped = Area { sheet: area.sheet, rect: r };
            let arr = ctx.area_array(clipped);
            let rows_ne: Vec<bool> = (0..arr.rows).map(|i| arr.row_values(i).iter().any(|v| !v.is_empty())).collect();
            let cols_ne: Vec<bool> = (0..arr.cols).map(|c| (0..arr.rows).any(|i| !arr.get(i, c).is_empty())).collect();
            let (r0, r1) = trim_bounds(&rows_ne, tr);
            let (c0, c1) = trim_bounds(&cols_ne, tc);
            if r0 >= r1 || c0 >= c1 {
                return Val::Refs(vec![Area { sheet: area.sheet, rect: Rect::single(r.top_left()) }]);
            }
            Val::Refs(vec![Area {
                sheet: area.sheet,
                rect: Rect { top: r.top + r0 as u32, bottom: r.top + r1 as u32 - 1, left: r.left + c0 as u32, right: r.left + c1 as u32 - 1 },
            }])
        }
        Val::Refs(_) => verr(ErrorKind::Value),
        v => {
            let arr = match ctx.deref(v) {
                Value::Array(a) => *a,
                Value::Error(e) => return verr(e),
                v => return Val::V(v),
            };
            let rows_ne: Vec<bool> = (0..arr.rows).map(|i| arr.row_values(i).iter().any(|v| !v.is_empty())).collect();
            let cols_ne: Vec<bool> = (0..arr.cols).map(|c| (0..arr.rows).any(|i| !arr.get(i, c).is_empty())).collect();
            let (r0, r1) = trim_bounds(&rows_ne, tr);
            let (c0, c1) = trim_bounds(&cols_ne, tc);
            if r0 >= r1 || c0 >= c1 {
                return verr(ErrorKind::Calc);
            }
            Val::V(to_value(sub(&arr, r0, r1, c0, c1)))
        }
    }
}

// ----- GROUPBY / PIVOTBY (basic) -------------------------------------------

/// Should the first row be treated as headers (auto-detect: text over numbers)?
fn detect_headers(values: &Array, setting: Option<i64>) -> (bool, bool) {
    match setting {
        Some(0) => (false, false),
        Some(1) => (true, false),
        Some(2) => (false, false),
        Some(_) => (true, true),
        None => {
            let auto = values.rows > 1
                && values.row_values(0).iter().all(|v| matches!(v, Value::Text(_)))
                && values.row_values(1).iter().any(|v| matches!(v, Value::Number(_)));
            (auto, false)
        }
    }
}

/// Group row indices by key, sorted by key ascending.
fn group_rows(keys: &Array, rows: &[usize]) -> Vec<(Vec<Value>, Vec<usize>)> {
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<(Vec<Value>, Vec<usize>)> = Vec::new();
    for &r in rows {
        let k = row_key(keys.row_values(r));
        let gi = *map.entry(k).or_insert_with(|| {
            groups.push((keys.row_values(r).to_vec(), Vec::new()));
            groups.len() - 1
        });
        groups[gi].1.push(r);
    }
    groups.sort_by(|a, b| {
        for (x, y) in a.0.iter().zip(b.0.iter()) {
            let o = sort_cmp(x, y);
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    groups
}

fn aggregate(ctx: &mut Ctx, l: &crate::eval::Lambda, values: &Array, col: usize, rows: &[usize]) -> Value {
    let vals: Vec<Value> = rows.iter().map(|&r| values.get(r, col).clone()).collect();
    single(ctx.call_lambda(l, vec![Val::V(Value::from(Array::column(vals)))]))
}

fn filter_rows(ctx: &mut Ctx, args: &[Expr], i: usize, start: usize, n: usize) -> Result<Vec<usize>, ErrorKind> {
    if is_missing(args, i) {
        return Ok((start..n).collect());
    }
    let f = ctx.eval_array(&args[i])?;
    if f.data.len() != n {
        return Err(ErrorKind::Value);
    }
    let mut out = Vec::new();
    for r in start..n {
        if f.data[r].to_bool()? {
            out.push(r);
        }
    }
    Ok(out)
}

fn groupby(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let keys = tryv!(ctx.eval_array(&args[0]));
    let values = tryv!(ctx.eval_array(&args[1]));
    if keys.rows != values.rows {
        return Value::Error(ErrorKind::Value);
    }
    let Some(l) = resolve_fn(ctx, &args[2], 1) else { return Value::Error(ErrorKind::Value) };
    let hdr = tryv!(opt_i64(ctx, args, 3));
    let total = tryv!(opt_i64(ctx, args, 4)).unwrap_or(1);
    let sort_order = tryv!(opt_i64(ctx, args, 5)).unwrap_or(1);
    let (has_hdr, show_hdr) = detect_headers(&values, hdr);
    let start = usize::from(has_hdr);
    let rows = tryv!(filter_rows(ctx, args, 6, start, keys.rows));
    let groups = group_rows(&keys, &rows);
    let width = keys.cols + values.cols;
    let mut body: Vec<Vec<Value>> = Vec::new();
    for (k, rs) in &groups {
        let mut line = k.clone();
        for c in 0..values.cols {
            line.push(aggregate(ctx, &l, &values, c, rs));
        }
        body.push(line);
    }
    if sort_order != 0 && sort_order.unsigned_abs() as usize <= width {
        let c = sort_order.unsigned_abs() as usize - 1;
        let asc = sort_order > 0;
        body.sort_by(|a, b| {
            let o = sort_cmp(&a[c], &b[c]);
            if asc {
                o
            } else {
                o.reverse()
            }
        });
    }
    let mut out: Vec<Vec<Value>> = Vec::new();
    if show_hdr {
        let mut h = keys.row_values(0).to_vec();
        h.extend_from_slice(values.row_values(0));
        out.push(h);
    }
    let total_line = if total != 0 {
        let mut t = vec![Value::Text("Total".into())];
        t.resize(keys.cols, Value::Text(String::new()));
        for c in 0..values.cols {
            t.push(aggregate(ctx, &l, &values, c, &rows));
        }
        Some(t)
    } else {
        None
    };
    if total < 0 {
        out.extend(total_line.clone());
    }
    out.extend(body);
    if total > 0 {
        out.extend(total_line);
    }
    if out.is_empty() {
        return Value::Error(ErrorKind::Calc);
    }
    to_value(Array::from_rows(out))
}

fn pivotby(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let rkeys = tryv!(ctx.eval_array(&args[0]));
    let ckeys = tryv!(ctx.eval_array(&args[1]));
    let values = tryv!(ctx.eval_array(&args[2]));
    if rkeys.rows != values.rows || ckeys.rows != values.rows || values.cols != 1 || ckeys.cols != 1 {
        return Value::Error(ErrorKind::Value);
    }
    let Some(l) = resolve_fn(ctx, &args[3], 1) else { return Value::Error(ErrorKind::Value) };
    let hdr = tryv!(opt_i64(ctx, args, 4));
    let row_total = tryv!(opt_i64(ctx, args, 5)).unwrap_or(1);
    let col_total = tryv!(opt_i64(ctx, args, 7)).unwrap_or(1);
    let (has_hdr, _) = detect_headers(&values, hdr);
    let start = usize::from(has_hdr);
    let rows = tryv!(filter_rows(ctx, args, 9, start, values.rows));
    let rgroups = group_rows(&rkeys, &rows);
    let cgroups = group_rows(&ckeys, &rows);
    let mut out: Vec<Vec<Value>> = Vec::new();
    let mut header = vec![Value::Text(String::new()); rkeys.cols];
    for (k, _) in &cgroups {
        header.push(k[0].clone());
    }
    if col_total != 0 {
        header.push(Value::Text("Total".into()));
    }
    out.push(header);
    let intersect = |a: &[usize], b: &[usize]| -> Vec<usize> { a.iter().copied().filter(|x| b.contains(x)).collect() };
    for (k, rs) in &rgroups {
        let mut line = k.clone();
        for (_, cs) in &cgroups {
            let both = intersect(rs, cs);
            line.push(if both.is_empty() { Value::Empty } else { aggregate(ctx, &l, &values, 0, &both) });
        }
        if col_total != 0 {
            line.push(aggregate(ctx, &l, &values, 0, rs));
        }
        out.push(line);
    }
    if row_total != 0 {
        let mut line = vec![Value::Text("Total".into())];
        line.resize(rkeys.cols, Value::Text(String::new()));
        for (_, cs) in &cgroups {
            line.push(aggregate(ctx, &l, &values, 0, cs));
        }
        if col_total != 0 {
            line.push(aggregate(ctx, &l, &values, 0, &rows));
        }
        out.push(line);
    }
    to_value(Array::from_rows(out))
}
