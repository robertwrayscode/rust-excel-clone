//! Financial and engineering functions checked against Excel's documented results.

use engine::{CellPos, ErrorKind, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}

fn eval(wb: &mut Workbook, f: &str) -> Value {
    wb.set_input(0, p("Z100"), f).unwrap();
    wb.recalculate();
    wb.value(0, p("Z100"))
}

/// Serial number for a date, as text to splice into formulas.
fn d(y: i64, m: i64, day: i64) -> String {
    format!("{}", engine::datetime::date_to_serial(y, m, day).unwrap())
}

/// Replace `DATE(y,m,d)` occurrences with serial numbers so the tests don't
/// depend on the date function library.
fn dates(f: &str) -> String {
    let mut out = String::new();
    let mut rest = f;
    while let Some(i) = rest.find("DATE(") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 5..];
        let j = after.find(')').unwrap();
        let parts: Vec<i64> = after[..j].split(',').map(|s| s.trim().parse().unwrap()).collect();
        out.push_str(&d(parts[0], parts[1], parts[2]));
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    out
}

#[track_caller]
fn approx(wb: &mut Workbook, f: &str, expected: f64, tol: f64) {
    let formula = dates(f);
    match eval(wb, &formula) {
        Value::Number(n) => {
            let scale = expected.abs().max(1.0);
            assert!((n - expected).abs() <= tol * scale, "{f}: got {n}, expected {expected}");
        }
        other => panic!("{f}: got {other:?}, expected {expected}"),
    }
}

#[track_caller]
fn close(wb: &mut Workbook, f: &str, expected: f64) {
    approx(wb, f, expected, 1e-9);
}

#[track_caller]
fn text(wb: &mut Workbook, f: &str, expected: &str) {
    let formula = dates(f);
    assert_eq!(eval(wb, &formula), Value::Text(expected.into()), "{f}");
}

#[track_caller]
fn err(wb: &mut Workbook, f: &str, e: ErrorKind) {
    let formula = dates(f);
    assert_eq!(eval(wb, &formula), Value::Error(e), "{f}");
}

// ===========================================================================
// Financial
// ===========================================================================

#[test]
fn tvm() {
    let mut wb = Workbook::new();
    close(&mut wb, "=PMT(0.08/12,10,10000)", -1037.03208935915);
    close(&mut wb, "=PMT(0.06/12,18*12,0,50000)", -129.081160867991);
    close(&mut wb, "=PMT(0,10,1000)", -100.0);
    close(&mut wb, "=FV(0.06/12,10,-200,-500,1)", 2581.40337406012);
    close(&mut wb, "=FV(0.12/12,12,-1000)", 12682.5030131789);
    close(&mut wb, "=PV(0.08/12,12*20,500,,0)", -59777.1458511878);
    close(&mut wb, "=NPER(0.12/12,-100,-1000,10000,1)", 59.6738656742946);
    close(&mut wb, "=NPER(0.12/12,-100,-1000,10000)", 60.0821228537617);
    close(&mut wb, "=NPER(0.12/12,-100,-1000)", -9.57859403981306);
    close(&mut wb, "=RATE(48,-200,8000)", 0.00770147248820165);
    close(&mut wb, "=RATE(4*12,-200,8000)*12", 0.0924176698584198);
    close(&mut wb, "=IPMT(0.1/12,1,36,8000)", -66.6666666666667);
    close(&mut wb, "=IPMT(0.1,3,3,8000)", -292.447129909366);
    close(&mut wb, "=PPMT(0.1/12,1,24,2000)", -75.6231860083873);
    close(&mut wb, "=PPMT(0.08,10,10,200000)", -27598.0534624214);
    close(&mut wb, "=CUMIPMT(0.09/12,360,125000,13,24,0)", -11135.2321307508);
    close(&mut wb, "=CUMIPMT(0.09/12,360,125000,1,1,0)", -937.5);
    close(&mut wb, "=CUMPRINC(0.09/12,360,125000,13,24,0)", -934.107123420873);
    close(&mut wb, "=CUMPRINC(0.09/12,360,125000,1,1,0)", -68.2782711809784);
    close(&mut wb, "=ISPMT(0.1/12,1,36,8000)", -64.8148148148148);
    close(&mut wb, "=EFFECT(0.0525,4)", 0.0535426673707584);
    close(&mut wb, "=NOMINAL(0.053543,4)", 0.052500319868356);
    close(&mut wb, "=PDURATION(0.025,2000,2200)", 3.85986616262266);
    close(&mut wb, "=RRI(96,10000,11000)", 1.1f64.powf(1.0 / 96.0) - 1.0);
    err(&mut wb, "=EFFECT(-1,4)", ErrorKind::Num);
    err(&mut wb, "=IPMT(0.1,5,3,8000)", ErrorKind::Num);
    err(&mut wb, "=RATE(10,100,100)", ErrorKind::Num);
}

#[test]
fn cash_flows() {
    let mut wb = Workbook::new();
    close(&mut wb, "=NPV(0.1,-10000,3000,4200,6800)", 1188.44341233522);
    close(&mut wb, "=IRR({-70000,12000,15000,18000,21000,26000})", 0.0866309480365316);
    close(&mut wb, "=IRR({-70000,12000,15000,18000,21000})", -0.0212448482734243);
    close(&mut wb, "=IRR({-70000,12000,15000},-0.1)", -0.443506941334635);
    close(&mut wb, "=MIRR({-120000,39000,30000,21000,37000,46000},0.1,0.12)", 0.126094130365905);
    close(&mut wb, "=MIRR({-120000,39000,30000,21000},0.1,0.12)", -0.0480446552499807);
    approx(&mut wb, "=XNPV(0.09,{-10000,2750,4250,3250,2750},{39448,39508,39751,39859,39904})", 2086.64760203287, 1e-8);
    approx(&mut wb, "=XIRR({-10000,2750,4250,3250,2750},{39448,39508,39751,39859,39904},0.1)", 0.373362535, 1e-8);
    close(&mut wb, "=FVSCHEDULE(1,{0.09,0.11,0.1})", 1.33089);
    err(&mut wb, "=IRR({1,2,3})", ErrorKind::Num);
    // ranges
    for (i, v) in ["-70000", "12000", "15000", "18000", "21000", "26000"].iter().enumerate() {
        wb.set_input(0, p(&format!("A{}", i + 1)), v).unwrap();
    }
    close(&mut wb, "=IRR(A1:A6)", 0.0866309480365316);
    close(&mut wb, "=NPV(0.1,A2:A6)+A1", 0.1 * 0.0 + {
        let v = [12000.0, 15000.0, 18000.0, 21000.0, 26000.0];
        v.iter().enumerate().map(|(i, x)| x / 1.1f64.powi(i as i32 + 1)).sum::<f64>() - 70000.0
    });
}

#[test]
fn depreciation() {
    let mut wb = Workbook::new();
    close(&mut wb, "=SLN(30000,7500,10)", 2250.0);
    close(&mut wb, "=SYD(30000,7500,10,1)", 4090.90909090909);
    close(&mut wb, "=SYD(30000,7500,10,10)", 409.090909090909);
    close(&mut wb, "=DB(1000000,100000,6,1,7)", 186083.333333333);
    close(&mut wb, "=DB(1000000,100000,6,2,7)", 259639.416666667);
    close(&mut wb, "=DB(1000000,100000,6,3,7)", 176814.44275);
    close(&mut wb, "=DB(1000000,100000,6,7,7)", 15845.0984738481);
    close(&mut wb, "=DDB(2400,300,10*365,1)", 1.31506849315068);
    close(&mut wb, "=DDB(2400,300,10*12,1,2)", 40.0);
    close(&mut wb, "=DDB(2400,300,10,1,2)", 480.0);
    close(&mut wb, "=DDB(2400,300,10,2,1.5)", 306.0);
    close(&mut wb, "=DDB(2400,300,10,10)", 22.1225472000001);
    close(&mut wb, "=VDB(2400,300,10*365,0,1)", 1.31506849315068);
    close(&mut wb, "=VDB(2400,300,10*12,0,1)", 40.0);
    close(&mut wb, "=VDB(2400,300,10,0,1)", 480.0);
    close(&mut wb, "=VDB(2400,300,10*12,6,18)", 396.306053264751);
    close(&mut wb, "=VDB(2400,300,10*12,6,18,1.5)", 311.808936658233);
    close(&mut wb, "=VDB(2400,300,10,0,0.875,1.5)", 315.0);
    close(&mut wb, "=AMORLINC(2400,DATE(2008,8,19),DATE(2008,12,31),300,1,0.15,1)", 360.0);
    close(&mut wb, "=AMORDEGRC(2400,DATE(2008,8,19),DATE(2008,12,31),300,1,0.15,1)", 776.0);
    close(&mut wb, "=DOLLARDE(1.02,16)", 1.125);
    close(&mut wb, "=DOLLARDE(1.1,32)", 1.3125);
    close(&mut wb, "=DOLLARFR(1.125,16)", 1.02);
    close(&mut wb, "=DOLLARFR(1.125,32)", 1.04);
    err(&mut wb, "=DOLLARDE(1.02,0)", ErrorKind::Div0);
}

#[test]
fn coupons() {
    let mut wb = Workbook::new();
    let args = "DATE(2011,1,25),DATE(2011,11,15),2,1";
    close(&mut wb, &format!("=COUPDAYBS({args})"), 71.0);
    close(&mut wb, &format!("=COUPDAYS({args})"), 181.0);
    close(&mut wb, &format!("=COUPDAYSNC({args})"), 110.0);
    close(&mut wb, &format!("=COUPNCD({args})"), 40678.0);
    close(&mut wb, &format!("=COUPNUM({args})"), 2.0);
    close(&mut wb, &format!("=COUPPCD({args})"), 40497.0);
    close(&mut wb, "=COUPDAYS(DATE(2011,1,25),DATE(2011,11,15),2,0)", 180.0);
    close(&mut wb, "=COUPDAYBS(DATE(2011,1,25),DATE(2011,11,15),2,0)", 70.0);
    close(&mut wb, "=COUPNUM(DATE(2007,1,25),DATE(2008,11,15),2,1)", 4.0);
    err(&mut wb, "=COUPNUM(DATE(2011,1,25),DATE(2011,11,15),3,1)", ErrorKind::Num);
    err(&mut wb, "=COUPNUM(DATE(2011,11,25),DATE(2011,11,15),2,1)", ErrorKind::Num);
}

#[test]
fn securities() {
    let mut wb = Workbook::new();
    close(&mut wb, "=ACCRINT(DATE(2008,3,1),DATE(2008,8,31),DATE(2008,5,1),0.1,1000,2,0)", 16.6666666666667);
    close(&mut wb, "=ACCRINT(DATE(2008,3,5),DATE(2008,8,31),DATE(2008,5,1),0.1,1000,2,0,TRUE)", 15.5555555555556);
    close(&mut wb, "=ACCRINT(DATE(2008,4,5),DATE(2008,8,31),DATE(2008,5,1),0.1,1000,2,0,FALSE)", 7.22222222222222);
    close(&mut wb, "=ACCRINTM(DATE(2008,4,1),DATE(2008,6,15),0.1,1000,3)", 20.5479452054795);
    close(&mut wb, "=DISC(DATE(2007,1,25),DATE(2007,6,15),97.975,100,1)", 0.0524202127659574);
    close(&mut wb, "=INTRATE(DATE(2008,2,15),DATE(2008,5,15),1000000,1014420,2)", 0.05768);
    close(&mut wb, "=RECEIVED(DATE(2008,2,15),DATE(2008,5,15),1000000,0.0575,2)", 1014584.6544071);
    close(&mut wb, "=PRICEDISC(DATE(2008,2,16),DATE(2008,3,1),0.0525,100,2)", 99.7958333333333);
    close(&mut wb, "=YIELDDISC(DATE(2008,2,16),DATE(2008,3,1),99.795,100,2)", 0.0528225719868583);
    close(&mut wb, "=PRICEMAT(DATE(2008,2,15),DATE(2008,4,13),DATE(2007,11,11),0.061,0.061,0)", 99.9844988755569);
    close(&mut wb, "=YIELDMAT(DATE(2008,3,15),DATE(2008,11,3),DATE(2007,11,8),0.0625,100.0123,0)", 0.0609543336915387);
    close(&mut wb, "=PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,0)", 94.6343616213221);
    close(&mut wb, "=YIELD(DATE(2008,2,15),DATE(2016,11,15),0.0575,95.04287,100,2,0)", 0.0650000068807314);
    close(&mut wb, "=DURATION(DATE(2018,7,1),DATE(2048,1,1),0.08,0.09,2,1)", 10.9191452815919);
    close(&mut wb, "=DURATION(DATE(2008,1,1),DATE(2016,1,1),0.08,0.09,2,1)", 5.99377495554519);
    close(&mut wb, "=MDURATION(DATE(2008,1,1),DATE(2016,1,1),0.08,0.09,2,1)", 5.99377495554519 / 1.045);
    close(&mut wb, "=TBILLEQ(DATE(2008,3,31),DATE(2008,6,1),0.0914)", 0.0941514936010053);
    close(&mut wb, "=TBILLPRICE(DATE(2008,3,31),DATE(2008,6,1),0.09)", 98.45);
    close(&mut wb, "=TBILLYIELD(DATE(2008,3,31),DATE(2008,6,1),98.45)", 0.0914169629253426);
    approx(&mut wb, "=ODDFPRICE(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0785,0.0625,100,2,1)", 113.597717474079, 1e-8);
    approx(&mut wb, "=ODDFYIELD(DATE(2008,11,11),DATE(2021,3,1),DATE(2008,10,15),DATE(2009,3,1),0.0575,84.5,100,2,0)", 0.0772455415972989, 1e-8);
    approx(&mut wb, "=ODDLPRICE(DATE(2008,2,7),DATE(2008,6,15),DATE(2007,10,15),0.0375,0.0405,100,2,0)", 99.8782860147213, 1e-8);
    approx(&mut wb, "=ODDLYIELD(DATE(2008,4,20),DATE(2008,6,15),DATE(2007,12,24),0.0375,99.875,100,2,0)", 0.0451922356291692, 1e-8);
    err(&mut wb, "=PRICE(DATE(2018,2,15),DATE(2017,11,15),0.0575,0.065,100,2,0)", ErrorKind::Num);
    err(&mut wb, "=PRICE(DATE(2008,2,15),DATE(2017,11,15),0.0575,0.065,100,2,5)", ErrorKind::Num);
}

// ===========================================================================
// Engineering
// ===========================================================================

/// Relative-tolerance check (for very small expected values).
#[track_caller]
fn rel(wb: &mut Workbook, f: &str, expected: f64, tol: f64) {
    match eval(wb, f) {
        Value::Number(n) => {
            assert!((n - expected).abs() <= tol * expected.abs(), "{f}: got {n}, expected {expected}");
        }
        other => panic!("{f}: got {other:?}, expected {expected}"),
    }
}

#[test]
fn bessel() {
    let mut wb = Workbook::new();
    // Excel documentation examples (Excel itself is accurate to ~1e-8).
    approx(&mut wb, "=BESSELI(1.5,1)", 0.981666428, 1e-8);
    approx(&mut wb, "=BESSELJ(1.9,2)", 0.329925728, 1e-8);
    approx(&mut wb, "=BESSELK(1.5,1)", 0.277387804, 1e-8);
    approx(&mut wb, "=BESSELY(2.5,1)", 0.145918138, 1e-8);
    // Reference values.
    rel(&mut wb, "=BESSELJ(10,0)", -0.2459357644513483, 1e-11);
    rel(&mut wb, "=BESSELJ(20,1)", 0.06683312417584993, 1e-10);
    rel(&mut wb, "=BESSELJ(-1.9,1)", -0.581157072713434, 1e-11);
    rel(&mut wb, "=BESSELY(10,0)", 0.05567116728359939, 1e-10);
    rel(&mut wb, "=BESSELY(0.5,2)", -5.441370837174266, 1e-11);
    rel(&mut wb, "=BESSELK(0.1,0)", 2.427069024702017, 1e-11);
    rel(&mut wb, "=BESSELK(3,2)", 0.06151045847174204, 1e-10);
    rel(&mut wb, "=BESSELI(10,0)", 2815.716628466254, 1e-12);
    rel(&mut wb, "=BESSELI(2,3)", 0.2127399592398527, 1e-12);
    err(&mut wb, "=BESSELK(-1,1)", ErrorKind::Num);
    err(&mut wb, "=BESSELJ(1,-1)", ErrorKind::Num);
}

#[test]
fn erf_functions() {
    let mut wb = Workbook::new();
    approx(&mut wb, "=ERF(0.745)", 0.70792892, 1e-8);
    rel(&mut wb, "=ERF(1)", 0.8427007929497149, 1e-14);
    rel(&mut wb, "=ERF(0.5)", 0.5204998778130465, 1e-14);
    rel(&mut wb, "=ERF(2)", 0.9953222650189527, 1e-14);
    rel(&mut wb, "=ERF(-1)", -0.8427007929497149, 1e-14);
    rel(&mut wb, "=ERF(0,1)", 0.8427007929497149, 1e-14);
    rel(&mut wb, "=ERF(1,2)", 0.9953222650189527 - 0.8427007929497149, 1e-13);
    rel(&mut wb, "=ERF.PRECISE(1)", 0.8427007929497149, 1e-14);
    rel(&mut wb, "=ERFC(1)", 0.1572992070502851, 1e-14);
    rel(&mut wb, "=ERFC(2)", 0.004677734981047266, 1e-13);
    rel(&mut wb, "=ERFC(3)", 2.209049699858544e-05, 1e-13);
    rel(&mut wb, "=ERFC(5)", 1.537459794428035e-12, 1e-13);
    rel(&mut wb, "=ERFC(-1)", 1.8427007929497148, 1e-14);
    rel(&mut wb, "=ERFC.PRECISE(1)", 0.1572992070502851, 1e-14);
    rel(&mut wb, "=ERF(0.001)", 0.001128378790969, 1e-12);
}

#[test]
fn base_conversion() {
    let mut wb = Workbook::new();
    close(&mut wb, "=BIN2DEC(1100100)", 100.0);
    close(&mut wb, "=BIN2DEC(1111111111)", -1.0);
    text(&mut wb, "=BIN2HEX(11111011,4)", "00FB");
    text(&mut wb, "=BIN2HEX(1110)", "E");
    text(&mut wb, "=BIN2HEX(1111111111)", "FFFFFFFFFF");
    text(&mut wb, "=BIN2OCT(1001,3)", "011");
    text(&mut wb, "=BIN2OCT(1111111111)", "7777777777");
    text(&mut wb, "=DEC2BIN(9,4)", "1001");
    text(&mut wb, "=DEC2BIN(-100)", "1110011100");
    text(&mut wb, "=DEC2BIN(-1)", "1111111111");
    text(&mut wb, "=DEC2HEX(100,4)", "0064");
    text(&mut wb, "=DEC2HEX(-54)", "FFFFFFFFCA");
    text(&mut wb, "=DEC2HEX(28)", "1C");
    text(&mut wb, "=DEC2OCT(58,3)", "072");
    text(&mut wb, "=DEC2OCT(-100)", "7777777634");
    text(&mut wb, "=HEX2BIN(\"F\",8)", "00001111");
    text(&mut wb, "=HEX2BIN(\"B7\")", "10110111");
    text(&mut wb, "=HEX2BIN(\"FFFFFFFFFF\")", "1111111111");
    close(&mut wb, "=HEX2DEC(\"A5\")", 165.0);
    close(&mut wb, "=HEX2DEC(\"FFFFFFFF5B\")", -165.0);
    close(&mut wb, "=HEX2DEC(\"3DA408B9\")", 1034160313.0);
    close(&mut wb, "=HEX2DEC(\"FFFFFFFFFF\")", -1.0);
    text(&mut wb, "=HEX2OCT(\"F\",3)", "017");
    text(&mut wb, "=HEX2OCT(\"3B4E\")", "35516");
    text(&mut wb, "=HEX2OCT(\"FFFFFFFF00\")", "7777777400");
    text(&mut wb, "=OCT2BIN(3,3)", "011");
    text(&mut wb, "=OCT2BIN(7777777000)", "1000000000");
    close(&mut wb, "=OCT2DEC(54)", 44.0);
    close(&mut wb, "=OCT2DEC(7777777533)", -165.0);
    text(&mut wb, "=OCT2HEX(100,4)", "0040");
    text(&mut wb, "=OCT2HEX(7777777533)", "FFFFFFFF5B");
    err(&mut wb, "=DEC2HEX(64,1)", ErrorKind::Num);
    err(&mut wb, "=DEC2BIN(512)", ErrorKind::Num);
    err(&mut wb, "=DEC2BIN(-513)", ErrorKind::Num);
    err(&mut wb, "=BIN2DEC(12)", ErrorKind::Num);
    err(&mut wb, "=HEX2DEC(\"FFFFFFFFFFF\")", ErrorKind::Num);
    err(&mut wb, "=HEX2BIN(\"200\")", ErrorKind::Num);
    err(&mut wb, "=DEC2BIN(3,-1)", ErrorKind::Num);
}

#[test]
fn bitwise_and_misc() {
    let mut wb = Workbook::new();
    close(&mut wb, "=BITAND(1,5)", 1.0);
    close(&mut wb, "=BITAND(13,25)", 9.0);
    close(&mut wb, "=BITOR(23,10)", 31.0);
    close(&mut wb, "=BITXOR(5,3)", 6.0);
    close(&mut wb, "=BITLSHIFT(4,2)", 16.0);
    close(&mut wb, "=BITLSHIFT(4,-2)", 1.0);
    close(&mut wb, "=BITRSHIFT(13,2)", 3.0);
    err(&mut wb, "=BITAND(-1,5)", ErrorKind::Num);
    err(&mut wb, "=BITAND(1.5,5)", ErrorKind::Num);
    err(&mut wb, "=BITLSHIFT(2^47,1)", ErrorKind::Num);
    close(&mut wb, "=DELTA(5,4)", 0.0);
    close(&mut wb, "=DELTA(5,5)", 1.0);
    close(&mut wb, "=DELTA(0.5)", 0.0);
    close(&mut wb, "=GESTEP(5,4)", 1.0);
    close(&mut wb, "=GESTEP(5,5)", 1.0);
    close(&mut wb, "=GESTEP(-4,-5)", 1.0);
    close(&mut wb, "=GESTEP(-1)", 0.0);
    err(&mut wb, "=DELTA(\"a\",1)", ErrorKind::Value);
}

#[test]
fn convert() {
    let mut wb = Workbook::new();
    close(&mut wb, "=CONVERT(1,\"lbm\",\"kg\")", 0.45359237);
    close(&mut wb, "=CONVERT(68,\"F\",\"C\")", 20.0);
    close(&mut wb, "=CONVERT(100,\"C\",\"F\")", 212.0);
    close(&mut wb, "=CONVERT(0,\"C\",\"K\")", 273.15);
    close(&mut wb, "=CONVERT(0,\"Reau\",\"C\")", 0.0);
    close(&mut wb, "=CONVERT(80,\"Reau\",\"C\")", 100.0);
    close(&mut wb, "=CONVERT(9,\"Rank\",\"K\")", 5.0);
    close(&mut wb, "=CONVERT(1,\"mK\",\"K\")", 0.001);
    err(&mut wb, "=CONVERT(2.5,\"ft\",\"sec\")", ErrorKind::NA);
    err(&mut wb, "=CONVERT(1,\"xyz\",\"m\")", ErrorKind::NA);
    close(&mut wb, "=CONVERT(CONVERT(100,\"ft\",\"m\"),\"ft\",\"m\")", 9.290304);
    close(&mut wb, "=CONVERT(1,\"km\",\"mi\")", 0.621371192237334);
    close(&mut wb, "=CONVERT(6,\"mm\",\"in\")", 0.236220472440945);
    close(&mut wb, "=CONVERT(1,\"kibyte\",\"byte\")", 1024.0);
    close(&mut wb, "=CONVERT(1,\"byte\",\"bit\")", 8.0);
    close(&mut wb, "=CONVERT(1,\"Mibit\",\"kbit\")", 1048.576);
    close(&mut wb, "=CONVERT(1,\"m2\",\"ft2\")", 10.7639104167097);
    close(&mut wb, "=CONVERT(1,\"km2\",\"m2\")", 1e6);
    close(&mut wb, "=CONVERT(1,\"ha\",\"m^2\")", 10000.0);
    close(&mut wb, "=CONVERT(1,\"gal\",\"l\")", 3.785411784);
    close(&mut wb, "=CONVERT(1,\"m3\",\"l\")", 1000.0);
    close(&mut wb, "=CONVERT(1,\"hr\",\"mn\")", 60.0);
    close(&mut wb, "=CONVERT(1,\"day\",\"sec\")", 86400.0);
    close(&mut wb, "=CONVERT(1,\"yr\",\"day\")", 365.25);
    close(&mut wb, "=CONVERT(1,\"atm\",\"Pa\")", 101325.0);
    close(&mut wb, "=CONVERT(1,\"atm\",\"mmHg\")", 760.0);
    close(&mut wb, "=CONVERT(1,\"mph\",\"m/s\")", 0.44704);
    close(&mut wb, "=CONVERT(1,\"kn\",\"km/h\")", 1.852);
    close(&mut wb, "=CONVERT(1,\"kJ\",\"J\")", 1000.0);
    close(&mut wb, "=CONVERT(1,\"Wh\",\"J\")", 3600.0);
    close(&mut wb, "=CONVERT(1,\"HP\",\"W\")", 745.69987158227);
    close(&mut wb, "=CONVERT(1,\"T\",\"ga\")", 10000.0);
    close(&mut wb, "=CONVERT(1,\"N\",\"dyn\")", 100000.0);
    close(&mut wb, "=CONVERT(1,\"lbf\",\"N\")", 4.4482216152605);
    close(&mut wb, "=CONVERT(1,\"ton\",\"lbm\")", 2000.0);
    close(&mut wb, "=CONVERT(1,\"ozm\",\"g\")", 28.349523125);
    close(&mut wb, "=CONVERT(1,\"ft\",\"in\")", 12.0);
    close(&mut wb, "=CONVERT(1,\"cup\",\"tbs\")", 16.0);
}

#[test]
fn complex_numbers() {
    let mut wb = Workbook::new();
    text(&mut wb, "=IMSUM(\"3+4i\",\"5-3i\")", "8+i");
    text(&mut wb, "=IMSUB(\"13+4i\",\"5+3i\")", "8+i");
    text(&mut wb, "=IMPRODUCT(\"3+4i\",\"5-3i\")", "27+11i");
    text(&mut wb, "=IMPRODUCT(\"1+2i\",30)", "30+60i");
    text(&mut wb, "=IMDIV(\"-238+240i\",\"10+24i\")", "5+12i");
    close(&mut wb, "=IMABS(\"5+12i\")", 13.0);
    close(&mut wb, "=IMAGINARY(\"3+4i\")", 4.0);
    close(&mut wb, "=IMAGINARY(\"0-j\")", -1.0);
    close(&mut wb, "=IMAGINARY(4)", 0.0);
    close(&mut wb, "=IMREAL(\"6-9i\")", 6.0);
    close(&mut wb, "=IMREAL(\"-i\")", 0.0);
    close(&mut wb, "=IMREAL(\"1.5e2+2i\")", 150.0);
    close(&mut wb, "=IMARGUMENT(\"3+4i\")", 0.927295218001612);
    text(&mut wb, "=IMCONJUGATE(\"3+4i\")", "3-4i");
    text(&mut wb, "=IMCONJUGATE(\"3+4j\")", "3-4j");
    text(&mut wb, "=COMPLEX(3,4)", "3+4i");
    text(&mut wb, "=COMPLEX(3,4,\"j\")", "3+4j");
    text(&mut wb, "=COMPLEX(0,1)", "i");
    text(&mut wb, "=COMPLEX(0,-1)", "-i");
    text(&mut wb, "=COMPLEX(1,0)", "1");
    text(&mut wb, "=COMPLEX(0,0)", "0");
    text(&mut wb, "=COMPLEX(1.5,-2.25)", "1.5-2.25i");
    err(&mut wb, "=COMPLEX(1,1,\"k\")", ErrorKind::Value);
    text(&mut wb, "=IMCOS(\"1+i\")", "0.833730025131149-0.988897705762865i");
    text(&mut wb, "=IMSIN(\"4+3i\")", "-7.61923172032141-6.548120040911i");
    text(&mut wb, "=IMEXP(\"1+i\")", "1.46869393991589+2.28735528717884i");
    text(&mut wb, "=IMLN(\"3+4i\")", "1.6094379124341+0.927295218001612i");
    text(&mut wb, "=IMSQRT(\"1+i\")", "1.09868411346781+0.455089860562227i");
    text(&mut wb, "=IMLOG10(\"3+4i\")", "0.698970004336019+0.402719196273373i");
    text(&mut wb, "=IMLOG2(\"3+4i\")", "2.32192809488736+1.33780421245098i");
    text(&mut wb, "=IMPOWER(\"2+3i\",3)", "-46+9.00000000000001i");
    let parts = [
        ("IMSINH", -27.0168132580039, 3.85373803791938),
        ("IMCOSH", -27.0349456030742, 3.85115333481178),
        ("IMTAN", 0.00490825806749606, 1.00070953606723),
        ("IMCOT", 0.0049011823943045, -0.999266927805902),
        ("IMSEC", -0.0652940278579471, -0.0752249603027732),
        ("IMSECH", -0.0362534969158689, -0.00516434460775318),
        ("IMCSC", -0.0754898329158637, 0.0648774713706355),
        ("IMCSCH", -0.036275889628626, -0.0051744731840194),
    ];
    for (f, re, im) in parts {
        rel(&mut wb, &format!("=IMREAL({f}(\"4+3i\"))"), re, 1e-12);
        rel(&mut wb, &format!("=IMAGINARY({f}(\"4+3i\"))"), im, 1e-12);
    }
    err(&mut wb, "=IMDIV(\"1+i\",0)", ErrorKind::Num);
    err(&mut wb, "=IMLN(0)", ErrorKind::Num);
    err(&mut wb, "=IMSUM(\"1+i\",\"1+j\")", ErrorKind::Value);
    err(&mut wb, "=IMABS(\"abc\")", ErrorKind::Num);
    // ranges
    wb.set_input(0, p("B1"), "1+i").unwrap();
    wb.set_input(0, p("B2"), "2-3i").unwrap();
    wb.set_input(0, p("B3"), "4").unwrap();
    text(&mut wb, "=IMSUM(B1:B3)", "7-2i");
    text(&mut wb, "=IMPRODUCT(B1:B2)", "5-i");
}
