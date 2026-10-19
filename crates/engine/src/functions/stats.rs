//! Statistical functions, plus the special functions (gamma, beta, normal...)
//! and aggregation kernels shared with the math module.

use super::{bool_or, num, num_or, Registry, VAR};
use crate::address::{Rect, MAX_COLS, MAX_ROWS};
use crate::ast::Expr;
use crate::criteria::Criteria;
use crate::eval::{Area, Ctx, Val};
use crate::value::{Array, ErrorKind, Value};

use special::*;

const CAT: &str = "Statistical";

type R<T> = Result<T, ErrorKind>;

// ===========================================================================
// Special functions
// ===========================================================================

/// Special mathematical functions used by the distribution functions.
pub mod special {
    use std::f64::consts::PI;

    const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_8;
    const SQRT_2PI: f64 = 2.506_628_274_631_000_5;
    const EPS: f64 = f64::EPSILON;
    const FPMIN: f64 = 1e-300;

    const LANCZOS_G: f64 = 7.0;
    const LANCZOS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    fn lanczos_sum(xm1: f64) -> f64 {
        let mut a = LANCZOS[0];
        for (i, c) in LANCZOS.iter().enumerate().skip(1) {
            a += c / (xm1 + i as f64);
        }
        a
    }

    /// Natural log of |Γ(x)|.
    pub fn lgamma(x: f64) -> f64 {
        if x.is_nan() {
            return f64::NAN;
        }
        if x <= 0.0 && x == x.floor() {
            return f64::INFINITY;
        }
        if x < 0.5 {
            return (PI / (PI * x).sin().abs()).ln() - lgamma(1.0 - x);
        }
        if x == 1.0 || x == 2.0 {
            return 0.0;
        }
        if x >= 10.0 {
            // Stirling series
            let z = 1.0 / (x * x);
            let series = (1.0 / 12.0
                + z * (-1.0 / 360.0
                    + z * (1.0 / 1260.0 + z * (-1.0 / 1680.0 + z * (1.0 / 1188.0 + z * (-691.0 / 360_360.0 + z / 156.0))))))
                / x;
            return (x - 0.5) * x.ln() - x + LN_SQRT_2PI + series;
        }
        let xm1 = x - 1.0;
        let t = xm1 + LANCZOS_G + 0.5;
        LN_SQRT_2PI + (xm1 + 0.5) * t.ln() - t + lanczos_sum(xm1).ln()
    }

    /// Γ(x).
    pub fn gamma(x: f64) -> f64 {
        if x.is_nan() {
            return f64::NAN;
        }
        if x == x.floor() {
            if x <= 0.0 {
                return f64::NAN;
            }
            if x <= 171.0 {
                let mut r = 1.0;
                let mut i = 2.0;
                while i < x {
                    r *= i;
                    i += 1.0;
                }
                return r;
            }
            return f64::INFINITY;
        }
        if x < 0.5 {
            return PI / ((PI * x).sin() * gamma(1.0 - x));
        }
        if x > 171.7 {
            return f64::INFINITY;
        }
        let xm1 = x - 1.0;
        let t = xm1 + LANCZOS_G + 0.5;
        let p = t.powf((xm1 + 0.5) * 0.5);
        SQRT_2PI * p * (p * (-t).exp()) * lanczos_sum(xm1)
    }

    /// ln B(a, b)
    pub fn lbeta(a: f64, b: f64) -> f64 {
        lgamma(a) + lgamma(b) - lgamma(a + b)
    }

    /// Error of Stirling's approximation: ln(n!) - ln(sqrt(2πn)(n/e)^n).
    pub fn stirlerr(n: f64) -> f64 {
        const S0: f64 = 1.0 / 12.0;
        const S1: f64 = 1.0 / 360.0;
        const S2: f64 = 1.0 / 1260.0;
        const S3: f64 = 1.0 / 1680.0;
        const S4: f64 = 1.0 / 1188.0;
        if n <= 15.0 {
            return lgamma(n + 1.0) - (n + 0.5) * n.ln() + n - LN_SQRT_2PI;
        }
        let nn = n * n;
        if n > 500.0 {
            return (S0 - S1 / nn) / n;
        }
        if n > 80.0 {
            return (S0 - (S1 - S2 / nn) / nn) / n;
        }
        if n > 35.0 {
            return (S0 - (S1 - (S2 - S3 / nn) / nn) / nn) / n;
        }
        (S0 - (S1 - (S2 - (S3 - S4 / nn) / nn) / nn) / nn) / n
    }

    /// Deviance term x ln(x/np) + np - x, computed accurately.
    pub fn bd0(x: f64, np: f64) -> f64 {
        if (x - np).abs() < 0.1 * (x + np) {
            let mut v = (x - np) / (x + np);
            let mut s = (x - np) * v;
            if s.abs() < f64::MIN_POSITIVE {
                return s;
            }
            let mut ej = 2.0 * x * v;
            v *= v;
            for j in 1..1000 {
                ej *= v;
                let s1 = s + ej / ((2 * j + 1) as f64);
                if s1 == s {
                    return s1;
                }
                s = s1;
            }
            return s;
        }
        x * (x / np).ln() + np - x
    }

    /// Poisson density λ^x e^-λ / Γ(x+1) for real x >= 0 (Loader's algorithm).
    pub fn dpois_raw(x: f64, lambda: f64) -> f64 {
        if lambda == 0.0 {
            return if x == 0.0 { 1.0 } else { 0.0 };
        }
        if x < 0.0 {
            return 0.0;
        }
        if x == 0.0 {
            return (-lambda).exp();
        }
        if !lambda.is_finite() {
            return 0.0;
        }
        (-stirlerr(x) - bd0(x, lambda)).exp() / (2.0 * PI * x).sqrt()
    }

    /// Binomial density for real x, n (Loader's algorithm).
    pub fn dbinom_raw(x: f64, n: f64, p: f64, q: f64) -> f64 {
        if p == 0.0 {
            return if x == 0.0 { 1.0 } else { 0.0 };
        }
        if q == 0.0 {
            return if x == n { 1.0 } else { 0.0 };
        }
        if x == 0.0 {
            if n == 0.0 {
                return 1.0;
            }
            let lc = if p < 0.1 { -bd0(n, n * q) - n * p } else { n * q.ln() };
            return lc.exp();
        }
        if x == n {
            let lc = if q < 0.1 { -bd0(n, n * p) - n * q } else { n * p.ln() };
            return lc.exp();
        }
        if x < 0.0 || x > n {
            return 0.0;
        }
        let lc = stirlerr(n) - stirlerr(x) - stirlerr(n - x) - bd0(x, n * p) - bd0(n - x, n * q);
        let lf = (2.0 * PI).ln() + x.ln() + (-x / n).ln_1p();
        (lc - 0.5 * lf).exp()
    }

    /// Regularized incomplete gamma functions (P(a,x), Q(a,x)).
    pub fn gamma_pq(a: f64, x: f64) -> (f64, f64) {
        if x.is_nan() || a.is_nan() || a <= 0.0 {
            return (f64::NAN, f64::NAN);
        }
        if x <= 0.0 {
            return (0.0, 1.0);
        }
        if x.is_infinite() {
            return (1.0, 0.0);
        }
        // x^a e^-x / Γ(a)
        let front = a * dpois_raw(a, x);
        if x < a + 1.0 {
            // series
            let mut ap = a;
            let mut del = 1.0 / a;
            let mut sum = del;
            for _ in 0..1_000_000 {
                ap += 1.0;
                del *= x / ap;
                sum += del;
                if del.abs() < sum.abs() * EPS {
                    break;
                }
            }
            let p = (sum * front).min(1.0);
            (p, 1.0 - p)
        } else {
            // continued fraction (modified Lentz)
            let mut b = x + 1.0 - a;
            let mut c = 1.0 / FPMIN;
            let mut d = 1.0 / b;
            let mut h = d;
            let mut i = 1.0;
            loop {
                let an = -i * (i - a);
                b += 2.0;
                d = an * d + b;
                if d.abs() < FPMIN {
                    d = FPMIN;
                }
                c = b + an / c;
                if c.abs() < FPMIN {
                    c = FPMIN;
                }
                d = 1.0 / d;
                let del = d * c;
                h *= del;
                if (del - 1.0).abs() < EPS || i > 1_000_000.0 {
                    break;
                }
                i += 1.0;
            }
            let q = (front * h).min(1.0);
            (1.0 - q, q)
        }
    }

    fn betacf(a: f64, b: f64, x: f64) -> f64 {
        let qab = a + b;
        let qap = a + 1.0;
        let qam = a - 1.0;
        let mut c = 1.0;
        let mut d = 1.0 - qab * x / qap;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        d = 1.0 / d;
        let mut h = d;
        for m in 1..1_000_000 {
            let m = m as f64;
            let m2 = 2.0 * m;
            let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
            d = 1.0 + aa * d;
            if d.abs() < FPMIN {
                d = FPMIN;
            }
            c = 1.0 + aa / c;
            if c.abs() < FPMIN {
                c = FPMIN;
            }
            d = 1.0 / d;
            h *= d * c;
            let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
            d = 1.0 + aa * d;
            if d.abs() < FPMIN {
                d = FPMIN;
            }
            c = 1.0 + aa / c;
            if c.abs() < FPMIN {
                c = FPMIN;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < EPS {
                break;
            }
        }
        h
    }

    /// Regularized incomplete beta I_x(a,b) and its complement; `y` must equal 1-x
    /// (passed separately for accuracy).
    pub fn beta_reg(a: f64, b: f64, x: f64, y: f64) -> (f64, f64) {
        if a.is_nan() || b.is_nan() || x.is_nan() || a <= 0.0 || b <= 0.0 {
            return (f64::NAN, f64::NAN);
        }
        if x <= 0.0 {
            return (0.0, 1.0);
        }
        if y <= 0.0 {
            return (1.0, 0.0);
        }
        if x < (a + 1.0) / (a + b + 2.0) {
            // x^a y^b / (a B(a,b))
            let front = dbinom_raw(a, a + b, x, y) * b / (a + b);
            let i = (front * betacf(a, b, x)).min(1.0);
            (i, 1.0 - i)
        } else {
            let front = dbinom_raw(b, a + b, y, x) * a / (a + b);
            let j = (front * betacf(b, a, y)).min(1.0);
            (1.0 - j, j)
        }
    }

    pub fn erf(x: f64) -> f64 {
        let (p, _) = gamma_pq(0.5, x * x);
        if x < 0.0 {
            -p
        } else {
            p
        }
    }

    pub fn erfc(x: f64) -> f64 {
        let (p, q) = gamma_pq(0.5, x * x);
        if x < 0.0 {
            1.0 + p
        } else {
            q
        }
    }

    /// Standard normal density.
    pub fn norm_pdf(z: f64) -> f64 {
        (-0.5 * z * z).exp() / SQRT_2PI
    }

    /// Standard normal CDF Φ(z).
    pub fn norm_cdf(z: f64) -> f64 {
        if z.is_nan() {
            return f64::NAN;
        }
        let (p, q) = gamma_pq(0.5, 0.5 * z * z);
        if z < 0.0 {
            0.5 * q
        } else {
            0.5 + 0.5 * p
        }
    }

    /// Upper tail 1-Φ(z).
    pub fn norm_sf(z: f64) -> f64 {
        norm_cdf(-z)
    }

    fn norm_inv_lower(p: f64) -> f64 {
        // p in (0, 0.5]
        const A: [f64; 6] = [
            -3.969_683_028_665_376e1,
            2.209_460_984_245_205e2,
            -2.759_285_104_469_687e2,
            1.383_577_518_672_69e2,
            -3.066_479_806_614_716e1,
            2.506_628_277_459_239,
        ];
        const B: [f64; 5] = [
            -5.447_609_879_822_406e1,
            1.615_858_368_580_409e2,
            -1.556_989_798_598_866e2,
            6.680_131_188_771_972e1,
            -1.328_068_155_288_572e1,
        ];
        const C: [f64; 6] = [
            -7.784_894_002_430_293e-3,
            -3.223_964_580_411_365e-1,
            -2.400_758_277_161_838,
            -2.549_732_539_343_734,
            4.374_664_141_464_968,
            2.938_163_982_698_783,
        ];
        const D: [f64; 4] = [7.784_695_709_041_462e-3, 3.224_671_290_700_398e-1, 2.445_134_137_142_996, 3.754_408_661_907_416];
        let mut x = if p < 0.02425 {
            let q = (-2.0 * p.ln()).sqrt();
            (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
                / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
        } else {
            let q = p - 0.5;
            let r = q * q;
            (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
                / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
        };
        for _ in 0..3 {
            let e = norm_cdf(x) - p;
            let u = e * SQRT_2PI * (0.5 * x * x).exp();
            if !u.is_finite() {
                break;
            }
            x -= u / (1.0 + x * u / 2.0);
        }
        x
    }

    /// Inverse standard normal CDF.
    pub fn norm_inv(p: f64) -> f64 {
        if !(p > 0.0 && p < 1.0) {
            return f64::NAN;
        }
        if p > 0.5 {
            -norm_inv_lower(1.0 - p)
        } else {
            norm_inv_lower(p)
        }
    }

    /// Upper tail of Student's t for t >= 0.
    pub fn t_sf(t: f64, df: f64) -> f64 {
        if t < 0.0 {
            return 1.0 - t_sf(-t, df);
        }
        let t2 = t * t;
        let (i, _) = beta_reg(df / 2.0, 0.5, df / (df + t2), t2 / (df + t2));
        0.5 * i
    }

    pub fn t_cdf(t: f64, df: f64) -> f64 {
        if t < 0.0 {
            t_sf(-t, df)
        } else {
            1.0 - t_sf(t, df)
        }
    }

    pub fn t_pdf(t: f64, df: f64) -> f64 {
        (lgamma((df + 1.0) / 2.0) - lgamma(df / 2.0) - 0.5 * (df * PI).ln() - (df + 1.0) / 2.0 * (t * t / df).ln_1p()).exp()
    }

    /// F distribution (lower, upper) tails.
    pub fn f_tails(x: f64, d1: f64, d2: f64) -> (f64, f64) {
        if x <= 0.0 {
            return (0.0, 1.0);
        }
        let z = d1 * x;
        beta_reg(d1 / 2.0, d2 / 2.0, z / (z + d2), d2 / (z + d2))
    }

    pub fn f_pdf(x: f64, d1: f64, d2: f64) -> f64 {
        if x <= 0.0 {
            return if x < 0.0 {
                0.0
            } else if d1 < 2.0 {
                f64::INFINITY
            } else if d1 == 2.0 {
                1.0
            } else {
                0.0
            };
        }
        (0.5 * (d1 * (d1 * x).ln() + d2 * d2.ln() - (d1 + d2) * (d1 * x + d2).ln()) - x.ln() - lbeta(d1 / 2.0, d2 / 2.0)).exp()
    }

    /// Gamma density with shape a and scale b.
    pub fn gamma_pdf(x: f64, a: f64, b: f64) -> f64 {
        if x < 0.0 {
            return 0.0;
        }
        if x == 0.0 {
            return if a < 1.0 {
                f64::INFINITY
            } else if a == 1.0 {
                1.0 / b
            } else {
                0.0
            };
        }
        a * dpois_raw(a, x / b) / x
    }

    pub fn beta_pdf(x: f64, a: f64, b: f64) -> f64 {
        if !(0.0..=1.0).contains(&x) {
            return 0.0;
        }
        if x == 0.0 {
            return if a < 1.0 {
                f64::INFINITY
            } else if a == 1.0 {
                b
            } else {
                0.0
            };
        }
        if x == 1.0 {
            return if b < 1.0 {
                f64::INFINITY
            } else if b == 1.0 {
                a
            } else {
                0.0
            };
        }
        ((a - 1.0) * x.ln() + (b - 1.0) * (-x).ln_1p() - lbeta(a, b)).exp()
    }

    /// Brent's root finder on [a, b] (f(a), f(b) must bracket a root).
    pub fn brent(f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64) -> Option<f64> {
        let mut fa = f(a);
        let mut fb = f(b);
        if fa == 0.0 {
            return Some(a);
        }
        if fb == 0.0 {
            return Some(b);
        }
        if fa.is_nan() || fb.is_nan() || (fa > 0.0) == (fb > 0.0) {
            return None;
        }
        let mut c = a;
        let mut fc = fa;
        let mut d = b - a;
        let mut e = d;
        for _ in 0..3000 {
            if (fb > 0.0) == (fc > 0.0) {
                c = a;
                fc = fa;
                d = b - a;
                e = d;
            }
            if fc.abs() < fb.abs() {
                a = b;
                b = c;
                c = a;
                fa = fb;
                fb = fc;
                fc = fa;
            }
            let tol = 2.0 * EPS * b.abs() + 1e-300;
            let m = 0.5 * (c - b);
            if m.abs() <= tol || fb == 0.0 {
                return Some(b);
            }
            if e.abs() < tol || fa.abs() <= fb.abs() {
                d = m;
                e = m;
            } else {
                let s = fb / fa;
                let (mut p, mut q);
                if a == c {
                    p = 2.0 * m * s;
                    q = 1.0 - s;
                } else {
                    let q0 = fa / fc;
                    let r = fb / fc;
                    p = s * (2.0 * m * q0 * (q0 - r) - (b - a) * (r - 1.0));
                    q = (q0 - 1.0) * (r - 1.0) * (s - 1.0);
                }
                if p > 0.0 {
                    q = -q;
                } else {
                    p = -p;
                }
                if 2.0 * p < 3.0 * m * q - (tol * q).abs() && p < (0.5 * e * q).abs() {
                    e = d;
                    d = p / q;
                } else {
                    d = m;
                    e = m;
                }
            }
            a = b;
            fa = fb;
            b += if d.abs() > tol { d } else { tol * m.signum() };
            fb = f(b);
            if fb.is_nan() {
                return None;
            }
        }
        Some(b)
    }

    /// Solve `g(x) = 0` for an increasing function on [lo, ∞), starting the
    /// upper bracket at `guess`.
    pub fn solve_increasing(g: &dyn Fn(f64) -> f64, lo: f64, guess: f64) -> Option<f64> {
        let mut hi = guess.max(1e-3);
        let mut n = 0;
        while g(hi) < 0.0 {
            hi *= 2.0;
            n += 1;
            if n > 2100 || !hi.is_finite() {
                return None;
            }
        }
        brent(g, lo, hi)
    }
}

// ===========================================================================
// Aggregation kernels
// ===========================================================================

pub(crate) fn sorted(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    s
}

pub(crate) fn mean(v: &[f64]) -> R<f64> {
    if v.is_empty() {
        return Err(ErrorKind::Div0);
    }
    Ok(v.iter().sum::<f64>() / v.len() as f64)
}

pub(crate) fn devsq(v: &[f64]) -> R<f64> {
    if v.is_empty() {
        return Err(ErrorKind::Num);
    }
    let m = mean(v)?;
    Ok(v.iter().map(|x| (x - m) * (x - m)).sum())
}

pub(crate) fn variance(v: &[f64], sample: bool) -> R<f64> {
    let n = v.len();
    if n == 0 || (sample && n < 2) {
        return Err(ErrorKind::Div0);
    }
    Ok(devsq(v)? / (n - sample as usize) as f64)
}

pub(crate) fn stdev(v: &[f64], sample: bool) -> R<f64> {
    variance(v, sample).map(f64::sqrt)
}

pub(crate) fn median(v: &[f64]) -> R<f64> {
    if v.is_empty() {
        return Err(ErrorKind::Num);
    }
    let s = sorted(v);
    let n = s.len();
    Ok(if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 })
}

/// Values with the highest frequency (>= 2), in order of first appearance.
fn modes(v: &[f64]) -> Vec<f64> {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for &x in v {
        if let Some(e) = counts.iter_mut().find(|e| e.0 == x) {
            e.1 += 1;
        } else {
            counts.push((x, 1));
        }
    }
    let max = counts.iter().map(|e| e.1).max().unwrap_or(0);
    if max < 2 {
        return vec![];
    }
    counts.into_iter().filter(|e| e.1 == max).map(|e| e.0).collect()
}

pub(crate) fn mode_sngl(v: &[f64]) -> R<f64> {
    modes(v).first().copied().ok_or(ErrorKind::NA)
}

pub(crate) fn product(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().product()
    }
}

pub(crate) fn large(v: &[f64], k: f64) -> R<f64> {
    let k = k.ceil();
    if k < 1.0 || k > v.len() as f64 {
        return Err(ErrorKind::Num);
    }
    let s = sorted(v);
    Ok(s[s.len() - k as usize])
}

pub(crate) fn small(v: &[f64], k: f64) -> R<f64> {
    let k = k.ceil();
    if k < 1.0 || k > v.len() as f64 {
        return Err(ErrorKind::Num);
    }
    Ok(sorted(v)[k as usize - 1])
}

pub(crate) fn percentile_inc(v: &[f64], k: f64) -> R<f64> {
    if v.is_empty() || !(0.0..=1.0).contains(&k) {
        return Err(ErrorKind::Num);
    }
    let s = sorted(v);
    let r = k * (s.len() - 1) as f64;
    let i = r.floor() as usize;
    let f = r - i as f64;
    if i + 1 >= s.len() {
        return Ok(s[s.len() - 1]);
    }
    Ok(s[i] + f * (s[i + 1] - s[i]))
}

pub(crate) fn percentile_exc(v: &[f64], k: f64) -> R<f64> {
    let n = v.len();
    if n == 0 || k <= 0.0 || k >= 1.0 {
        return Err(ErrorKind::Num);
    }
    let r = k * (n + 1) as f64;
    if r < 1.0 || r > n as f64 {
        return Err(ErrorKind::Num);
    }
    let s = sorted(v);
    let i = r.floor() as usize;
    let f = r - i as f64;
    if i >= n {
        return Ok(s[n - 1]);
    }
    Ok(s[i - 1] + f * (s[i] - s[i - 1]))
}

pub(crate) fn quartile_inc(v: &[f64], q: f64) -> R<f64> {
    let q = q.trunc();
    if !(0.0..=4.0).contains(&q) {
        return Err(ErrorKind::Num);
    }
    percentile_inc(v, q / 4.0)
}

pub(crate) fn quartile_exc(v: &[f64], q: f64) -> R<f64> {
    let q = q.trunc();
    if q <= 0.0 || q >= 4.0 {
        return Err(ErrorKind::Num);
    }
    percentile_exc(v, q / 4.0)
}

// ===========================================================================
// Argument helpers
// ===========================================================================

pub(crate) fn res(r: R<f64>) -> Value {
    match r {
        Ok(v) => Value::number(v),
        Err(e) => Value::Error(e),
    }
}

/// Numbers of a single argument with SUM semantics.
pub(crate) fn nums(ctx: &mut Ctx, e: &Expr) -> R<Vec<f64>> {
    ctx.collect_numbers(std::slice::from_ref(e))
}

/// Strictly-numeric dense matrix (any non-number -> #VALUE!).
pub(crate) fn numeric_matrix(ctx: &mut Ctx, e: &Expr) -> R<(usize, usize, Vec<f64>)> {
    let a = ctx.eval_array(e)?;
    let mut out = Vec::with_capacity(a.data.len());
    for v in &a.data {
        match v {
            Value::Number(n) => out.push(*n),
            Value::Error(e) => return Err(*e),
            _ => return Err(ErrorKind::Value),
        }
    }
    Ok((a.rows, a.cols, out))
}

/// Paired numeric values from two arrays of the same size (pairs with a
/// non-numeric member are skipped; errors propagate; size mismatch -> #N/A).
pub(crate) fn paired(ctx: &mut Ctx, a: &Expr, b: &Expr) -> R<(Vec<f64>, Vec<f64>)> {
    let x = ctx.eval_array(a)?;
    let y = ctx.eval_array(b)?;
    if x.data.len() != y.data.len() {
        return Err(ErrorKind::NA);
    }
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (p, q) in x.data.iter().zip(y.data.iter()) {
        if let Value::Error(e) = p {
            return Err(*e);
        }
        if let Value::Error(e) = q {
            return Err(*e);
        }
        if let (Value::Number(p), Value::Number(q)) = (p, q) {
            xs.push(*p);
            ys.push(*q);
        }
    }
    Ok((xs, ys))
}

/// Evaluate a "k"-style argument which may be an array; map `f` over it.
pub(crate) fn map_k(ctx: &mut Ctx, e: &Expr, f: &dyn Fn(f64) -> R<f64>) -> Value {
    match ctx.eval_value(e) {
        Value::Array(a) => {
            let data = a.data.iter().map(|v| res(v.to_number().and_then(f))).collect();
            Value::from(Array::new(a.rows, a.cols, data))
        }
        v => res(v.to_number().and_then(f)),
    }
}

// ----- criteria ranges -------------------------------------------------------

/// A criteria/sum range: dense values plus its area when it is a reference.
pub(crate) struct RangeArg {
    pub arr: Array,
    pub area: Option<Area>,
}

pub(crate) fn range_arg(ctx: &mut Ctx, e: &Expr) -> R<RangeArg> {
    match ctx.eval(e) {
        Val::Refs(a) if a.len() == 1 => {
            let arr = ctx.area_array(a[0]);
            Ok(RangeArg { arr, area: Some(a[0]) })
        }
        Val::Refs(_) => Err(ErrorKind::Value),
        Val::V(Value::Array(a)) => Ok(RangeArg { arr: *a, area: None }),
        Val::V(v) => Ok(RangeArg { arr: Array::new(1, 1, vec![v]), area: None }),
    }
}

/// A range resized (from its top-left) to rows x cols, like SUMIF's sum_range.
pub(crate) fn resized_range(ctx: &mut Ctx, e: &Expr, rows: usize, cols: usize) -> R<Array> {
    let src = match ctx.eval(e) {
        Val::Refs(a) if a.len() == 1 => {
            let area = a[0];
            let tl = area.rect.top_left();
            let rect = Rect {
                top: tl.row,
                left: tl.col,
                bottom: (tl.row as u64 + rows as u64 - 1).min(MAX_ROWS as u64 - 1) as u32,
                right: (tl.col as u64 + cols as u64 - 1).min(MAX_COLS as u64 - 1) as u32,
            };
            ctx.area_array(Area { sheet: area.sheet, rect })
        }
        Val::Refs(_) => return Err(ErrorKind::Value),
        Val::V(Value::Array(a)) => *a,
        Val::V(Value::Error(e)) => return Err(e),
        Val::V(v) => Array::new(1, 1, vec![v]),
    };
    if src.rows == rows && src.cols == cols {
        return Ok(src);
    }
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            data.push(if r < src.rows && c < src.cols { src.get(r, c).clone() } else { Value::Empty });
        }
    }
    Ok(Array::new(rows, cols, data))
}

/// Core of the *IF/*IFS functions. `target` (if any) supplies the values that
/// are aggregated; otherwise a `TRUE` is passed for each matching cell.
pub(crate) fn ifs_eval(
    ctx: &mut Ctx,
    target: Option<Array>,
    ranges: Vec<Array>,
    crit_exprs: &[&Expr],
    agg: fn(&[Value]) -> Value,
) -> Value {
    let (rows, cols) = (ranges[0].rows, ranges[0].cols);
    if ranges.iter().any(|r| r.rows != rows || r.cols != cols) {
        return Value::Error(ErrorKind::Value);
    }
    if let Some(t) = &target {
        if t.rows != rows || t.cols != cols {
            return Value::Error(ErrorKind::Value);
        }
    }
    let crits: Vec<Value> = crit_exprs.iter().map(|e| ctx.eval_value(e)).collect();
    let compute = |cv: &[Value]| -> Value {
        let cs: Vec<Criteria> = cv.iter().map(Criteria::new).collect();
        let mut matched = Vec::new();
        for i in 0..rows * cols {
            if cs.iter().zip(ranges.iter()).all(|(c, r)| c.matches(&r.data[i])) {
                matched.push(match &target {
                    Some(t) => t.data[i].clone(),
                    None => Value::Bool(true),
                });
            }
        }
        agg(&matched)
    };
    let mut orows = 1;
    let mut ocols = 1;
    let mut any = false;
    for c in &crits {
        if let Value::Array(a) = c {
            any = true;
            orows = orows.max(a.rows);
            ocols = ocols.max(a.cols);
        }
    }
    if !any {
        return compute(&crits);
    }
    let mut data = Vec::with_capacity(orows * ocols);
    for r in 0..orows {
        for c in 0..ocols {
            let cv: Vec<Value> = crits
                .iter()
                .map(|v| match v {
                    Value::Array(a) => a.get_broadcast(r, c),
                    v => v.clone(),
                })
                .collect();
            data.push(compute(&cv));
        }
    }
    Value::from(Array::new(orows, ocols, data))
}

pub(crate) fn agg_sum(v: &[Value]) -> Value {
    let mut s = 0.0;
    for x in v {
        match x {
            Value::Number(n) => s += n,
            Value::Error(e) => return Value::Error(*e),
            _ => {}
        }
    }
    Value::number(s)
}

fn agg_count(v: &[Value]) -> Value {
    Value::Number(v.len() as f64)
}

fn agg_numbers(v: &[Value]) -> R<Vec<f64>> {
    let mut out = Vec::new();
    for x in v {
        match x {
            Value::Number(n) => out.push(*n),
            Value::Error(e) => return Err(*e),
            _ => {}
        }
    }
    Ok(out)
}

fn agg_avg(v: &[Value]) -> Value {
    res(agg_numbers(v).and_then(|n| mean(&n)))
}

fn agg_max(v: &[Value]) -> Value {
    res(agg_numbers(v).map(|n| n.iter().cloned().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x)))).unwrap_or(0.0)))
}

fn agg_min(v: &[Value]) -> Value {
    res(agg_numbers(v).map(|n| n.iter().cloned().fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x)))).unwrap_or(0.0)))
}

/// SUMIF-style: (range, criteria, [value_range]).
pub(crate) fn if_single(ctx: &mut Ctx, args: &[Expr], agg: fn(&[Value]) -> Value) -> Value {
    let range = match range_arg(ctx, &args[0]) {
        Ok(r) => r,
        Err(e) => return Value::Error(e),
    };
    let target = match args.get(2) {
        Some(e) if !matches!(e, Expr::Missing) => match resized_range(ctx, e, range.arr.rows, range.arr.cols) {
            Ok(a) => a,
            Err(e) => return Value::Error(e),
        },
        _ => range.arr.clone(),
    };
    ifs_eval(ctx, Some(target), vec![range.arr], &[&args[1]], agg)
}

/// SUMIFS-style: (value_range, range1, crit1, ...). With `has_target` false
/// (COUNTIFS) the arguments are just range/criteria pairs.
pub(crate) fn if_multi(ctx: &mut Ctx, args: &[Expr], has_target: bool, agg: fn(&[Value]) -> Value) -> Value {
    let (target, pairs) = if has_target {
        match range_arg(ctx, &args[0]) {
            Ok(r) => (Some(r.arr), &args[1..]),
            Err(e) => return Value::Error(e),
        }
    } else {
        (None, args)
    };
    if pairs.is_empty() || pairs.len() % 2 != 0 {
        return Value::Error(ErrorKind::Value);
    }
    let mut ranges = Vec::new();
    let mut crits = Vec::new();
    for p in pairs.chunks(2) {
        match range_arg(ctx, &p[0]) {
            Ok(r) => ranges.push(r.arr),
            Err(e) => return Value::Error(e),
        }
        crits.push(&p[1]);
    }
    ifs_eval(ctx, target, ranges, &crits, agg)
}

// ===========================================================================
// Registration
// ===========================================================================

pub fn register(r: &mut Registry) {
    // ----- descriptive statistics -----
    r.raw("AVEDEV", 1, VAR, avedev, CAT, "AVEDEV(number1, [number2], ...)", "Returns the average of the absolute deviations of data points from their mean.");
    r.raw("AVERAGE", 1, VAR, average, CAT, "AVERAGE(number1, [number2], ...)", "Returns the average (arithmetic mean) of its arguments.");
    r.raw("AVERAGEA", 1, VAR, averagea, CAT, "AVERAGEA(value1, [value2], ...)", "Returns the average of its arguments, including text and logical values.");
    r.raw("AVERAGEIF", 2, 3, averageif, CAT, "AVERAGEIF(range, criteria, [average_range])", "Returns the average of the cells that meet a criterion.");
    r.raw("AVERAGEIFS", 3, VAR, averageifs, CAT, "AVERAGEIFS(average_range, criteria_range1, criteria1, [criteria_range2, criteria2], ...)", "Returns the average of the cells that meet multiple criteria.");
    r.raw("COUNT", 1, VAR, count, CAT, "COUNT(value1, [value2], ...)", "Counts how many numbers are in the list of arguments.");
    r.raw("COUNTA", 1, VAR, counta, CAT, "COUNTA(value1, [value2], ...)", "Counts how many values are in the list of arguments.");
    r.raw("COUNTBLANK", 1, 1, countblank, CAT, "COUNTBLANK(range)", "Counts the number of blank cells within a range.");
    r.raw("COUNTIF", 2, 2, countif, CAT, "COUNTIF(range, criteria)", "Counts the number of cells within a range that meet the given criteria.");
    r.raw("COUNTIFS", 2, VAR, countifs, CAT, "COUNTIFS(criteria_range1, criteria1, [criteria_range2, criteria2], ...)", "Counts the number of cells within a range that meet multiple criteria.");
    r.raw("DEVSQ", 1, VAR, devsq_fn, CAT, "DEVSQ(number1, [number2], ...)", "Returns the sum of squares of deviations.");
    r.raw("GEOMEAN", 1, VAR, geomean, CAT, "GEOMEAN(number1, [number2], ...)", "Returns the geometric mean.");
    r.raw("HARMEAN", 1, VAR, harmean, CAT, "HARMEAN(number1, [number2], ...)", "Returns the harmonic mean.");
    r.raw("KURT", 1, VAR, kurt, CAT, "KURT(number1, [number2], ...)", "Returns the kurtosis of a data set.");
    r.raw("LARGE", 2, 2, large_fn, CAT, "LARGE(array, k)", "Returns the k-th largest value in a data set.");
    r.raw("MAX", 1, VAR, max_fn, CAT, "MAX(number1, [number2], ...)", "Returns the maximum value in a list of arguments.");
    r.raw("MAXA", 1, VAR, maxa, CAT, "MAXA(value1, [value2], ...)", "Returns the maximum value in a list of arguments, including text and logical values.");
    r.raw("MAXIFS", 3, VAR, maxifs, CAT, "MAXIFS(max_range, criteria_range1, criteria1, [criteria_range2, criteria2], ...)", "Returns the maximum value among cells specified by a set of criteria.");
    r.raw("MEDIAN", 1, VAR, median_fn, CAT, "MEDIAN(number1, [number2], ...)", "Returns the median of the given numbers.");
    r.raw("MIN", 1, VAR, min_fn, CAT, "MIN(number1, [number2], ...)", "Returns the minimum value in a list of arguments.");
    r.raw("MINA", 1, VAR, mina, CAT, "MINA(value1, [value2], ...)", "Returns the smallest value in a list of arguments, including text and logical values.");
    r.raw("MINIFS", 3, VAR, minifs, CAT, "MINIFS(min_range, criteria_range1, criteria1, [criteria_range2, criteria2], ...)", "Returns the minimum value among cells specified by a set of criteria.");
    r.raw("MODE.MULT", 1, VAR, mode_mult, CAT, "MODE.MULT(number1, [number2], ...)", "Returns a vertical array of the most frequently occurring values.");
    r.raw("MODE.SNGL", 1, VAR, mode_sngl_fn, CAT, "MODE.SNGL(number1, [number2], ...)", "Returns the most common value in a data set.");
    r.raw("MODE", 1, VAR, mode_sngl_fn, CAT, "MODE(number1, [number2], ...)", "Returns the most common value in a data set (compatibility).");
    r.raw("PERCENTILE.EXC", 2, 2, percentile_exc_fn, CAT, "PERCENTILE.EXC(array, k)", "Returns the k-th percentile, where k is in the range 0..1 exclusive.");
    r.raw("PERCENTILE.INC", 2, 2, percentile_inc_fn, CAT, "PERCENTILE.INC(array, k)", "Returns the k-th percentile, where k is in the range 0..1 inclusive.");
    r.raw("PERCENTILE", 2, 2, percentile_inc_fn, CAT, "PERCENTILE(array, k)", "Returns the k-th percentile of values in a range (compatibility).");
    r.raw("PERCENTRANK.EXC", 2, 3, percentrank_exc, CAT, "PERCENTRANK.EXC(array, x, [significance])", "Returns the rank of a value as a percentage (0..1 exclusive) of the data set.");
    r.raw("PERCENTRANK.INC", 2, 3, percentrank_inc, CAT, "PERCENTRANK.INC(array, x, [significance])", "Returns the rank of a value as a percentage (0..1 inclusive) of the data set.");
    r.raw("PERCENTRANK", 2, 3, percentrank_inc, CAT, "PERCENTRANK(array, x, [significance])", "Returns the percentage rank of a value in a data set (compatibility).");
    r.raw("QUARTILE.EXC", 2, 2, quartile_exc_fn, CAT, "QUARTILE.EXC(array, quart)", "Returns the quartile of a data set, based on percentile values 0..1 exclusive.");
    r.raw("QUARTILE.INC", 2, 2, quartile_inc_fn, CAT, "QUARTILE.INC(array, quart)", "Returns the quartile of a data set, based on percentile values 0..1 inclusive.");
    r.raw("QUARTILE", 2, 2, quartile_inc_fn, CAT, "QUARTILE(array, quart)", "Returns the quartile of a data set (compatibility).");
    r.raw("RANK.AVG", 2, 3, rank_avg, CAT, "RANK.AVG(number, ref, [order])", "Returns the rank of a number in a list; ties get the average rank.");
    r.raw("RANK.EQ", 2, 3, rank_eq, CAT, "RANK.EQ(number, ref, [order])", "Returns the rank of a number in a list; ties get the top rank.");
    r.raw("RANK", 2, 3, rank_eq, CAT, "RANK(number, ref, [order])", "Returns the rank of a number in a list (compatibility).");
    r.raw("SKEW", 1, VAR, skew, CAT, "SKEW(number1, [number2], ...)", "Returns the skewness of a distribution.");
    r.raw("SKEW.P", 1, VAR, skew_p, CAT, "SKEW.P(number1, [number2], ...)", "Returns the skewness of a distribution based on a population.");
    r.raw("SMALL", 2, 2, small_fn, CAT, "SMALL(array, k)", "Returns the k-th smallest value in a data set.");
    r.raw("STDEV.P", 1, VAR, stdev_p, CAT, "STDEV.P(number1, [number2], ...)", "Calculates standard deviation based on the entire population.");
    r.raw("STDEV.S", 1, VAR, stdev_s, CAT, "STDEV.S(number1, [number2], ...)", "Estimates standard deviation based on a sample.");
    r.raw("STDEV", 1, VAR, stdev_s, CAT, "STDEV(number1, [number2], ...)", "Estimates standard deviation based on a sample (compatibility).");
    r.raw("STDEVP", 1, VAR, stdev_p, CAT, "STDEVP(number1, [number2], ...)", "Calculates standard deviation based on the entire population (compatibility).");
    r.raw("STDEVA", 1, VAR, stdeva, CAT, "STDEVA(value1, [value2], ...)", "Estimates standard deviation based on a sample, including text and logical values.");
    r.raw("STDEVPA", 1, VAR, stdevpa, CAT, "STDEVPA(value1, [value2], ...)", "Calculates standard deviation based on the entire population, including text and logical values.");
    r.raw("TRIMMEAN", 2, 2, trimmean, CAT, "TRIMMEAN(array, percent)", "Returns the mean of the interior of a data set.");
    r.raw("VAR.P", 1, VAR, var_p, CAT, "VAR.P(number1, [number2], ...)", "Calculates variance based on the entire population.");
    r.raw("VAR.S", 1, VAR, var_s, CAT, "VAR.S(number1, [number2], ...)", "Estimates variance based on a sample.");
    r.raw("VAR", 1, VAR, var_s, CAT, "VAR(number1, [number2], ...)", "Estimates variance based on a sample (compatibility).");
    r.raw("VARP", 1, VAR, var_p, CAT, "VARP(number1, [number2], ...)", "Calculates variance based on the entire population (compatibility).");
    r.raw("VARA", 1, VAR, vara, CAT, "VARA(value1, [value2], ...)", "Estimates variance based on a sample, including text and logical values.");
    r.raw("VARPA", 1, VAR, varpa, CAT, "VARPA(value1, [value2], ...)", "Calculates variance based on the entire population, including text and logical values.");
    r.raw("FREQUENCY", 2, 2, frequency, CAT, "FREQUENCY(data_array, bins_array)", "Calculates how often values occur within ranges of values, returning a vertical array.");

    // ----- two-variable statistics -----
    r.raw("CORREL", 2, 2, correl, CAT, "CORREL(array1, array2)", "Returns the correlation coefficient between two data sets.");
    r.raw("PEARSON", 2, 2, correl, CAT, "PEARSON(array1, array2)", "Returns the Pearson product moment correlation coefficient.");
    r.raw("RSQ", 2, 2, rsq, CAT, "RSQ(known_ys, known_xs)", "Returns the square of the Pearson product moment correlation coefficient.");
    r.raw("COVARIANCE.P", 2, 2, covar_p, CAT, "COVARIANCE.P(array1, array2)", "Returns population covariance, the average of the products of paired deviations.");
    r.raw("COVARIANCE.S", 2, 2, covar_s, CAT, "COVARIANCE.S(array1, array2)", "Returns the sample covariance, the average of the products of paired deviations.");
    r.raw("COVAR", 2, 2, covar_p, CAT, "COVAR(array1, array2)", "Returns covariance, the average of the products of paired deviations (compatibility).");
    r.raw("SLOPE", 2, 2, slope, CAT, "SLOPE(known_ys, known_xs)", "Returns the slope of the linear regression line.");
    r.raw("INTERCEPT", 2, 2, intercept, CAT, "INTERCEPT(known_ys, known_xs)", "Returns the intercept of the linear regression line.");
    r.raw("STEYX", 2, 2, steyx, CAT, "STEYX(known_ys, known_xs)", "Returns the standard error of the predicted y-value for each x in the regression.");
    r.raw("FORECAST", 3, 3, forecast, CAT, "FORECAST(x, known_ys, known_xs)", "Calculates a future value along a linear trend (compatibility).");
    r.raw("FORECAST.LINEAR", 3, 3, forecast, CAT, "FORECAST.LINEAR(x, known_ys, known_xs)", "Calculates a future value along a linear trend.");
    r.raw("LINEST", 1, 4, linest, CAT, "LINEST(known_ys, [known_xs], [const], [stats])", "Returns the parameters of a linear trend.");
    r.raw("LOGEST", 1, 4, logest, CAT, "LOGEST(known_ys, [known_xs], [const], [stats])", "Returns the parameters of an exponential trend.");
    r.raw("TREND", 1, 4, trend, CAT, "TREND(known_ys, [known_xs], [new_xs], [const])", "Returns values along a linear trend.");
    r.raw("GROWTH", 1, 4, growth, CAT, "GROWTH(known_ys, [known_xs], [new_xs], [const])", "Returns values along an exponential trend.");
    r.raw("PROB", 3, 4, prob, CAT, "PROB(x_range, prob_range, lower_limit, [upper_limit])", "Returns the probability that values in a range are between two limits.");

    // ----- hypothesis tests -----
    r.raw("CHISQ.TEST", 2, 2, chisq_test, CAT, "CHISQ.TEST(actual_range, expected_range)", "Returns the test for independence.");
    r.raw("CHITEST", 2, 2, chisq_test, CAT, "CHITEST(actual_range, expected_range)", "Returns the test for independence (compatibility).");
    r.raw("F.TEST", 2, 2, f_test, CAT, "F.TEST(array1, array2)", "Returns the result of an F-test.");
    r.raw("FTEST", 2, 2, f_test, CAT, "FTEST(array1, array2)", "Returns the result of an F-test (compatibility).");
    r.raw("T.TEST", 4, 4, t_test, CAT, "T.TEST(array1, array2, tails, type)", "Returns the probability associated with a Student's t-test.");
    r.raw("TTEST", 4, 4, t_test, CAT, "TTEST(array1, array2, tails, type)", "Returns the probability associated with a Student's t-test (compatibility).");
    r.raw("Z.TEST", 2, 3, z_test, CAT, "Z.TEST(array, x, [sigma])", "Returns the one-tailed P-value of a z-test.");
    r.raw("ZTEST", 2, 3, z_test, CAT, "ZTEST(array, x, [sigma])", "Returns the one-tailed P-value of a z-test (compatibility).");

    // ----- distributions -----
    r.scalar("BETA.DIST", 4, 6, beta_dist, CAT, "BETA.DIST(x, alpha, beta, cumulative, [A], [B])", "Returns the beta distribution.");
    r.scalar("BETADIST", 3, 5, betadist, CAT, "BETADIST(x, alpha, beta, [A], [B])", "Returns the cumulative beta distribution (compatibility).");
    r.scalar("BETA.INV", 3, 5, beta_inv, CAT, "BETA.INV(probability, alpha, beta, [A], [B])", "Returns the inverse of the cumulative beta distribution.");
    r.scalar("BETAINV", 3, 5, beta_inv, CAT, "BETAINV(probability, alpha, beta, [A], [B])", "Returns the inverse of the cumulative beta distribution (compatibility).");
    r.scalar("BINOM.DIST", 4, 4, binom_dist, CAT, "BINOM.DIST(number_s, trials, probability_s, cumulative)", "Returns the individual term binomial distribution probability.");
    r.scalar("BINOMDIST", 4, 4, binom_dist, CAT, "BINOMDIST(number_s, trials, probability_s, cumulative)", "Returns the individual term binomial distribution probability (compatibility).");
    r.scalar("BINOM.DIST.RANGE", 3, 4, binom_dist_range, CAT, "BINOM.DIST.RANGE(trials, probability_s, number_s, [number_s2])", "Returns the probability of a trial result using a binomial distribution.");
    r.scalar("BINOM.INV", 3, 3, binom_inv, CAT, "BINOM.INV(trials, probability_s, alpha)", "Returns the smallest value for which the cumulative binomial distribution is >= a criterion value.");
    r.scalar("CRITBINOM", 3, 3, binom_inv, CAT, "CRITBINOM(trials, probability_s, alpha)", "Returns the smallest value for which the cumulative binomial distribution is >= a criterion value (compatibility).");
    r.scalar("CHISQ.DIST", 3, 3, chisq_dist, CAT, "CHISQ.DIST(x, deg_freedom, cumulative)", "Returns the chi-squared distribution.");
    r.scalar("CHISQ.DIST.RT", 2, 2, chisq_dist_rt, CAT, "CHISQ.DIST.RT(x, deg_freedom)", "Returns the right-tailed probability of the chi-squared distribution.");
    r.scalar("CHIDIST", 2, 2, chisq_dist_rt, CAT, "CHIDIST(x, deg_freedom)", "Returns the right-tailed probability of the chi-squared distribution (compatibility).");
    r.scalar("CHISQ.INV", 2, 2, chisq_inv, CAT, "CHISQ.INV(probability, deg_freedom)", "Returns the inverse of the left-tailed probability of the chi-squared distribution.");
    r.scalar("CHISQ.INV.RT", 2, 2, chisq_inv_rt, CAT, "CHISQ.INV.RT(probability, deg_freedom)", "Returns the inverse of the right-tailed probability of the chi-squared distribution.");
    r.scalar("CHIINV", 2, 2, chisq_inv_rt, CAT, "CHIINV(probability, deg_freedom)", "Returns the inverse of the right-tailed probability of the chi-squared distribution (compatibility).");
    r.scalar("CONFIDENCE.NORM", 3, 3, confidence_norm, CAT, "CONFIDENCE.NORM(alpha, standard_dev, size)", "Returns the confidence interval for a population mean, using a normal distribution.");
    r.scalar("CONFIDENCE", 3, 3, confidence_norm, CAT, "CONFIDENCE(alpha, standard_dev, size)", "Returns the confidence interval for a population mean (compatibility).");
    r.scalar("CONFIDENCE.T", 3, 3, confidence_t, CAT, "CONFIDENCE.T(alpha, standard_dev, size)", "Returns the confidence interval for a population mean, using a Student's t distribution.");
    r.scalar("EXPON.DIST", 3, 3, expon_dist, CAT, "EXPON.DIST(x, lambda, cumulative)", "Returns the exponential distribution.");
    r.scalar("EXPONDIST", 3, 3, expon_dist, CAT, "EXPONDIST(x, lambda, cumulative)", "Returns the exponential distribution (compatibility).");
    r.scalar("F.DIST", 4, 4, f_dist, CAT, "F.DIST(x, deg_freedom1, deg_freedom2, cumulative)", "Returns the F probability distribution.");
    r.scalar("F.DIST.RT", 3, 3, f_dist_rt, CAT, "F.DIST.RT(x, deg_freedom1, deg_freedom2)", "Returns the right-tailed F probability distribution.");
    r.scalar("FDIST", 3, 3, f_dist_rt, CAT, "FDIST(x, deg_freedom1, deg_freedom2)", "Returns the right-tailed F probability distribution (compatibility).");
    r.scalar("F.INV", 3, 3, f_inv, CAT, "F.INV(probability, deg_freedom1, deg_freedom2)", "Returns the inverse of the F probability distribution.");
    r.scalar("F.INV.RT", 3, 3, f_inv_rt, CAT, "F.INV.RT(probability, deg_freedom1, deg_freedom2)", "Returns the inverse of the right-tailed F probability distribution.");
    r.scalar("FINV", 3, 3, f_inv_rt, CAT, "FINV(probability, deg_freedom1, deg_freedom2)", "Returns the inverse of the right-tailed F probability distribution (compatibility).");
    r.scalar("FISHER", 1, 1, fisher, CAT, "FISHER(x)", "Returns the Fisher transformation.");
    r.scalar("FISHERINV", 1, 1, fisherinv, CAT, "FISHERINV(y)", "Returns the inverse of the Fisher transformation.");
    r.scalar("GAMMA", 1, 1, gamma_fn, CAT, "GAMMA(x)", "Returns the Gamma function value.");
    r.scalar("GAMMA.DIST", 4, 4, gamma_dist, CAT, "GAMMA.DIST(x, alpha, beta, cumulative)", "Returns the gamma distribution.");
    r.scalar("GAMMADIST", 4, 4, gamma_dist, CAT, "GAMMADIST(x, alpha, beta, cumulative)", "Returns the gamma distribution (compatibility).");
    r.scalar("GAMMA.INV", 3, 3, gamma_inv, CAT, "GAMMA.INV(probability, alpha, beta)", "Returns the inverse of the gamma cumulative distribution.");
    r.scalar("GAMMAINV", 3, 3, gamma_inv, CAT, "GAMMAINV(probability, alpha, beta)", "Returns the inverse of the gamma cumulative distribution (compatibility).");
    r.scalar("GAMMALN", 1, 1, gammaln, CAT, "GAMMALN(x)", "Returns the natural logarithm of the gamma function.");
    r.scalar("GAMMALN.PRECISE", 1, 1, gammaln, CAT, "GAMMALN.PRECISE(x)", "Returns the natural logarithm of the gamma function.");
    r.scalar("GAUSS", 1, 1, gauss, CAT, "GAUSS(z)", "Returns 0.5 less than the standard normal cumulative distribution.");
    r.scalar("HYPGEOM.DIST", 5, 5, hypgeom_dist, CAT, "HYPGEOM.DIST(sample_s, number_sample, population_s, number_pop, cumulative)", "Returns the hypergeometric distribution.");
    r.scalar("HYPGEOMDIST", 4, 4, hypgeomdist, CAT, "HYPGEOMDIST(sample_s, number_sample, population_s, number_pop)", "Returns the hypergeometric distribution (compatibility).");
    r.scalar("LOGNORM.DIST", 4, 4, lognorm_dist, CAT, "LOGNORM.DIST(x, mean, standard_dev, cumulative)", "Returns the lognormal distribution.");
    r.scalar("LOGNORMDIST", 3, 3, lognormdist, CAT, "LOGNORMDIST(x, mean, standard_dev)", "Returns the cumulative lognormal distribution (compatibility).");
    r.scalar("LOGNORM.INV", 3, 3, lognorm_inv, CAT, "LOGNORM.INV(probability, mean, standard_dev)", "Returns the inverse of the lognormal cumulative distribution.");
    r.scalar("LOGINV", 3, 3, lognorm_inv, CAT, "LOGINV(probability, mean, standard_dev)", "Returns the inverse of the lognormal cumulative distribution (compatibility).");
    r.scalar("NEGBINOM.DIST", 4, 4, negbinom_dist, CAT, "NEGBINOM.DIST(number_f, number_s, probability_s, cumulative)", "Returns the negative binomial distribution.");
    r.scalar("NEGBINOMDIST", 3, 3, negbinomdist, CAT, "NEGBINOMDIST(number_f, number_s, probability_s)", "Returns the negative binomial distribution (compatibility).");
    r.scalar("NORM.DIST", 4, 4, norm_dist, CAT, "NORM.DIST(x, mean, standard_dev, cumulative)", "Returns the normal distribution.");
    r.scalar("NORMDIST", 4, 4, norm_dist, CAT, "NORMDIST(x, mean, standard_dev, cumulative)", "Returns the normal distribution (compatibility).");
    r.scalar("NORM.INV", 3, 3, norm_inv_fn, CAT, "NORM.INV(probability, mean, standard_dev)", "Returns the inverse of the normal cumulative distribution.");
    r.scalar("NORMINV", 3, 3, norm_inv_fn, CAT, "NORMINV(probability, mean, standard_dev)", "Returns the inverse of the normal cumulative distribution (compatibility).");
    r.scalar("NORM.S.DIST", 2, 2, norm_s_dist, CAT, "NORM.S.DIST(z, cumulative)", "Returns the standard normal distribution.");
    r.scalar("NORMSDIST", 1, 1, normsdist, CAT, "NORMSDIST(z)", "Returns the standard normal cumulative distribution (compatibility).");
    r.scalar("NORM.S.INV", 1, 1, norm_s_inv, CAT, "NORM.S.INV(probability)", "Returns the inverse of the standard normal cumulative distribution.");
    r.scalar("NORMSINV", 1, 1, norm_s_inv, CAT, "NORMSINV(probability)", "Returns the inverse of the standard normal cumulative distribution (compatibility).");
    r.scalar("PERMUT", 2, 2, permut, CAT, "PERMUT(number, number_chosen)", "Returns the number of permutations for a given number of objects.");
    r.scalar("PERMUTATIONA", 2, 2, permutationa, CAT, "PERMUTATIONA(number, number_chosen)", "Returns the number of permutations (with repetitions) for a given number of objects.");
    r.scalar("PHI", 1, 1, phi, CAT, "PHI(x)", "Returns the value of the density function for a standard normal distribution.");
    r.scalar("POISSON.DIST", 3, 3, poisson_dist, CAT, "POISSON.DIST(x, mean, cumulative)", "Returns the Poisson distribution.");
    r.scalar("POISSON", 3, 3, poisson_dist, CAT, "POISSON(x, mean, cumulative)", "Returns the Poisson distribution (compatibility).");
    r.scalar("STANDARDIZE", 3, 3, standardize, CAT, "STANDARDIZE(x, mean, standard_dev)", "Returns a normalized value.");
    r.scalar("T.DIST", 3, 3, t_dist, CAT, "T.DIST(x, deg_freedom, cumulative)", "Returns the left-tailed Student's t-distribution.");
    r.scalar("T.DIST.2T", 2, 2, t_dist_2t, CAT, "T.DIST.2T(x, deg_freedom)", "Returns the two-tailed Student's t-distribution.");
    r.scalar("T.DIST.RT", 2, 2, t_dist_rt, CAT, "T.DIST.RT(x, deg_freedom)", "Returns the right-tailed Student's t-distribution.");
    r.scalar("TDIST", 3, 3, tdist, CAT, "TDIST(x, deg_freedom, tails)", "Returns the Student's t-distribution (compatibility).");
    r.scalar("T.INV", 2, 2, t_inv, CAT, "T.INV(probability, deg_freedom)", "Returns the left-tailed inverse of the Student's t-distribution.");
    r.scalar("T.INV.2T", 2, 2, t_inv_2t, CAT, "T.INV.2T(probability, deg_freedom)", "Returns the two-tailed inverse of the Student's t-distribution.");
    r.scalar("TINV", 2, 2, t_inv_2t, CAT, "TINV(probability, deg_freedom)", "Returns the two-tailed inverse of the Student's t-distribution (compatibility).");
    r.scalar("WEIBULL.DIST", 4, 4, weibull_dist, CAT, "WEIBULL.DIST(x, alpha, beta, cumulative)", "Returns the Weibull distribution.");
    r.scalar("WEIBULL", 4, 4, weibull_dist, CAT, "WEIBULL(x, alpha, beta, cumulative)", "Returns the Weibull distribution (compatibility).");
}

// ===========================================================================
// Descriptive statistics
// ===========================================================================

fn avedev(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = ctx.collect_numbers(args)?;
        if v.is_empty() {
            return Err(ErrorKind::Num);
        }
        let m = mean(&v)?;
        Ok(v.iter().map(|x| (x - m).abs()).sum::<f64>() / v.len() as f64)
    })())
}

fn average(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| mean(&v)))
}

fn averagea(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).and_then(|v| mean(&v)))
}

fn averageif(ctx: &mut Ctx, args: &[Expr]) -> Value {
    if_single(ctx, args, agg_avg)
}

fn averageifs(ctx: &mut Ctx, args: &[Expr]) -> Value {
    if_multi(ctx, args, true, agg_avg)
}

fn count(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let mut n = 0usize;
    let r = ctx.for_each_value(args, &mut |v, from_ref| {
        match v {
            Value::Number(_) => n += 1,
            Value::Bool(_) if !from_ref => n += 1,
            Value::Text(_) if !from_ref && v.to_number().is_ok() => n += 1,
            _ => {}
        }
        Ok(())
    });
    match r {
        Ok(()) => Value::Number(n as f64),
        Err(e) => Value::Error(e),
    }
}

fn counta(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let mut n = 0usize;
    for a in args {
        if matches!(a, Expr::Missing) {
            n += 1;
            continue;
        }
        match ctx.eval(a) {
            Val::V(Value::Array(arr)) => n += arr.data.iter().filter(|v| !v.is_empty()).count(),
            Val::V(_) => n += 1,
            Val::Refs(areas) => {
                for area in areas {
                    n += ctx.area_values(area).len();
                }
            }
        }
    }
    Value::Number(n as f64)
}

fn countblank(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let r = match range_arg(ctx, &args[0]) {
        Ok(r) => r,
        Err(e) => return Value::Error(e),
    };
    // Count over the full (unclipped) area for references.
    let mut blanks = r.arr.data.iter().filter(|v| v.is_empty() || matches!(v, Value::Text(t) if t.is_empty())).count() as f64;
    if let Some(area) = r.area {
        let full = area.rect.cell_count() as f64;
        blanks += full - r.arr.data.len() as f64;
    }
    Value::Number(blanks)
}

fn countif(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let range = match range_arg(ctx, &args[0]) {
        Ok(r) => r,
        Err(e) => return Value::Error(e),
    };
    ifs_eval(ctx, None, vec![range.arr], &[&args[1]], agg_count)
}

fn countifs(ctx: &mut Ctx, args: &[Expr]) -> Value {
    if_multi(ctx, args, false, agg_count)
}

fn devsq_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| devsq(&v)))
}

fn geomean(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = ctx.collect_numbers(args)?;
        if v.is_empty() || v.iter().any(|x| *x <= 0.0) {
            return Err(ErrorKind::Num);
        }
        Ok((v.iter().map(|x| x.ln()).sum::<f64>() / v.len() as f64).exp())
    })())
}

fn harmean(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = ctx.collect_numbers(args)?;
        if v.is_empty() || v.iter().any(|x| *x <= 0.0) {
            return Err(ErrorKind::Num);
        }
        Ok(v.len() as f64 / v.iter().map(|x| 1.0 / x).sum::<f64>())
    })())
}

fn kurt(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = ctx.collect_numbers(args)?;
        let n = v.len() as f64;
        if v.len() < 4 {
            return Err(ErrorKind::Div0);
        }
        let m = mean(&v)?;
        let s = stdev(&v, true)?;
        if s == 0.0 {
            return Err(ErrorKind::Div0);
        }
        let sum4: f64 = v.iter().map(|x| ((x - m) / s).powi(4)).sum();
        Ok(n * (n + 1.0) / ((n - 1.0) * (n - 2.0) * (n - 3.0)) * sum4 - 3.0 * (n - 1.0) * (n - 1.0) / ((n - 2.0) * (n - 3.0)))
    })())
}

fn skew(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = ctx.collect_numbers(args)?;
        let n = v.len() as f64;
        if v.len() < 3 {
            return Err(ErrorKind::Div0);
        }
        let m = mean(&v)?;
        let s = stdev(&v, true)?;
        if s == 0.0 {
            return Err(ErrorKind::Div0);
        }
        let sum3: f64 = v.iter().map(|x| ((x - m) / s).powi(3)).sum();
        Ok(n / ((n - 1.0) * (n - 2.0)) * sum3)
    })())
}

fn skew_p(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = ctx.collect_numbers(args)?;
        if v.is_empty() {
            return Err(ErrorKind::Div0);
        }
        let m = mean(&v)?;
        let s = stdev(&v, false)?;
        if s == 0.0 {
            return Err(ErrorKind::Div0);
        }
        Ok(v.iter().map(|x| ((x - m) / s).powi(3)).sum::<f64>() / v.len() as f64)
    })())
}

fn large_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match nums(ctx, &args[0]) {
        Ok(v) => map_k(ctx, &args[1], &|k| large(&v, k)),
        Err(e) => Value::Error(e),
    }
}

fn small_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match nums(ctx, &args[0]) {
        Ok(v) => map_k(ctx, &args[1], &|k| small(&v, k)),
        Err(e) => Value::Error(e),
    }
}

fn fold_max(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }
}

fn fold_min(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().cloned().fold(f64::INFINITY, f64::min)
    }
}

fn max_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).map(|v| fold_max(&v)))
}

fn maxa(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).map(|v| fold_max(&v)))
}

fn min_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).map(|v| fold_min(&v)))
}

fn mina(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).map(|v| fold_min(&v)))
}

fn maxifs(ctx: &mut Ctx, args: &[Expr]) -> Value {
    if_multi(ctx, args, true, agg_max)
}

fn minifs(ctx: &mut Ctx, args: &[Expr]) -> Value {
    if_multi(ctx, args, true, agg_min)
}

fn median_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| median(&v)))
}

fn mode_mult(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match ctx.collect_numbers(args) {
        Ok(v) => {
            let m = modes(&v);
            if m.is_empty() {
                Value::Error(ErrorKind::NA)
            } else {
                Value::from(Array::column(m.into_iter().map(Value::Number).collect()))
            }
        }
        Err(e) => Value::Error(e),
    }
}

fn mode_sngl_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| mode_sngl(&v)))
}

fn percentile_exc_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match nums(ctx, &args[0]) {
        Ok(v) => map_k(ctx, &args[1], &|k| percentile_exc(&v, k)),
        Err(e) => Value::Error(e),
    }
}

fn percentile_inc_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match nums(ctx, &args[0]) {
        Ok(v) => map_k(ctx, &args[1], &|k| percentile_inc(&v, k)),
        Err(e) => Value::Error(e),
    }
}

fn quartile_exc_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match nums(ctx, &args[0]) {
        Ok(v) => map_k(ctx, &args[1], &|q| quartile_exc(&v, q)),
        Err(e) => Value::Error(e),
    }
}

fn quartile_inc_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match nums(ctx, &args[0]) {
        Ok(v) => map_k(ctx, &args[1], &|q| quartile_inc(&v, q)),
        Err(e) => Value::Error(e),
    }
}

/// Round to 15 significant digits (Excel's precision) to avoid binary noise.
pub(crate) fn round15(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{:.14e}", x).parse().unwrap_or(x)
}

fn percentrank(ctx: &mut Ctx, args: &[Expr], exc: bool) -> Value {
    res((|| {
        let v = nums(ctx, &args[0])?;
        let x = ctx.eval_number(&args[1])?;
        let sig = ctx.opt_number(args, 2, 3.0)?.trunc();
        if sig < 1.0 || v.is_empty() {
            return Err(ErrorKind::Num);
        }
        let s = sorted(&v);
        let n = s.len();
        if x < s[0] || x > s[n - 1] {
            return Err(ErrorKind::NA);
        }
        let less = s.iter().filter(|y| **y < x).count();
        let exact = s.contains(&x);
        let r = if exc {
            if exact {
                (less + 1) as f64 / (n + 1) as f64
            } else {
                let lo = s[less - 1];
                let hi = s[less];
                (less as f64 + (x - lo) / (hi - lo)) / (n + 1) as f64
            }
        } else if n == 1 {
            1.0
        } else if exact {
            less as f64 / (n - 1) as f64
        } else {
            let lo = s[less - 1];
            let hi = s[less];
            ((less - 1) as f64 + (x - lo) / (hi - lo)) / (n - 1) as f64
        };
        let f = 10f64.powi(sig as i32);
        Ok(round15(r * f).floor() / f)
    })())
}

fn percentrank_exc(ctx: &mut Ctx, args: &[Expr]) -> Value {
    percentrank(ctx, args, true)
}

fn percentrank_inc(ctx: &mut Ctx, args: &[Expr]) -> Value {
    percentrank(ctx, args, false)
}

fn rank(ctx: &mut Ctx, args: &[Expr], avg: bool) -> Value {
    res((|| {
        let x = ctx.eval_number(&args[0])?;
        let v = nums(ctx, &args[1])?;
        let asc = ctx.opt_number(args, 2, 0.0)? != 0.0;
        let eq = v.iter().filter(|y| **y == x).count();
        if eq == 0 {
            return Err(ErrorKind::NA);
        }
        let better = v.iter().filter(|y| if asc { **y < x } else { **y > x }).count();
        let r = better as f64 + 1.0;
        Ok(if avg { r + (eq as f64 - 1.0) / 2.0 } else { r })
    })())
}

fn rank_avg(ctx: &mut Ctx, args: &[Expr]) -> Value {
    rank(ctx, args, true)
}

fn rank_eq(ctx: &mut Ctx, args: &[Expr]) -> Value {
    rank(ctx, args, false)
}

fn stdev_p(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| stdev(&v, false)))
}
fn stdev_s(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| stdev(&v, true)))
}
fn stdeva(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).and_then(|v| stdev(&v, true)))
}
fn stdevpa(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).and_then(|v| stdev(&v, false)))
}
fn var_p(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| variance(&v, false)))
}
fn var_s(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers(args).and_then(|v| variance(&v, true)))
}
fn vara(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).and_then(|v| variance(&v, true)))
}
fn varpa(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(ctx.collect_numbers_a(args).and_then(|v| variance(&v, false)))
}

fn trimmean(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = nums(ctx, &args[0])?;
        let p = ctx.eval_number(&args[1])?;
        if !(0.0..1.0).contains(&p) || v.is_empty() {
            return Err(ErrorKind::Num);
        }
        let k = (v.len() as f64 * p / 2.0).floor() as usize;
        let s = sorted(&v);
        mean(&s[k..s.len() - k])
    })())
}

fn frequency(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let data = match nums(ctx, &args[0]) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let bins = match nums(ctx, &args[1]) {
        Ok(v) => v,
        Err(e) => return Value::Error(e),
    };
    let mut order: Vec<usize> = (0..bins.len()).collect();
    order.sort_by(|a, b| bins[*a].partial_cmp(&bins[*b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut counts = vec![0.0; bins.len() + 1];
    for x in data {
        match order.iter().find(|i| x <= bins[**i]) {
            Some(i) => counts[*i] += 1.0,
            None => counts[bins.len()] += 1.0,
        }
    }
    Value::from(Array::column(counts.into_iter().map(Value::Number).collect()))
}

// ===========================================================================
// Two-variable statistics and regression
// ===========================================================================

struct Moments {
    n: f64,
    sx: f64,
    sy: f64,
    sxy: f64,
    mx: f64,
    my: f64,
}

fn moments(x: &[f64], y: &[f64]) -> Moments {
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let mut m = Moments { n, sx: 0.0, sy: 0.0, sxy: 0.0, mx, my };
    for (a, b) in x.iter().zip(y) {
        m.sx += (a - mx) * (a - mx);
        m.sy += (b - my) * (b - my);
        m.sxy += (a - mx) * (b - my);
    }
    m
}

fn correl(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let (x, y) = paired(ctx, &args[0], &args[1])?;
        if x.len() < 2 {
            return Err(ErrorKind::Div0);
        }
        let m = moments(&x, &y);
        if m.sx == 0.0 || m.sy == 0.0 {
            return Err(ErrorKind::Div0);
        }
        Ok(m.sxy / (m.sx * m.sy).sqrt())
    })())
}

fn rsq(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let (y, x) = paired(ctx, &args[0], &args[1])?;
        if x.len() < 2 {
            return Err(ErrorKind::Div0);
        }
        let m = moments(&x, &y);
        if m.sx == 0.0 || m.sy == 0.0 {
            return Err(ErrorKind::Div0);
        }
        Ok(m.sxy * m.sxy / (m.sx * m.sy))
    })())
}

fn covar(ctx: &mut Ctx, args: &[Expr], sample: bool) -> Value {
    res((|| {
        let (x, y) = paired(ctx, &args[0], &args[1])?;
        if x.is_empty() || (sample && x.len() < 2) {
            return Err(ErrorKind::Div0);
        }
        let m = moments(&x, &y);
        Ok(m.sxy / if sample { m.n - 1.0 } else { m.n })
    })())
}

fn covar_p(ctx: &mut Ctx, args: &[Expr]) -> Value {
    covar(ctx, args, false)
}
fn covar_s(ctx: &mut Ctx, args: &[Expr]) -> Value {
    covar(ctx, args, true)
}

fn slope_intercept(ctx: &mut Ctx, ys: &Expr, xs: &Expr) -> R<(f64, f64, Moments)> {
    let (y, x) = paired(ctx, ys, xs)?;
    if x.is_empty() {
        return Err(ErrorKind::Div0);
    }
    let m = moments(&x, &y);
    if m.sx == 0.0 {
        return Err(ErrorKind::Div0);
    }
    let b = m.sxy / m.sx;
    Ok((b, m.my - b * m.mx, m))
}

fn slope(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(slope_intercept(ctx, &args[0], &args[1]).map(|r| r.0))
}

fn intercept(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res(slope_intercept(ctx, &args[0], &args[1]).map(|r| r.1))
}

fn steyx(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let (y, x) = paired(ctx, &args[0], &args[1])?;
        if x.len() < 3 {
            return Err(ErrorKind::Div0);
        }
        let m = moments(&x, &y);
        if m.sx == 0.0 {
            return Err(ErrorKind::Div0);
        }
        Ok(((m.sy - m.sxy * m.sxy / m.sx) / (m.n - 2.0)).max(0.0).sqrt())
    })())
}

fn forecast(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let x = match ctx.eval_value(&args[0]) {
        Value::Error(e) => return Value::Error(e),
        v => v,
    };
    match slope_intercept(ctx, &args[1], &args[2]) {
        Ok((b, a, _)) => crate::eval::map_numbers(x, |x| a + b * x),
        Err(e) => Value::Error(e),
    }
}

/// Result of a least-squares fit.
struct Fit {
    /// Coefficients for each x variable (0 for dropped collinear columns).
    coef: Vec<f64>,
    se: Vec<f64>,
    b: f64,
    se_b: Option<f64>,
    r2: f64,
    sey: f64,
    f: f64,
    df: f64,
    ssreg: f64,
    ssresid: f64,
}

/// Ordinary least squares with Householder QR. `xs` holds one column per variable.
fn ols(y: &[f64], xs: &[Vec<f64>], konst: bool) -> R<Fit> {
    let n = y.len();
    let k = xs.len();
    let ym = if konst { y.iter().sum::<f64>() / n as f64 } else { 0.0 };
    let xm: Vec<f64> = xs.iter().map(|c| if konst { c.iter().sum::<f64>() / n as f64 } else { 0.0 }).collect();
    // column-major centred design matrix
    let mut a: Vec<Vec<f64>> = xs.iter().zip(&xm).map(|(c, m)| c.iter().map(|v| v - m).collect()).collect();
    let mut qty: Vec<f64> = y.iter().map(|v| v - ym).collect();
    let mut kept: Vec<usize> = Vec::new();
    let mut rmat: Vec<Vec<f64>> = Vec::new(); // rows of R restricted to kept columns (filled later)
    let mut row = 0usize;
    let orig_norm: Vec<f64> = a.iter().map(|c| c.iter().map(|v| v * v).sum::<f64>().sqrt()).collect();
    let mut full_r: Vec<Vec<f64>> = Vec::new(); // full_r[r][j] for all j
    for j in 0..k {
        if row >= n {
            break;
        }
        let norm = a[j][row..].iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm <= 1e-11 * orig_norm[j].max(f64::MIN_POSITIVE) || norm == 0.0 {
            continue;
        }
        let alpha = if a[j][row] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = a[j][row..].to_vec();
        v[0] -= alpha;
        let vnorm2: f64 = v.iter().map(|x| x * x).sum();
        if vnorm2 > 0.0 {
            for col in a.iter_mut().skip(j) {
                let dot: f64 = v.iter().zip(&col[row..]).map(|(p, q)| p * q).sum();
                let f = 2.0 * dot / vnorm2;
                for (i, vi) in v.iter().enumerate() {
                    col[row + i] -= f * vi;
                }
            }
            let dot: f64 = v.iter().zip(&qty[row..]).map(|(p, q)| p * q).sum();
            let f = 2.0 * dot / vnorm2;
            for (i, vi) in v.iter().enumerate() {
                qty[row + i] -= f * vi;
            }
        }
        full_r.push((0..k).map(|c| a[c][row]).collect());
        kept.push(j);
        row += 1;
    }
    let p = kept.len();
    for fr in full_r.iter().take(p) {
        rmat.push(kept.iter().map(|&c| fr[c]).collect());
    }
    // back substitution
    let mut beta = vec![0.0; p];
    for i in (0..p).rev() {
        let mut s = qty[i];
        for j in i + 1..p {
            s -= rmat[i][j] * beta[j];
        }
        beta[i] = s / rmat[i][i];
    }
    // R^-1
    let mut rinv = vec![vec![0.0; p]; p];
    for i in 0..p {
        rinv[i][i] = 1.0 / rmat[i][i];
        for j in (0..i).rev() {
            let mut s = 0.0;
            for m in j + 1..=i {
                s += rmat[j][m] * rinv[m][i];
            }
            rinv[j][i] = -s / rmat[j][j];
        }
    }
    // (X'X)^-1 = Rinv Rinv^T
    let mut xtx_inv = vec![vec![0.0; p]; p];
    for i in 0..p {
        for j in 0..p {
            xtx_inv[i][j] = (i.max(j)..p).map(|m| rinv[i][m] * rinv[j][m]).sum();
        }
    }
    let mut coef = vec![0.0; k];
    for (i, &c) in kept.iter().enumerate() {
        coef[c] = beta[i];
    }
    let b = if konst { ym - coef.iter().zip(&xm).map(|(c, m)| c * m).sum::<f64>() } else { 0.0 };
    let mut ssresid = 0.0;
    for i in 0..n {
        let fit: f64 = b + (0..k).map(|j| coef[j] * xs[j][i]).sum::<f64>();
        ssresid += (y[i] - fit) * (y[i] - fit);
    }
    let sstot: f64 = if konst { y.iter().map(|v| (v - ym) * (v - ym)).sum() } else { y.iter().map(|v| v * v).sum() };
    let ssreg = sstot - ssresid;
    let df = n as f64 - p as f64 - if konst { 1.0 } else { 0.0 };
    let sigma2 = ssresid / df;
    let mut se = vec![0.0; k];
    for (i, &c) in kept.iter().enumerate() {
        se[c] = (sigma2 * xtx_inv[i][i]).sqrt();
    }
    let se_b = if konst {
        let mut q = 1.0 / n as f64;
        for i in 0..p {
            for j in 0..p {
                q += xm[kept[i]] * xtx_inv[i][j] * xm[kept[j]];
            }
        }
        Some((sigma2 * q).sqrt())
    } else {
        None
    };
    let r2 = if sstot == 0.0 { 1.0 } else { ssreg / sstot };
    Ok(Fit {
        coef,
        se,
        b,
        se_b,
        r2,
        sey: sigma2.sqrt(),
        f: (ssreg / p as f64) / sigma2,
        df,
        ssreg,
        ssresid,
    })
}

/// Parsed regression inputs: y values, x variables (columns), and whether the
/// data is laid out in rows.
struct RegData {
    y: Vec<f64>,
    xs: Vec<Vec<f64>>,
    by_row: bool,
    yshape: (usize, usize),
}

fn reg_input(ctx: &mut Ctx, args: &[Expr], log: bool) -> R<RegData> {
    let (yr, yc, mut y) = numeric_matrix(ctx, &args[0])?;
    if log {
        if y.iter().any(|v| *v <= 0.0) {
            return Err(ErrorKind::Num);
        }
        y = y.iter().map(|v| v.ln()).collect();
    }
    let n = y.len();
    let by_row = yr == 1 && yc > 1;
    let xs = match args.get(1) {
        None | Some(Expr::Missing) => vec![(1..=n).map(|i| i as f64).collect()],
        Some(e) => {
            let (xr, xc, x) = numeric_matrix(ctx, e)?;
            if xr == yr && xc == yc {
                vec![x]
            } else if yc == 1 && xr == yr {
                (0..xc).map(|c| (0..xr).map(|r| x[r * xc + c]).collect()).collect()
            } else if yr == 1 && xc == yc {
                (0..xr).map(|r| x[r * xc..(r + 1) * xc].to_vec()).collect()
            } else {
                return Err(ErrorKind::Ref);
            }
        }
    };
    Ok(RegData { y, xs, by_row, yshape: (yr, yc) })
}

fn linest_impl(ctx: &mut Ctx, args: &[Expr], log: bool) -> Value {
    let r = (|| -> R<Value> {
        let d = reg_input(ctx, args, log)?;
        let konst = match args.get(2) {
            None | Some(Expr::Missing) => true,
            Some(e) => ctx.eval_bool(e)?,
        };
        let stats = match args.get(3) {
            None | Some(Expr::Missing) => false,
            Some(e) => ctx.eval_bool(e)?,
        };
        let _ = d.by_row;
        let fit = ols(&d.y, &d.xs, konst)?;
        let k = d.xs.len();
        let cols = k + 1;
        let tr = |x: f64| if log { x.exp() } else { x };
        let mut first: Vec<Value> = fit.coef.iter().rev().map(|c| Value::number(tr(*c))).collect();
        first.push(Value::number(if konst { tr(fit.b) } else if log { 1.0 } else { 0.0 }));
        if !stats {
            return Ok(Value::from(Array::row(first)));
        }
        let na = || Value::Error(ErrorKind::NA);
        let mut data = first;
        for s in fit.se.iter().rev() {
            data.push(Value::number(*s));
        }
        data.push(match fit.se_b {
            Some(s) => Value::number(s),
            None => na(),
        });
        let mut row3 = vec![Value::number(fit.r2), Value::number(fit.sey)];
        let mut row4 = vec![Value::number(fit.f), Value::number(fit.df)];
        let mut row5 = vec![Value::number(fit.ssreg), Value::number(fit.ssresid)];
        for row in [&mut row3, &mut row4, &mut row5] {
            while row.len() < cols {
                row.push(na());
            }
            row.truncate(cols.max(2));
        }
        let width = cols.max(2);
        let mut all = Vec::new();
        for chunk in data.chunks(cols) {
            let mut c = chunk.to_vec();
            while c.len() < width {
                c.push(na());
            }
            all.extend(c);
        }
        all.extend(row3);
        all.extend(row4);
        all.extend(row5);
        Ok(Value::from(Array::new(5, width, all)))
    })();
    r.unwrap_or_else(Value::Error)
}

fn linest(ctx: &mut Ctx, args: &[Expr]) -> Value {
    linest_impl(ctx, args, false)
}

fn logest(ctx: &mut Ctx, args: &[Expr]) -> Value {
    linest_impl(ctx, args, true)
}

fn trend_impl(ctx: &mut Ctx, args: &[Expr], log: bool) -> Value {
    let r = (|| -> R<Value> {
        let d = reg_input(ctx, args, log)?;
        let konst = match args.get(3) {
            None | Some(Expr::Missing) => true,
            Some(e) => ctx.eval_bool(e)?,
        };
        let fit = ols(&d.y, &d.xs, konst)?;
        let k = d.xs.len();
        let tr = |x: f64| if log { x.exp() } else { x };
        // new_x: default known_x (or 1..n)
        let (nr, nc, nx) = match args.get(2) {
            Some(e) if !matches!(e, Expr::Missing) => numeric_matrix(ctx, e)?,
            _ => match args.get(1) {
                Some(e) if !matches!(e, Expr::Missing) => numeric_matrix(ctx, e)?,
                _ => {
                    let (yr, yc) = d.yshape;
                    (yr, yc, (1..=d.y.len()).map(|i| i as f64).collect())
                }
            },
        };
        if k == 1 {
            let data = nx.iter().map(|x| Value::number(tr(fit.b + fit.coef[0] * x))).collect();
            return Ok(Value::from(Array::new(nr, nc, data)));
        }
        if d.by_row {
            if nr != k {
                return Err(ErrorKind::Ref);
            }
            let data = (0..nc)
                .map(|c| Value::number(tr(fit.b + (0..k).map(|j| fit.coef[j] * nx[j * nc + c]).sum::<f64>())))
                .collect();
            Ok(Value::from(Array::row(data)))
        } else {
            if nc != k {
                return Err(ErrorKind::Ref);
            }
            let data = (0..nr)
                .map(|r| Value::number(tr(fit.b + (0..k).map(|j| fit.coef[j] * nx[r * nc + j]).sum::<f64>())))
                .collect();
            Ok(Value::from(Array::column(data)))
        }
    })();
    r.unwrap_or_else(Value::Error)
}

fn trend(ctx: &mut Ctx, args: &[Expr]) -> Value {
    trend_impl(ctx, args, false)
}

fn growth(ctx: &mut Ctx, args: &[Expr]) -> Value {
    trend_impl(ctx, args, true)
}

fn prob(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let (x, p) = paired(ctx, &args[0], &args[1])?;
        let lo = ctx.eval_number(&args[2])?;
        let hi = match args.get(3) {
            None | Some(Expr::Missing) => lo,
            Some(e) => ctx.eval_number(e)?,
        };
        if p.iter().any(|q| *q <= 0.0 || *q > 1.0) {
            return Err(ErrorKind::Num);
        }
        if (p.iter().sum::<f64>() - 1.0).abs() > 1e-7 {
            return Err(ErrorKind::Num);
        }
        Ok(x.iter().zip(&p).filter(|(v, _)| **v >= lo && **v <= hi).map(|(_, q)| q).sum())
    })())
}

// ===========================================================================
// Hypothesis tests
// ===========================================================================

fn chisq_test(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let a = ctx.eval_array(&args[0])?;
        let e = ctx.eval_array(&args[1])?;
        if a.rows != e.rows || a.cols != e.cols {
            return Err(ErrorKind::NA);
        }
        let df = if a.rows > 1 && a.cols > 1 {
            (a.rows - 1) * (a.cols - 1)
        } else if a.rows == 1 {
            a.cols.saturating_sub(1)
        } else {
            a.rows - 1
        };
        if df == 0 {
            return Err(ErrorKind::NA);
        }
        let mut chi = 0.0;
        for (x, y) in a.data.iter().zip(e.data.iter()) {
            if let Value::Error(k) = x {
                return Err(*k);
            }
            if let Value::Error(k) = y {
                return Err(*k);
            }
            if let (Value::Number(x), Value::Number(y)) = (x, y) {
                if *y == 0.0 {
                    return Err(ErrorKind::Div0);
                }
                if *y < 0.0 {
                    return Err(ErrorKind::Num);
                }
                chi += (x - y) * (x - y) / y;
            }
        }
        Ok(gamma_pq(df as f64 / 2.0, chi / 2.0).1)
    })())
}

fn f_test(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let a = nums(ctx, &args[0])?;
        let b = nums(ctx, &args[1])?;
        if a.len() < 2 || b.len() < 2 {
            return Err(ErrorKind::Div0);
        }
        let va = variance(&a, true)?;
        let vb = variance(&b, true)?;
        if va == 0.0 || vb == 0.0 {
            return Err(ErrorKind::Div0);
        }
        let (lo, hi) = f_tails(va / vb, (a.len() - 1) as f64, (b.len() - 1) as f64);
        Ok((2.0 * lo.min(hi)).min(1.0))
    })())
}

fn t_test(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let tails = ctx.eval_number(&args[2])?.trunc();
        let ty = ctx.eval_number(&args[3])?.trunc();
        if tails != 1.0 && tails != 2.0 {
            return Err(ErrorKind::Num);
        }
        let (t, df) = match ty as i64 {
            1 => {
                let (x, y) = paired(ctx, &args[0], &args[1])?;
                let d: Vec<f64> = x.iter().zip(&y).map(|(a, b)| a - b).collect();
                if d.len() < 2 {
                    return Err(ErrorKind::Div0);
                }
                let m = mean(&d)?;
                let s = stdev(&d, true)?;
                (m / (s / (d.len() as f64).sqrt()), d.len() as f64 - 1.0)
            }
            2 | 3 => {
                let x = nums(ctx, &args[0])?;
                let y = nums(ctx, &args[1])?;
                if x.len() < 2 || y.len() < 2 {
                    return Err(ErrorKind::Div0);
                }
                let (n1, n2) = (x.len() as f64, y.len() as f64);
                let (m1, m2) = (mean(&x)?, mean(&y)?);
                let (v1, v2) = (variance(&x, true)?, variance(&y, true)?);
                if ty == 2.0 {
                    let sp = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
                    ((m1 - m2) / (sp * (1.0 / n1 + 1.0 / n2)).sqrt(), n1 + n2 - 2.0)
                } else {
                    let a = v1 / n1;
                    let b = v2 / n2;
                    let df = (a + b) * (a + b) / (a * a / (n1 - 1.0) + b * b / (n2 - 1.0));
                    ((m1 - m2) / (a + b).sqrt(), df)
                }
            }
            _ => return Err(ErrorKind::Num),
        };
        if !t.is_finite() || !df.is_finite() {
            return Err(ErrorKind::Div0);
        }
        Ok(tails * t_sf(t.abs(), df))
    })())
}

fn z_test(ctx: &mut Ctx, args: &[Expr]) -> Value {
    res((|| {
        let v = nums(ctx, &args[0])?;
        let x = ctx.eval_number(&args[1])?;
        if v.is_empty() {
            return Err(ErrorKind::NA);
        }
        let sigma = match args.get(2) {
            None | Some(Expr::Missing) => stdev(&v, true)?,
            Some(e) => ctx.eval_number(e)?,
        };
        if sigma == 0.0 {
            return Err(ErrorKind::Div0);
        }
        let z = (mean(&v)? - x) / (sigma / (v.len() as f64).sqrt());
        Ok(norm_sf(z))
    })())
}

// ===========================================================================
// Distributions (scalar functions)
// ===========================================================================

fn sres(f: impl FnOnce() -> R<f64>) -> Value {
    res(f())
}

fn chk(cond: bool) -> R<()> {
    if cond {
        Ok(())
    } else {
        Err(ErrorKind::Num)
    }
}

fn beta_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let al = num(a, 1)?;
        let be = num(a, 2)?;
        let cum = bool_or(a, 3, false)?;
        let lo = num_or(a, 4, 0.0)?;
        let hi = num_or(a, 5, 1.0)?;
        chk(al > 0.0 && be > 0.0 && lo < hi && x >= lo && x <= hi)?;
        let z = (x - lo) / (hi - lo);
        let zc = (hi - x) / (hi - lo);
        if cum {
            Ok(beta_reg(al, be, z, zc).0)
        } else {
            Ok(beta_pdf(z, al, be) / (hi - lo))
        }
    })
}

fn betadist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let al = num(a, 1)?;
        let be = num(a, 2)?;
        let lo = num_or(a, 3, 0.0)?;
        let hi = num_or(a, 4, 1.0)?;
        chk(al > 0.0 && be > 0.0 && lo < hi && x >= lo && x <= hi)?;
        Ok(beta_reg(al, be, (x - lo) / (hi - lo), (hi - x) / (hi - lo)).0)
    })
}

fn beta_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let al = num(a, 1)?;
        let be = num(a, 2)?;
        let lo = num_or(a, 3, 0.0)?;
        let hi = num_or(a, 4, 1.0)?;
        chk(al > 0.0 && be > 0.0 && lo < hi && p > 0.0 && p <= 1.0)?;
        if p == 1.0 {
            return Ok(hi);
        }
        let g = |z: f64| beta_reg(al, be, z, 1.0 - z).0 - p;
        let z = brent(&g, 0.0, 1.0).ok_or(ErrorKind::Num)?;
        Ok(lo + z * (hi - lo))
    })
}

fn binom_pmf(k: f64, n: f64, p: f64) -> f64 {
    dbinom_raw(k, n, p, 1.0 - p)
}

fn binom_cdf(k: f64, n: f64, p: f64) -> f64 {
    if k >= n {
        return 1.0;
    }
    if k < 0.0 {
        return 0.0;
    }
    // P(X <= k) = I_{1-p}(n-k, k+1)
    beta_reg(n - k, k + 1.0, 1.0 - p, p).0
}

fn binom_dist(a: &[Value]) -> Value {
    sres(|| {
        let k = num(a, 0)?.trunc();
        let n = num(a, 1)?.trunc();
        let p = num(a, 2)?;
        let cum = bool_or(a, 3, false)?;
        chk(k >= 0.0 && k <= n && (0.0..=1.0).contains(&p))?;
        Ok(if cum { binom_cdf(k, n, p) } else { binom_pmf(k, n, p) })
    })
}

fn binom_dist_range(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?.trunc();
        let p = num(a, 1)?;
        let s = num(a, 2)?.trunc();
        let s2 = match a.get(3) {
            None | Some(Value::Empty) => s,
            Some(v) => v.to_number()?.trunc(),
        };
        chk(n >= 0.0 && (0.0..=1.0).contains(&p) && s >= 0.0 && s <= n && s2 >= s && s2 <= n)?;
        if s2 - s < 1000.0 {
            let mut t = 0.0;
            let mut k = s;
            while k <= s2 {
                t += binom_pmf(k, n, p);
                k += 1.0;
            }
            Ok(t)
        } else {
            Ok(binom_cdf(s2, n, p) - binom_cdf(s - 1.0, n, p))
        }
    })
}

fn binom_inv(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?.trunc();
        let p = num(a, 1)?;
        let alpha = num(a, 2)?;
        chk(n >= 0.0 && (0.0..=1.0).contains(&p) && (0.0..=1.0).contains(&alpha))?;
        if n > 100_000.0 {
            // binary search on the (monotone) CDF
            let (mut lo, mut hi) = (0.0f64, n);
            while lo < hi {
                let mid = ((lo + hi) / 2.0).floor();
                if binom_cdf(mid, n, p) >= alpha * (1.0 - 1e-15) {
                    hi = mid;
                } else {
                    lo = mid + 1.0;
                }
            }
            return Ok(lo);
        }
        let mut cum = 0.0;
        let mut k = 0.0;
        while k < n {
            cum += binom_pmf(k, n, p);
            if cum >= alpha * (1.0 - 1e-15) {
                return Ok(k);
            }
            k += 1.0;
        }
        Ok(n)
    })
}

fn chi_df(a: &[Value], i: usize) -> R<f64> {
    let df = num(a, i)?.trunc();
    chk((1.0..=1e10).contains(&df))?;
    Ok(df)
}

fn chisq_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let df = chi_df(a, 1)?;
        let cum = bool_or(a, 2, false)?;
        chk(x >= 0.0)?;
        Ok(if cum { gamma_pq(df / 2.0, x / 2.0).0 } else { gamma_pdf(x, df / 2.0, 2.0) })
    })
}

fn chisq_dist_rt(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let df = chi_df(a, 1)?;
        chk(x >= 0.0)?;
        Ok(gamma_pq(df / 2.0, x / 2.0).1)
    })
}

/// Inverse of the gamma distribution (shape a, scale 1) for lower (or upper) tail p.
fn gamma_quantile(p: f64, a: f64, upper: bool) -> R<f64> {
    let target_zero = if upper { p == 1.0 } else { p == 0.0 };
    if target_zero {
        return Ok(0.0);
    }
    let g = |x: f64| {
        let (lo, hi) = gamma_pq(a, x);
        if upper {
            p - hi
        } else {
            lo - p
        }
    };
    solve_increasing(&g, 0.0, a.max(1.0)).ok_or(ErrorKind::Num)
}

fn chisq_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let df = chi_df(a, 1)?;
        chk((0.0..1.0).contains(&p))?;
        Ok(2.0 * gamma_quantile(p, df / 2.0, false)?)
    })
}

fn chisq_inv_rt(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let df = chi_df(a, 1)?;
        chk(p > 0.0 && p <= 1.0)?;
        Ok(2.0 * gamma_quantile(p, df / 2.0, true)?)
    })
}

fn confidence_norm(a: &[Value]) -> Value {
    sres(|| {
        let alpha = num(a, 0)?;
        let sd = num(a, 1)?;
        let n = num(a, 2)?.trunc();
        chk(alpha > 0.0 && alpha < 1.0 && sd > 0.0 && n >= 1.0)?;
        Ok(-norm_inv(alpha / 2.0) * sd / n.sqrt())
    })
}

fn t_quantile_upper(p: f64, df: f64) -> R<f64> {
    // t such that P(T > t) = p, for p in (0, 0.5]
    if p == 0.5 {
        return Ok(0.0);
    }
    let g = |t: f64| p - t_sf(t, df);
    solve_increasing(&g, 0.0, 2.0).ok_or(ErrorKind::Num)
}

fn confidence_t(a: &[Value]) -> Value {
    sres(|| {
        let alpha = num(a, 0)?;
        let sd = num(a, 1)?;
        let n = num(a, 2)?.trunc();
        chk(alpha > 0.0 && alpha < 1.0 && sd > 0.0 && n >= 1.0)?;
        if n == 1.0 {
            return Err(ErrorKind::Div0);
        }
        Ok(t_quantile_upper(alpha / 2.0, n - 1.0)? * sd / n.sqrt())
    })
}

fn expon_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let l = num(a, 1)?;
        let cum = bool_or(a, 2, false)?;
        chk(x >= 0.0 && l > 0.0)?;
        Ok(if cum { -(-l * x).exp_m1() } else { l * (-l * x).exp() })
    })
}

fn f_dfs(a: &[Value]) -> R<(f64, f64)> {
    let d1 = num(a, 1)?.trunc();
    let d2 = num(a, 2)?.trunc();
    chk(d1 >= 1.0 && d2 >= 1.0 && d1 < 1e10 && d2 < 1e10)?;
    Ok((d1, d2))
}

fn f_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let (d1, d2) = f_dfs(a)?;
        let cum = bool_or(a, 3, false)?;
        chk(x >= 0.0)?;
        Ok(if cum { f_tails(x, d1, d2).0 } else { f_pdf(x, d1, d2) })
    })
}

fn f_dist_rt(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let (d1, d2) = f_dfs(a)?;
        chk(x >= 0.0)?;
        Ok(f_tails(x, d1, d2).1)
    })
}

fn f_quantile(p: f64, d1: f64, d2: f64, upper: bool) -> R<f64> {
    if (upper && p == 1.0) || (!upper && p == 0.0) {
        return Ok(0.0);
    }
    let g = |x: f64| {
        let (lo, hi) = f_tails(x, d1, d2);
        if upper {
            p - hi
        } else {
            lo - p
        }
    };
    solve_increasing(&g, 0.0, 1.0).ok_or(ErrorKind::Num)
}

fn f_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let (d1, d2) = f_dfs(a)?;
        chk((0.0..1.0).contains(&p))?;
        f_quantile(p, d1, d2, false)
    })
}

fn f_inv_rt(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let (d1, d2) = f_dfs(a)?;
        chk(p > 0.0 && p <= 1.0)?;
        f_quantile(p, d1, d2, true)
    })
}

fn fisher(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        chk(x > -1.0 && x < 1.0)?;
        Ok(0.5 * ((1.0 + x) / (1.0 - x)).ln())
    })
}

fn fisherinv(a: &[Value]) -> Value {
    sres(|| Ok(num(a, 0)?.tanh()))
}

fn gamma_fn(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        chk(!(x <= 0.0 && x == x.floor()))?;
        Ok(gamma(x))
    })
}

fn gamma_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let al = num(a, 1)?;
        let be = num(a, 2)?;
        let cum = bool_or(a, 3, false)?;
        chk(x >= 0.0 && al > 0.0 && be > 0.0)?;
        Ok(if cum { gamma_pq(al, x / be).0 } else { gamma_pdf(x, al, be) })
    })
}

fn gamma_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let al = num(a, 1)?;
        let be = num(a, 2)?;
        chk((0.0..1.0).contains(&p) && al > 0.0 && be > 0.0)?;
        Ok(be * gamma_quantile(p, al, false)?)
    })
}

fn gammaln(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        chk(x > 0.0)?;
        Ok(lgamma(x))
    })
}

fn gauss(a: &[Value]) -> Value {
    sres(|| {
        let z = num(a, 0)?;
        // Φ(z) - 0.5 computed without cancellation
        let p = 0.5 * gamma_pq(0.5, 0.5 * z * z).0;
        Ok(if z < 0.0 { -p } else { p })
    })
}

fn hyp_pmf(k: f64, n: f64, kk: f64, nn: f64) -> f64 {
    // R's dhyper: x=k white drawn, r=K whites, b=N-K blacks, n drawn
    let r = kk;
    let b = nn - kk;
    let p = n / nn;
    let q = (nn - n) / nn;
    let p1 = dbinom_raw(k, r, p, q);
    let p2 = dbinom_raw(n - k, b, p, q);
    let p3 = dbinom_raw(n, r + b, p, q);
    p1 * p2 / p3
}

fn hyp_args(a: &[Value]) -> R<(f64, f64, f64, f64)> {
    let k = num(a, 0)?.trunc();
    let n = num(a, 1)?.trunc();
    let kk = num(a, 2)?.trunc();
    let nn = num(a, 3)?.trunc();
    chk(k >= 0.0 && n > 0.0 && n <= nn && kk > 0.0 && kk <= nn && nn > 0.0)?;
    chk(k <= n.min(kk) && k >= (n - nn + kk).max(0.0))?;
    Ok((k, n, kk, nn))
}

fn hypgeom_dist(a: &[Value]) -> Value {
    sres(|| {
        let (k, n, kk, nn) = hyp_args(a)?;
        let cum = bool_or(a, 4, false)?;
        if !cum {
            return Ok(hyp_pmf(k, n, kk, nn));
        }
        let lo = (n - nn + kk).max(0.0);
        let mut s = 0.0;
        let mut i = lo;
        while i <= k {
            s += hyp_pmf(i, n, kk, nn);
            i += 1.0;
        }
        Ok(s.min(1.0))
    })
}

fn hypgeomdist(a: &[Value]) -> Value {
    sres(|| {
        let (k, n, kk, nn) = hyp_args(a)?;
        Ok(hyp_pmf(k, n, kk, nn))
    })
}

fn lognorm_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let m = num(a, 1)?;
        let s = num(a, 2)?;
        let cum = bool_or(a, 3, false)?;
        chk(x > 0.0 && s > 0.0)?;
        let z = (x.ln() - m) / s;
        Ok(if cum { norm_cdf(z) } else { norm_pdf(z) / (x * s) })
    })
}

fn lognormdist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let m = num(a, 1)?;
        let s = num(a, 2)?;
        chk(x > 0.0 && s > 0.0)?;
        Ok(norm_cdf((x.ln() - m) / s))
    })
}

fn lognorm_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let m = num(a, 1)?;
        let s = num(a, 2)?;
        chk(p > 0.0 && p < 1.0 && s > 0.0)?;
        Ok((m + s * norm_inv(p)).exp())
    })
}

fn negbinom_args(a: &[Value]) -> R<(f64, f64, f64)> {
    let f = num(a, 0)?.trunc();
    let s = num(a, 1)?.trunc();
    let p = num(a, 2)?;
    chk(f >= 0.0 && s >= 1.0 && (0.0..=1.0).contains(&p))?;
    Ok((f, s, p))
}

fn negbinom_pmf(f: f64, s: f64, p: f64) -> f64 {
    if p == 1.0 {
        return if f == 0.0 { 1.0 } else { 0.0 };
    }
    s / (s + f) * dbinom_raw(s, s + f, p, 1.0 - p)
}

fn negbinom_dist(a: &[Value]) -> Value {
    sres(|| {
        let (f, s, p) = negbinom_args(a)?;
        let cum = bool_or(a, 3, false)?;
        if cum {
            Ok(beta_reg(s, f + 1.0, p, 1.0 - p).0)
        } else {
            Ok(negbinom_pmf(f, s, p))
        }
    })
}

fn negbinomdist(a: &[Value]) -> Value {
    sres(|| {
        let (f, s, p) = negbinom_args(a)?;
        Ok(negbinom_pmf(f, s, p))
    })
}

fn norm_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let m = num(a, 1)?;
        let s = num(a, 2)?;
        let cum = bool_or(a, 3, false)?;
        chk(s > 0.0)?;
        let z = (x - m) / s;
        Ok(if cum { norm_cdf(z) } else { norm_pdf(z) / s })
    })
}

fn norm_inv_fn(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let m = num(a, 1)?;
        let s = num(a, 2)?;
        chk(p > 0.0 && p < 1.0 && s > 0.0)?;
        Ok(m + s * norm_inv(p))
    })
}

fn norm_s_dist(a: &[Value]) -> Value {
    sres(|| {
        let z = num(a, 0)?;
        let cum = bool_or(a, 1, false)?;
        Ok(if cum { norm_cdf(z) } else { norm_pdf(z) })
    })
}

fn normsdist(a: &[Value]) -> Value {
    sres(|| Ok(norm_cdf(num(a, 0)?)))
}

fn norm_s_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        chk(p > 0.0 && p < 1.0)?;
        Ok(norm_inv(p))
    })
}

fn permut(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?.trunc();
        let k = num(a, 1)?.trunc();
        chk(n >= 0.0 && k >= 0.0 && n >= k)?;
        let mut r = 1.0;
        let mut i = 0.0;
        while i < k {
            r *= n - i;
            if !r.is_finite() {
                return Err(ErrorKind::Num);
            }
            i += 1.0;
        }
        Ok(r)
    })
}

fn permutationa(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?.trunc();
        let k = num(a, 1)?.trunc();
        chk(n >= 0.0 && k >= 0.0)?;
        Ok(n.powf(k))
    })
}

fn phi(a: &[Value]) -> Value {
    sres(|| Ok(norm_pdf(num(a, 0)?)))
}

fn poisson_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?.trunc();
        let m = num(a, 1)?;
        let cum = bool_or(a, 2, false)?;
        chk(x >= 0.0 && m >= 0.0)?;
        if cum {
            if m == 0.0 {
                return Ok(1.0);
            }
            Ok(gamma_pq(x + 1.0, m).1)
        } else {
            Ok(dpois_raw(x, m))
        }
    })
}

fn standardize(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let m = num(a, 1)?;
        let s = num(a, 2)?;
        chk(s > 0.0)?;
        Ok((x - m) / s)
    })
}

fn t_df(a: &[Value], i: usize) -> R<f64> {
    let df = num(a, i)?.trunc();
    chk(df >= 1.0)?;
    Ok(df)
}

fn t_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let df = t_df(a, 1)?;
        let cum = bool_or(a, 2, false)?;
        Ok(if cum { t_cdf(x, df) } else { t_pdf(x, df) })
    })
}

fn t_dist_2t(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let df = t_df(a, 1)?;
        chk(x >= 0.0)?;
        Ok((2.0 * t_sf(x, df)).min(1.0))
    })
}

fn t_dist_rt(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let df = t_df(a, 1)?;
        Ok(t_sf(x, df))
    })
}

fn tdist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let df = t_df(a, 1)?;
        let tails = num(a, 2)?.trunc();
        chk(x >= 0.0 && (tails == 1.0 || tails == 2.0))?;
        Ok((tails * t_sf(x, df)).min(1.0))
    })
}

fn t_inv(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let df = t_df(a, 1)?;
        chk(p > 0.0 && p < 1.0)?;
        if p < 0.5 {
            Ok(-t_quantile_upper(p, df)?)
        } else {
            Ok(t_quantile_upper(1.0 - p, df)?)
        }
    })
}

fn t_inv_2t(a: &[Value]) -> Value {
    sres(|| {
        let p = num(a, 0)?;
        let df = t_df(a, 1)?;
        chk(p > 0.0 && p <= 1.0)?;
        t_quantile_upper(p / 2.0, df)
    })
}

fn weibull_dist(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let al = num(a, 1)?;
        let be = num(a, 2)?;
        let cum = bool_or(a, 3, false)?;
        chk(x >= 0.0 && al > 0.0 && be > 0.0)?;
        let z = (x / be).powf(al);
        Ok(if cum { -(-z).exp_m1() } else { al / be.powf(al) * x.powf(al - 1.0) * (-z).exp() })
    })
}
