//! Engineering functions: Bessel functions, base conversion, bitwise
//! operations, complex numbers, error functions and unit conversion.

use super::{num, text, Registry, VAR};
use crate::ast::Expr;
use crate::eval::Ctx;
use crate::tryv;
use crate::value::{format_number_general, ErrorKind, Value};

const CAT: &str = "Engineering";

type R<T> = Result<T, ErrorKind>;

pub fn register(r: &mut Registry) {
    // Bessel
    r.scalar("BESSELI", 2, 2, besseli_fn, CAT, "BESSELI(x, n)", "Returns the modified Bessel function In(x).");
    r.scalar("BESSELJ", 2, 2, besselj_fn, CAT, "BESSELJ(x, n)", "Returns the Bessel function Jn(x).");
    r.scalar("BESSELK", 2, 2, besselk_fn, CAT, "BESSELK(x, n)", "Returns the modified Bessel function Kn(x).");
    r.scalar("BESSELY", 2, 2, bessely_fn, CAT, "BESSELY(x, n)", "Returns the Bessel function Yn(x).");
    // Base conversion
    r.scalar("BIN2DEC", 1, 1, bin2dec, CAT, "BIN2DEC(number)", "Converts a binary number to decimal.");
    r.scalar("BIN2HEX", 1, 2, bin2hex, CAT, "BIN2HEX(number, [places])", "Converts a binary number to hexadecimal.");
    r.scalar("BIN2OCT", 1, 2, bin2oct, CAT, "BIN2OCT(number, [places])", "Converts a binary number to octal.");
    r.scalar("DEC2BIN", 1, 2, dec2bin, CAT, "DEC2BIN(number, [places])", "Converts a decimal number to binary.");
    r.scalar("DEC2HEX", 1, 2, dec2hex, CAT, "DEC2HEX(number, [places])", "Converts a decimal number to hexadecimal.");
    r.scalar("DEC2OCT", 1, 2, dec2oct, CAT, "DEC2OCT(number, [places])", "Converts a decimal number to octal.");
    r.scalar("HEX2BIN", 1, 2, hex2bin, CAT, "HEX2BIN(number, [places])", "Converts a hexadecimal number to binary.");
    r.scalar("HEX2DEC", 1, 1, hex2dec, CAT, "HEX2DEC(number)", "Converts a hexadecimal number to decimal.");
    r.scalar("HEX2OCT", 1, 2, hex2oct, CAT, "HEX2OCT(number, [places])", "Converts a hexadecimal number to octal.");
    r.scalar("OCT2BIN", 1, 2, oct2bin, CAT, "OCT2BIN(number, [places])", "Converts an octal number to binary.");
    r.scalar("OCT2DEC", 1, 1, oct2dec, CAT, "OCT2DEC(number)", "Converts an octal number to decimal.");
    r.scalar("OCT2HEX", 1, 2, oct2hex, CAT, "OCT2HEX(number, [places])", "Converts an octal number to hexadecimal.");
    // Bitwise
    r.scalar("BITAND", 2, 2, bitand, CAT, "BITAND(number1, number2)", "Returns a bitwise 'AND' of two numbers.");
    r.scalar("BITOR", 2, 2, bitor, CAT, "BITOR(number1, number2)", "Returns a bitwise 'OR' of two numbers.");
    r.scalar("BITXOR", 2, 2, bitxor, CAT, "BITXOR(number1, number2)", "Returns a bitwise 'exclusive OR' of two numbers.");
    r.scalar("BITLSHIFT", 2, 2, bitlshift, CAT, "BITLSHIFT(number, shift_amount)", "Returns a number shifted left by shift_amount bits.");
    r.scalar("BITRSHIFT", 2, 2, bitrshift, CAT, "BITRSHIFT(number, shift_amount)", "Returns a number shifted right by shift_amount bits.");
    // Misc
    r.scalar("DELTA", 1, 2, delta, CAT, "DELTA(number1, [number2])", "Tests whether two values are equal (returns 1 or 0).");
    r.scalar("GESTEP", 1, 2, gestep, CAT, "GESTEP(number, [step])", "Tests whether a number is greater than or equal to a threshold value.");
    r.scalar("ERF", 1, 2, erf_fn, CAT, "ERF(lower_limit, [upper_limit])", "Returns the error function.");
    r.scalar("ERF.PRECISE", 1, 1, erf_precise_fn, CAT, "ERF.PRECISE(x)", "Returns the error function.");
    r.scalar("ERFC", 1, 1, erfc_fn, CAT, "ERFC(x)", "Returns the complementary error function.");
    r.scalar("ERFC.PRECISE", 1, 1, erfc_fn, CAT, "ERFC.PRECISE(x)", "Returns the complementary error function integrated between x and infinity.");
    r.scalar("CONVERT", 3, 3, convert_fn, CAT, "CONVERT(number, from_unit, to_unit)", "Converts a number from one measurement system to another.");
    // Complex numbers
    r.scalar("COMPLEX", 2, 3, complex_fn, CAT, "COMPLEX(real_num, i_num, [suffix])", "Converts real and imaginary coefficients into a complex number.");
    r.scalar("IMABS", 1, 1, imabs, CAT, "IMABS(inumber)", "Returns the absolute value (modulus) of a complex number.");
    r.scalar("IMAGINARY", 1, 1, imaginary, CAT, "IMAGINARY(inumber)", "Returns the imaginary coefficient of a complex number.");
    r.scalar("IMREAL", 1, 1, imreal, CAT, "IMREAL(inumber)", "Returns the real coefficient of a complex number.");
    r.scalar("IMARGUMENT", 1, 1, imargument, CAT, "IMARGUMENT(inumber)", "Returns the argument theta, an angle expressed in radians.");
    r.scalar("IMCONJUGATE", 1, 1, imconjugate, CAT, "IMCONJUGATE(inumber)", "Returns the complex conjugate of a complex number.");
    r.scalar("IMCOS", 1, 1, imcos, CAT, "IMCOS(inumber)", "Returns the cosine of a complex number.");
    r.scalar("IMCOSH", 1, 1, imcosh, CAT, "IMCOSH(inumber)", "Returns the hyperbolic cosine of a complex number.");
    r.scalar("IMCOT", 1, 1, imcot, CAT, "IMCOT(inumber)", "Returns the cotangent of a complex number.");
    r.scalar("IMCSC", 1, 1, imcsc, CAT, "IMCSC(inumber)", "Returns the cosecant of a complex number.");
    r.scalar("IMCSCH", 1, 1, imcsch, CAT, "IMCSCH(inumber)", "Returns the hyperbolic cosecant of a complex number.");
    r.scalar("IMSEC", 1, 1, imsec, CAT, "IMSEC(inumber)", "Returns the secant of a complex number.");
    r.scalar("IMSECH", 1, 1, imsech, CAT, "IMSECH(inumber)", "Returns the hyperbolic secant of a complex number.");
    r.scalar("IMSIN", 1, 1, imsin, CAT, "IMSIN(inumber)", "Returns the sine of a complex number.");
    r.scalar("IMSINH", 1, 1, imsinh, CAT, "IMSINH(inumber)", "Returns the hyperbolic sine of a complex number.");
    r.scalar("IMTAN", 1, 1, imtan, CAT, "IMTAN(inumber)", "Returns the tangent of a complex number.");
    r.scalar("IMEXP", 1, 1, imexp, CAT, "IMEXP(inumber)", "Returns the exponential of a complex number.");
    r.scalar("IMLN", 1, 1, imln, CAT, "IMLN(inumber)", "Returns the natural logarithm of a complex number.");
    r.scalar("IMLOG10", 1, 1, imlog10, CAT, "IMLOG10(inumber)", "Returns the base-10 logarithm of a complex number.");
    r.scalar("IMLOG2", 1, 1, imlog2, CAT, "IMLOG2(inumber)", "Returns the base-2 logarithm of a complex number.");
    r.scalar("IMSQRT", 1, 1, imsqrt, CAT, "IMSQRT(inumber)", "Returns the square root of a complex number.");
    r.scalar("IMPOWER", 2, 2, impower, CAT, "IMPOWER(inumber, number)", "Returns a complex number raised to an integer power.");
    r.scalar("IMDIV", 2, 2, imdiv, CAT, "IMDIV(inumber1, inumber2)", "Returns the quotient of two complex numbers.");
    r.scalar("IMSUB", 2, 2, imsub, CAT, "IMSUB(inumber1, inumber2)", "Returns the difference between two complex numbers.");
    r.raw("IMSUM", 1, VAR, imsum, CAT, "IMSUM(inumber1, [inumber2], ...)", "Returns the sum of complex numbers.");
    r.raw("IMPRODUCT", 1, VAR, improduct, CAT, "IMPRODUCT(inumber1, [inumber2], ...)", "Returns the product of complex numbers.");
}

fn out(x: f64) -> Value {
    Value::number(x)
}

// ===========================================================================
// Bessel functions
// ===========================================================================

fn ln_factorial(n: u32) -> f64 {
    (2..=n).map(|k| (k as f64).ln()).sum()
}

/// Power series sum_k s^k (x/2)^(2k+n) / (k! (n+k)!), s = -1 for J and +1 for I.
fn bessel_series(x: f64, n: u32, sign: f64) -> f64 {
    if x == 0.0 {
        return if n == 0 { 1.0 } else { 0.0 };
    }
    let h = x.abs() / 2.0;
    let mut term = (n as f64 * h.ln() - ln_factorial(n)).exp();
    let mut sum = term;
    let q = h * h;
    let mut k = 0.0;
    loop {
        k += 1.0;
        term *= sign * q / (k * (n as f64 + k));
        sum += term;
        if term.abs() <= 1e-17 * sum.abs() || k > 2000.0 {
            break;
        }
    }
    if x < 0.0 && n % 2 == 1 {
        -sum
    } else {
        sum
    }
}

/// Trapezoid rule on [0, pi] for a smooth, even, 2*pi-periodic integrand
/// (spectrally accurate).
fn periodic_trapezoid(f: &dyn Fn(f64) -> f64, m: usize) -> f64 {
    let h = std::f64::consts::PI / m as f64;
    let mut s = (f(0.0) + f(std::f64::consts::PI)) / 2.0;
    for k in 1..m {
        s += f(k as f64 * h);
    }
    s * h
}

/// Composite 5-point Gauss-Legendre quadrature.
fn gauss_legendre(f: &dyn Fn(f64) -> f64, a: f64, b: f64, panels: usize) -> f64 {
    const X: [f64; 5] = [0.0, -0.538_469_310_105_683_1, 0.538_469_310_105_683_1, -0.906_179_845_938_664, 0.906_179_845_938_664];
    const W: [f64; 5] = [0.568_888_888_888_888_9, 0.478_628_670_499_366_5, 0.478_628_670_499_366_5, 0.236_926_885_056_189_1, 0.236_926_885_056_189_1];
    let h = (b - a) / panels as f64;
    let mut s = 0.0;
    for p in 0..panels {
        let lo = a + p as f64 * h;
        let mid = lo + h / 2.0;
        for i in 0..5 {
            s += W[i] * f(mid + X[i] * h / 2.0);
        }
    }
    s * h / 2.0
}

fn bessel_j(x: f64, n: u32) -> f64 {
    if x.abs() <= 8.0 || n as f64 > x.abs() {
        return bessel_series(x, n, -1.0);
    }
    let nf = n as f64;
    let m = (2.0 * (x.abs() + nf) + 64.0) as usize;
    periodic_trapezoid(&|t| (nf * t - x * t.sin()).cos(), m) / std::f64::consts::PI
}

fn bessel_i(x: f64, n: u32) -> f64 {
    bessel_series(x, n, 1.0)
}

fn bessel_k(x: f64, n: u32) -> f64 {
    // K_n(x) = int_0^inf exp(-x cosh t) cosh(n t) dt; even analytic integrand,
    // so the trapezoid rule over the whole line converges exponentially.
    let nf = n as f64;
    let f = |t: f64| 0.5 * ((nf * t - x * t.cosh()).exp() + (-nf * t - x * t.cosh()).exp());
    let h = 0.05;
    let peak = (nf / x).asinh();
    let mut s = f(0.0) / 2.0;
    let mut k = 1.0;
    loop {
        let t = k * h;
        let v = f(t);
        s += v;
        if (t > peak && v <= 1e-18 * s) || k > 1e6 {
            break;
        }
        k += 1.0;
    }
    s * h
}

fn bessel_y01(x: f64, n: u32) -> f64 {
    let nf = n as f64;
    let pi = std::f64::consts::PI;
    let panels = (4.0 * x + 40.0) as usize;
    let a = gauss_legendre(&|t| (x * t.sin() - nf * t).sin(), 0.0, pi, panels);
    let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
    let g = |t: f64| ((nf * t - x * t.sinh()).exp()) + sign * ((-nf * t - x * t.sinh()).exp());
    // Integrate in chunks until the integrand is negligible.
    let mut b = 0.0;
    let mut t0 = 0.0;
    // e^{-x sinh t} varies on a scale of ~1/x near t = 0.
    let w = 0.25f64.min(1.0 / x);
    loop {
        let part = gauss_legendre(&g, t0, t0 + w, 4);
        b += part;
        t0 += w;
        if (x * t0.sinh() - nf * t0 > 45.0 && part.abs() <= 1e-18 * b.abs().max(1e-300)) || t0 > 60.0 {
            break;
        }
    }
    (a - b) / pi
}

fn bessel_y(x: f64, n: u32) -> f64 {
    let y0 = bessel_y01(x, 0);
    if n == 0 {
        return y0;
    }
    let mut y1 = bessel_y01(x, 1);
    let mut prev = y0;
    for k in 1..n {
        let next = 2.0 * k as f64 / x * y1 - prev;
        prev = y1;
        y1 = next;
    }
    y1
}

fn bessel_args(a: &[Value]) -> R<(f64, u32)> {
    let x = num(a, 0)?;
    let n = num(a, 1)?.trunc();
    if n < 0.0 || n > 1e4 {
        return Err(ErrorKind::Num);
    }
    Ok((x, n as u32))
}

fn besseli_fn(a: &[Value]) -> Value {
    let (x, n) = tryv!(bessel_args(a));
    out(bessel_i(x, n))
}

fn besselj_fn(a: &[Value]) -> Value {
    let (x, n) = tryv!(bessel_args(a));
    out(bessel_j(x, n))
}

fn besselk_fn(a: &[Value]) -> Value {
    let (x, n) = tryv!(bessel_args(a));
    if x <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(bessel_k(x, n))
}

fn bessely_fn(a: &[Value]) -> Value {
    let (x, n) = tryv!(bessel_args(a));
    if x <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(bessel_y(x, n))
}

// ===========================================================================
// Base conversion
// ===========================================================================

/// Text of a base-conversion input (numbers are written in General format).
fn base_input(v: &Value) -> R<String> {
    match v {
        Value::Bool(_) => Err(ErrorKind::Value),
        Value::Error(e) => Err(*e),
        v => Ok(v.to_text()?.trim().to_string()),
    }
}

/// Parse a 10-digit two's complement number in the given radix.
fn parse_base(v: &Value, radix: u32) -> R<i64> {
    let s = base_input(v)?;
    if s.len() > 10 {
        return Err(ErrorKind::Num);
    }
    if s.is_empty() {
        return Ok(0);
    }
    let mut n: i64 = 0;
    for c in s.chars() {
        let d = c.to_digit(radix).ok_or(ErrorKind::Num)? as i64;
        n = n * radix as i64 + d;
    }
    let full = (radix as i64).pow(10);
    if n >= full / 2 {
        n -= full;
    }
    Ok(n)
}

/// Format `n` in the given radix with 10-digit two's complement for negatives
/// and optional zero padding.
fn format_base(n: i64, radix: u32, places: Option<&Value>) -> R<String> {
    let full = (radix as i64).pow(10);
    if n < -full / 2 || n >= full / 2 {
        return Err(ErrorKind::Num);
    }
    if n < 0 {
        return Ok(to_radix(n + full, radix));
    }
    let s = to_radix(n, radix);
    match places {
        None | Some(Value::Empty) => Ok(s),
        Some(p) => {
            let p = p.to_number()?.trunc();
            if p < 0.0 || p > 10.0 || (p as usize) < s.len() {
                return Err(ErrorKind::Num);
            }
            Ok(format!("{:0>width$}", s, width = p as usize))
        }
    }
}

fn to_radix(mut n: i64, radix: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut digits = Vec::new();
    while n > 0 {
        digits.push(std::char::from_digit((n % radix as i64) as u32, radix).unwrap().to_ascii_uppercase());
        n /= radix as i64;
    }
    digits.iter().rev().collect()
}

fn convert_base(a: &[Value], from: u32, to: u32) -> Value {
    let n = tryv!(parse_base(&a[0], from));
    match format_base(n, to, a.get(1)) {
        Ok(s) => Value::Text(s),
        Err(e) => Value::Error(e),
    }
}

fn to_dec(a: &[Value], from: u32) -> Value {
    match parse_base(&a[0], from) {
        Ok(n) => Value::Number(n as f64),
        Err(e) => Value::Error(e),
    }
}

fn from_dec(a: &[Value], to: u32) -> Value {
    let x = match &a[0] {
        Value::Bool(_) => return Value::Error(ErrorKind::Value),
        v => tryv!(v.to_number()),
    };
    let x = x.trunc();
    let full = (to as f64).powi(10);
    if x < -full / 2.0 || x >= full / 2.0 {
        return Value::Error(ErrorKind::Num);
    }
    match format_base(x as i64, to, a.get(1)) {
        Ok(s) => Value::Text(s),
        Err(e) => Value::Error(e),
    }
}

fn bin2dec(a: &[Value]) -> Value {
    to_dec(a, 2)
}
fn bin2hex(a: &[Value]) -> Value {
    convert_base(a, 2, 16)
}
fn bin2oct(a: &[Value]) -> Value {
    convert_base(a, 2, 8)
}
fn dec2bin(a: &[Value]) -> Value {
    from_dec(a, 2)
}
fn dec2hex(a: &[Value]) -> Value {
    from_dec(a, 16)
}
fn dec2oct(a: &[Value]) -> Value {
    from_dec(a, 8)
}
fn hex2bin(a: &[Value]) -> Value {
    convert_base(a, 16, 2)
}
fn hex2dec(a: &[Value]) -> Value {
    to_dec(a, 16)
}
fn hex2oct(a: &[Value]) -> Value {
    convert_base(a, 16, 8)
}
fn oct2bin(a: &[Value]) -> Value {
    convert_base(a, 8, 2)
}
fn oct2dec(a: &[Value]) -> Value {
    to_dec(a, 8)
}
fn oct2hex(a: &[Value]) -> Value {
    convert_base(a, 8, 16)
}

// ===========================================================================
// Bitwise
// ===========================================================================

const BIT_LIMIT: f64 = 281_474_976_710_656.0; // 2^48

fn bit_arg(a: &[Value], i: usize) -> R<u64> {
    let x = match &a[i] {
        Value::Bool(_) => return Err(ErrorKind::Value),
        v => v.to_number()?,
    };
    if x < 0.0 || x >= BIT_LIMIT || x.fract() != 0.0 {
        return Err(ErrorKind::Num);
    }
    Ok(x as u64)
}

fn bit_op(a: &[Value], f: fn(u64, u64) -> u64) -> Value {
    let x = tryv!(bit_arg(a, 0));
    let y = tryv!(bit_arg(a, 1));
    out(f(x, y) as f64)
}

fn bitand(a: &[Value]) -> Value {
    bit_op(a, |x, y| x & y)
}
fn bitor(a: &[Value]) -> Value {
    bit_op(a, |x, y| x | y)
}
fn bitxor(a: &[Value]) -> Value {
    bit_op(a, |x, y| x ^ y)
}

fn shift(a: &[Value], left: bool) -> Value {
    let x = tryv!(bit_arg(a, 0));
    let s = tryv!(num(a, 1)).trunc();
    if s.abs() > 53.0 {
        return Value::Error(ErrorKind::Num);
    }
    let s = if left { s } else { -s };
    let r = if s >= 0.0 { x as f64 * 2f64.powi(s as i32) } else { (x >> (-s) as u32) as f64 };
    if r >= BIT_LIMIT {
        return Value::Error(ErrorKind::Num);
    }
    out(r)
}

fn bitlshift(a: &[Value]) -> Value {
    shift(a, true)
}
fn bitrshift(a: &[Value]) -> Value {
    shift(a, false)
}

// ===========================================================================
// DELTA, GESTEP, ERF
// ===========================================================================

fn strict_num(a: &[Value], i: usize, default: f64) -> R<f64> {
    match a.get(i) {
        None | Some(Value::Empty) => Ok(default),
        Some(Value::Bool(_)) => Err(ErrorKind::Value),
        Some(v) => v.to_number(),
    }
}

fn delta(a: &[Value]) -> Value {
    let x = tryv!(strict_num(a, 0, 0.0));
    let y = tryv!(strict_num(a, 1, 0.0));
    out(if x == y { 1.0 } else { 0.0 })
}

fn gestep(a: &[Value]) -> Value {
    let x = tryv!(strict_num(a, 0, 0.0));
    let y = tryv!(strict_num(a, 1, 0.0));
    out(if x >= y { 1.0 } else { 0.0 })
}

const FRAC_2_SQRT_PI: f64 = std::f64::consts::FRAC_2_SQRT_PI;

/// erf(x) by the series 2x/sqrt(pi) e^{-x^2} sum (2x^2)^n / (2n+1)!!, valid
/// for moderate |x| with no cancellation.
fn erf_series(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut n = 0.0;
    loop {
        n += 1.0;
        term *= 2.0 * x2 / (2.0 * n + 1.0);
        sum += term;
        if term < 1e-17 * sum || n > 500.0 {
            break;
        }
    }
    FRAC_2_SQRT_PI * x * (-x2).exp() * sum
}

/// erfc(x) for x >= 2 by continued fraction (modified Lentz).
fn erfc_cf(x: f64) -> f64 {
    let tiny = 1e-300;
    let mut f = x;
    let mut c = f;
    let mut d = 0.0;
    for k in 1..10_000 {
        let a = k as f64 / 2.0;
        d = x + a * d;
        if d == 0.0 {
            d = tiny;
        }
        c = x + a / c;
        if c == 0.0 {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    (-x * x).exp() / std::f64::consts::PI.sqrt() / f
}

pub(crate) fn erf(x: f64) -> f64 {
    if x < 0.0 {
        return -erf(-x);
    }
    if x < 3.0 {
        erf_series(x)
    } else {
        1.0 - erfc_cf(x)
    }
}

pub(crate) fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x < 2.0 {
        1.0 - erf_series(x)
    } else {
        erfc_cf(x)
    }
}

fn erf_fn(a: &[Value]) -> Value {
    let lo = tryv!(strict_num(a, 0, 0.0));
    match a.get(1) {
        None | Some(Value::Empty) => out(erf(lo)),
        Some(_) => {
            let hi = tryv!(strict_num(a, 1, 0.0));
            out(erf(hi) - erf(lo))
        }
    }
}

fn erf_precise_fn(a: &[Value]) -> Value {
    let x = tryv!(strict_num(a, 0, 0.0));
    out(erf(x))
}

fn erfc_fn(a: &[Value]) -> Value {
    let x = tryv!(strict_num(a, 0, 0.0));
    out(erfc(x))
}

// ===========================================================================
// CONVERT
// ===========================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum UCat {
    Mass,
    Distance,
    Time,
    Pressure,
    Force,
    Energy,
    Power,
    Magnetism,
    Temperature,
    Volume,
    Area,
    Information,
    Speed,
}

/// Prefix support: 0 = none, 1 = metric, 2 = metric and binary.
struct Unit {
    names: &'static [&'static str],
    cat: UCat,
    factor: f64,
    offset: f64,
    prefix: u8,
    dim: i32,
}

const fn u(names: &'static [&'static str], cat: UCat, factor: f64, prefix: u8) -> Unit {
    Unit { names, cat, factor, offset: 0.0, prefix, dim: 1 }
}

const fn ud(names: &'static [&'static str], cat: UCat, factor: f64, prefix: u8, dim: i32) -> Unit {
    Unit { names, cat, factor, offset: 0.0, prefix, dim }
}

const IN: f64 = 0.0254;
const FT: f64 = 0.3048;
const YD: f64 = 0.9144;
const MI: f64 = 1609.344;
const NMI: f64 = 1852.0;
const LY: f64 = 9_460_730_472_580_800.0;
const PICA_PT: f64 = 0.0254 / 72.0;
const ANG: f64 = 1e-10;

use UCat::*;

static UNITS: &[Unit] = &[
    // Mass (base: gram)
    u(&["g"], Mass, 1.0, 1),
    u(&["sg"], Mass, 14_593.902_937_206_4, 0),
    u(&["lbm"], Mass, 453.592_37, 0),
    u(&["u"], Mass, 1.660_538_86e-24, 1),
    u(&["ozm"], Mass, 28.349_523_125, 0),
    u(&["grain"], Mass, 0.064_798_91, 0),
    u(&["cwt", "shweight"], Mass, 45_359.237, 0),
    u(&["uk_cwt", "lcwt", "hweight"], Mass, 50_802.345_44, 0),
    u(&["stone"], Mass, 6_350.293_18, 0),
    u(&["ton"], Mass, 907_184.74, 0),
    u(&["uk_ton", "LTON", "brton"], Mass, 1_016_046.908_8, 0),
    // Distance (base: meter)
    u(&["m"], Distance, 1.0, 1),
    u(&["mi"], Distance, MI, 0),
    u(&["Nmi"], Distance, NMI, 0),
    u(&["in"], Distance, IN, 0),
    u(&["ft"], Distance, FT, 0),
    u(&["yd"], Distance, YD, 0),
    u(&["ang"], Distance, ANG, 1),
    u(&["ell"], Distance, 1.143, 0),
    u(&["ly"], Distance, LY, 1),
    u(&["parsec", "pc"], Distance, 30_856_775_812_815_500.0, 1),
    u(&["Pica", "Picapt"], Distance, PICA_PT, 0),
    u(&["pica"], Distance, IN / 6.0, 0),
    u(&["survey_mi"], Distance, 1609.347_218_694_437, 0),
    // Time (base: second)
    u(&["yr"], Time, 31_557_600.0, 1),
    u(&["day", "d"], Time, 86_400.0, 1),
    u(&["hr"], Time, 3_600.0, 1),
    u(&["mn", "min"], Time, 60.0, 1),
    u(&["sec", "s"], Time, 1.0, 1),
    // Pressure (base: pascal)
    u(&["Pa", "p"], Pressure, 1.0, 1),
    u(&["atm", "at"], Pressure, 101_325.0, 1),
    u(&["mmHg"], Pressure, 133.322_368_421_053, 1),
    u(&["psi"], Pressure, 6_894.757_293_168_36, 0),
    u(&["Torr"], Pressure, 133.322_368_421_053, 0),
    // Force (base: newton)
    u(&["N"], Force, 1.0, 1),
    u(&["dyn", "dy"], Force, 1e-5, 1),
    u(&["lbf"], Force, 4.448_221_615_260_5, 0),
    u(&["pond"], Force, 0.009_806_65, 1),
    // Energy (base: joule)
    u(&["J"], Energy, 1.0, 1),
    u(&["e"], Energy, 1e-7, 1),
    u(&["c"], Energy, 4.184, 1),
    u(&["cal"], Energy, 4.1868, 1),
    u(&["eV", "ev"], Energy, 1.602_176_487e-19, 1),
    u(&["HPh", "hh"], Energy, 2_684_519.537_696_17, 0),
    u(&["Wh", "wh"], Energy, 3_600.0, 1),
    u(&["flb"], Energy, 1.355_817_948_331_4, 0),
    u(&["BTU", "btu"], Energy, 1_055.055_852_62, 0),
    // Power (base: watt)
    u(&["HP", "h"], Power, 745.699_871_582_270, 0),
    u(&["PS"], Power, 735.498_75, 0),
    u(&["W", "w"], Power, 1.0, 1),
    // Magnetism (base: tesla)
    u(&["T"], Magnetism, 1.0, 1),
    u(&["ga"], Magnetism, 1e-4, 1),
    // Temperature (base: kelvin; value_K = x * factor + offset)
    Unit { names: &["C", "cel"], cat: Temperature, factor: 1.0, offset: 273.15, prefix: 0, dim: 1 },
    Unit { names: &["F", "fah"], cat: Temperature, factor: 5.0 / 9.0, offset: 273.15 - 32.0 * 5.0 / 9.0, prefix: 0, dim: 1 },
    Unit { names: &["K", "kel"], cat: Temperature, factor: 1.0, offset: 0.0, prefix: 1, dim: 1 },
    Unit { names: &["Rank"], cat: Temperature, factor: 5.0 / 9.0, offset: 0.0, prefix: 0, dim: 1 },
    Unit { names: &["Reau"], cat: Temperature, factor: 1.25, offset: 273.15, prefix: 0, dim: 1 },
    // Volume (base: cubic meter)
    u(&["tsp"], Volume, 4.928_921_593_75e-6, 0),
    u(&["tspm"], Volume, 5e-6, 0),
    u(&["tbs"], Volume, 1.478_676_478_125e-5, 0),
    u(&["oz"], Volume, 2.957_352_956_25e-5, 0),
    u(&["cup"], Volume, 2.365_882_365e-4, 0),
    u(&["pt", "us_pt"], Volume, 4.731_764_73e-4, 0),
    u(&["uk_pt"], Volume, 5.682_612_5e-4, 0),
    u(&["qt"], Volume, 9.463_529_46e-4, 0),
    u(&["uk_qt"], Volume, 1.136_522_5e-3, 0),
    u(&["gal"], Volume, 3.785_411_784e-3, 0),
    u(&["uk_gal"], Volume, 4.546_09e-3, 0),
    u(&["l", "L", "lt"], Volume, 1e-3, 1),
    ud(&["ang3", "ang^3"], Volume, ANG * ANG * ANG, 1, 3),
    u(&["barrel"], Volume, 0.158_987_294_928, 0),
    u(&["bushel"], Volume, 0.035_239_070_166_88, 0),
    u(&["ft3", "ft^3"], Volume, FT * FT * FT, 0),
    u(&["in3", "in^3"], Volume, IN * IN * IN, 0),
    ud(&["ly3", "ly^3"], Volume, LY * LY * LY, 1, 3),
    ud(&["m3", "m^3"], Volume, 1.0, 1, 3),
    u(&["mi3", "mi^3"], Volume, MI * MI * MI, 0),
    u(&["yd3", "yd^3"], Volume, YD * YD * YD, 0),
    u(&["Nmi3", "Nmi^3"], Volume, NMI * NMI * NMI, 0),
    u(&["Pica3", "Picapt3", "Pica^3", "Picapt^3"], Volume, PICA_PT * PICA_PT * PICA_PT, 0),
    u(&["GRT", "regton"], Volume, 2.831_684_659_2, 0),
    u(&["MTON"], Volume, 1.132_673_863_68, 0),
    // Area (base: square meter)
    u(&["uk_acre"], Area, 4_046.856_422_4, 0),
    u(&["us_acre"], Area, 4_046.872_609_874_25, 0),
    ud(&["ang2", "ang^2"], Area, ANG * ANG, 1, 2),
    u(&["ar"], Area, 100.0, 1),
    u(&["ft2", "ft^2"], Area, FT * FT, 0),
    u(&["ha"], Area, 10_000.0, 0),
    u(&["in2", "in^2"], Area, IN * IN, 0),
    ud(&["ly2", "ly^2"], Area, LY * LY, 1, 2),
    ud(&["m2", "m^2"], Area, 1.0, 1, 2),
    u(&["Morgen"], Area, 2_500.0, 0),
    u(&["mi2", "mi^2"], Area, MI * MI, 0),
    u(&["Nmi2", "Nmi^2"], Area, NMI * NMI, 0),
    u(&["Pica2", "Picapt2", "Pica^2", "Picapt^2"], Area, PICA_PT * PICA_PT, 0),
    u(&["yd2", "yd^2"], Area, YD * YD, 0),
    // Information (base: bit)
    u(&["bit"], Information, 1.0, 2),
    u(&["byte"], Information, 8.0, 2),
    // Speed (base: meter per second)
    u(&["admkn"], Speed, 1853.184 / 3600.0, 0),
    u(&["kn"], Speed, 1852.0 / 3600.0, 0),
    u(&["m/h", "m/hr"], Speed, 1.0 / 3600.0, 1),
    u(&["m/s", "m/sec"], Speed, 1.0, 1),
    u(&["mph"], Speed, MI / 3600.0, 0),
];

const METRIC_PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("da", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
    ("e", 1e1),
];

const BINARY_PREFIXES: &[(&str, f64)] = &[
    ("Yi", 1_208_925_819_614_629_174_706_176.0),
    ("Zi", 1_180_591_620_717_411_303_424.0),
    ("Ei", 1_152_921_504_606_846_976.0),
    ("Pi", 1_125_899_906_842_624.0),
    ("Ti", 1_099_511_627_776.0),
    ("Gi", 1_073_741_824.0),
    ("Mi", 1_048_576.0),
    ("ki", 1_024.0),
];

fn find_exact(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.names.contains(&name))
}

/// Resolve a unit string to (unit, multiplier from prefix).
fn find_unit(name: &str) -> Option<(&'static Unit, f64)> {
    if let Some(u) = find_exact(name) {
        return Some((u, 1.0));
    }
    for (p, m) in BINARY_PREFIXES {
        if let Some(rest) = name.strip_prefix(p) {
            if let Some(u) = find_exact(rest) {
                if u.prefix == 2 {
                    return Some((u, *m));
                }
            }
        }
    }
    for (p, m) in METRIC_PREFIXES {
        if let Some(rest) = name.strip_prefix(p) {
            if let Some(u) = find_exact(rest) {
                if u.prefix >= 1 {
                    return Some((u, m.powi(u.dim)));
                }
            }
        }
    }
    None
}

fn convert_fn(a: &[Value]) -> Value {
    let x = match &a[0] {
        Value::Bool(_) => return Value::Error(ErrorKind::Value),
        v => tryv!(v.to_number()),
    };
    let from = tryv!(text(a, 1));
    let to = tryv!(text(a, 2));
    let (Some((fu, fm)), Some((tu, tm))) = (find_unit(&from), find_unit(&to)) else {
        return Value::Error(ErrorKind::NA);
    };
    if fu.cat != tu.cat {
        return Value::Error(ErrorKind::NA);
    }
    let base = x * fm * fu.factor + fu.offset;
    out((base - tu.offset) / (tu.factor * tm))
}

// ===========================================================================
// Complex numbers
// ===========================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cx {
    re: f64,
    im: f64,
}

impl Cx {
    fn new(re: f64, im: f64) -> Cx {
        Cx { re, im }
    }
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.re - o.re, self.im - o.im)
    }
    fn mul(self, o: Cx) -> Cx {
        Cx::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
    fn div(self, o: Cx) -> R<Cx> {
        let d = o.re * o.re + o.im * o.im;
        if d == 0.0 {
            return Err(ErrorKind::Num);
        }
        Ok(Cx::new((self.re * o.re + self.im * o.im) / d, (self.im * o.re - self.re * o.im) / d))
    }
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    fn is_zero(self) -> bool {
        self.re == 0.0 && self.im == 0.0
    }
    fn sin(self) -> Cx {
        Cx::new(self.re.sin() * self.im.cosh(), self.re.cos() * self.im.sinh())
    }
    fn cos(self) -> Cx {
        Cx::new(self.re.cos() * self.im.cosh(), -self.re.sin() * self.im.sinh())
    }
    fn sinh(self) -> Cx {
        Cx::new(self.re.sinh() * self.im.cos(), self.re.cosh() * self.im.sin())
    }
    fn cosh(self) -> Cx {
        Cx::new(self.re.cosh() * self.im.cos(), self.re.sinh() * self.im.sin())
    }
    fn exp(self) -> Cx {
        let e = self.re.exp();
        Cx::new(e * self.im.cos(), e * self.im.sin())
    }
    fn ln(self) -> R<Cx> {
        if self.is_zero() {
            return Err(ErrorKind::Num);
        }
        Ok(Cx::new(self.abs().ln(), self.arg()))
    }
    fn recip(self) -> R<Cx> {
        Cx::new(1.0, 0.0).div(self)
    }
}

fn parse_real(s: &str) -> R<f64> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'+' | b'-' | b'e' | b'E')) {
        return Err(ErrorKind::Num);
    }
    s.parse::<f64>().map_err(|_| ErrorKind::Num)
}

/// Parse Excel's complex text form ("3+4i", "-2.5j", "i", "7").
fn parse_complex_text(s: &str) -> R<(Cx, Option<char>)> {
    if s.is_empty() {
        return Ok((Cx::new(0.0, 0.0), None));
    }
    let last = s.chars().last().unwrap();
    if last != 'i' && last != 'j' {
        return Ok((Cx::new(parse_real(s)?, 0.0), None));
    }
    let body = &s[..s.len() - 1];
    // Split at the last sign that isn't the leading one or an exponent sign.
    let bytes = body.as_bytes();
    let mut split = None;
    for k in (1..bytes.len()).rev() {
        if (bytes[k] == b'+' || bytes[k] == b'-') && !matches!(bytes[k - 1], b'e' | b'E') {
            split = Some(k);
            break;
        }
    }
    let (re_s, im_s) = match split {
        Some(k) => (&body[..k], &body[k..]),
        None => ("", body),
    };
    let re = if re_s.is_empty() { 0.0 } else { parse_real(re_s)? };
    let im = match im_s {
        "" | "+" => 1.0,
        "-" => -1.0,
        s => parse_real(s)?,
    };
    Ok((Cx::new(re, im), Some(last)))
}

fn parse_complex(v: &Value) -> R<(Cx, Option<char>)> {
    match v {
        Value::Empty => Ok((Cx::new(0.0, 0.0), None)),
        Value::Number(n) => Ok((Cx::new(*n, 0.0), None)),
        Value::Bool(_) => Err(ErrorKind::Value),
        Value::Error(e) => Err(*e),
        Value::Text(s) => parse_complex_text(s),
        Value::Array(_) => parse_complex(&v.clone().scalar()),
    }
}

fn merge_suffix(a: Option<char>, b: Option<char>) -> R<Option<char>> {
    match (a, b) {
        (Some(x), Some(y)) if x != y => Err(ErrorKind::Value),
        (Some(x), _) => Ok(Some(x)),
        (None, y) => Ok(y),
    }
}

fn format_complex(c: Cx, suffix: Option<char>) -> Value {
    if !c.re.is_finite() || !c.im.is_finite() {
        return Value::Error(ErrorKind::Num);
    }
    let sfx = suffix.unwrap_or('i');
    let re = if c.re == 0.0 { "0".to_string() } else { format_number_general(c.re) };
    let im = if c.im == 0.0 { "0".to_string() } else { format_number_general(c.im) };
    if im == "0" {
        return Value::Text(re);
    }
    let im_part = match im.as_str() {
        "1" => String::new(),
        "-1" => "-".to_string(),
        s => s.to_string(),
    };
    if re == "0" {
        return Value::Text(format!("{im_part}{sfx}"));
    }
    let sign = if im.starts_with('-') { "" } else { "+" };
    Value::Text(format!("{re}{sign}{im_part}{sfx}"))
}

fn cx_result(r: R<Cx>, suffix: Option<char>) -> Value {
    match r {
        Ok(c) => format_complex(c, suffix),
        Err(e) => Value::Error(e),
    }
}

fn unary(a: &[Value], f: fn(Cx) -> R<Cx>) -> Value {
    let (z, s) = tryv!(parse_complex(&a[0]));
    cx_result(f(z), s)
}

fn complex_fn(a: &[Value]) -> Value {
    let re = tryv!(strict_num(a, 0, 0.0));
    let im = tryv!(strict_num(a, 1, 0.0));
    let sfx = match a.get(2) {
        None | Some(Value::Empty) => 'i',
        Some(v) => match tryv!(v.to_text()).as_str() {
            "" | "i" => 'i',
            "j" => 'j',
            _ => return Value::Error(ErrorKind::Value),
        },
    };
    format_complex(Cx::new(re, im), Some(sfx))
}

fn imabs(a: &[Value]) -> Value {
    let (z, _) = tryv!(parse_complex(&a[0]));
    out(z.abs())
}

fn imaginary(a: &[Value]) -> Value {
    let (z, _) = tryv!(parse_complex(&a[0]));
    out(z.im)
}

fn imreal(a: &[Value]) -> Value {
    let (z, _) = tryv!(parse_complex(&a[0]));
    out(z.re)
}

fn imargument(a: &[Value]) -> Value {
    let (z, _) = tryv!(parse_complex(&a[0]));
    if z.is_zero() {
        return Value::Error(ErrorKind::Div0);
    }
    out(z.arg())
}

fn imconjugate(a: &[Value]) -> Value {
    unary(a, |z| Ok(Cx::new(z.re, -z.im)))
}
fn imcos(a: &[Value]) -> Value {
    unary(a, |z| Ok(z.cos()))
}
fn imcosh(a: &[Value]) -> Value {
    unary(a, |z| Ok(z.cosh()))
}
fn imsin(a: &[Value]) -> Value {
    unary(a, |z| Ok(z.sin()))
}
fn imsinh(a: &[Value]) -> Value {
    unary(a, |z| Ok(z.sinh()))
}
fn imtan(a: &[Value]) -> Value {
    unary(a, |z| z.sin().div(z.cos()))
}
fn imcot(a: &[Value]) -> Value {
    unary(a, |z| z.cos().div(z.sin()))
}
fn imsec(a: &[Value]) -> Value {
    unary(a, |z| z.cos().recip())
}
fn imcsc(a: &[Value]) -> Value {
    unary(a, |z| z.sin().recip())
}
fn imsech(a: &[Value]) -> Value {
    unary(a, |z| z.cosh().recip())
}
fn imcsch(a: &[Value]) -> Value {
    unary(a, |z| z.sinh().recip())
}
fn imexp(a: &[Value]) -> Value {
    unary(a, |z| Ok(z.exp()))
}
fn imln(a: &[Value]) -> Value {
    unary(a, |z| z.ln())
}
fn imlog10(a: &[Value]) -> Value {
    unary(a, |z| {
        let l = z.ln()?;
        let k = std::f64::consts::LN_10;
        Ok(Cx::new(l.re / k, l.im / k))
    })
}
fn imlog2(a: &[Value]) -> Value {
    unary(a, |z| {
        let l = z.ln()?;
        let k = std::f64::consts::LN_2;
        Ok(Cx::new(l.re / k, l.im / k))
    })
}

fn polar_pow(z: Cx, n: f64) -> R<Cx> {
    if z.is_zero() {
        return if n > 0.0 { Ok(Cx::new(0.0, 0.0)) } else { Err(ErrorKind::Num) };
    }
    let r = z.abs().powf(n);
    let t = z.arg() * n;
    Ok(Cx::new(r * t.cos(), r * t.sin()))
}

fn imsqrt(a: &[Value]) -> Value {
    unary(a, |z| if z.is_zero() { Ok(z) } else { polar_pow(z, 0.5) })
}

fn impower(a: &[Value]) -> Value {
    let (z, s) = tryv!(parse_complex(&a[0]));
    let n = tryv!(strict_num(a, 1, 0.0));
    cx_result(polar_pow(z, n), s)
}

fn binary(a: &[Value], f: fn(Cx, Cx) -> R<Cx>) -> Value {
    let (x, s1) = tryv!(parse_complex(&a[0]));
    let (y, s2) = tryv!(parse_complex(&a[1]));
    let s = tryv!(merge_suffix(s1, s2));
    cx_result(f(x, y), s)
}

fn imdiv(a: &[Value]) -> Value {
    binary(a, |x, y| x.div(y))
}

fn imsub(a: &[Value]) -> Value {
    binary(a, |x, y| Ok(x.sub(y)))
}

/// Collect complex numbers from arguments (scalars, arrays or ranges).
fn collect_complex(ctx: &mut Ctx, args: &[Expr]) -> R<(Vec<Cx>, Option<char>)> {
    let mut vals = Vec::new();
    let mut sfx = None;
    let mut err = None;
    ctx.for_each_value(args, &mut |v, from_ref| {
        if from_ref && v.is_empty() {
            return Ok(());
        }
        let (z, s) = parse_complex(v)?;
        match merge_suffix(sfx, s) {
            Ok(m) => sfx = m,
            Err(e) => {
                err = Some(e);
                return Err(e);
            }
        }
        vals.push(z);
        Ok(())
    })?;
    if let Some(e) = err {
        return Err(e);
    }
    Ok((vals, sfx))
}

fn imsum(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let (vals, s) = tryv!(collect_complex(ctx, args));
    let sum = vals.into_iter().fold(Cx::new(0.0, 0.0), Cx::add);
    format_complex(sum, s)
}

fn improduct(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let (vals, s) = tryv!(collect_complex(ctx, args));
    let prod = vals.into_iter().fold(Cx::new(1.0, 0.0), Cx::mul);
    format_complex(prod, s)
}
