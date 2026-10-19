//! Conditional formatting evaluation.

use std::collections::HashMap;

use crate::address::{CellPos, Rect};
use crate::ast::Expr;
use crate::parser::parse_formula;
use crate::style::Color;
use crate::value::Value;
use crate::workbook::{CfOperator, CfRule, CfStyle, Workbook};

/// Formatting produced by conditional rules for one cell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CfFormat {
    pub fill: Option<Color>,
    pub font_color: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    /// Data bar: fraction of the cell width (0..1) and color.
    pub data_bar: Option<(f32, Color)>,
}

impl CfFormat {
    fn apply_style(&mut self, s: &CfStyle) {
        if self.fill.is_none() {
            self.fill = s.fill;
        }
        if self.font_color.is_none() {
            self.font_color = s.font_color;
        }
        self.bold |= s.bold;
        self.italic |= s.italic;
    }
}

/// Parse a rule operand (a formula stored without the leading '=', or a constant).
fn operand(s: &str) -> Option<Expr> {
    let t = s.trim();
    let t = t.strip_prefix('=').unwrap_or(t);
    if t.is_empty() {
        return None;
    }
    if let Ok(p) = crate::workbook::parse_input(t) {
        match p.value {
            Value::Number(n) if p.formula.is_none() => return Some(Expr::Number(n)),
            Value::Bool(b) if p.formula.is_none() => return Some(Expr::Bool(b)),
            _ => {}
        }
    }
    match parse_formula(t) {
        Ok(Expr::Name(_)) | Err(_) => Some(Expr::Text(t.trim_matches('"').to_string())),
        Ok(e) => Some(e),
    }
}

fn eval_operand(wb: &Workbook, sheet: usize, range: Rect, p: CellPos, e: &Expr) -> Value {
    let mut e = e.clone();
    e.shift_relative(p.row as i64 - range.top as i64, p.col as i64 - range.left as i64);
    crate::eval::eval_formula(wb, sheet, p, &e).scalar()
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Numeric values in `range` (bounded by the sheet's data extent).
fn range_values(wb: &Workbook, sheet: usize, range: Rect) -> Vec<(CellPos, Value)> {
    let s = &wb.sheets[sheet];
    let Some(ext) = s.data_extent() else { return vec![] };
    let Some(r) = range.intersection(&ext) else { return vec![] };
    let mut out: Vec<(CellPos, Value)> = s.cells_in(r).filter(|(_, c)| !c.is_blank()).map(|(p, _)| (*p, s.value_at(*p))).collect();
    for p in s.spill_owner.keys() {
        if r.contains(*p) {
            out.push((*p, s.value_at(*p)));
        }
    }
    out
}

fn numbers(vals: &[(CellPos, Value)]) -> Vec<f64> {
    vals.iter().filter_map(|(_, v)| if let Value::Number(n) = v { Some(*n) } else { None }).collect()
}

fn compare_op(op: &CfOperator, v: &Value, a: &Value, b: &Value) -> bool {
    use std::cmp::Ordering::*;
    if v.is_empty() {
        return false;
    }
    let ca = v.compare(a);
    match op {
        CfOperator::Equal => ca == Equal,
        CfOperator::NotEqual => ca != Equal,
        CfOperator::Greater => ca == Greater,
        CfOperator::Less => ca == Less,
        CfOperator::GreaterOrEqual => ca != Less,
        CfOperator::LessOrEqual => ca != Greater,
        CfOperator::Between | CfOperator::NotBetween => {
            let (lo, hi) = if a.compare(b) == Greater { (b, a) } else { (a, b) };
            let inside = v.compare(lo) != Less && v.compare(hi) != Greater;
            if *op == CfOperator::Between {
                inside
            } else {
                !inside
            }
        }
    }
}

/// Evaluate all conditional formats of `sheet` for the cells in `area`.
pub fn evaluate(wb: &Workbook, sheet: usize, area: Rect) -> HashMap<CellPos, CfFormat> {
    let mut out: HashMap<CellPos, CfFormat> = HashMap::new();
    let s = &wb.sheets[sheet];
    for cf in &s.cond_formats {
        let Some(vis) = cf.range.intersection(&area) else { continue };
        let range = cf.range;
        match &cf.rule {
            CfRule::CellValue { op, a, b, style } => {
                let ea = operand(a);
                let eb = operand(b);
                for (p, v) in range_values(wb, sheet, vis) {
                    let va = ea.as_ref().map(|e| eval_operand(wb, sheet, range, p, e)).unwrap_or(Value::Empty);
                    let vb = eb.as_ref().map(|e| eval_operand(wb, sheet, range, p, e)).unwrap_or(Value::Empty);
                    if compare_op(op, &v, &va, &vb) {
                        out.entry(p).or_default().apply_style(style);
                    }
                }
            }
            CfRule::Expression { formula, style } => {
                let Ok(e) = parse_formula(formula.trim_start_matches('=')) else { continue };
                if vis.cell_count() > 200_000 {
                    continue;
                }
                for p in vis.iter() {
                    let v = eval_operand(wb, sheet, range, p, &e);
                    if v.to_bool().unwrap_or(false) {
                        out.entry(p).or_default().apply_style(style);
                    }
                }
            }
            CfRule::TextContains { text, style } => {
                let needle = text.to_lowercase();
                for (p, v) in range_values(wb, sheet, vis) {
                    if let Ok(t) = v.to_text() {
                        if !needle.is_empty() && t.to_lowercase().contains(&needle) {
                            out.entry(p).or_default().apply_style(style);
                        }
                    }
                }
            }
            CfRule::ColorScale { min, mid, max } => {
                let all = range_values(wb, sheet, range);
                let mut nums = numbers(&all);
                if nums.is_empty() {
                    continue;
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let lo = nums[0];
                let hi = *nums.last().unwrap();
                let median = percentile(&nums, 0.5);
                for (p, v) in range_values(wb, sheet, vis) {
                    let Value::Number(x) = v else { continue };
                    let c = match mid {
                        Some(m) => {
                            if x <= median {
                                let t = if median > lo { (x - lo) / (median - lo) } else { 1.0 };
                                lerp(*min, *m, t as f32)
                            } else {
                                let t = if hi > median { (x - median) / (hi - median) } else { 1.0 };
                                lerp(*m, *max, t as f32)
                            }
                        }
                        None => {
                            let t = if hi > lo { (x - lo) / (hi - lo) } else { 0.5 };
                            lerp(*min, *max, t as f32)
                        }
                    };
                    let f = out.entry(p).or_default();
                    if f.fill.is_none() {
                        f.fill = Some(c);
                    }
                }
            }
            CfRule::DataBar { color } => {
                let all = range_values(wb, sheet, range);
                let nums = numbers(&all);
                if nums.is_empty() {
                    continue;
                }
                let lo = nums.iter().cloned().fold(f64::INFINITY, f64::min).min(0.0);
                let hi = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                for (p, v) in range_values(wb, sheet, vis) {
                    let Value::Number(x) = v else { continue };
                    let t = if hi > lo { ((x - lo) / (hi - lo)) as f32 } else { 1.0 };
                    let f = out.entry(p).or_default();
                    if f.data_bar.is_none() {
                        f.data_bar = Some((0.1 + 0.9 * t.clamp(0.0, 1.0), *color));
                    }
                }
            }
            CfRule::TopBottom { top, rank, percent, style } => {
                let all = range_values(wb, sheet, range);
                let mut nums = numbers(&all);
                if nums.is_empty() {
                    continue;
                }
                nums.sort_by(|a, b| b.partial_cmp(a).unwrap());
                if !top {
                    nums.reverse();
                }
                let n = if *percent {
                    ((nums.len() as f64 * *rank as f64 / 100.0).floor() as usize).max(1)
                } else {
                    *rank as usize
                };
                if n == 0 {
                    continue;
                }
                let threshold = nums[(n - 1).min(nums.len() - 1)];
                for (p, v) in range_values(wb, sheet, vis) {
                    let Value::Number(x) = v else { continue };
                    if (*top && x >= threshold) || (!*top && x <= threshold) {
                        out.entry(p).or_default().apply_style(style);
                    }
                }
            }
            CfRule::AboveAverage { above, style } => {
                let all = range_values(wb, sheet, range);
                let nums = numbers(&all);
                if nums.is_empty() {
                    continue;
                }
                let avg = nums.iter().sum::<f64>() / nums.len() as f64;
                for (p, v) in range_values(wb, sheet, vis) {
                    let Value::Number(x) = v else { continue };
                    if (*above && x > avg) || (!*above && x < avg) {
                        out.entry(p).or_default().apply_style(style);
                    }
                }
            }
            CfRule::Duplicates { unique, style } => {
                let all = range_values(wb, sheet, range);
                let key = |v: &Value| match v {
                    Value::Text(t) => format!("t{}", t.to_lowercase()),
                    v => format!("v{}", v.display()),
                };
                let mut counts: HashMap<String, usize> = HashMap::new();
                for (_, v) in &all {
                    if !v.is_empty() {
                        *counts.entry(key(v)).or_default() += 1;
                    }
                }
                for (p, v) in range_values(wb, sheet, vis) {
                    if v.is_empty() {
                        continue;
                    }
                    let c = counts.get(&key(&v)).copied().unwrap_or(0);
                    if (*unique && c == 1) || (!*unique && c > 1) {
                        out.entry(p).or_default().apply_style(style);
                    }
                }
            }
        }
    }
    out
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p * (sorted.len() - 1) as f64;
    let i = pos.floor() as usize;
    let f = pos - i as f64;
    if i + 1 < sorted.len() {
        sorted[i] + (sorted[i + 1] - sorted[i]) * f
    } else {
        sorted[i]
    }
}

/// Human-readable description of a rule (for the rules manager).
pub fn describe(rule: &CfRule) -> String {
    match rule {
        CfRule::CellValue { op, a, b, .. } => {
            let o = match op {
                CfOperator::Between => return format!("Cell value between {} and {}", a, b),
                CfOperator::NotBetween => return format!("Cell value not between {} and {}", a, b),
                CfOperator::Equal => "=",
                CfOperator::NotEqual => "<>",
                CfOperator::Greater => ">",
                CfOperator::Less => "<",
                CfOperator::GreaterOrEqual => ">=",
                CfOperator::LessOrEqual => "<=",
            };
            format!("Cell value {} {}", o, a)
        }
        CfRule::Expression { formula, .. } => format!("Formula: {}", formula),
        CfRule::TextContains { text, .. } => format!("Text contains \"{}\"", text),
        CfRule::ColorScale { mid, .. } => if mid.is_some() { "3-color scale".into() } else { "2-color scale".into() },
        CfRule::DataBar { .. } => "Data bar".into(),
        CfRule::TopBottom { top, rank, percent, .. } => {
            format!("{} {}{}", if *top { "Top" } else { "Bottom" }, rank, if *percent { "%" } else { "" })
        }
        CfRule::AboveAverage { above, .. } => if *above { "Above average".into() } else { "Below average".into() },
        CfRule::Duplicates { unique, .. } => if *unique { "Unique values".into() } else { "Duplicate values".into() },
    }
}
