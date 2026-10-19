//! Criteria matching used by COUNTIF, SUMIFS, the D-functions, AutoFilter, etc.

use std::cmp::Ordering;

use crate::value::{parse_number_text, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A compiled criterion such as `">5"`, `"<>apple"`, `"a*"`, or a plain value.
#[derive(Debug, Clone)]
pub struct Criteria {
    op: Op,
    target: Value,
    /// Lower-cased wildcard pattern for text equality (`*`, `?`, `~` escapes).
    pattern: Option<Vec<PatTok>>,
}

#[derive(Debug, Clone, PartialEq)]
enum PatTok {
    Char(char),
    Any,
    One,
}

fn compile_pattern(s: &str) -> Option<Vec<PatTok>> {
    let mut out = Vec::new();
    let mut has_wild = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '~' => {
                if let Some(n) = chars.next() {
                    out.push(PatTok::Char(n.to_lowercase().next().unwrap_or(n)));
                } else {
                    out.push(PatTok::Char('~'));
                }
            }
            '*' => {
                has_wild = true;
                out.push(PatTok::Any)
            }
            '?' => {
                has_wild = true;
                out.push(PatTok::One)
            }
            c => out.push(PatTok::Char(c.to_lowercase().next().unwrap_or(c))),
        }
    }
    if has_wild || s.contains('~') {
        Some(out)
    } else {
        None
    }
}

fn pattern_match(p: &[PatTok], s: &[char]) -> bool {
    // iterative wildcard matching with backtracking on '*'
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() {
            match &p[pi] {
                PatTok::Char(c) if *c == s[si] => {
                    pi += 1;
                    si += 1;
                    continue;
                }
                PatTok::One => {
                    pi += 1;
                    si += 1;
                    continue;
                }
                PatTok::Any => {
                    star = Some((pi, si));
                    pi += 1;
                    continue;
                }
                _ => {}
            }
        }
        if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == PatTok::Any {
        pi += 1;
    }
    pi == p.len()
}

/// Match text against an Excel wildcard pattern (case-insensitive).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = compile_pattern(pattern).unwrap_or_else(|| pattern.to_lowercase().chars().map(PatTok::Char).collect());
    let s: Vec<char> = text.to_lowercase().chars().collect();
    pattern_match(&p, &s)
}

impl Criteria {
    /// Build a criterion from a criteria argument value.
    pub fn new(v: &Value) -> Criteria {
        match v {
            Value::Text(s) => {
                let (op, rest) = if let Some(r) = s.strip_prefix("<=") {
                    (Op::Le, r)
                } else if let Some(r) = s.strip_prefix(">=") {
                    (Op::Ge, r)
                } else if let Some(r) = s.strip_prefix("<>") {
                    (Op::Ne, r)
                } else if let Some(r) = s.strip_prefix('<') {
                    (Op::Lt, r)
                } else if let Some(r) = s.strip_prefix('>') {
                    (Op::Gt, r)
                } else if let Some(r) = s.strip_prefix('=') {
                    (Op::Eq, r)
                } else {
                    (Op::Eq, s.as_str())
                };
                let target = if rest.is_empty() {
                    Value::Empty
                } else if let Some(n) = parse_number_text(rest) {
                    Value::Number(n)
                } else if rest.eq_ignore_ascii_case("TRUE") {
                    Value::Bool(true)
                } else if rest.eq_ignore_ascii_case("FALSE") {
                    Value::Bool(false)
                } else if let Some(e) = crate::value::ErrorKind::parse(rest) {
                    Value::Error(e)
                } else {
                    Value::Text(rest.to_string())
                };
                let pattern = match (&target, op) {
                    (Value::Text(t), Op::Eq | Op::Ne) => compile_pattern(t),
                    _ => None,
                };
                Criteria { op, target, pattern }
            }
            Value::Empty => Criteria { op: Op::Eq, target: Value::Number(0.0), pattern: None },
            other => Criteria { op: Op::Eq, target: other.clone().scalar(), pattern: None },
        }
    }

    /// Does a cell value satisfy the criterion?
    pub fn matches(&self, v: &Value) -> bool {
        // Empty criterion "=" matches blank cells; "<>" matches non-blank.
        if self.target.is_empty() {
            let blank = v.is_empty() || matches!(v, Value::Text(t) if t.is_empty());
            return match self.op {
                Op::Eq => blank,
                Op::Ne => !blank,
                _ => false,
            };
        }
        let r = match (&self.target, v) {
            (Value::Number(t), Value::Number(x)) => Some(x.partial_cmp(t).unwrap_or(Ordering::Equal)),
            (Value::Number(t), Value::Text(s)) if self.op == Op::Eq || self.op == Op::Ne => {
                // "=5" matches text "5"? Excel: numeric criteria match numeric-looking text in COUNTIF
                parse_number_text(s).map(|x| x.partial_cmp(t).unwrap_or(Ordering::Equal))
            }
            (Value::Text(t), Value::Text(s)) => {
                if let Some(p) = &self.pattern {
                    let chars: Vec<char> = s.to_lowercase().chars().collect();
                    let m = pattern_match(p, &chars);
                    return match self.op {
                        Op::Eq => m,
                        Op::Ne => !m,
                        _ => false,
                    };
                }
                Some(crate::value::compare_text(s, t))
            }
            (Value::Bool(t), Value::Bool(x)) => Some(x.cmp(t)),
            (Value::Error(t), Value::Error(x)) => Some(if t == x { Ordering::Equal } else { Ordering::Less }),
            _ => None,
        };
        match r {
            None => self.op == Op::Ne,
            Some(o) => match self.op {
                Op::Eq => o == Ordering::Equal,
                Op::Ne => o != Ordering::Equal,
                Op::Lt => o == Ordering::Less,
                Op::Le => o != Ordering::Greater,
                Op::Gt => o == Ordering::Greater,
                Op::Ge => o != Ordering::Less,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn criteria() {
        let c = Criteria::new(&Value::from(">5"));
        assert!(c.matches(&Value::Number(6.0)));
        assert!(!c.matches(&Value::Number(5.0)));
        assert!(!c.matches(&Value::from("x")));
        let c = Criteria::new(&Value::from("a*"));
        assert!(c.matches(&Value::from("Apple")));
        assert!(!c.matches(&Value::from("banana")));
        let c = Criteria::new(&Value::from("<>a*"));
        assert!(c.matches(&Value::from("banana")));
        let c = Criteria::new(&Value::from("?at"));
        assert!(c.matches(&Value::from("cat")));
        assert!(!c.matches(&Value::from("at")));
        let c = Criteria::new(&Value::Number(3.0));
        assert!(c.matches(&Value::Number(3.0)));
        let c = Criteria::new(&Value::from(""));
        assert!(c.matches(&Value::Empty));
        let c = Criteria::new(&Value::from("<>"));
        assert!(c.matches(&Value::Number(1.0)));
        assert!(!c.matches(&Value::Empty));
        assert!(wildcard_match("*~*", "a*"));
    }
}
