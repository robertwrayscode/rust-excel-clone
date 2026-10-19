//! Financial functions: time value of money, cash-flow analysis, depreciation
//! and securities (bonds, bills, coupons).

use super::{bool_or, num, num_or, Registry, VAR};
use crate::ast::Expr;
use crate::datetime::{date_to_serial, days_in_month, is_leap_year, serial_to_date};
use crate::eval::Ctx;
use crate::tryv;
use crate::value::{ErrorKind, Value};

const CAT: &str = "Financial";

type R<T> = Result<T, ErrorKind>;

pub fn register(r: &mut Registry) {
    // ----- time value of money -----
    r.scalar("PMT", 3, 5, pmt_fn, CAT, "PMT(rate, nper, pv, [fv], [type])", "Calculates the payment for a loan based on constant payments and a constant interest rate.");
    r.scalar("FV", 3, 5, fv_fn, CAT, "FV(rate, nper, pmt, [pv], [type])", "Returns the future value of an investment.");
    r.scalar("PV", 3, 5, pv_fn, CAT, "PV(rate, nper, pmt, [fv], [type])", "Returns the present value of an investment.");
    r.scalar("NPER", 3, 5, nper_fn, CAT, "NPER(rate, pmt, pv, [fv], [type])", "Returns the number of periods for an investment.");
    r.scalar("RATE", 3, 6, rate_fn, CAT, "RATE(nper, pmt, pv, [fv], [type], [guess])", "Returns the interest rate per period of an annuity.");
    r.scalar("IPMT", 4, 6, ipmt_fn, CAT, "IPMT(rate, per, nper, pv, [fv], [type])", "Returns the interest payment for an investment for a given period.");
    r.scalar("PPMT", 4, 6, ppmt_fn, CAT, "PPMT(rate, per, nper, pv, [fv], [type])", "Returns the payment on the principal for an investment for a given period.");
    r.scalar("CUMIPMT", 6, 6, cumipmt_fn, CAT, "CUMIPMT(rate, nper, pv, start_period, end_period, type)", "Returns the cumulative interest paid between two periods.");
    r.scalar("CUMPRINC", 6, 6, cumprinc_fn, CAT, "CUMPRINC(rate, nper, pv, start_period, end_period, type)", "Returns the cumulative principal paid on a loan between two periods.");
    r.scalar("ISPMT", 4, 4, ispmt_fn, CAT, "ISPMT(rate, per, nper, pv)", "Calculates the interest paid during a specific period of an investment (straight-line principal).");
    r.scalar("EFFECT", 2, 2, effect_fn, CAT, "EFFECT(nominal_rate, npery)", "Returns the effective annual interest rate.");
    r.scalar("NOMINAL", 2, 2, nominal_fn, CAT, "NOMINAL(effect_rate, npery)", "Returns the annual nominal interest rate.");
    r.scalar("PDURATION", 3, 3, pduration_fn, CAT, "PDURATION(rate, pv, fv)", "Returns the number of periods required by an investment to reach a specified value.");
    r.scalar("RRI", 3, 3, rri_fn, CAT, "RRI(nper, pv, fv)", "Returns an equivalent interest rate for the growth of an investment.");
    r.raw("NPV", 2, VAR, npv_fn, CAT, "NPV(rate, value1, [value2], ...)", "Returns the net present value of an investment based on periodic cash flows and a discount rate.");
    r.raw("IRR", 1, 2, irr_fn, CAT, "IRR(values, [guess])", "Returns the internal rate of return for a series of cash flows.");
    r.raw("MIRR", 3, 3, mirr_fn, CAT, "MIRR(values, finance_rate, reinvest_rate)", "Returns the internal rate of return where positive and negative cash flows are financed at different rates.");
    r.raw("XNPV", 3, 3, xnpv_fn, CAT, "XNPV(rate, values, dates)", "Returns the net present value for a schedule of cash flows that is not necessarily periodic.");
    r.raw("XIRR", 2, 3, xirr_fn, CAT, "XIRR(values, dates, [guess])", "Returns the internal rate of return for a schedule of cash flows that is not necessarily periodic.");
    r.raw("FVSCHEDULE", 2, 2, fvschedule_fn, CAT, "FVSCHEDULE(principal, schedule)", "Returns the future value of an initial principal after applying a series of compound interest rates.");
    // ----- depreciation -----
    r.scalar("SLN", 3, 3, sln_fn, CAT, "SLN(cost, salvage, life)", "Returns the straight-line depreciation of an asset for one period.");
    r.scalar("SYD", 4, 4, syd_fn, CAT, "SYD(cost, salvage, life, per)", "Returns the sum-of-years' digits depreciation of an asset for a specified period.");
    r.scalar("DB", 4, 5, db_fn, CAT, "DB(cost, salvage, life, period, [month])", "Returns the depreciation of an asset using the fixed-declining balance method.");
    r.scalar("DDB", 4, 5, ddb_fn, CAT, "DDB(cost, salvage, life, period, [factor])", "Returns the depreciation of an asset using the double-declining balance method.");
    r.scalar("VDB", 5, 7, vdb_fn, CAT, "VDB(cost, salvage, life, start_period, end_period, [factor], [no_switch])", "Returns the depreciation of an asset for any period using a declining balance method.");
    r.scalar("AMORLINC", 6, 7, amorlinc_fn, CAT, "AMORLINC(cost, date_purchased, first_period, salvage, period, rate, [basis])", "Returns the depreciation for each accounting period (French linear method).");
    r.scalar("AMORDEGRC", 6, 7, amordegrc_fn, CAT, "AMORDEGRC(cost, date_purchased, first_period, salvage, period, rate, [basis])", "Returns the depreciation for each accounting period using a depreciation coefficient (French method).");
    // ----- misc -----
    r.scalar("DOLLARDE", 2, 2, dollarde_fn, CAT, "DOLLARDE(fractional_dollar, fraction)", "Converts a dollar price expressed as a fraction into a decimal number.");
    r.scalar("DOLLARFR", 2, 2, dollarfr_fn, CAT, "DOLLARFR(decimal_dollar, fraction)", "Converts a decimal dollar price into a dollar price expressed as a fraction.");
    // ----- coupons -----
    r.scalar("COUPDAYBS", 3, 4, coupdaybs_fn, CAT, "COUPDAYBS(settlement, maturity, frequency, [basis])", "Returns the number of days from the beginning of the coupon period to the settlement date.");
    r.scalar("COUPDAYS", 3, 4, coupdays_fn, CAT, "COUPDAYS(settlement, maturity, frequency, [basis])", "Returns the number of days in the coupon period that contains the settlement date.");
    r.scalar("COUPDAYSNC", 3, 4, coupdaysnc_fn, CAT, "COUPDAYSNC(settlement, maturity, frequency, [basis])", "Returns the number of days from the settlement date to the next coupon date.");
    r.scalar("COUPNCD", 3, 4, coupncd_fn, CAT, "COUPNCD(settlement, maturity, frequency, [basis])", "Returns the next coupon date after the settlement date.");
    r.scalar("COUPNUM", 3, 4, coupnum_fn, CAT, "COUPNUM(settlement, maturity, frequency, [basis])", "Returns the number of coupons payable between the settlement date and maturity date.");
    r.scalar("COUPPCD", 3, 4, couppcd_fn, CAT, "COUPPCD(settlement, maturity, frequency, [basis])", "Returns the previous coupon date before the settlement date.");
    // ----- securities -----
    r.scalar("ACCRINT", 6, 8, accrint_fn, CAT, "ACCRINT(issue, first_interest, settlement, rate, par, frequency, [basis], [calc_method])", "Returns the accrued interest for a security that pays periodic interest.");
    r.scalar("ACCRINTM", 3, 5, accrintm_fn, CAT, "ACCRINTM(issue, settlement, rate, [par], [basis])", "Returns the accrued interest for a security that pays interest at maturity.");
    r.scalar("DISC", 4, 5, disc_fn, CAT, "DISC(settlement, maturity, pr, redemption, [basis])", "Returns the discount rate for a security.");
    r.scalar("INTRATE", 4, 5, intrate_fn, CAT, "INTRATE(settlement, maturity, investment, redemption, [basis])", "Returns the interest rate for a fully invested security.");
    r.scalar("RECEIVED", 4, 5, received_fn, CAT, "RECEIVED(settlement, maturity, investment, discount, [basis])", "Returns the amount received at maturity for a fully invested security.");
    r.scalar("PRICEDISC", 4, 5, pricedisc_fn, CAT, "PRICEDISC(settlement, maturity, discount, redemption, [basis])", "Returns the price per $100 face value of a discounted security.");
    r.scalar("YIELDDISC", 4, 5, yielddisc_fn, CAT, "YIELDDISC(settlement, maturity, pr, redemption, [basis])", "Returns the annual yield for a discounted security.");
    r.scalar("PRICEMAT", 5, 6, pricemat_fn, CAT, "PRICEMAT(settlement, maturity, issue, rate, yld, [basis])", "Returns the price per $100 face value of a security that pays interest at maturity.");
    r.scalar("YIELDMAT", 5, 6, yieldmat_fn, CAT, "YIELDMAT(settlement, maturity, issue, rate, pr, [basis])", "Returns the annual yield of a security that pays interest at maturity.");
    r.scalar("PRICE", 6, 7, price_fn, CAT, "PRICE(settlement, maturity, rate, yld, redemption, frequency, [basis])", "Returns the price per $100 face value of a security that pays periodic interest.");
    r.scalar("YIELD", 6, 7, yield_fn, CAT, "YIELD(settlement, maturity, rate, pr, redemption, frequency, [basis])", "Returns the yield on a security that pays periodic interest.");
    r.scalar("DURATION", 5, 6, duration_fn, CAT, "DURATION(settlement, maturity, coupon, yld, frequency, [basis])", "Returns the annual (Macaulay) duration of a security with periodic interest payments.");
    r.scalar("MDURATION", 5, 6, mduration_fn, CAT, "MDURATION(settlement, maturity, coupon, yld, frequency, [basis])", "Returns the modified Macaulay duration for a security with an assumed par value of $100.");
    r.scalar("TBILLEQ", 3, 3, tbilleq_fn, CAT, "TBILLEQ(settlement, maturity, discount)", "Returns the bond-equivalent yield for a Treasury bill.");
    r.scalar("TBILLPRICE", 3, 3, tbillprice_fn, CAT, "TBILLPRICE(settlement, maturity, discount)", "Returns the price per $100 face value for a Treasury bill.");
    r.scalar("TBILLYIELD", 3, 3, tbillyield_fn, CAT, "TBILLYIELD(settlement, maturity, pr)", "Returns the yield for a Treasury bill.");
    r.scalar("ODDFPRICE", 8, 9, oddfprice_fn, CAT, "ODDFPRICE(settlement, maturity, issue, first_coupon, rate, yld, redemption, frequency, [basis])", "Returns the price per $100 face value of a security with an odd first period.");
    r.scalar("ODDFYIELD", 8, 9, oddfyield_fn, CAT, "ODDFYIELD(settlement, maturity, issue, first_coupon, rate, pr, redemption, frequency, [basis])", "Returns the yield of a security with an odd first period.");
    r.scalar("ODDLPRICE", 7, 8, oddlprice_fn, CAT, "ODDLPRICE(settlement, maturity, last_interest, rate, yld, redemption, frequency, [basis])", "Returns the price per $100 face value of a security with an odd last period.");
    r.scalar("ODDLYIELD", 7, 8, oddlyield_fn, CAT, "ODDLYIELD(settlement, maturity, last_interest, rate, pr, redemption, frequency, [basis])", "Returns the yield of a security with an odd last period.");
}

fn out(x: f64) -> Value {
    Value::number(x)
}

fn res(r: R<f64>) -> Value {
    match r {
        Ok(x) => Value::number(x),
        Err(e) => Value::Error(e),
    }
}

// ===========================================================================
// Time value of money
// ===========================================================================

fn type_flag(args: &[Value], i: usize) -> R<f64> {
    Ok(if num_or(args, i, 0.0)? != 0.0 { 1.0 } else { 0.0 })
}

pub(crate) fn fv(rate: f64, nper: f64, pmt: f64, pv: f64, t: f64) -> f64 {
    if rate == 0.0 {
        -(pv + pmt * nper)
    } else {
        let q = (1.0 + rate).powf(nper);
        -(pv * q + pmt * (1.0 + rate * t) * (q - 1.0) / rate)
    }
}

pub(crate) fn pmt(rate: f64, nper: f64, pv: f64, fv: f64, t: f64) -> f64 {
    if rate == 0.0 {
        -(pv + fv) / nper
    } else {
        let q = (1.0 + rate).powf(nper);
        -(fv + pv * q) * rate / ((1.0 + rate * t) * (q - 1.0))
    }
}

fn pv(rate: f64, nper: f64, pmt: f64, fv: f64, t: f64) -> f64 {
    if rate == 0.0 {
        -(fv + pmt * nper)
    } else {
        let q = (1.0 + rate).powf(nper);
        -(fv + pmt * (1.0 + rate * t) * (q - 1.0) / rate) / q
    }
}

fn ipmt(rate: f64, per: f64, nper: f64, pv: f64, fv_: f64, t: f64) -> f64 {
    let p = pmt(rate, nper, pv, fv_, t);
    let interest = if per == 1.0 {
        if t == 1.0 {
            0.0
        } else {
            -pv
        }
    } else if t == 1.0 {
        fv(rate, per - 2.0, p, pv, 1.0) - p
    } else {
        fv(rate, per - 1.0, p, pv, 0.0)
    };
    interest * rate
}

fn pmt_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let nper = tryv!(num(a, 1));
    let pv = tryv!(num(a, 2));
    let fv = tryv!(num_or(a, 3, 0.0));
    let t = tryv!(type_flag(a, 4));
    if nper == 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(pmt(rate, nper, pv, fv, t))
}

fn fv_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let nper = tryv!(num(a, 1));
    let p = tryv!(num(a, 2));
    let pv = tryv!(num_or(a, 3, 0.0));
    let t = tryv!(type_flag(a, 4));
    out(fv(rate, nper, p, pv, t))
}

fn pv_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let nper = tryv!(num(a, 1));
    let p = tryv!(num(a, 2));
    let fv = tryv!(num_or(a, 3, 0.0));
    let t = tryv!(type_flag(a, 4));
    out(pv(rate, nper, p, fv, t))
}

fn nper_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let p = tryv!(num(a, 1));
    let pv = tryv!(num(a, 2));
    let fv = tryv!(num_or(a, 3, 0.0));
    let t = tryv!(type_flag(a, 4));
    if rate == 0.0 {
        if p == 0.0 {
            return Value::Error(ErrorKind::Num);
        }
        return out(-(pv + fv) / p);
    }
    let num_ = p * (1.0 + rate * t) - fv * rate;
    let den = pv * rate + p * (1.0 + rate * t);
    if den == 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let x = num_ / den;
    if x <= 0.0 || rate <= -1.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(x.ln() / (1.0 + rate).ln())
}

/// Newton's method with an analytic derivative. Returns `None` on failure.
fn newton(f: &dyn Fn(f64) -> (f64, f64), guess: f64, lower: f64) -> Option<f64> {
    let mut x = guess;
    for _ in 0..200 {
        let (y, dy) = f(x);
        if !y.is_finite() || !dy.is_finite() {
            return None;
        }
        if y == 0.0 {
            return Some(x);
        }
        if dy == 0.0 {
            return None;
        }
        let mut nx = x - y / dy;
        if nx <= lower {
            nx = (x + lower) / 2.0;
        }
        if (nx - x).abs() <= 1e-13 * nx.abs().max(1.0) {
            return Some(nx);
        }
        x = nx;
    }
    None
}

/// Bisection over a scanned bracket, used as a fallback for root finding.
fn bracket_solve(f: &dyn Fn(f64) -> f64, lo: f64, hi: f64, guess: f64) -> Option<f64> {
    // Scan outward from guess for a sign change.
    let mut pts: Vec<f64> = Vec::new();
    let n = 400;
    for i in 0..=n {
        let x = lo + (hi - lo) * (i as f64 / n as f64).powi(3);
        pts.push(x);
    }
    pts.push(guess);
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Choose the bracket closest to the guess.
    let mut best: Option<(f64, f64, f64)> = None;
    let mut prev: Option<(f64, f64)> = None;
    for &x in &pts {
        let y = f(x);
        if !y.is_finite() {
            prev = None;
            continue;
        }
        if let Some((px, py)) = prev {
            if py == 0.0 || py.signum() != y.signum() {
                let d = ((px + x) / 2.0 - guess).abs();
                if best.map(|b| d < b.2).unwrap_or(true) {
                    best = Some((px, x, d));
                }
            }
        }
        prev = Some((x, y));
    }
    let (mut a, mut b, _) = best?;
    let mut fa = f(a);
    for _ in 0..200 {
        let m = (a + b) / 2.0;
        let fm = f(m);
        if fm == 0.0 || (b - a).abs() < 1e-15 * m.abs().max(1.0) {
            return Some(m);
        }
        if fa.signum() == fm.signum() {
            a = m;
            fa = fm;
        } else {
            b = m;
        }
    }
    Some((a + b) / 2.0)
}

/// Solve f(x)=0 with Newton from `guess`, falling back to other starting
/// points and finally bisection. `lower` is an exclusive lower bound.
fn solve(f: &dyn Fn(f64) -> (f64, f64), guess: f64, lower: f64, upper: f64) -> Option<f64> {
    let ok = |x: f64| -> bool {
        let (y, _) = f(x);
        let (y2, _) = f(x * (1.0 + 1e-9) + 1e-12);
        // Accept if the residual is tiny relative to the local scale.
        y.is_finite() && (y.abs() < 1e-7 || y.abs() <= (y2 - y).abs() * 1e3)
    };
    let mut guesses = vec![guess];
    for g in [0.1, 0.0, 0.01, 0.05, 0.2, 0.5, -0.05, -0.2, -0.5, 1.0, 2.0] {
        if !guesses.contains(&g) {
            guesses.push(g);
        }
    }
    for g in guesses {
        if g <= lower {
            continue;
        }
        if let Some(x) = newton(f, g, lower) {
            if x > lower && x.is_finite() && ok(x) {
                return Some(x);
            }
        }
    }
    let g = |x: f64| f(x).0;
    let x = bracket_solve(&g, lower + 1e-10, upper, guess)?;
    if ok(x) {
        Some(x)
    } else {
        None
    }
}

fn rate_fn(a: &[Value]) -> Value {
    let n = tryv!(num(a, 0));
    let p = tryv!(num(a, 1));
    let pv = tryv!(num(a, 2));
    let fv = tryv!(num_or(a, 3, 0.0));
    let t = tryv!(type_flag(a, 4));
    let guess = tryv!(num_or(a, 5, 0.1));
    if n <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let f = move |r: f64| -> (f64, f64) {
        if r.abs() < 1e-12 {
            // Use limits as r -> 0
            let y = pv + p * n + fv;
            let dy = pv * n + p * (n * (n - 1.0) / 2.0 + t * n);
            return (y, dy);
        }
        let q = (1.0 + r).powf(n);
        let dq = n * (1.0 + r).powf(n - 1.0);
        let g = (q - 1.0) / r;
        let dg = (dq * r - (q - 1.0)) / (r * r);
        let y = pv * q + p * (1.0 + r * t) * g + fv;
        let dy = pv * dq + p * t * g + p * (1.0 + r * t) * dg;
        (y, dy)
    };
    match solve(&f, guess, -1.0, 10.0) {
        Some(r) => out(r),
        None => Value::Error(ErrorKind::Num),
    }
}

fn ipmt_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let per = tryv!(num(a, 1));
    let nper = tryv!(num(a, 2));
    let pv = tryv!(num(a, 3));
    let fv = tryv!(num_or(a, 4, 0.0));
    let t = tryv!(type_flag(a, 5));
    if per < 1.0 || per > nper || nper <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(ipmt(rate, per, nper, pv, fv, t))
}

fn ppmt_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let per = tryv!(num(a, 1));
    let nper = tryv!(num(a, 2));
    let pv = tryv!(num(a, 3));
    let fv = tryv!(num_or(a, 4, 0.0));
    let t = tryv!(type_flag(a, 5));
    if per < 1.0 || per > nper || nper <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(pmt(rate, nper, pv, fv, t) - ipmt(rate, per, nper, pv, fv, t))
}

fn cum_args(a: &[Value]) -> R<(f64, f64, f64, i64, i64, f64)> {
    let rate = num(a, 0)?;
    let nper = num(a, 1)?;
    let pv = num(a, 2)?;
    let start = num(a, 3)?;
    let end = num(a, 4)?;
    let t = num(a, 5)?;
    if rate <= 0.0 || nper <= 0.0 || pv <= 0.0 || start < 1.0 || end < 1.0 || start > end || (t != 0.0 && t != 1.0) {
        return Err(ErrorKind::Num);
    }
    let nper = nper.trunc();
    let (s, e) = (start.ceil() as i64, end.trunc() as i64);
    if e as f64 > nper || s > e {
        return Err(ErrorKind::Num);
    }
    Ok((rate, nper, pv, s, e, t))
}

fn cumipmt_fn(a: &[Value]) -> Value {
    let (rate, nper, pv, s, e, t) = tryv!(cum_args(a));
    let mut sum = 0.0;
    for per in s..=e {
        sum += ipmt(rate, per as f64, nper, pv, 0.0, t);
    }
    out(sum)
}

fn cumprinc_fn(a: &[Value]) -> Value {
    let (rate, nper, pv, s, e, t) = tryv!(cum_args(a));
    let p = pmt(rate, nper, pv, 0.0, t);
    let mut sum = 0.0;
    for per in s..=e {
        sum += p - ipmt(rate, per as f64, nper, pv, 0.0, t);
    }
    out(sum)
}

fn ispmt_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let per = tryv!(num(a, 1));
    let nper = tryv!(num(a, 2));
    let pv = tryv!(num(a, 3));
    if nper == 0.0 {
        return Value::Error(ErrorKind::Div0);
    }
    out(pv * rate * (per / nper - 1.0))
}

fn effect_fn(a: &[Value]) -> Value {
    let nominal = tryv!(num(a, 0));
    let n = tryv!(num(a, 1)).trunc();
    if nominal <= 0.0 || n < 1.0 {
        return Value::Error(ErrorKind::Num);
    }
    out((1.0 + nominal / n).powf(n) - 1.0)
}

fn nominal_fn(a: &[Value]) -> Value {
    let eff = tryv!(num(a, 0));
    let n = tryv!(num(a, 1)).trunc();
    if eff <= 0.0 || n < 1.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(((1.0 + eff).powf(1.0 / n) - 1.0) * n)
}

fn pduration_fn(a: &[Value]) -> Value {
    let rate = tryv!(num(a, 0));
    let pv = tryv!(num(a, 1));
    let fv = tryv!(num(a, 2));
    if rate <= 0.0 || pv <= 0.0 || fv <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out((fv.ln() - pv.ln()) / (1.0 + rate).ln())
}

fn rri_fn(a: &[Value]) -> Value {
    let n = tryv!(num(a, 0));
    let pv = tryv!(num(a, 1));
    let fv = tryv!(num(a, 2));
    if n <= 0.0 || pv == 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let x = fv / pv;
    if x < 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(x.powf(1.0 / n) - 1.0)
}

// ===========================================================================
// Cash flows (range arguments)
// ===========================================================================

/// Numbers from a single range/array argument; text, logicals and blanks in
/// references/arrays are ignored, errors propagate.
fn flow_values(ctx: &mut Ctx, e: &Expr) -> R<Vec<f64>> {
    ctx.collect_numbers(std::slice::from_ref(e))
}

/// Values for XNPV/XIRR, which require every cell to be numeric.
fn strict_values(ctx: &mut Ctx, e: &Expr) -> R<Vec<f64>> {
    let arr = ctx.eval_array(e)?;
    let mut v = Vec::with_capacity(arr.data.len());
    for x in arr.data.iter() {
        match x {
            Value::Number(n) => v.push(*n),
            Value::Error(e) => return Err(*e),
            Value::Empty => return Err(ErrorKind::Num),
            other => v.push(other.to_number()?),
        }
    }
    Ok(v)
}

fn npv(rate: f64, values: &[f64]) -> f64 {
    values.iter().enumerate().map(|(i, v)| v / (1.0 + rate).powi(i as i32 + 1)).sum()
}

fn npv_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let rate = tryv!(ctx.eval_number(&args[0]));
    let values = tryv!(ctx.collect_numbers(&args[1..]));
    if rate == -1.0 {
        return Value::Error(ErrorKind::Div0);
    }
    out(npv(rate, &values))
}

fn irr_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let values = tryv!(flow_values(ctx, &args[0]));
    let guess = tryv!(ctx.opt_number(args, 1, 0.1));
    if !values.iter().any(|v| *v > 0.0) || !values.iter().any(|v| *v < 0.0) {
        return Value::Error(ErrorKind::Num);
    }
    let f = |r: f64| -> (f64, f64) {
        let mut y = 0.0;
        let mut dy = 0.0;
        let b = 1.0 + r;
        for (i, v) in values.iter().enumerate() {
            let i = i as f64;
            y += v / b.powf(i);
            dy -= i * v / b.powf(i + 1.0);
        }
        (y, dy)
    };
    match solve(&f, guess, -1.0, 100.0) {
        Some(r) => out(r),
        None => Value::Error(ErrorKind::Num),
    }
}

fn mirr_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let values = tryv!(flow_values(ctx, &args[0]));
    let fr = tryv!(ctx.eval_number(&args[1]));
    let rr = tryv!(ctx.eval_number(&args[2]));
    let n = values.len() as f64;
    if n < 2.0 {
        return Value::Error(ErrorKind::Div0);
    }
    let mut npv_pos = 0.0;
    let mut npv_neg = 0.0;
    for (i, v) in values.iter().enumerate() {
        if *v > 0.0 {
            npv_pos += v / (1.0 + rr).powi(i as i32 + 1);
        } else {
            npv_neg += v / (1.0 + fr).powi(i as i32 + 1);
        }
    }
    if npv_neg == 0.0 || npv_pos == 0.0 {
        return Value::Error(ErrorKind::Div0);
    }
    let x = -npv_pos * (1.0 + rr).powf(n) / (npv_neg * (1.0 + fr));
    out(x.powf(1.0 / (n - 1.0)) - 1.0)
}

fn xflows(ctx: &mut Ctx, ve: &Expr, de: &Expr) -> R<(Vec<f64>, Vec<f64>)> {
    let values = strict_values(ctx, ve)?;
    let dates = strict_values(ctx, de)?;
    if values.len() != dates.len() || values.is_empty() {
        return Err(ErrorKind::Num);
    }
    let dates: Vec<f64> = dates.iter().map(|d| d.trunc()).collect();
    if dates.iter().any(|d| *d < 0.0 || *d < dates[0]) {
        return Err(ErrorKind::Num);
    }
    Ok((values, dates))
}

fn xnpv(rate: f64, values: &[f64], dates: &[f64]) -> f64 {
    let d0 = dates[0];
    values.iter().zip(dates).map(|(v, d)| v / (1.0 + rate).powf((d - d0) / 365.0)).sum()
}

fn xnpv_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let rate = tryv!(ctx.eval_number(&args[0]));
    let (values, dates) = tryv!(xflows(ctx, &args[1], &args[2]));
    if rate <= -1.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(xnpv(rate, &values, &dates))
}

fn xirr_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let (values, dates) = tryv!(xflows(ctx, &args[0], &args[1]));
    let guess = tryv!(ctx.opt_number(args, 2, 0.1));
    if !values.iter().any(|v| *v > 0.0) || !values.iter().any(|v| *v < 0.0) {
        return Value::Error(ErrorKind::Num);
    }
    let d0 = dates[0];
    let f = |r: f64| -> (f64, f64) {
        let mut y = 0.0;
        let mut dy = 0.0;
        let b = 1.0 + r;
        for (v, d) in values.iter().zip(&dates) {
            let e = (d - d0) / 365.0;
            y += v / b.powf(e);
            dy -= e * v / b.powf(e + 1.0);
        }
        (y, dy)
    };
    match solve(&f, guess, -1.0, 100.0) {
        Some(r) => out(r),
        None => Value::Error(ErrorKind::Num),
    }
}

fn fvschedule_fn(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let principal = tryv!(ctx.eval_number(&args[0]));
    let arr = tryv!(ctx.eval_array(&args[1]));
    let mut x = principal;
    for v in arr.data.iter() {
        let r = match v {
            Value::Empty => 0.0,
            Value::Number(n) => *n,
            Value::Error(e) => return Value::Error(*e),
            _ => return Value::Error(ErrorKind::Value),
        };
        x *= 1.0 + r;
    }
    out(x)
}

// ===========================================================================
// Depreciation
// ===========================================================================

fn sln_fn(a: &[Value]) -> Value {
    let cost = tryv!(num(a, 0));
    let salvage = tryv!(num(a, 1));
    let life = tryv!(num(a, 2));
    if life == 0.0 {
        return Value::Error(ErrorKind::Div0);
    }
    out((cost - salvage) / life)
}

fn syd_fn(a: &[Value]) -> Value {
    let cost = tryv!(num(a, 0));
    let salvage = tryv!(num(a, 1));
    let life = tryv!(num(a, 2));
    let per = tryv!(num(a, 3));
    if life <= 0.0 || per <= 0.0 || per > life {
        return Value::Error(ErrorKind::Num);
    }
    out((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
}

fn db_fn(a: &[Value]) -> Value {
    let cost = tryv!(num(a, 0));
    let salvage = tryv!(num(a, 1));
    let life = tryv!(num(a, 2));
    let period = tryv!(num(a, 3)).trunc();
    let month = tryv!(num_or(a, 4, 12.0)).trunc();
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || !(1.0..=12.0).contains(&month) {
        return Value::Error(ErrorKind::Num);
    }
    if period > life + 1.0 || (period > life && month == 12.0) {
        return Value::Error(ErrorKind::Num);
    }
    if cost == 0.0 {
        return out(0.0);
    }
    let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0).round() / 1000.0;
    let mut total = 0.0;
    let mut dep = cost * rate * month / 12.0;
    if period == 1.0 {
        return out(dep);
    }
    total += dep;
    let p = period as i64;
    for i in 2..=p {
        if i as f64 == life + 1.0 {
            dep = (cost - total) * rate * (12.0 - month) / 12.0;
        } else {
            dep = (cost - total) * rate;
        }
        total += dep;
    }
    out(dep)
}

fn ddb_calc(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> f64 {
    let mut rate = factor / life;
    let old_value;
    if rate >= 1.0 {
        rate = 1.0;
        old_value = if period == 1.0 { cost } else { 0.0 };
    } else {
        old_value = cost * (1.0 - rate).powf(period - 1.0);
    }
    let new_value = cost * (1.0 - rate).powf(period);
    let ddb = if new_value < salvage { old_value - salvage } else { old_value - new_value };
    ddb.max(0.0)
}

fn ddb_fn(a: &[Value]) -> Value {
    let cost = tryv!(num(a, 0));
    let salvage = tryv!(num(a, 1));
    let life = tryv!(num(a, 2));
    let period = tryv!(num(a, 3));
    let factor = tryv!(num_or(a, 4, 2.0));
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || factor <= 0.0 || period > life {
        return Value::Error(ErrorKind::Num);
    }
    out(ddb_calc(cost, salvage, life, period, factor))
}

fn inter_vdb(cost: f64, salvage: f64, life: f64, life1: f64, period: f64, factor: f64) -> f64 {
    let mut vdb = 0.0;
    let int_end = period.ceil();
    let loop_end = int_end as i64;
    let mut sln = 0.0;
    let mut salvage_value = cost - salvage;
    let mut now_sln = false;
    for i in 1..=loop_end {
        let mut term;
        if !now_sln {
            let ddb = ddb_calc(cost, salvage, life, i as f64, factor);
            sln = salvage_value / (life1 - (i - 1) as f64);
            if sln > ddb {
                term = sln;
                now_sln = true;
            } else {
                term = ddb;
                salvage_value -= ddb;
            }
        } else {
            term = sln;
        }
        if i == loop_end {
            term *= period + 1.0 - int_end;
        }
        vdb += term;
    }
    vdb
}

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0)
}

fn vdb_fn(a: &[Value]) -> Value {
    let mut cost = tryv!(num(a, 0));
    let salvage = tryv!(num(a, 1));
    let life = tryv!(num(a, 2));
    let start = tryv!(num(a, 3));
    let end = tryv!(num(a, 4));
    let factor = tryv!(num_or(a, 5, 2.0));
    let no_switch = tryv!(bool_or(a, 6, false));
    if start < 0.0 || end < start || end > life || cost < 0.0 || salvage < 0.0 || factor <= 0.0 || life <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let int_start = start.floor();
    let int_end = end.ceil();
    if no_switch {
        let loop_start = int_start as i64;
        let loop_end = int_end as i64;
        let mut vdb = 0.0;
        for i in (loop_start + 1)..=loop_end {
            let mut term = ddb_calc(cost, salvage, life, i as f64, factor);
            if i == loop_start + 1 {
                term *= end.min((loop_start + 1) as f64) - start;
            } else if i == loop_end {
                term *= end + 1.0 - int_end;
            }
            vdb += term;
        }
        return out(vdb);
    }
    let mut part = 0.0;
    if !approx_eq(start, int_start) {
        let temp_int_end = int_start + 1.0;
        let temp_value = cost - inter_vdb(cost, salvage, life, life, int_start, factor);
        part += (start - int_start) * inter_vdb(temp_value, salvage, life, life - int_start, temp_int_end - int_start, factor);
    }
    if !approx_eq(end, int_end) {
        let temp_int_start = int_end - 1.0;
        let temp_value = cost - inter_vdb(cost, salvage, life, life, temp_int_start, factor);
        part += (int_end - end) * inter_vdb(temp_value, salvage, life, life - temp_int_start, int_end - temp_int_start, factor);
    }
    cost -= inter_vdb(cost, salvage, life, life, int_start, factor);
    let vdb = inter_vdb(cost, salvage, life, life - int_start, int_end - int_start, factor);
    out(vdb - part)
}

fn amor_args(a: &[Value]) -> R<(f64, i64, i64, f64, f64, f64, i32)> {
    let cost = num(a, 0)?;
    let date = date_arg(a, 1)?;
    let first = date_arg(a, 2)?;
    let salvage = num(a, 3)?;
    let period = num(a, 4)?.trunc();
    let rate = num(a, 5)?;
    let basis = basis_arg(a, 6)?;
    if basis == 2 || cost < 0.0 || salvage < 0.0 || salvage > cost || period < 0.0 || rate <= 0.0 || date > first {
        return Err(ErrorKind::Num);
    }
    Ok((cost, date, first, salvage, period, rate, basis))
}

fn amorlinc_fn(a: &[Value]) -> Value {
    let (cost, date, first, salvage, period, rate, basis) = tryv!(amor_args(a));
    let one_rate = cost * rate;
    let cost_delta = cost - salvage;
    let f0_rate = yearfrac(date, first, basis) * rate * cost;
    let full = ((cost - salvage - f0_rate) / one_rate).trunc();
    let r = if period == 0.0 {
        f0_rate
    } else if period <= full {
        one_rate
    } else if period == full + 1.0 {
        cost_delta - one_rate * full - f0_rate
    } else {
        0.0
    };
    out(r)
}

fn amordegrc_fn(a: &[Value]) -> Value {
    let (mut cost, date, first, salvage, period, mut rate, basis) = tryv!(amor_args(a));
    let use_per = 1.0 / rate;
    let coeff = if use_per < 3.0 {
        1.0
    } else if use_per < 5.0 {
        1.5
    } else if use_per <= 6.0 {
        2.0
    } else {
        2.5
    };
    rate *= coeff;
    let mut n_rate = (yearfrac(date, first, basis) * rate * cost).round();
    cost -= n_rate;
    let mut rest = cost - salvage;
    let per = period as i64;
    for n in 0..per {
        n_rate = (rate * cost).round();
        rest -= n_rate;
        if rest < 0.0 {
            return match per - n {
                0 | 1 => out((cost * 0.5).round()),
                _ => out(0.0),
            };
        }
        cost -= n_rate;
    }
    out(n_rate)
}

fn dollar_digits(f: f64) -> f64 {
    10f64.powf(f.log10().ceil())
}

fn round15(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let mag = x.abs().log10().floor() as i32;
    let scale = 10f64.powi(14 - mag);
    if !scale.is_finite() {
        return x;
    }
    (x * scale).round() / scale
}

fn dollarde_fn(a: &[Value]) -> Value {
    let x = tryv!(num(a, 0));
    let f = tryv!(num(a, 1)).trunc();
    if f < 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    if f == 0.0 {
        return Value::Error(ErrorKind::Div0);
    }
    let ip = x.trunc();
    let fp = round15(x - ip);
    out(round15(ip + fp * dollar_digits(f) / f))
}

fn dollarfr_fn(a: &[Value]) -> Value {
    let x = tryv!(num(a, 0));
    let f = tryv!(num(a, 1)).trunc();
    if f < 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    if f == 0.0 {
        return Value::Error(ErrorKind::Div0);
    }
    let ip = x.trunc();
    let fp = round15(x - ip);
    out(round15(ip + fp * f / dollar_digits(f)))
}

// ===========================================================================
// Dates and day counts
// ===========================================================================

fn date_arg(a: &[Value], i: usize) -> R<i64> {
    let v = num(a, i)?;
    if !(0.0..2_958_466.0).contains(&v) {
        return Err(ErrorKind::Num);
    }
    Ok(v.trunc() as i64)
}

fn basis_arg(a: &[Value], i: usize) -> R<i32> {
    let b = num_or(a, i, 0.0)?.trunc();
    if !(0.0..=4.0).contains(&b) {
        return Err(ErrorKind::Num);
    }
    Ok(b as i32)
}

fn freq_arg(a: &[Value], i: usize) -> R<i32> {
    let f = num(a, i)?.trunc();
    if f != 1.0 && f != 2.0 && f != 4.0 {
        return Err(ErrorKind::Num);
    }
    Ok(f as i32)
}

fn ymd(s: i64) -> (i64, u32, u32) {
    serial_to_date(s as f64).unwrap_or((1900, 1, 1))
}

fn serial(y: i64, m: u32, d: u32) -> i64 {
    date_to_serial(y, m as i64, d as i64).unwrap_or(0.0) as i64
}

fn is_eom(s: i64) -> bool {
    let (y, m, d) = ymd(s);
    d == days_in_month(y, m)
}

/// Add months to a serial date. With `eom`, the result is the last day of the
/// target month; otherwise the day is clamped to the month length.
fn add_months(s: i64, months: i64, eom: bool) -> i64 {
    let (y, m, d) = ymd(s);
    let total = y * 12 + (m as i64 - 1) + months;
    let ny = total.div_euclid(12);
    let nm = (total.rem_euclid(12) + 1) as u32;
    let dim = days_in_month(ny, nm);
    let nd = if eom { dim } else { d.min(dim) };
    serial(ny, nm, nd)
}

fn is_last_feb(y: i64, m: u32, d: u32) -> bool {
    m == 2 && d == days_in_month(y, 2)
}

/// 30/360 US (NASD) day count, as used by YEARFRAC/coupon functions with basis 0.
fn days360_us(s1: i64, s2: i64) -> i64 {
    let (y1, m1, d1) = ymd(s1);
    let (y2, m2, d2) = ymd(s2);
    let mut dd1 = d1 as i64;
    let mut dd2 = d2 as i64;
    if is_last_feb(y1, m1, d1) && is_last_feb(y2, m2, d2) {
        dd2 = 30;
    }
    if is_last_feb(y1, m1, d1) {
        dd1 = 30;
    }
    if dd2 == 31 && dd1 >= 30 {
        dd2 = 30;
    }
    if dd1 == 31 {
        dd1 = 30;
    }
    (y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (dd2 - dd1)
}

/// 30E/360 (European) day count.
fn days360_eu(s1: i64, s2: i64) -> i64 {
    let (y1, m1, d1) = ymd(s1);
    let (y2, m2, d2) = ymd(s2);
    let dd1 = (d1 as i64).min(30);
    let dd2 = (d2 as i64).min(30);
    (y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (dd2 - dd1)
}

/// Number of days between two dates under the given basis.
fn day_count(s1: i64, s2: i64, basis: i32) -> f64 {
    match basis {
        0 => days360_us(s1, s2) as f64,
        4 => days360_eu(s1, s2) as f64,
        _ => (s2 - s1) as f64,
    }
}

fn feb29_between(s1: i64, s2: i64) -> bool {
    let (y1, _, _) = ymd(s1);
    let (y2, _, _) = ymd(s2);
    for y in y1..=y2 {
        if is_leap_year(y) {
            let f = serial(y, 2, 29);
            if s1 <= f && f <= s2 {
                return true;
            }
        }
    }
    false
}

/// Fraction of a year between two dates, matching Excel's YEARFRAC.
pub(crate) fn yearfrac(mut s1: i64, mut s2: i64, basis: i32) -> f64 {
    if s1 > s2 {
        std::mem::swap(&mut s1, &mut s2);
    }
    match basis {
        0 => days360_us(s1, s2) as f64 / 360.0,
        1 => {
            let (y1, m1, d1) = ymd(s1);
            let (y2, m2, d2) = ymd(s2);
            let within_year = y1 == y2 || (y2 == y1 + 1 && (m1 > m2 || (m1 == m2 && d1 >= d2)));
            let year_len = if within_year {
                if y1 == y2 && is_leap_year(y1) {
                    366.0
                } else if feb29_between(s1, s2) || (m2 == 2 && d2 == 29) {
                    366.0
                } else {
                    365.0
                }
            } else {
                let years = (y2 - y1 + 1) as f64;
                let days = (serial(y2 + 1, 1, 1) - serial(y1, 1, 1)) as f64;
                days / years
            };
            (s2 - s1) as f64 / year_len
        }
        2 => (s2 - s1) as f64 / 360.0,
        3 => (s2 - s1) as f64 / 365.0,
        _ => days360_eu(s1, s2) as f64 / 360.0,
    }
}

/// Length in days of a (quasi-)coupon period ending at `end` for the basis.
fn period_len(start: i64, end: i64, freq: i32, basis: i32) -> f64 {
    match basis {
        1 => (end - start) as f64,
        3 => 365.0 / freq as f64,
        _ => 360.0 / freq as f64,
    }
}

struct Coupon {
    pcd: i64,
    ncd: i64,
    num: i64,
}

/// Previous/next coupon dates and number of remaining coupons, with coupon
/// dates counted back from maturity (end-of-month rule when maturity is EOM).
fn coupon_dates(settle: i64, mat: i64, freq: i32) -> Coupon {
    let step = 12 / freq as i64;
    let eom = is_eom(mat);
    let c = |k: i64| add_months(mat, -k * step, eom);
    let mut k = (((mat - settle) as f64 / 365.25 * freq as f64) as i64 - 1).max(1);
    while k > 1 && c(k - 1) <= settle {
        k -= 1;
    }
    while c(k) > settle {
        k += 1;
    }
    Coupon { pcd: c(k), ncd: c(k - 1), num: k }
}

struct CoupArgs {
    settle: i64,
    mat: i64,
    freq: i32,
    basis: i32,
}

fn coup_args(a: &[Value]) -> R<CoupArgs> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let freq = freq_arg(a, 2)?;
    let basis = basis_arg(a, 3)?;
    if settle >= mat {
        return Err(ErrorKind::Num);
    }
    Ok(CoupArgs { settle, mat, freq, basis })
}

fn coupdays(c: &CoupArgs) -> f64 {
    let cd = coupon_dates(c.settle, c.mat, c.freq);
    period_len(cd.pcd, cd.ncd, c.freq, c.basis)
}

fn coupdaybs(c: &CoupArgs) -> f64 {
    let cd = coupon_dates(c.settle, c.mat, c.freq);
    day_count(cd.pcd, c.settle, c.basis)
}

fn coupdaysnc(c: &CoupArgs) -> f64 {
    if c.basis == 0 || c.basis == 4 {
        return coupdays(c) - coupdaybs(c);
    }
    let cd = coupon_dates(c.settle, c.mat, c.freq);
    (cd.ncd - c.settle) as f64
}

fn coupdaybs_fn(a: &[Value]) -> Value {
    let c = tryv!(coup_args(a));
    out(coupdaybs(&c))
}

fn coupdays_fn(a: &[Value]) -> Value {
    let c = tryv!(coup_args(a));
    out(coupdays(&c))
}

fn coupdaysnc_fn(a: &[Value]) -> Value {
    let c = tryv!(coup_args(a));
    out(coupdaysnc(&c))
}

fn coupncd_fn(a: &[Value]) -> Value {
    let c = tryv!(coup_args(a));
    out(coupon_dates(c.settle, c.mat, c.freq).ncd as f64)
}

fn couppcd_fn(a: &[Value]) -> Value {
    let c = tryv!(coup_args(a));
    out(coupon_dates(c.settle, c.mat, c.freq).pcd as f64)
}

fn coupnum_fn(a: &[Value]) -> Value {
    let c = tryv!(coup_args(a));
    out(coupon_dates(c.settle, c.mat, c.freq).num as f64)
}

// ===========================================================================
// Securities
// ===========================================================================

fn accrint_fn(a: &[Value]) -> Value {
    let issue = tryv!(date_arg(a, 0));
    let first = tryv!(date_arg(a, 1));
    let settle = tryv!(date_arg(a, 2));
    let rate = tryv!(num(a, 3));
    let par = tryv!(num_or(a, 4, 1000.0));
    let freq = tryv!(freq_arg(a, 5));
    let basis = tryv!(basis_arg(a, 6));
    let calc_method = tryv!(bool_or(a, 7, true));
    if rate <= 0.0 || par <= 0.0 || issue >= settle {
        return Value::Error(ErrorKind::Num);
    }
    let start = if !calc_method && settle > first { first } else { issue };
    let step = 12 / freq as i64;
    let eom = is_eom(first);
    let c = |j: i64| add_months(first, j * step, eom);
    // Find the quasi-coupon period containing `start`.
    let mut j = 0;
    while c(j) > start {
        j -= 1;
    }
    while c(j + 1) <= start {
        j += 1;
    }
    let mut sum = 0.0;
    loop {
        let (p0, p1) = (c(j), c(j + 1));
        if p0 >= settle {
            break;
        }
        let s = start.max(p0);
        let e = settle.min(p1);
        if e > s {
            sum += day_count(s, e, basis) / period_len(p0, p1, freq, basis);
        }
        j += 1;
    }
    out(par * rate / freq as f64 * sum)
}

fn accrintm_fn(a: &[Value]) -> Value {
    let issue = tryv!(date_arg(a, 0));
    let settle = tryv!(date_arg(a, 1));
    let rate = tryv!(num(a, 2));
    let par = tryv!(num_or(a, 3, 1000.0));
    let basis = tryv!(basis_arg(a, 4));
    if rate <= 0.0 || par <= 0.0 || issue >= settle {
        return Value::Error(ErrorKind::Num);
    }
    out(par * rate * yearfrac(issue, settle, basis))
}

/// settlement, maturity, x, y, [basis] where settlement < maturity.
fn sec_args(a: &[Value]) -> R<(i64, i64, f64, f64, i32)> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let x = num(a, 2)?;
    let y = num(a, 3)?;
    let basis = basis_arg(a, 4)?;
    if settle >= mat {
        return Err(ErrorKind::Num);
    }
    Ok((settle, mat, x, y, basis))
}

fn disc_fn(a: &[Value]) -> Value {
    let (s, m, pr, red, b) = tryv!(sec_args(a));
    if pr <= 0.0 || red <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out((red - pr) / red / yearfrac(s, m, b))
}

fn intrate_fn(a: &[Value]) -> Value {
    let (s, m, inv, red, b) = tryv!(sec_args(a));
    if inv <= 0.0 || red <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out((red - inv) / inv / yearfrac(s, m, b))
}

fn received_fn(a: &[Value]) -> Value {
    let (s, m, inv, disc, b) = tryv!(sec_args(a));
    if inv <= 0.0 || disc <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let d = 1.0 - disc * yearfrac(s, m, b);
    if d <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(inv / d)
}

fn pricedisc_fn(a: &[Value]) -> Value {
    let (s, m, disc, red, b) = tryv!(sec_args(a));
    if disc <= 0.0 || red <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(red - disc * red * yearfrac(s, m, b))
}

fn yielddisc_fn(a: &[Value]) -> Value {
    let (s, m, pr, red, b) = tryv!(sec_args(a));
    if pr <= 0.0 || red <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out((red - pr) / pr / yearfrac(s, m, b))
}

fn mat_args(a: &[Value]) -> R<(i64, i64, i64, f64, f64, i32)> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let issue = date_arg(a, 2)?;
    let rate = num(a, 3)?;
    let x = num(a, 4)?;
    let basis = basis_arg(a, 5)?;
    if settle >= mat || rate < 0.0 || x < 0.0 || issue > settle {
        return Err(ErrorKind::Num);
    }
    Ok((settle, mat, issue, rate, x, basis))
}

fn pricemat_fn(a: &[Value]) -> Value {
    let (s, m, i, rate, yld, b) = tryv!(mat_args(a));
    let dim = yearfrac(i, m, b);
    let dsm = yearfrac(s, m, b);
    let ai = yearfrac(i, s, b);
    out((100.0 + dim * rate * 100.0) / (1.0 + dsm * yld) - ai * rate * 100.0)
}

fn yieldmat_fn(a: &[Value]) -> Value {
    let (s, m, i, rate, pr, b) = tryv!(mat_args(a));
    if pr <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let dim = yearfrac(i, m, b);
    let dsm = yearfrac(s, m, b);
    let ai = yearfrac(i, s, b);
    let x = pr / 100.0 + ai * rate;
    out(((1.0 + dim * rate) - x) / x / dsm)
}

struct Bond {
    c: CoupArgs,
    rate: f64,
    redemption: f64,
}

fn bond_price(b: &Bond, yld: f64) -> f64 {
    let f = b.c.freq as f64;
    let e = coupdays(&b.c);
    let a = coupdaybs(&b.c);
    let dsc = coupdaysnc(&b.c);
    let n = coupon_dates(b.c.settle, b.c.mat, b.c.freq).num as f64;
    let t1 = 100.0 * b.rate / f;
    let t2 = 1.0 + yld / f;
    let t3 = dsc / e;
    let mut p = b.redemption / t2.powf(n - 1.0 + t3);
    let mut k = 1.0;
    while k <= n {
        p += t1 / t2.powf(k - 1.0 + t3);
        k += 1.0;
    }
    p - t1 * a / e
}

fn bond_args(a: &[Value]) -> R<(Bond, f64)> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let rate = num(a, 2)?;
    let x = num(a, 3)?;
    let redemption = num(a, 4)?;
    let freq = freq_arg(a, 5)?;
    let basis = basis_arg(a, 6)?;
    if settle >= mat || rate < 0.0 || x < 0.0 || redemption <= 0.0 {
        return Err(ErrorKind::Num);
    }
    Ok((Bond { c: CoupArgs { settle, mat, freq, basis }, rate, redemption }, x))
}

fn price_fn(a: &[Value]) -> Value {
    let (b, yld) = tryv!(bond_args(a));
    out(bond_price(&b, yld))
}

/// Solve price(y) = target for a yield; price is decreasing in y.
fn solve_yield(price: &dyn Fn(f64) -> f64, target: f64, guess: f64) -> Option<f64> {
    let f = |y: f64| -> (f64, f64) {
        let h = 1e-7 * y.abs().max(1e-3);
        let p = price(y) - target;
        let dp = (price(y + h) - price(y - h)) / (2.0 * h);
        (p, dp)
    };
    let mut y = guess;
    for _ in 0..100 {
        let (p, dp) = f(y);
        if !p.is_finite() || !dp.is_finite() || dp == 0.0 {
            break;
        }
        let ny = y - p / dp;
        if (ny - y).abs() < 1e-14 * ny.abs().max(1.0) {
            return Some(ny);
        }
        y = ny;
    }
    // Bisection fallback.
    let g = |y: f64| price(y) - target;
    let (mut lo, mut hi) = (-0.999, 1.0);
    while g(hi) > 0.0 && hi < 1e6 {
        hi *= 2.0;
    }
    let (glo, ghi) = (g(lo), g(hi));
    if !glo.is_finite() || !ghi.is_finite() || glo.signum() == ghi.signum() {
        return None;
    }
    for _ in 0..300 {
        let m = (lo + hi) / 2.0;
        if g(m).signum() == glo.signum() {
            lo = m;
        } else {
            hi = m;
        }
    }
    Some((lo + hi) / 2.0)
}

fn yield_fn(a: &[Value]) -> Value {
    let (b, pr) = tryv!(bond_args(a));
    if pr <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let n = coupon_dates(b.c.settle, b.c.mat, b.c.freq).num;
    if n <= 1 {
        let f = b.c.freq as f64;
        let e = coupdays(&b.c);
        let a_ = coupdaybs(&b.c);
        let dsr = coupdaysnc(&b.c);
        let x = pr / 100.0 + a_ / e * b.rate / f;
        let y = ((b.redemption / 100.0 + b.rate / f) - x) / x * f * e / dsr;
        return out(y);
    }
    match solve_yield(&|y| bond_price(&b, y), pr, b.rate.max(0.05)) {
        Some(y) => out(y),
        None => Value::Error(ErrorKind::Num),
    }
}

fn duration_calc(a: &[Value]) -> R<f64> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let coupon = num(a, 2)?;
    let yld = num(a, 3)?;
    let freq = freq_arg(a, 4)?;
    let basis = basis_arg(a, 5)?;
    if settle >= mat || coupon < 0.0 || yld < 0.0 {
        return Err(ErrorKind::Num);
    }
    let f = freq as f64;
    let c = CoupArgs { settle, mat, freq, basis };
    let n = coupon_dates(settle, mat, freq).num as f64;
    let cpn = coupon * 100.0 / f;
    let y = 1.0 + yld / f;
    // Time to the first cash flow, in periods, minus one.
    let diff = coupdaysnc(&c) / coupdays(&c) - 1.0;
    let mut dur = 0.0;
    let mut p = 0.0;
    let mut t = 1.0;
    while t < n {
        dur += (t + diff) * cpn / y.powf(t + diff);
        p += cpn / y.powf(t + diff);
        t += 1.0;
    }
    dur += (n + diff) * (cpn + 100.0) / y.powf(n + diff);
    p += (cpn + 100.0) / y.powf(n + diff);
    Ok(dur / p / f)
}

fn duration_fn(a: &[Value]) -> Value {
    res(duration_calc(a))
}

fn mduration_fn(a: &[Value]) -> Value {
    let d = tryv!(duration_calc(a));
    let yld = tryv!(num(a, 3));
    let f = tryv!(freq_arg(a, 4)) as f64;
    out(d / (1.0 + yld / f))
}

fn tbill_args(a: &[Value]) -> R<(f64, f64)> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let x = num(a, 2)?;
    if settle >= mat || x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    // Maturity may be at most one year after settlement.
    let (y, m, d) = ymd(settle);
    let limit = date_to_serial(y + 1, m as i64, d as i64).unwrap_or(f64::MAX) as i64;
    if mat > limit {
        return Err(ErrorKind::Num);
    }
    Ok(((mat - settle) as f64, x))
}

fn tbillprice_fn(a: &[Value]) -> Value {
    let (dsm, disc) = tryv!(tbill_args(a));
    let p = 100.0 * (1.0 - disc * dsm / 360.0);
    if p <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    out(p)
}

fn tbillyield_fn(a: &[Value]) -> Value {
    let (dsm, pr) = tryv!(tbill_args(a));
    out((100.0 - pr) / pr * 360.0 / dsm)
}

fn tbilleq_fn(a: &[Value]) -> Value {
    let (dsm, disc) = tryv!(tbill_args(a));
    if dsm <= 182.0 {
        return out(365.0 * disc / (360.0 - disc * dsm));
    }
    let price = 1.0 - disc * dsm / 360.0;
    if price <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let frac = dsm / 365.0;
    let tmp = (frac * frac - (2.0 * frac - 1.0) * (1.0 - 1.0 / price)).sqrt();
    out(2.0 * (tmp - frac) / (2.0 * frac - 1.0))
}

// ----- odd period securities ----------------------------------------------

struct OddLast {
    sum_dc: f64,
    sum_a: f64,
    sum_dsc: f64,
}

fn odd_last(settle: i64, mat: i64, last: i64, freq: i32, basis: i32) -> OddLast {
    let step = 12 / freq as i64;
    let eom = is_eom(last);
    let q = |j: i64| add_months(last, j * step, eom);
    let mut r = OddLast { sum_dc: 0.0, sum_a: 0.0, sum_dsc: 0.0 };
    let mut j = 1;
    loop {
        let (p0, p1) = (q(j - 1), q(j));
        if p0 >= mat {
            break;
        }
        let nl = period_len(p0, p1, freq, basis);
        r.sum_dc += day_count(p0, p1.min(mat), basis) / nl;
        if settle > p0 {
            r.sum_a += day_count(p0, p1.min(settle), basis) / nl;
        }
        let s = settle.max(p0);
        let e = p1.min(mat);
        if e > s {
            r.sum_dsc += day_count(s, e, basis) / nl;
        }
        j += 1;
    }
    r
}

fn oddl_args(a: &[Value]) -> R<(i64, i64, i64, f64, f64, f64, i32, i32)> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let last = date_arg(a, 2)?;
    let rate = num(a, 3)?;
    let x = num(a, 4)?;
    let red = num(a, 5)?;
    let freq = freq_arg(a, 6)?;
    let basis = basis_arg(a, 7)?;
    if !(mat > settle && settle > last) || rate < 0.0 || x < 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    Ok((settle, mat, last, rate, x, red, freq, basis))
}

fn oddlprice_fn(a: &[Value]) -> Value {
    let (settle, mat, last, rate, yld, red, freq, basis) = tryv!(oddl_args(a));
    let o = odd_last(settle, mat, last, freq, basis);
    let f = freq as f64;
    let c = 100.0 * rate / f;
    out((red + o.sum_dc * c) / (1.0 + o.sum_dsc * yld / f) - o.sum_a * c)
}

fn oddlyield_fn(a: &[Value]) -> Value {
    let (settle, mat, last, rate, pr, red, freq, basis) = tryv!(oddl_args(a));
    if pr <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    let o = odd_last(settle, mat, last, freq, basis);
    let f = freq as f64;
    let c = 100.0 * rate / f;
    let x = pr + o.sum_a * c;
    out(((red + o.sum_dc * c) - x) / x * f / o.sum_dsc)
}

struct OddFirst {
    settle: i64,
    mat: i64,
    issue: i64,
    first: i64,
    rate: f64,
    red: f64,
    freq: i32,
    basis: i32,
}

fn oddf_args(a: &[Value]) -> R<(OddFirst, f64)> {
    let settle = date_arg(a, 0)?;
    let mat = date_arg(a, 1)?;
    let issue = date_arg(a, 2)?;
    let first = date_arg(a, 3)?;
    let rate = num(a, 4)?;
    let x = num(a, 5)?;
    let red = num(a, 6)?;
    let freq = freq_arg(a, 7)?;
    let basis = basis_arg(a, 8)?;
    if !(mat >= first && first > settle && settle > issue) || rate < 0.0 || x < 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    Ok((OddFirst { settle, mat, issue, first, rate, red, freq, basis }, x))
}

fn oddf_price(o: &OddFirst, yld: f64) -> f64 {
    let step = 12 / o.freq as i64;
    let f = o.freq as f64;
    let eom = is_eom(o.first);
    let back = |k: i64| add_months(o.first, -k * step, eom);
    // Number of quasi-coupon periods in the odd first period.
    let mut nc = 1;
    while back(nc) > o.issue {
        nc += 1;
    }
    let q = |j: i64| back(nc - j); // q(0) <= issue < q(1) < ... < q(nc) = first
    let mut sum_dc = 0.0;
    let mut sum_a = 0.0;
    let mut dsc = 0.0;
    let mut e = 1.0;
    let mut nq = 0.0;
    for j in 1..=nc {
        let (p0, p1) = (q(j - 1), q(j));
        let nl = period_len(p0, p1, o.freq, o.basis);
        let s = o.issue.max(p0);
        sum_dc += day_count(s, p1, o.basis) / nl;
        if o.settle > s {
            sum_a += day_count(s, p1.min(o.settle), o.basis) / nl;
        }
        if p0 <= o.settle && o.settle < p1 {
            dsc = day_count(o.settle, p1, o.basis);
            e = nl;
            nq = (nc - j) as f64;
        }
    }
    // Regular coupons after the first coupon up to maturity.
    let eom_m = is_eom(o.mat);
    let mut n = 0i64;
    while add_months(o.mat, -n * step, eom_m) > o.first {
        n += 1;
    }
    let n = n as f64;
    let c = 100.0 * o.rate / f;
    let t2 = 1.0 + yld / f;
    let base = nq + dsc / e;
    let mut p = o.red / t2.powf(n + base) + c * sum_dc / t2.powf(base);
    let mut k = 1.0;
    while k <= n {
        p += c / t2.powf(k + base);
        k += 1.0;
    }
    p - c * sum_a
}

fn oddfprice_fn(a: &[Value]) -> Value {
    let (o, yld) = tryv!(oddf_args(a));
    out(oddf_price(&o, yld))
}

fn oddfyield_fn(a: &[Value]) -> Value {
    let (o, pr) = tryv!(oddf_args(a));
    if pr <= 0.0 {
        return Value::Error(ErrorKind::Num);
    }
    match solve_yield(&|y| oddf_price(&o, y), pr, o.rate.max(0.05)) {
        Some(y) => out(y),
        None => Value::Error(ErrorKind::Num),
    }
}
