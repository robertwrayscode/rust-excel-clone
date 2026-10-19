//! Math and trigonometry functions.

use super::stats::{self, numeric_matrix, paired, res, round15, special};
use super::{num, num_or, text, Registry, VAR};
use crate::ast::Expr;
use crate::eval::{Ctx, Val};
use crate::value::{Array, ErrorKind, Value};

const CAT: &str = "Math & Trig";

type R<T> = Result<T, ErrorKind>;

/// Evaluate a closure producing a number and wrap it as a Value.
fn sres(f: impl FnOnce() -> R<f64>) -> Value {
    res(f())
}

fn chk(cond: bool, e: ErrorKind) -> R<()> {
    if cond {
        Ok(())
    } else {
        Err(e)
    }
}

/// One-argument numeric function.
fn f1(a: &[Value], f: impl FnOnce(f64) -> R<f64>) -> Value {
    sres(|| f(num(a, 0)?))
}

/// Excel rejects trig arguments with |x| >= 2^27.
fn trig_arg(x: f64) -> R<f64> {
    chk(x.abs() < 134_217_728.0, ErrorKind::Num)?;
    Ok(x)
}

pub fn register(r: &mut Registry) {
    // ----- elementary -----
    r.scalar("ABS", 1, 1, |a| f1(a, |x| Ok(x.abs())), CAT, "ABS(number)", "Returns the absolute value of a number.");
    r.scalar("ACOS", 1, 1, |a| f1(a, |x| { chk(x.abs() <= 1.0, ErrorKind::Num)?; Ok(x.acos()) }), CAT, "ACOS(number)", "Returns the arccosine of a number.");
    r.scalar("ACOSH", 1, 1, |a| f1(a, |x| { chk(x >= 1.0, ErrorKind::Num)?; Ok(x.acosh()) }), CAT, "ACOSH(number)", "Returns the inverse hyperbolic cosine of a number.");
    r.scalar("ACOT", 1, 1, |a| f1(a, |x| Ok(std::f64::consts::FRAC_PI_2 - x.atan())), CAT, "ACOT(number)", "Returns the arccotangent of a number.");
    r.scalar("ACOTH", 1, 1, |a| f1(a, |x| { chk(x.abs() > 1.0, ErrorKind::Num)?; Ok(0.5 * ((x + 1.0) / (x - 1.0)).ln()) }), CAT, "ACOTH(number)", "Returns the hyperbolic arccotangent of a number.");
    r.scalar("ASIN", 1, 1, |a| f1(a, |x| { chk(x.abs() <= 1.0, ErrorKind::Num)?; Ok(x.asin()) }), CAT, "ASIN(number)", "Returns the arcsine of a number.");
    r.scalar("ASINH", 1, 1, |a| f1(a, |x| Ok(x.asinh())), CAT, "ASINH(number)", "Returns the inverse hyperbolic sine of a number.");
    r.scalar("ATAN", 1, 1, |a| f1(a, |x| Ok(x.atan())), CAT, "ATAN(number)", "Returns the arctangent of a number.");
    r.scalar("ATAN2", 2, 2, atan2, CAT, "ATAN2(x_num, y_num)", "Returns the arctangent from x- and y-coordinates.");
    r.scalar("ATANH", 1, 1, |a| f1(a, |x| { chk(x.abs() < 1.0, ErrorKind::Num)?; Ok(x.atanh()) }), CAT, "ATANH(number)", "Returns the inverse hyperbolic tangent of a number.");
    r.scalar("COS", 1, 1, |a| f1(a, |x| Ok(trig_arg(x)?.cos())), CAT, "COS(number)", "Returns the cosine of a number.");
    r.scalar("COSH", 1, 1, |a| f1(a, |x| Ok(x.cosh())), CAT, "COSH(number)", "Returns the hyperbolic cosine of a number.");
    r.scalar("COT", 1, 1, |a| f1(a, |x| { trig_arg(x)?; chk(x != 0.0, ErrorKind::Div0)?; Ok(1.0 / x.tan()) }), CAT, "COT(number)", "Returns the cotangent of an angle.");
    r.scalar("COTH", 1, 1, |a| f1(a, |x| { trig_arg(x)?; chk(x != 0.0, ErrorKind::Div0)?; Ok(1.0 / x.tanh()) }), CAT, "COTH(number)", "Returns the hyperbolic cotangent of a number.");
    r.scalar("CSC", 1, 1, |a| f1(a, |x| { trig_arg(x)?; chk(x != 0.0, ErrorKind::Div0)?; Ok(1.0 / x.sin()) }), CAT, "CSC(number)", "Returns the cosecant of an angle.");
    r.scalar("CSCH", 1, 1, |a| f1(a, |x| { trig_arg(x)?; chk(x != 0.0, ErrorKind::Div0)?; Ok(1.0 / x.sinh()) }), CAT, "CSCH(number)", "Returns the hyperbolic cosecant of an angle.");
    r.scalar("SEC", 1, 1, |a| f1(a, |x| Ok(1.0 / trig_arg(x)?.cos())), CAT, "SEC(number)", "Returns the secant of an angle.");
    r.scalar("SECH", 1, 1, |a| f1(a, |x| Ok(1.0 / trig_arg(x)?.cosh())), CAT, "SECH(number)", "Returns the hyperbolic secant of an angle.");
    r.scalar("SIN", 1, 1, |a| f1(a, |x| Ok(trig_arg(x)?.sin())), CAT, "SIN(number)", "Returns the sine of the given angle.");
    r.scalar("SINH", 1, 1, |a| f1(a, |x| Ok(x.sinh())), CAT, "SINH(number)", "Returns the hyperbolic sine of a number.");
    r.scalar("TAN", 1, 1, |a| f1(a, |x| Ok(trig_arg(x)?.tan())), CAT, "TAN(number)", "Returns the tangent of a number.");
    r.scalar("TANH", 1, 1, |a| f1(a, |x| Ok(x.tanh())), CAT, "TANH(number)", "Returns the hyperbolic tangent of a number.");
    r.scalar("DEGREES", 1, 1, |a| f1(a, |x| Ok(x.to_degrees())), CAT, "DEGREES(angle)", "Converts radians to degrees.");
    r.scalar("RADIANS", 1, 1, |a| f1(a, |x| Ok(x.to_radians())), CAT, "RADIANS(angle)", "Converts degrees to radians.");
    r.scalar("PI", 0, 0, |_| Value::Number(std::f64::consts::PI), CAT, "PI()", "Returns the value of pi.");
    r.scalar("EXP", 1, 1, |a| f1(a, |x| Ok(x.exp())), CAT, "EXP(number)", "Returns e raised to the power of a given number.");
    r.scalar("LN", 1, 1, |a| f1(a, |x| { chk(x > 0.0, ErrorKind::Num)?; Ok(x.ln()) }), CAT, "LN(number)", "Returns the natural logarithm of a number.");
    r.scalar("LOG10", 1, 1, |a| f1(a, |x| { chk(x > 0.0, ErrorKind::Num)?; Ok(x.log10()) }), CAT, "LOG10(number)", "Returns the base-10 logarithm of a number.");
    r.scalar("LOG", 1, 2, log, CAT, "LOG(number, [base])", "Returns the logarithm of a number to the base you specify.");
    r.scalar("SQRT", 1, 1, |a| f1(a, |x| { chk(x >= 0.0, ErrorKind::Num)?; Ok(x.sqrt()) }), CAT, "SQRT(number)", "Returns a positive square root.");
    r.scalar("SQRTPI", 1, 1, |a| f1(a, |x| { chk(x >= 0.0, ErrorKind::Num)?; Ok((x * std::f64::consts::PI).sqrt()) }), CAT, "SQRTPI(number)", "Returns the square root of (number * pi).");
    r.scalar("POWER", 2, 2, power, CAT, "POWER(number, power)", "Returns the result of a number raised to a power.");
    r.scalar("SIGN", 1, 1, |a| f1(a, |x| Ok(if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 })), CAT, "SIGN(number)", "Returns the sign of a number.");
    r.scalar("MOD", 2, 2, mod_fn, CAT, "MOD(number, divisor)", "Returns the remainder from division.");
    r.scalar("QUOTIENT", 2, 2, quotient, CAT, "QUOTIENT(numerator, denominator)", "Returns the integer portion of a division.");
    r.scalar("FACT", 1, 1, |a| f1(a, fact), CAT, "FACT(number)", "Returns the factorial of a number.");
    r.scalar("FACTDOUBLE", 1, 1, |a| f1(a, factdouble), CAT, "FACTDOUBLE(number)", "Returns the double factorial of a number.");
    r.scalar("COMBIN", 2, 2, combin_fn, CAT, "COMBIN(number, number_chosen)", "Returns the number of combinations for a given number of objects.");
    r.scalar("COMBINA", 2, 2, combina, CAT, "COMBINA(number, number_chosen)", "Returns the number of combinations with repetitions for a given number of items.");
    r.raw("GCD", 1, VAR, gcd, CAT, "GCD(number1, [number2], ...)", "Returns the greatest common divisor.");
    r.raw("LCM", 1, VAR, lcm, CAT, "LCM(number1, [number2], ...)", "Returns the least common multiple.");
    r.raw("MULTINOMIAL", 1, VAR, multinomial, CAT, "MULTINOMIAL(number1, [number2], ...)", "Returns the multinomial of a set of numbers.");
    r.raw("SERIESSUM", 4, 4, seriessum, CAT, "SERIESSUM(x, n, m, coefficients)", "Returns the sum of a power series based on the formula.");

    // ----- rounding -----
    r.scalar("ROUND", 2, 2, |a| sres(|| Ok(round_digits(num(a, 0)?, num(a, 1)?.trunc(), Rounding::Half))), CAT, "ROUND(number, num_digits)", "Rounds a number to a specified number of digits.");
    r.scalar("ROUNDDOWN", 2, 2, |a| sres(|| Ok(round_digits(num(a, 0)?, num(a, 1)?.trunc(), Rounding::Down))), CAT, "ROUNDDOWN(number, num_digits)", "Rounds a number down, toward zero.");
    r.scalar("ROUNDUP", 2, 2, |a| sres(|| Ok(round_digits(num(a, 0)?, num(a, 1)?.trunc(), Rounding::Up))), CAT, "ROUNDUP(number, num_digits)", "Rounds a number up, away from zero.");
    r.scalar("TRUNC", 1, 2, |a| sres(|| Ok(round_digits(num(a, 0)?, num_or(a, 1, 0.0)?.trunc(), Rounding::Down))), CAT, "TRUNC(number, [num_digits])", "Truncates a number to an integer (or to the given precision).");
    r.scalar("INT", 1, 1, |a| f1(a, |x| Ok(x.floor())), CAT, "INT(number)", "Rounds a number down to the nearest integer.");
    r.scalar("EVEN", 1, 1, |a| f1(a, |x| Ok(even_odd(x, true))), CAT, "EVEN(number)", "Rounds a number up to the nearest even integer.");
    r.scalar("ODD", 1, 1, |a| f1(a, |x| Ok(even_odd(x, false))), CAT, "ODD(number)", "Rounds a number up to the nearest odd integer.");
    r.scalar("MROUND", 2, 2, mround, CAT, "MROUND(number, multiple)", "Returns a number rounded to the desired multiple.");
    r.scalar("CEILING", 2, 2, ceiling, CAT, "CEILING(number, significance)", "Rounds a number up to the nearest multiple of significance.");
    r.scalar("CEILING.MATH", 1, 3, ceiling_math, CAT, "CEILING.MATH(number, [significance], [mode])", "Rounds a number up to the nearest integer or multiple of significance.");
    r.scalar("CEILING.PRECISE", 1, 2, ceiling_precise, CAT, "CEILING.PRECISE(number, [significance])", "Rounds a number up to the nearest integer or multiple of significance, regardless of sign.");
    r.scalar("ISO.CEILING", 1, 2, ceiling_precise, CAT, "ISO.CEILING(number, [significance])", "Rounds a number up to the nearest integer or multiple of significance, regardless of sign.");
    r.scalar("FLOOR", 2, 2, floor, CAT, "FLOOR(number, significance)", "Rounds a number down, toward zero, to the nearest multiple of significance.");
    r.scalar("FLOOR.MATH", 1, 3, floor_math, CAT, "FLOOR.MATH(number, [significance], [mode])", "Rounds a number down to the nearest integer or multiple of significance.");
    r.scalar("FLOOR.PRECISE", 1, 2, floor_precise, CAT, "FLOOR.PRECISE(number, [significance])", "Rounds a number down to the nearest integer or multiple of significance, regardless of sign.");

    // ----- conversion -----
    r.scalar("ARABIC", 1, 1, arabic, CAT, "ARABIC(text)", "Converts a Roman numeral to an Arabic numeral.");
    r.scalar("ROMAN", 1, 2, roman, CAT, "ROMAN(number, [form])", "Converts an Arabic numeral to Roman, as text.");
    r.scalar("BASE", 2, 3, base, CAT, "BASE(number, radix, [min_length])", "Converts a number into a text representation with the given radix (base).");
    r.scalar("DECIMAL", 2, 2, decimal, CAT, "DECIMAL(text, radix)", "Converts a text representation of a number in a given base into a decimal number.");

    // ----- random -----
    r.scalar("RAND", 0, 0, |_| Value::Number(rand_f64()), CAT, "RAND()", "Returns a random number between 0 and 1.");
    r.scalar("RANDBETWEEN", 2, 2, randbetween, CAT, "RANDBETWEEN(bottom, top)", "Returns a random number between the numbers you specify.");

    // ----- matrices -----
    r.raw("MDETERM", 1, 1, mdeterm, CAT, "MDETERM(array)", "Returns the matrix determinant of an array.");
    r.raw("MINVERSE", 1, 1, minverse, CAT, "MINVERSE(array)", "Returns the matrix inverse of an array.");
    r.raw("MMULT", 2, 2, mmult, CAT, "MMULT(array1, array2)", "Returns the matrix product of two arrays.");
    r.scalar("MUNIT", 1, 1, munit, CAT, "MUNIT(dimension)", "Returns the unit matrix for the specified dimension.");

    // ----- sums and aggregates -----
    r.raw("SUM", 1, VAR, sum, CAT, "SUM(number1, [number2], ...)", "Adds all the numbers.");
    r.raw("PRODUCT", 1, VAR, product, CAT, "PRODUCT(number1, [number2], ...)", "Multiplies all the numbers given as arguments.");
    r.raw("SUMSQ", 1, VAR, sumsq, CAT, "SUMSQ(number1, [number2], ...)", "Returns the sum of the squares of the arguments.");
    r.raw("SUMIF", 2, 3, sumif, CAT, "SUMIF(range, criteria, [sum_range])", "Adds the cells specified by a given criteria.");
    r.raw("SUMIFS", 3, VAR, sumifs, CAT, "SUMIFS(sum_range, criteria_range1, criteria1, [criteria_range2, criteria2], ...)", "Adds the cells in a range that meet multiple criteria.");
    r.raw("SUMPRODUCT", 1, VAR, sumproduct, CAT, "SUMPRODUCT(array1, [array2], [array3], ...)", "Returns the sum of the products of corresponding array components.");
    r.raw("SUMX2MY2", 2, 2, |c, a| sumx(c, a, |x, y| x * x - y * y), CAT, "SUMX2MY2(array_x, array_y)", "Returns the sum of the difference of squares of corresponding values in two arrays.");
    r.raw("SUMX2PY2", 2, 2, |c, a| sumx(c, a, |x, y| x * x + y * y), CAT, "SUMX2PY2(array_x, array_y)", "Returns the sum of the sum of squares of corresponding values in two arrays.");
    r.raw("SUMXMY2", 2, 2, |c, a| sumx(c, a, |x, y| (x - y) * (x - y)), CAT, "SUMXMY2(array_x, array_y)", "Returns the sum of squares of differences of corresponding values in two arrays.");
    r.raw("SUBTOTAL", 2, VAR, subtotal, CAT, "SUBTOTAL(function_num, ref1, [ref2], ...)", "Returns a subtotal in a list or database.");
    r.raw("AGGREGATE", 3, VAR, aggregate, CAT, "AGGREGATE(function_num, options, ref1, [ref2], ...)", "Returns an aggregate in a list or database, optionally ignoring hidden rows and errors.");
}

// ===========================================================================
// Elementary
// ===========================================================================

fn atan2(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let y = num(a, 1)?;
        chk(x != 0.0 || y != 0.0, ErrorKind::Div0)?;
        Ok(y.atan2(x))
    })
}

fn log(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let b = num_or(a, 1, 10.0)?;
        chk(x > 0.0 && b > 0.0, ErrorKind::Num)?;
        chk(b != 1.0, ErrorKind::Div0)?;
        if b == 10.0 {
            return Ok(x.log10());
        }
        Ok(x.ln() / b.ln())
    })
}

fn power(a: &[Value]) -> Value {
    sres(|| {
        let x = num(a, 0)?;
        let y = num(a, 1)?;
        if x == 0.0 {
            chk(y != 0.0, ErrorKind::Num)?;
            chk(y > 0.0, ErrorKind::Div0)?;
        }
        Ok(x.powf(y))
    })
}

fn mod_fn(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let d = num(a, 1)?;
        chk(d != 0.0, ErrorKind::Div0)?;
        let q = (n / d).floor();
        let mut r = n - d * q;
        // guard against binary noise making |r| reach |d|
        if r != 0.0 && (r.abs() >= d.abs() || (r < 0.0) != (d < 0.0)) {
            r = 0.0;
        }
        if (r - d).abs() < d.abs() * 1e-15 {
            r = 0.0;
        }
        Ok(r)
    })
}

fn quotient(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let d = num(a, 1)?;
        chk(d != 0.0, ErrorKind::Div0)?;
        Ok((n / d).trunc())
    })
}

fn fact(x: f64) -> R<f64> {
    chk(x >= 0.0, ErrorKind::Num)?;
    let n = x.trunc();
    chk(n <= 170.0, ErrorKind::Num)?;
    let mut r = 1.0;
    let mut i = 2.0;
    while i <= n {
        r *= i;
        i += 1.0;
    }
    Ok(r)
}

fn factdouble(x: f64) -> R<f64> {
    chk(x >= -1.0, ErrorKind::Num)?;
    let n = x.trunc();
    let mut r = 1.0;
    let mut i = n;
    while i > 1.0 {
        r *= i;
        if !r.is_finite() {
            return Err(ErrorKind::Num);
        }
        i -= 2.0;
    }
    Ok(r)
}

/// Binomial coefficient for non-negative integers n >= k.
pub(crate) fn combin(n: f64, k: f64) -> f64 {
    let k = k.min(n - k);
    if k < 0.0 {
        return 0.0;
    }
    if k > 1e6 {
        return (special::lgamma(n + 1.0) - special::lgamma(k + 1.0) - special::lgamma(n - k + 1.0)).exp().round();
    }
    let mut r = 1.0;
    let mut i = 1.0;
    while i <= k {
        r = r * (n - k + i) / i;
        i += 1.0;
    }
    if r < 9.007_199_254_740_992e15 {
        r.round()
    } else {
        r
    }
}

fn combin_fn(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?.trunc();
        let k = num(a, 1)?.trunc();
        chk(n >= 0.0 && k >= 0.0 && n >= k, ErrorKind::Num)?;
        Ok(combin(n, k))
    })
}

fn combina(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?.trunc();
        let k = num(a, 1)?.trunc();
        chk(n >= 0.0 && k >= 0.0, ErrorKind::Num)?;
        if n == 0.0 && k == 0.0 {
            return Ok(1.0);
        }
        if n == 0.0 {
            return Ok(0.0);
        }
        Ok(combin(n + k - 1.0, k))
    })
}

fn integer_args(ctx: &mut Ctx, args: &[Expr]) -> R<Vec<f64>> {
    let v = ctx.collect_numbers(args)?;
    let mut out = Vec::with_capacity(v.len());
    for x in v {
        chk((0.0..9.007_199_254_740_992e15).contains(&x), ErrorKind::Num)?;
        out.push(x.trunc());
    }
    Ok(out)
}

fn gcd2(mut a: f64, mut b: f64) -> f64 {
    while b != 0.0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn gcd(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| Ok(integer_args(ctx, args)?.into_iter().fold(0.0, gcd2)))
}

fn lcm(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let v = integer_args(ctx, args)?;
        let mut l = 1.0;
        for x in v {
            if x == 0.0 {
                return Ok(0.0);
            }
            l = l / gcd2(l, x) * x;
        }
        Ok(l)
    })
}

fn multinomial(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let v = ctx.collect_numbers(args)?;
        let mut total = 0.0;
        let mut r = 1.0;
        for x in v {
            chk(x >= 0.0, ErrorKind::Num)?;
            let x = x.trunc();
            total += x;
            r *= combin(total, x);
        }
        Ok(r)
    })
}

fn seriessum(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let x = ctx.eval_number(&args[0])?;
        let n = ctx.eval_number(&args[1])?;
        let m = ctx.eval_number(&args[2])?;
        let coef = ctx.eval_array(&args[3])?;
        let mut s = 0.0;
        for (i, c) in coef.data.iter().enumerate() {
            let c = match c {
                Value::Number(c) => *c,
                Value::Error(e) => return Err(*e),
                _ => return Err(ErrorKind::Value),
            };
            s += c * x.powf(n + i as f64 * m);
        }
        Ok(s)
    })
}

// ===========================================================================
// Rounding
// ===========================================================================

#[derive(Clone, Copy)]
enum Rounding {
    Half,
    Down,
    Up,
}

fn round_int(x: f64, mode: Rounding) -> f64 {
    let x = round15(x);
    match mode {
        Rounding::Half => x.round(),
        Rounding::Down => x.trunc(),
        Rounding::Up => {
            if x >= 0.0 {
                x.ceil()
            } else {
                x.floor()
            }
        }
    }
}

fn round_digits(x: f64, digits: f64, mode: Rounding) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    if digits >= 0.0 {
        if digits > 308.0 {
            return x;
        }
        let p = 10f64.powi(digits as i32);
        let scaled = x * p;
        if !scaled.is_finite() || scaled.abs() >= 1e17 {
            return x;
        }
        round_int(scaled, mode) / p
    } else {
        if digits < -308.0 {
            return 0.0;
        }
        let p = 10f64.powi((-digits) as i32);
        round_int(x / p, mode) * p
    }
}

fn even_odd(x: f64, even: bool) -> f64 {
    let s = if x < 0.0 { -1.0 } else { 1.0 };
    let a = round15(x.abs()).ceil();
    let r = if even {
        if a % 2.0 == 0.0 {
            a
        } else {
            a + 1.0
        }
    } else if a % 2.0 == 1.0 {
        a
    } else {
        a + 1.0
    };
    s * r
}

/// round(x/s) style helpers with binary-noise protection.
fn q_floor(x: f64, s: f64) -> f64 {
    round15(round15(x / s).floor() * s)
}
fn q_ceil(x: f64, s: f64) -> f64 {
    round15(round15(x / s).ceil() * s)
}

fn mround(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let m = num(a, 1)?;
        if m == 0.0 || n == 0.0 {
            return Ok(0.0);
        }
        chk((n > 0.0) == (m > 0.0), ErrorKind::Num)?;
        Ok(round15(round_int(n / m, Rounding::Half) * m))
    })
}

fn ceiling(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let s = num(a, 1)?;
        if n == 0.0 || s == 0.0 {
            return Ok(0.0);
        }
        chk(!(n > 0.0 && s < 0.0), ErrorKind::Num)?;
        if n < 0.0 && s < 0.0 {
            Ok(-q_ceil(-n, -s))
        } else {
            Ok(q_ceil(n, s))
        }
    })
}

fn ceiling_math(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let s = num_or(a, 1, 1.0)?.abs();
        let mode = num_or(a, 2, 0.0)?;
        if s == 0.0 || n == 0.0 {
            return Ok(0.0);
        }
        if n < 0.0 && mode != 0.0 {
            Ok(-q_ceil(-n, s))
        } else {
            Ok(q_ceil(n, s))
        }
    })
}

fn ceiling_precise(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let s = num_or(a, 1, 1.0)?.abs();
        if s == 0.0 || n == 0.0 {
            return Ok(0.0);
        }
        Ok(q_ceil(n, s))
    })
}

fn floor(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let s = num(a, 1)?;
        if n == 0.0 {
            return Ok(0.0);
        }
        chk(s != 0.0, ErrorKind::Div0)?;
        chk(!(n > 0.0 && s < 0.0), ErrorKind::Num)?;
        if n < 0.0 && s < 0.0 {
            Ok(-q_floor(-n, -s))
        } else {
            Ok(q_floor(n, s))
        }
    })
}

fn floor_math(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let s = num_or(a, 1, 1.0)?.abs();
        let mode = num_or(a, 2, 0.0)?;
        if s == 0.0 || n == 0.0 {
            return Ok(0.0);
        }
        if n < 0.0 && mode != 0.0 {
            Ok(-q_floor(-n, s))
        } else {
            Ok(q_floor(n, s))
        }
    })
}

fn floor_precise(a: &[Value]) -> Value {
    sres(|| {
        let n = num(a, 0)?;
        let s = num_or(a, 1, 1.0)?.abs();
        if s == 0.0 || n == 0.0 {
            return Ok(0.0);
        }
        Ok(q_floor(n, s))
    })
}

// ===========================================================================
// Conversion
// ===========================================================================

fn roman_value(c: char) -> Option<i64> {
    Some(match c.to_ascii_uppercase() {
        'I' => 1,
        'V' => 5,
        'X' => 10,
        'L' => 50,
        'C' => 100,
        'D' => 500,
        'M' => 1000,
        _ => return None,
    })
}

fn arabic(a: &[Value]) -> Value {
    let s = match text(a, 0) {
        Ok(s) => s,
        Err(e) => return Value::Error(e),
    };
    let t = s.trim();
    if t.len() > 255 {
        return Value::Error(ErrorKind::Value);
    }
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t),
    };
    let mut vals = Vec::new();
    for c in body.chars() {
        match roman_value(c) {
            Some(v) => vals.push(v),
            None => return Value::Error(ErrorKind::Value),
        }
    }
    let mut total = 0i64;
    for i in 0..vals.len() {
        if i + 1 < vals.len() && vals[i] < vals[i + 1] {
            total -= vals[i];
        } else {
            total += vals[i];
        }
    }
    Value::Number(if neg { -total } else { total } as f64)
}

fn roman(a: &[Value]) -> Value {
    let r = (|| -> R<String> {
        let n = num(a, 0)?.trunc();
        let form = match a.get(1) {
            None | Some(Value::Empty) => 0.0,
            Some(Value::Bool(true)) => 0.0,
            Some(Value::Bool(false)) => 4.0,
            Some(v) => v.to_number()?.trunc(),
        };
        chk((0.0..=3999.0).contains(&n), ErrorKind::Value)?;
        chk((0.0..=4.0).contains(&form), ErrorKind::Value)?;
        const CHARS: [char; 7] = ['M', 'D', 'C', 'L', 'X', 'V', 'I'];
        const VALUES: [i64; 7] = [1000, 500, 100, 50, 10, 5, 1];
        let max_index = VALUES.len() - 1;
        let mode = form as usize;
        let mut val = n as i64;
        let mut out = String::new();
        for i in 0..=max_index / 2 {
            let mut index = 2 * i;
            let digit = val / VALUES[index];
            if digit % 5 == 4 {
                let index2 = if digit == 4 { index - 1 } else { index - 2 };
                let mut steps = 0;
                while steps < mode && index < max_index {
                    steps += 1;
                    if VALUES[index2] - VALUES[index + 1] <= val {
                        index += 1;
                    } else {
                        steps = mode;
                    }
                }
                out.push(CHARS[index]);
                out.push(CHARS[index2]);
                val += VALUES[index];
                val -= VALUES[index2];
            } else {
                if digit > 4 {
                    out.push(CHARS[index - 1]);
                }
                for _ in 0..digit % 5 {
                    out.push(CHARS[index]);
                }
                val %= VALUES[index];
            }
        }
        Ok(out)
    })();
    match r {
        Ok(s) => Value::Text(s),
        Err(e) => Value::Error(e),
    }
}

fn base(a: &[Value]) -> Value {
    let r = (|| -> R<String> {
        let n = num(a, 0)?.trunc();
        let radix = num(a, 1)?.trunc();
        let min_len = num_or(a, 2, 0.0)?.trunc();
        chk((0.0..9.007_199_254_740_992e15).contains(&n), ErrorKind::Num)?;
        chk((2.0..=36.0).contains(&radix), ErrorKind::Num)?;
        chk((0.0..=255.0).contains(&min_len), ErrorKind::Num)?;
        let radix = radix as u64;
        let mut v = n as u64;
        let mut digits = Vec::new();
        loop {
            digits.push(std::char::from_digit((v % radix) as u32, radix as u32).unwrap().to_ascii_uppercase());
            v /= radix;
            if v == 0 {
                break;
            }
        }
        while digits.len() < min_len as usize {
            digits.push('0');
        }
        Ok(digits.into_iter().rev().collect())
    })();
    match r {
        Ok(s) => Value::Text(s),
        Err(e) => Value::Error(e),
    }
}

fn decimal(a: &[Value]) -> Value {
    sres(|| {
        let s = text(a, 0)?;
        let radix = num(a, 1)?.trunc();
        chk((2.0..=36.0).contains(&radix), ErrorKind::Num)?;
        chk(s.len() <= 255, ErrorKind::Value)?;
        let mut v = 0.0;
        for c in s.trim().chars() {
            let d = c.to_digit(radix as u32).ok_or(ErrorKind::Num)?;
            v = v * radix + d as f64;
        }
        chk(v < 9.007_199_254_740_992e15, ErrorKind::Num)?;
        Ok(v)
    })
}

// ===========================================================================
// Random numbers
// ===========================================================================

fn rand_f64() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|s| {
        let mut x = s.get();
        if x == 0 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15);
            x = nanos ^ (&x as *const u64 as u64).rotate_left(32) ^ 0x2545_F491_4F6C_DD1D;
            if x == 0 {
                x = 0x9E37_79B9_7F4A_7C15;
            }
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (r >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn randbetween(a: &[Value]) -> Value {
    sres(|| {
        let lo = num(a, 0)?.ceil();
        let hi = num(a, 1)?.floor();
        chk(lo <= hi, ErrorKind::Num)?;
        Ok((lo + (rand_f64() * (hi - lo + 1.0)).floor()).min(hi))
    })
}

// ===========================================================================
// Matrices
// ===========================================================================

fn square_matrix(ctx: &mut Ctx, e: &Expr) -> R<(usize, Vec<f64>)> {
    let (r, c, m) = numeric_matrix(ctx, e)?;
    chk(r == c, ErrorKind::Value)?;
    Ok((r, m))
}

/// LU decomposition with partial pivoting; returns determinant.
fn determinant(n: usize, mut m: Vec<f64>) -> f64 {
    let mut det = 1.0;
    for col in 0..n {
        let piv = (col..n).max_by(|a, b| m[a * n + col].abs().partial_cmp(&m[b * n + col].abs()).unwrap()).unwrap();
        if m[piv * n + col] == 0.0 {
            return 0.0;
        }
        if piv != col {
            for j in 0..n {
                m.swap(piv * n + j, col * n + j);
            }
            det = -det;
        }
        let p = m[col * n + col];
        det *= p;
        for r in col + 1..n {
            let f = m[r * n + col] / p;
            if f != 0.0 {
                for j in col..n {
                    m[r * n + j] -= f * m[col * n + j];
                }
            }
        }
    }
    det
}

fn mdeterm(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let (n, m) = square_matrix(ctx, &args[0])?;
        Ok(determinant(n, m))
    })
}

fn minverse(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let r = (|| -> R<Value> {
        let (n, mut m) = square_matrix(ctx, &args[0])?;
        let mut inv = vec![0.0; n * n];
        for i in 0..n {
            inv[i * n + i] = 1.0;
        }
        let scale = m.iter().fold(0.0f64, |a, b| a.max(b.abs()));
        for col in 0..n {
            let piv = (col..n).max_by(|a, b| m[a * n + col].abs().partial_cmp(&m[b * n + col].abs()).unwrap()).unwrap();
            if m[piv * n + col].abs() <= scale * 1e-15 {
                return Err(ErrorKind::Num);
            }
            if piv != col {
                for j in 0..n {
                    m.swap(piv * n + j, col * n + j);
                    inv.swap(piv * n + j, col * n + j);
                }
            }
            let p = m[col * n + col];
            for j in 0..n {
                m[col * n + j] /= p;
                inv[col * n + j] /= p;
            }
            for r in 0..n {
                if r != col {
                    let f = m[r * n + col];
                    if f != 0.0 {
                        for j in 0..n {
                            m[r * n + j] -= f * m[col * n + j];
                            inv[r * n + j] -= f * inv[col * n + j];
                        }
                    }
                }
            }
        }
        Ok(Value::from(Array::new(n, n, inv.into_iter().map(Value::number).collect())))
    })();
    r.unwrap_or_else(Value::Error)
}

fn mmult(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let r = (|| -> R<Value> {
        let (ar, ac, a) = numeric_matrix(ctx, &args[0])?;
        let (br, bc, b) = numeric_matrix(ctx, &args[1])?;
        chk(ac == br, ErrorKind::Value)?;
        let mut out = Vec::with_capacity(ar * bc);
        for i in 0..ar {
            for j in 0..bc {
                out.push(Value::number((0..ac).map(|k| a[i * ac + k] * b[k * bc + j]).sum()));
            }
        }
        Ok(Value::from(Array::new(ar, bc, out)))
    })();
    r.unwrap_or_else(Value::Error)
}

fn munit(a: &[Value]) -> Value {
    let n = match num(a, 0) {
        Ok(n) => n.trunc(),
        Err(e) => return Value::Error(e),
    };
    if !(1.0..=32767.0).contains(&n) {
        return Value::Error(ErrorKind::Value);
    }
    let n = n as usize;
    let mut data = vec![Value::Number(0.0); n * n];
    for i in 0..n {
        data[i * n + i] = Value::Number(1.0);
    }
    Value::from(Array::new(n, n, data))
}

// ===========================================================================
// Sums and aggregates
// ===========================================================================

fn sum(ctx: &mut Ctx, args: &[Expr]) -> Value {
    match ctx.collect_numbers(args) {
        Ok(v) => Value::number(v.iter().sum()),
        Err(e) => Value::Error(e),
    }
}

fn product(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| Ok(stats::product(&ctx.collect_numbers(args)?)))
}

fn sumsq(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| Ok(ctx.collect_numbers(args)?.iter().map(|x| x * x).sum()))
}

fn sumif(ctx: &mut Ctx, args: &[Expr]) -> Value {
    stats::if_single(ctx, args, stats::agg_sum)
}

fn sumifs(ctx: &mut Ctx, args: &[Expr]) -> Value {
    stats::if_multi(ctx, args, true, stats::agg_sum)
}

fn sumproduct(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let mut arrays = Vec::with_capacity(args.len());
        for a in args {
            arrays.push(ctx.eval_array(a)?);
        }
        let (rows, cols) = (arrays[0].rows, arrays[0].cols);
        chk(arrays.iter().all(|a| a.rows == rows && a.cols == cols), ErrorKind::Value)?;
        let mut total = 0.0;
        for i in 0..rows * cols {
            let mut p = 1.0;
            for a in &arrays {
                match &a.data[i] {
                    Value::Number(n) => p *= n,
                    Value::Error(e) => return Err(*e),
                    _ => p = 0.0,
                }
            }
            total += p;
        }
        Ok(total)
    })
}

fn sumx(ctx: &mut Ctx, args: &[Expr], f: fn(f64, f64) -> f64) -> Value {
    sres(|| {
        let (x, y) = paired(ctx, &args[0], &args[1])?;
        Ok(x.iter().zip(&y).map(|(a, b)| f(*a, *b)).sum())
    })
}

/// Does a formula contain a SUBTOTAL or AGGREGATE call?
fn has_subtotal(e: &Expr) -> bool {
    match e {
        Expr::Call(n, args) => n == "SUBTOTAL" || n == "AGGREGATE" || args.iter().any(has_subtotal),
        Expr::CallExpr(f, args) => has_subtotal(f) || args.iter().any(has_subtotal),
        Expr::Unary(_, x) | Expr::Paren(x) => has_subtotal(x),
        Expr::Binary(_, a, b) => has_subtotal(a) || has_subtotal(b),
        Expr::Array(rows) => rows.iter().flatten().any(has_subtotal),
        _ => false,
    }
}

/// Values for SUBTOTAL/AGGREGATE, skipping hidden rows and nested subtotals as requested.
fn agg_values(ctx: &mut Ctx, args: &[Expr], skip_hidden: bool, skip_nested: bool) -> R<Vec<Value>> {
    let mut out = Vec::new();
    for a in args {
        match ctx.eval(a) {
            Val::Refs(areas) => {
                for area in areas {
                    for (p, v) in ctx.area_values(area) {
                        let sheet = &ctx.wb.sheets[area.sheet];
                        if skip_hidden && sheet.hidden_rows.contains(&p.row) {
                            continue;
                        }
                        if skip_nested {
                            if let Some(f) = sheet.cells.get(&p).and_then(|c| c.formula.as_deref()) {
                                if has_subtotal(f) {
                                    continue;
                                }
                            }
                        }
                        out.push(v);
                    }
                }
            }
            Val::V(Value::Array(arr)) => out.extend(arr.data.into_iter().filter(|v| !v.is_empty())),
            Val::V(Value::Empty) => {}
            Val::V(v) => out.push(v),
        }
    }
    Ok(out)
}

/// Apply aggregate function `f` (1..=13) to values.
fn aggregate_fn(f: i64, vals: &[Value], ignore_errors: bool) -> R<f64> {
    match f {
        2 => return Ok(vals.iter().filter(|v| matches!(v, Value::Number(_))).count() as f64),
        3 => return Ok(vals.iter().filter(|v| !v.is_empty() && !(ignore_errors && v.is_error())).count() as f64),
        _ => {}
    }
    let mut nums = Vec::with_capacity(vals.len());
    for v in vals {
        match v {
            Value::Number(n) => nums.push(*n),
            Value::Error(e) if !ignore_errors => return Err(*e),
            _ => {}
        }
    }
    match f {
        1 => stats::mean(&nums),
        4 => Ok(if nums.is_empty() { 0.0 } else { nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max) }),
        5 => Ok(if nums.is_empty() { 0.0 } else { nums.iter().cloned().fold(f64::INFINITY, f64::min) }),
        6 => Ok(stats::product(&nums)),
        7 => stats::stdev(&nums, true),
        8 => stats::stdev(&nums, false),
        9 => Ok(nums.iter().sum()),
        10 => stats::variance(&nums, true),
        11 => stats::variance(&nums, false),
        12 => stats::median(&nums),
        13 => stats::mode_sngl(&nums),
        _ => Err(ErrorKind::Value),
    }
}

fn subtotal(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let f = ctx.eval_number(&args[0])?.trunc() as i64;
        let (f, hidden) = match f {
            1..=11 => (f, false),
            101..=111 => (f - 100, true),
            _ => return Err(ErrorKind::Value),
        };
        let vals = agg_values(ctx, &args[1..], hidden, true)?;
        aggregate_fn(f, &vals, false)
    })
}

fn aggregate(ctx: &mut Ctx, args: &[Expr]) -> Value {
    sres(|| {
        let f = ctx.eval_number(&args[0])?.trunc() as i64;
        let opt = match &args[1] {
            Expr::Missing => 0,
            e => ctx.eval_number(e)?.trunc() as i64,
        };
        chk((1..=19).contains(&f), ErrorKind::Value)?;
        chk((0..=7).contains(&opt), ErrorKind::Value)?;
        let skip_nested = opt <= 3;
        let skip_hidden = matches!(opt, 1 | 3 | 5 | 7);
        let ignore_errors = matches!(opt, 2 | 3 | 6 | 7);
        if f <= 13 {
            let vals = agg_values(ctx, &args[2..], skip_hidden, skip_nested)?;
            return aggregate_fn(f, &vals, ignore_errors);
        }
        chk(args.len() == 4, ErrorKind::Value)?;
        let vals = agg_values(ctx, &args[2..3], skip_hidden, skip_nested)?;
        let mut nums = Vec::new();
        for v in vals {
            match v {
                Value::Number(n) => nums.push(n),
                Value::Error(e) if !ignore_errors => return Err(e),
                _ => {}
            }
        }
        let k = ctx.eval_number(&args[3])?;
        match f {
            14 => stats::large(&nums, k),
            15 => stats::small(&nums, k),
            16 => stats::percentile_inc(&nums, k),
            17 => stats::quartile_inc(&nums, k),
            18 => stats::percentile_exc(&nums, k),
            _ => stats::quartile_exc(&nums, k),
        }
    })
}
