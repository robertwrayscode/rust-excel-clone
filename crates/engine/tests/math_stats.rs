//! Tests for the Math & Trig and Statistical function libraries.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use engine::{CellPos, ErrorKind, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}

fn eval(wb: &mut Workbook, f: &str) -> Value {
    wb.set_input(0, p("Z100"), f).unwrap();
    wb.recalculate();
    wb.value(0, p("Z100"))
}

fn book(cells: &[(&str, &str)]) -> Workbook {
    let mut wb = Workbook::new();
    for (a, v) in cells {
        wb.set_input(0, p(a), v).unwrap();
    }
    wb
}

/// Standard test data:
/// A1:A8 = 2,4,4,4,5,5,7,9   B1:B8 = 1,3,2,5,4,6,8,9   E1:E8 = 3,1,4,1,5,9,2,6
/// C1:C5 = apple, banana, apple, cherry, (blank)   D1:D5 = 10,20,30,40,50
fn data() -> Workbook {
    let mut cells: Vec<(String, String)> = Vec::new();
    for (i, (a, b)) in [(2, 1), (4, 3), (4, 2), (4, 5), (5, 4), (5, 6), (7, 8), (9, 9)].iter().enumerate() {
        cells.push((format!("A{}", i + 1), a.to_string()));
        cells.push((format!("B{}", i + 1), b.to_string()));
    }
    for (i, e) in [3, 1, 4, 1, 5, 9, 2, 6].iter().enumerate() {
        cells.push((format!("E{}", i + 1), e.to_string()));
    }
    for (i, (c, d)) in [("apple", 10), ("banana", 20), ("apple", 30), ("cherry", 40), ("", 50)].iter().enumerate() {
        if !c.is_empty() {
            cells.push((format!("C{}", i + 1), c.to_string()));
        }
        cells.push((format!("D{}", i + 1), d.to_string()));
    }
    let refs: Vec<(&str, &str)> = cells.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    book(&refs)
}

fn close(actual: &Value, expected: f64, rel: f64, f: &str) {
    match actual {
        Value::Number(n) => {
            let tol = rel * expected.abs().max(1e-300);
            assert!((n - expected).abs() <= tol, "{f}: got {n:.17e}, expected {expected:.17e}");
        }
        other => panic!("{f}: got {other:?}, expected {expected}"),
    }
}

fn check_all(wb: &mut Workbook, cases: &[(&str, f64)], rel: f64) {
    for (f, v) in cases {
        let r = eval(wb, &format!("={f}"));
        close(&r, *v, rel, f);
    }
}

fn check_err(wb: &mut Workbook, cases: &[(&str, ErrorKind)]) {
    for (f, e) in cases {
        let r = eval(wb, &format!("={f}"));
        assert_eq!(r, Value::Error(*e), "{f}");
    }
}

fn check_text(wb: &mut Workbook, cases: &[(&str, &str)]) {
    for (f, t) in cases {
        let r = eval(wb, &format!("={f}"));
        assert_eq!(r, Value::Text(t.to_string()), "{f}");
    }
}

fn array_of(v: &Value) -> Vec<Vec<f64>> {
    match v {
        Value::Array(a) => (0..a.rows)
            .map(|r| {
                (0..a.cols)
                    .map(|c| match a.get(r, c) {
                        Value::Number(n) => *n,
                        Value::Error(_) => f64::NAN,
                        o => panic!("non-number {o:?}"),
                    })
                    .collect()
            })
            .collect(),
        Value::Number(n) => vec![vec![*n]],
        o => panic!("not an array: {o:?}"),
    }
}

/// Evaluate a formula as a dynamic array anchored at a cell and read back its values.
fn eval_array(wb: &mut Workbook, f: &str) -> Value {
    wb.set_input(0, p("Z100"), f).unwrap();
    wb.recalculate();
    wb.sheets[0].cells.get(&p("Z100")).map(|c| c.value.clone()).unwrap_or(Value::Empty)
}

// ===========================================================================
// Math & Trig
// ===========================================================================

#[test]
fn elementary_math() {
    let mut wb = Workbook::new();
    let pi = std::f64::consts::PI;
    check_all(
        &mut wb,
        &[
            ("ABS(-3.5)", 3.5),
            ("ACOS(0.5)", 0.5f64.acos()),
            ("ACOSH(10)", 10f64.acosh()),
            ("ACOT(2)", 0.4636476090008061),
            ("ACOTH(6)", 0.16823611831060645),
            ("ASIN(0.5)", 0.5f64.asin()),
            ("ASINH(-2.5)", (-2.5f64).asinh()),
            ("ATAN(1)", pi / 4.0),
            ("ATAN2(1,1)", pi / 4.0),
            ("ATAN2(-1,-1)", -3.0 * pi / 4.0),
            ("ATANH(0.76159416)", 1.0000000096297197),
            ("COS(1.047)", 1.047f64.cos()),
            ("COSH(4)", 4f64.cosh()),
            ("COT(30)", -0.15611995216165922),
            ("COTH(2)", 1.0373147207275482),
            ("CSC(15)", 1.5377805615408537),
            ("CSCH(1.5)", 0.46964244059522464),
            ("SEC(45)", 1.9035944074044246),
            ("SECH(0.5)", 0.886818883970074),
            ("SIN(PI()/2)", 1.0),
            ("SINH(1)", 1f64.sinh()),
            ("TAN(0.785)", 0.785f64.tan()),
            ("TANH(-2)", (-2f64).tanh()),
            ("DEGREES(PI())", 180.0),
            ("RADIANS(270)", 4.71238898038469),
            ("PI()", pi),
            ("EXP(2)", 2f64.exp()),
            ("LN(86)", 86f64.ln()),
            ("LOG10(86)", 86f64.log10()),
            ("LOG(8,2)", 3.0),
            ("LOG(10)", 1.0),
            ("LOG(86,2.7182818)", 4.454347342888286),
            ("SQRT(16)", 4.0),
            ("SQRTPI(2)", 2.5066282746310002),
            ("POWER(5,2)", 25.0),
            ("POWER(98.6,3.2)", 2401077.2220695773),
            ("SIGN(-0.00001)", -1.0),
            ("SIGN(0)", 0.0),
            ("MOD(3,2)", 1.0),
            ("MOD(-3,2)", 1.0),
            ("MOD(3,-2)", -1.0),
            ("MOD(-3,-2)", -1.0),
            ("MOD(10,2.5)", 0.0),
            ("MOD(5.5,1)", 0.5),
            ("QUOTIENT(5,2)", 2.0),
            ("QUOTIENT(-10,3)", -3.0),
            ("FACT(5)", 120.0),
            ("FACT(1.9)", 1.0),
            ("FACT(0)", 1.0),
            ("FACTDOUBLE(6)", 48.0),
            ("FACTDOUBLE(7)", 105.0),
            ("FACTDOUBLE(-1)", 1.0),
            ("COMBIN(8,2)", 28.0),
            ("COMBIN(60,30)", 1.1826458156486142e17),
            ("COMBINA(4,3)", 20.0),
            ("COMBINA(10,3)", 220.0),
            ("GCD(5,2)", 1.0),
            ("GCD(24,36)", 12.0),
            ("GCD(7,1)", 1.0),
            ("GCD(5,0)", 5.0),
            ("LCM(5,2)", 10.0),
            ("LCM(24,36)", 72.0),
            ("LCM(3,0)", 0.0),
            ("MULTINOMIAL(2,3,4)", 1260.0),
            ("SERIESSUM(PI()/4,0,2,{1,-0.5,0.041666667,-0.001388889})", 0.7071032149236011),
        ],
        1e-9,
    );
    check_err(
        &mut wb,
        &[
            ("ACOS(2)", ErrorKind::Num),
            ("ACOSH(0.5)", ErrorKind::Num),
            ("ACOTH(0.5)", ErrorKind::Num),
            ("ATANH(1)", ErrorKind::Num),
            ("ATAN2(0,0)", ErrorKind::Div0),
            ("COT(0)", ErrorKind::Div0),
            ("CSC(0)", ErrorKind::Div0),
            ("SIN(2^28)", ErrorKind::Num),
            ("LN(0)", ErrorKind::Num),
            ("LOG(10,1)", ErrorKind::Div0),
            ("LOG(-1)", ErrorKind::Num),
            ("SQRT(-1)", ErrorKind::Num),
            ("POWER(0,0)", ErrorKind::Num),
            ("POWER(0,-1)", ErrorKind::Div0),
            ("POWER(-8,1/3)", ErrorKind::Num),
            ("MOD(3,0)", ErrorKind::Div0),
            ("QUOTIENT(1,0)", ErrorKind::Div0),
            ("FACT(-1)", ErrorKind::Num),
            ("FACT(171)", ErrorKind::Num),
            ("COMBIN(2,3)", ErrorKind::Num),
            ("GCD(-1,2)", ErrorKind::Num),
            ("MULTINOMIAL(1,-2)", ErrorKind::Num),
            ("ABS(\"x\")", ErrorKind::Value),
            ("SQRT(1/0)", ErrorKind::Div0),
        ],
    );
}

#[test]
fn rounding() {
    let mut wb = Workbook::new();
    check_all(
        &mut wb,
        &[
            ("ROUND(2.5,0)", 3.0),
            ("ROUND(-2.5,0)", -3.0),
            ("ROUND(2.15,1)", 2.2),
            ("ROUND(2.149,1)", 2.1),
            ("ROUND(-1.475,2)", -1.48),
            ("ROUND(1.005,2)", 1.01),
            ("ROUND(21.5,-1)", 20.0),
            ("ROUND(626.3,-3)", 1000.0),
            ("ROUND(1.98,-1)", 0.0),
            ("ROUND(-50.55,-2)", -100.0),
            ("ROUNDDOWN(3.2,0)", 3.0),
            ("ROUNDDOWN(76.9,0)", 76.0),
            ("ROUNDDOWN(3.14159,3)", 3.141),
            ("ROUNDDOWN(-3.14159,1)", -3.1),
            ("ROUNDDOWN(31415.92654,-2)", 31400.0),
            ("ROUNDUP(3.2,0)", 4.0),
            ("ROUNDUP(76.9,0)", 77.0),
            ("ROUNDUP(3.14159,3)", 3.142),
            ("ROUNDUP(-3.14159,1)", -3.2),
            ("ROUNDUP(31415.92654,-2)", 31500.0),
            ("ROUNDUP(0.1*3,1)", 0.3),
            ("TRUNC(8.9)", 8.0),
            ("TRUNC(-8.9)", -8.0),
            ("TRUNC(0.45,1)", 0.4),
            ("INT(8.9)", 8.0),
            ("INT(-8.9)", -9.0),
            ("EVEN(1.5)", 2.0),
            ("EVEN(3)", 4.0),
            ("EVEN(2)", 2.0),
            ("EVEN(-1)", -2.0),
            ("ODD(1.5)", 3.0),
            ("ODD(3)", 3.0),
            ("ODD(2)", 3.0),
            ("ODD(-1)", -1.0),
            ("ODD(-2)", -3.0),
            ("ODD(0)", 1.0),
            ("EVEN(0)", 0.0),
            ("MROUND(10,3)", 9.0),
            ("MROUND(-10,-3)", -9.0),
            ("MROUND(1.3,0.2)", 1.4),
            ("MROUND(5,0)", 0.0),
            ("CEILING(2.5,1)", 3.0),
            ("CEILING(-2.5,-2)", -4.0),
            ("CEILING(-2.5,2)", -2.0),
            ("CEILING(1.5,0.1)", 1.5),
            ("CEILING(0.234,0.01)", 0.24),
            ("CEILING(1.1,0.1)", 1.1),
            ("CEILING.MATH(24.3,5)", 25.0),
            ("CEILING.MATH(6.7)", 7.0),
            ("CEILING.MATH(-8.1,2)", -8.0),
            ("CEILING.MATH(-5.5,2,-1)", -6.0),
            ("CEILING.PRECISE(4.3)", 5.0),
            ("CEILING.PRECISE(-4.3)", -4.0),
            ("CEILING.PRECISE(4.3,2)", 6.0),
            ("CEILING.PRECISE(4.3,-2)", 6.0),
            ("CEILING.PRECISE(-4.3,2)", -4.0),
            ("ISO.CEILING(4.3)", 5.0),
            ("ISO.CEILING(-4.3,-2)", -4.0),
            ("FLOOR(3.7,2)", 2.0),
            ("FLOOR(-2.5,-2)", -2.0),
            ("FLOOR(-2.5,2)", -4.0),
            ("FLOOR(1.58,0.1)", 1.5),
            ("FLOOR(0.234,0.01)", 0.23),
            ("FLOOR.MATH(24.3,5)", 20.0),
            ("FLOOR.MATH(6.7)", 6.0),
            ("FLOOR.MATH(-8.1,2)", -10.0),
            ("FLOOR.MATH(-5.5,2,-1)", -4.0),
            ("FLOOR.PRECISE(-3.2,-1)", -4.0),
            ("FLOOR.PRECISE(3.2)", 3.0),
            ("FLOOR.PRECISE(-3.2,1)", -4.0),
            ("FLOOR.PRECISE(3.2,-1)", 3.0),
        ],
        1e-12,
    );
    check_err(
        &mut wb,
        &[
            ("MROUND(5,-2)", ErrorKind::Num),
            ("CEILING(2.5,-2)", ErrorKind::Num),
            ("FLOOR(2.5,-2)", ErrorKind::Num),
            ("FLOOR(2.5,0)", ErrorKind::Div0),
        ],
    );
}

#[test]
fn conversion_and_random() {
    let mut wb = Workbook::new();
    check_text(
        &mut wb,
        &[
            ("ROMAN(499,0)", "CDXCIX"),
            ("ROMAN(499,1)", "LDVLIV"),
            ("ROMAN(499,2)", "XDIX"),
            ("ROMAN(499,3)", "VDIV"),
            ("ROMAN(499,4)", "ID"),
            ("ROMAN(2013)", "MMXIII"),
            ("ROMAN(1999,FALSE)", "MIM"),
            ("ROMAN(0)", ""),
            ("BASE(7,2)", "111"),
            ("BASE(100,16)", "64"),
            ("BASE(15,2,10)", "0000001111"),
            ("BASE(35,36)", "Z"),
        ],
    );
    check_all(
        &mut wb,
        &[
            ("ARABIC(\"LVII\")", 57.0),
            ("ARABIC(\"mcmxii\")", 1912.0),
            ("ARABIC(\"\")", 0.0),
            ("ARABIC(\"-CD\")", -400.0),
            ("DECIMAL(\"FF\",16)", 255.0),
            ("DECIMAL(\"111\",2)", 7.0),
            ("DECIMAL(\"zap\",36)", 45745.0),
        ],
        1e-12,
    );
    check_err(
        &mut wb,
        &[
            ("ARABIC(\"ABC\")", ErrorKind::Value),
            ("ROMAN(4000)", ErrorKind::Value),
            ("ROMAN(-1)", ErrorKind::Value),
            ("BASE(-1,2)", ErrorKind::Num),
            ("BASE(10,37)", ErrorKind::Num),
            ("DECIMAL(\"12\",2)", ErrorKind::Num),
            ("RANDBETWEEN(5,1)", ErrorKind::Num),
        ],
    );
    for _ in 0..20 {
        match eval(&mut wb, "=RAND()") {
            Value::Number(n) => assert!((0.0..1.0).contains(&n)),
            o => panic!("{o:?}"),
        }
        match eval(&mut wb, "=RANDBETWEEN(-3,3)") {
            Value::Number(n) => assert!((-3.0..=3.0).contains(&n) && n == n.trunc()),
            o => panic!("{o:?}"),
        }
    }
}

#[test]
fn matrices() {
    let mut wb = book(&[("A1", "1"), ("B1", "3"), ("C1", "8"), ("A2", "4"), ("B2", "3"), ("C2", "6"), ("A3", "2"), ("B3", "6"), ("C3", "-1")]);
    check_all(&mut wb, &[("MDETERM(A1:C3)", 153.0), ("MDETERM({3,6;1,1})", -3.0), ("MDETERM({1,2;2,4})", 0.0)], 1e-12);
    check_err(&mut wb, &[("MDETERM({1,2,3})", ErrorKind::Value), ("MINVERSE({1,2;2,4})", ErrorKind::Num), ("MMULT({1,2},{1,2})", ErrorKind::Value)]);
    let inv = array_of(&eval_array(&mut wb, "=MINVERSE({4,-1;2,0})"));
    let want = [[0.0, 0.5], [-1.0, 2.0]];
    for r in 0..2 {
        for c in 0..2 {
            assert!((inv[r][c] - want[r][c]).abs() < 1e-12);
        }
    }
    let prod = array_of(&eval_array(&mut wb, "=MMULT({1,3;7,2},{2,0;0,2})"));
    assert_eq!(prod, vec![vec![2.0, 6.0], vec![14.0, 4.0]]);
    let ident = array_of(&eval_array(&mut wb, "=MMULT(A1:C3,MINVERSE(A1:C3))"));
    for (r, row) in ident.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            assert!((v - if r == c { 1.0 } else { 0.0 }).abs() < 1e-12);
        }
    }
    assert_eq!(array_of(&eval_array(&mut wb, "=MUNIT(3)")), vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]]);
    check_all(&mut wb, &[("SUM(MUNIT(4))", 4.0)], 1e-12);
}

#[test]
fn sums() {
    let mut wb = data();
    wb.set_input(0, p("F1"), "TRUE").unwrap();
    wb.set_input(0, p("F2"), "'5").unwrap();
    wb.set_input(0, p("F3"), "3").unwrap();
    check_all(
        &mut wb,
        &[
            ("SUM(A1:A8)", 40.0),
            ("SUM(1,\"2\",TRUE)", 4.0),
            ("SUM(F1:F3)", 3.0),
            ("PRODUCT(A1:A3)", 32.0),
            ("PRODUCT(2,3,F1:F3)", 18.0),
            ("SUMSQ(3,4)", 25.0),
            ("SUMSQ(A1:A3)", 36.0),
            ("SUMIF(A1:A8,\">4\")", 26.0),
            ("SUMIF(A1:A8,4)", 12.0),
            ("SUMIF(C1:C5,\"apple\",D1:D5)", 40.0),
            ("SUMIF(C1:C5,\"a*\",D1)", 40.0),
            ("SUMIF(C1:C5,\"\",D1:D5)", 50.0),
            ("SUMIF(C1:C5,\"<>apple\",D1:D5)", 110.0),
            ("SUMIF(C:C,\"apple\",D:D)", 40.0),
            ("SUMIFS(D1:D5,C1:C5,\"apple\",D1:D5,\">15\")", 30.0),
            ("SUMIFS(B1:B8,A1:A8,\">=4\",A1:A8,\"<6\")", 20.0),
            ("SUMPRODUCT(A1:A3,B1:B3)", 22.0),
            ("SUMPRODUCT({3,4;8,6;1,9},{2,7;6,7;5,3})", 156.0),
            ("SUMPRODUCT((A1:A8>4)*B1:B8)", 27.0),
            ("SUMPRODUCT(A1:A8)", 40.0),
            ("SUMX2MY2({2,3,9,1,8,7,5},{6,5,11,7,5,4,4})", -55.0),
            ("SUMX2PY2({2,3,9,1,8,7,5},{6,5,11,7,5,4,4})", 521.0),
            ("SUMXMY2({2,3,9,1,8,7,5},{6,5,11,7,5,4,4})", 79.0),
        ],
        1e-12,
    );
    let r = array_of(&eval_array(&mut wb, "=SUMIF(C1:C5,{\"apple\",\"banana\"},D1:D5)"));
    assert_eq!(r, vec![vec![40.0, 20.0]]);
    check_err(
        &mut wb,
        &[
            ("SUMPRODUCT({1,2},{1,2,3})", ErrorKind::Value),
            ("SUMIFS(D1:D5,C1:C4,\"apple\")", ErrorKind::Value),
            ("SUMX2MY2({1,2},{1,2,3})", ErrorKind::NA),
            ("SUM(1,\"x\")", ErrorKind::Value),
        ],
    );
}

#[test]
fn subtotal_and_aggregate() {
    let mut wb = book(&[
        ("A1", "120"),
        ("A2", "10"),
        ("A3", "150"),
        ("A4", "23"),
        ("A5", "=SUBTOTAL(9,A1:A4)"),
        ("B1", "#DIV/0!"),
        ("B2", "72"),
        ("B3", "30"),
        ("B4", "#NUM!"),
        ("B5", "31"),
        ("B6", "96"),
        ("B7", "32"),
        ("B8", "81"),
        ("B9", "33"),
        ("B10", "53"),
        ("B11", "34"),
        ("C1", "82"),
        ("C2", "65"),
        ("C3", "95"),
        ("C4", "63"),
        ("C5", "53"),
        ("C6", "71"),
        ("C7", "55"),
        ("C8", "83"),
        ("C9", "100"),
        ("C10", "91"),
        ("C11", "89"),
    ]);
    check_all(
        &mut wb,
        &[
            ("SUBTOTAL(9,A1:A4)", 303.0),
            ("SUBTOTAL(1,A1:A4)", 75.75),
            ("SUBTOTAL(9,A1:A5)", 303.0),
            ("SUBTOTAL(2,A1:A5)", 4.0),
            ("SUBTOTAL(3,A1:A5)", 4.0),
            ("SUBTOTAL(4,A1:A4)", 150.0),
            ("SUBTOTAL(5,A1:A4)", 10.0),
            ("SUBTOTAL(6,A1:A2)", 1200.0),
            ("SUBTOTAL(7,A1:A4)", 69.70592992469626),
            ("SUBTOTAL(8,A1:A4)", 60.36710610920487),
            ("SUBTOTAL(10,A1:A4)", 4858.916666666667),
            ("SUBTOTAL(11,A1:A4)", 3644.1875),
            ("SUBTOTAL(109,A1:A4)", 303.0),
            ("SUM(A1:A5)", 606.0),
            ("AGGREGATE(4,6,B1:B11)", 96.0),
            ("AGGREGATE(14,6,B1:B11,3)", 72.0),
            ("AGGREGATE(12,6,B1:B11,C1:C11)", 68.0),
            ("AGGREGATE(9,4,C1:C11)", 847.0),
            ("AGGREGATE(9,0,A1:A5)", 303.0),
            ("AGGREGATE(9,4,A1:A5)", 606.0),
            ("AGGREGATE(16,6,B1:B11,0.5)", 34.0),
            ("AGGREGATE(2,0,B1:B11)", 9.0),
        ],
        1e-8,
    );
    check_err(&mut wb, &[("AGGREGATE(4,0,B1:B11)", ErrorKind::Div0), ("SUBTOTAL(12,A1:A4)", ErrorKind::Value), ("AGGREGATE(15,6,B1:B11)", ErrorKind::Value)]);
    // hidden rows
    wb.sheets[0].hidden_rows.insert(1); // row 2
    check_all(&mut wb, &[("SUBTOTAL(109,A1:A4)", 293.0), ("SUBTOTAL(9,A1:A4)", 303.0), ("SUBTOTAL(102,A1:A4)", 3.0), ("AGGREGATE(9,5,A1:A4)", 293.0)], 1e-12);
}

// ===========================================================================
// Statistical
// ===========================================================================

#[test]
fn descriptive_stats() {
    let mut wb = data();
    wb.set_input(0, p("G1"), "TRUE").unwrap();
    wb.set_input(0, p("G2"), "text").unwrap();
    wb.set_input(0, p("G3"), "4").unwrap();
    wb.set_input(0, p("G4"), "8").unwrap();
    check_all(
        &mut wb,
        &[
            ("STDEV.S(2,4,4,4,5,5,7,9)", 2.1380899352994),
            ("STDEV.S(A1:A8)", 2.138089935299395),
            ("STDEV(A1:A8)", 2.138089935299395),
            ("STDEV.P(A1:A8)", 2.0),
            ("STDEVP(A1:A8)", 2.0),
            ("VAR.S(A1:A8)", 4.571428571428571),
            ("VAR(A1:A8)", 4.571428571428571),
            ("VAR.P(A1:A8)", 4.0),
            ("VARP(A1:A8)", 4.0),
            ("AVERAGE(A1:A8)", 5.0),
            ("AVERAGE(G1:G4)", 6.0),
            ("AVERAGEA(G1:G4)", 3.25),
            ("AVERAGE(1,TRUE,\"3\")", 5.0 / 3.0),
            ("STDEVA(G1:G4)", 3.5939764421413),
            ("STDEVPA(G1:G4)", 3.112474899497183),
            ("VARA(G1:G4)", 12.916666666666666),
            ("VARPA(G1:G4)", 9.6875),
            ("MAXA(G1:G2)", 1.0),
            ("MINA(G1:G4)", 0.0),
            ("MAX(A1:A8)", 9.0),
            ("MIN(A1:A8)", 2.0),
            ("MAX(G2)", 0.0),
            ("MAX(-1,-5)", -1.0),
            ("MEDIAN(A1:A8)", 4.5),
            ("MEDIAN(1,2,3,4,5)", 3.0),
            ("MODE.SNGL(A1:A8)", 4.0),
            ("MODE(A1:A8)", 4.0),
            ("MODE.SNGL(1,2,2,3,3)", 2.0),
            ("AVEDEV(A1:A8)", 1.5),
            ("DEVSQ(A1:A8)", 32.0),
            ("KURT(A1:A8)", 0.9406249999999998),
            ("SKEW(A1:A8)", 0.8184875533567996),
            ("SKEW.P(A1:A8)", 0.65625),
            ("GEOMEAN(A1:A8)", 4.603215596046736),
            ("HARMEAN(A1:A8)", 4.201750729470613),
            ("LARGE(A1:A8,1)", 9.0),
            ("LARGE(A1:A8,3)", 5.0),
            ("SMALL(A1:A8,2)", 4.0),
            ("SMALL({3,4,5,2,3,4,6,4,7},4)", 4.0),
            ("PERCENTILE.INC({1,2,3,4},0.3)", 1.9),
            ("PERCENTILE({1,2,3,4},0.3)", 1.9),
            ("PERCENTILE.EXC({1,2,3,6,6,6,7,8,9},0.25)", 2.5),
            ("PERCENTILE.EXC(A1:A8,0.5)", 4.5),
            ("QUARTILE.INC({1,2,4,7,8,9,10,12},1)", 3.5),
            ("QUARTILE({1,2,4,7,8,9,10,12},3)", 9.25),
            ("QUARTILE.EXC({6,7,15,36,39,40,41,42,43,47,49},1)", 15.0),
            ("QUARTILE.EXC({6,7,15,36,39,40,41,42,43,47,49},3)", 43.0),
            ("PERCENTRANK.INC({13,12,11,8,4,3,2,1,1,1},2)", 0.333),
            ("PERCENTRANK.INC({13,12,11,8,4,3,2,1,1,1},4)", 0.555),
            ("PERCENTRANK.INC({13,12,11,8,4,3,2,1,1,1},8)", 0.666),
            ("PERCENTRANK.INC({13,12,11,8,4,3,2,1,1,1},5)", 0.583),
            ("PERCENTRANK({13,12,11,8,4,3,2,1,1,1},5,5)", 0.58333),
            ("PERCENTRANK.EXC({1,2,3,6,6,6,7,8,9},7)", 0.7),
            ("PERCENTRANK.EXC({1,2,3,6,6,6,7,8,9},5.43)", 0.381),
            ("PERCENTRANK.EXC({1,2,3,6,6,6,7,8,9},5.43,1)", 0.3),
            ("RANK.EQ(4,A1:A8)", 5.0),
            ("RANK(9,A1:A8)", 1.0),
            ("RANK.EQ(4,A1:A8,1)", 2.0),
            ("RANK.AVG(4,A1:A8)", 6.0),
            ("RANK.AVG(5,A1:A8,1)", 5.5),
            ("TRIMMEAN({4,5,6,7,2,3,4,5,1,2,3},0.2)", 3.777777777777778),
            ("TRIMMEAN(A1:A8,0.25)", 4.833333333333333),
            ("COUNT(A1:A8,C1:C5)", 8.0),
            ("COUNT(1,\"2\",\"x\",TRUE,1/0)", 3.0),
            ("COUNTA(C1:C5)", 4.0),
            ("COUNTA(1,\"\",\"x\")", 3.0),
            ("COUNTBLANK(C1:C5)", 1.0),
            ("COUNTBLANK(C1:C10)", 6.0),
            ("COUNTIF(A1:A8,4)", 3.0),
            ("COUNTIF(A1:A8,\">=5\")", 4.0),
            ("COUNTIF(C1:C5,\"apple\")", 2.0),
            ("COUNTIF(C1:C5,\"*an*\")", 1.0),
            ("COUNTIF(C1:C5,\"<>apple\")", 3.0),
            ("COUNTIF(C1:C5,\"\")", 1.0),
            ("COUNTIF(C1:C5,\"<>\")", 4.0),
            ("COUNTIFS(A1:A8,\">3\",B1:B8,\"<6\")", 4.0),
            ("COUNTIFS(C1:C5,\"apple\",D1:D5,\">10\")", 1.0),
            ("AVERAGEIF(A1:A8,\">4\")", 6.5),
            ("AVERAGEIF(C1:C5,\"apple\",D1:D5)", 20.0),
            ("AVERAGEIFS(D1:D5,C1:C5,\"<>banana\",D1:D5,\"<45\")", 80.0 / 3.0),
            ("MAXIFS(D1:D5,C1:C5,\"apple\")", 30.0),
            ("MINIFS(D1:D5,C1:C5,\"apple\")", 10.0),
            ("MAXIFS(D1:D5,C1:C5,\"zzz\")", 0.0),
        ],
        1e-9,
    );
    check_err(
        &mut wb,
        &[
            ("AVERAGE(G2)", ErrorKind::Div0),
            ("STDEV.S(1)", ErrorKind::Div0),
            ("MEDIAN(G2)", ErrorKind::Num),
            ("MODE.SNGL(1,2,3)", ErrorKind::NA),
            ("LARGE(A1:A8,9)", ErrorKind::Num),
            ("SMALL(A1:A8,0)", ErrorKind::Num),
            ("PERCENTILE.INC(A1:A8,1.1)", ErrorKind::Num),
            ("PERCENTILE.EXC(A1:A8,0.05)", ErrorKind::Num),
            ("QUARTILE.INC(A1:A8,5)", ErrorKind::Num),
            ("QUARTILE.EXC(A1:A8,0)", ErrorKind::Num),
            ("PERCENTRANK.INC(A1:A8,10)", ErrorKind::NA),
            ("RANK.EQ(3,A1:A8)", ErrorKind::NA),
            ("GEOMEAN(1,0)", ErrorKind::Num),
            ("HARMEAN(-1)", ErrorKind::Num),
            ("KURT(1,2,3)", ErrorKind::Div0),
            ("SKEW(1,1,1)", ErrorKind::Div0),
            ("AVERAGEIF(A1:A8,\">100\")", ErrorKind::Div0),
            ("TRIMMEAN(A1:A8,1)", ErrorKind::Num),
            ("AVERAGEA(\"x\")", ErrorKind::Value),
        ],
    );
    let modes = array_of(&eval_array(&mut wb, "=MODE.MULT({1,2,3,4,3,2,1,2,3})"));
    assert_eq!(modes, vec![vec![2.0], vec![3.0]]);
    let freq = array_of(&eval_array(&mut wb, "=FREQUENCY({79,85,78,85,50,81,95,88,97},{70,79,89})"));
    assert_eq!(freq, vec![vec![1.0], vec![2.0], vec![4.0], vec![2.0]]);
    let large = array_of(&eval_array(&mut wb, "=LARGE(A1:A8,{1,2,3})"));
    assert_eq!(large, vec![vec![9.0, 7.0, 5.0]]);
}

#[test]
fn regression_stats() {
    let mut wb = data();
    check_all(
        &mut wb,
        &[
            ("CORREL(A1:A8,B1:B8)", 0.9254290458586865),
            ("PEARSON(A1:A8,B1:B8)", 0.9254290458586865),
            ("SLOPE(B1:B8,A1:A8)", 1.21875),
            ("INTERCEPT(B1:B8,A1:A8)", -1.34375),
            ("RSQ(B1:B8,A1:A8)", 0.856418918918919),
            ("STEYX(B1:B8,A1:A8)", 1.1524430571616109),
            ("FORECAST(10,B1:B8,A1:A8)", 10.84375),
            ("FORECAST.LINEAR(10,B1:B8,A1:A8)", 10.84375),
            ("COVARIANCE.S(A1:A8,B1:B8)", 5.571428571428571),
            ("COVARIANCE.P(A1:A8,B1:B8)", 4.875),
            ("COVAR(A1:A8,B1:B8)", 4.875),
            ("TREND(B1:B8,A1:A8,10)", 10.84375),
            ("GROWTH(B1:B8,A1:A8,10)", 17.264345448981032),
            ("PROB({0,1,2,3},{0.2,0.3,0.1,0.4},2)", 0.1),
            ("PROB({0,1,2,3},{0.2,0.3,0.1,0.4},1,3)", 0.8),
        ],
        1e-8,
    );
    let l = array_of(&eval_array(&mut wb, "=LINEST(B1:B8,A1:A8)"));
    assert!((l[0][0] - 1.21875).abs() < 1e-12 && (l[0][1] + 1.34375).abs() < 1e-12);
    let l = array_of(&eval_array(&mut wb, "=LINEST(B1:B8,A1:A8,TRUE,TRUE)"));
    assert_eq!(l.len(), 5);
    let want = [
        [1.21875, -1.34375],
        [0.20372507516258273, 1.0970931050963721],
        [0.856418918918919, 1.1524430571616109],
        [35.788235294117646, 6.0],
        [47.53125, 7.96875],
    ];
    for r in 0..5 {
        for c in 0..2 {
            assert!((l[r][c] - want[r][c]).abs() < 1e-9 * want[r][c].abs().max(1.0), "LINEST stats [{r}][{c}] = {}", l[r][c]);
        }
    }
    let l = array_of(&eval_array(&mut wb, "=LINEST(B1:B8,A1:A8,FALSE,TRUE)"));
    assert!((l[0][0] - 0.9870689655172413).abs() < 1e-12 && l[0][1] == 0.0 && l[1][1].is_nan());
    let l = array_of(&eval_array(&mut wb, "=LOGEST(B1:B8,A1:A8)"));
    assert!((l[0][0] - 1.347607435800806).abs() < 1e-12 && (l[0][1] - 0.8740094549652219).abs() < 1e-12);
    let t = array_of(&eval_array(&mut wb, "=TREND({1,2,3},,{4,5})"));
    assert!((t[0][0] - 4.0).abs() < 1e-12 && (t[0][1] - 5.0).abs() < 1e-12);
    let t = array_of(&eval_array(&mut wb, "=TREND(B1:B8,A1:A8)"));
    assert_eq!(t.len(), 8);
    assert!((t[0][0] - (1.21875 * 2.0 - 1.34375)).abs() < 1e-12);
    // Multiple regression: B = b + m1*A + m2*E, with x1 in H and x2 in I.
    for (i, (x1, x2)) in [(2, 3), (4, 1), (4, 4), (4, 1), (5, 5), (5, 9), (7, 2), (9, 6)].iter().enumerate() {
        wb.set_input(0, p(&format!("H{}", i + 1)), &x1.to_string()).unwrap();
        wb.set_input(0, p(&format!("I{}", i + 1)), &x2.to_string()).unwrap();
    }
    wb.set_input(0, p("K1"), "=LINEST(B1:B8,H1:I8)").unwrap();
    wb.recalculate();
    let coef = [wb.value(0, p("K1")), wb.value(0, p("L1")), wb.value(0, p("M1"))];
    close(&coef[0], -0.0019697964543674137, 1e-8, "m2");
    close(&coef[1], 1.2195502298095868, 1e-9, "m1");
    close(&coef[2], -1.3401181877872612, 1e-9, "b");
    check_err(&mut wb, &[("CORREL(A1:A8,B1:B7)", ErrorKind::NA), ("SLOPE({1,2},{3,3})", ErrorKind::Div0), ("STEYX({1,2},{3,4})", ErrorKind::Div0)]);
}

#[test]
fn hypothesis_tests() {
    let mut wb = data();
    let act = [["58", "35"], ["11", "25"], ["10", "23"]];
    let exp = [["45.35", "47.65"], ["17.56", "18.44"], ["16.09", "16.91"]];
    for r in 0..3 {
        for c in 0..2 {
            wb.set_input(0, CellPos::new(20 + r as u32, c as u32), act[r][c]).unwrap();
            wb.set_input(0, CellPos::new(20 + r as u32, 2 + c as u32), exp[r][c]).unwrap();
        }
    }
    check_all(
        &mut wb,
        &[
            ("T.TEST(A1:A8,B1:B8,2,1)", 0.5630278036985402),
            ("T.TEST(A1:A8,B1:B8,2,2)", 0.8443565908845522),
            ("T.TEST(A1:A8,B1:B8,1,3)", 0.42228081551722235),
            ("TTEST(A1:A8,B1:B8,2,2)", 0.8443565908845522),
            ("F.TEST(A1:A8,B1:B8)", 0.4846998335091659),
            ("FTEST(A1:A8,B1:B8)", 0.4846998335091659),
            ("Z.TEST(A1:A8,4)", 0.09293836618293788),
            ("Z.TEST(A1:A8,6,1.5)", 0.9703267806040401),
            ("ZTEST(A1:A8,4)", 0.09293836618293788),
            ("CHISQ.TEST(A21:B23,C21:D23)", 0.0003081920170083095),
            ("CHITEST(A21:B23,C21:D23)", 0.0003081920170083095),
        ],
        1e-8,
    );
    check_err(&mut wb, &[("T.TEST(A1:A8,B1:B8,3,1)", ErrorKind::Num), ("T.TEST(A1:A8,B1:B7,2,1)", ErrorKind::NA), ("CHISQ.TEST(A21:B23,C21:C23)", ErrorKind::NA)]);
}

#[test]
fn distributions() {
    let mut wb = Workbook::new();
    let cases: &[(&str, f64)] = &[
        ("NORM.S.DIST(1,TRUE)", 0.841344746068543),
        ("NORM.S.DIST(1,FALSE)", 0.24197072451914335),
        ("NORM.S.DIST(-10,TRUE)", 7.6198530241605261e-24),
        ("NORM.S.DIST(5,TRUE)", 0.99999971334842812),
        ("NORMSDIST(1)", 0.841344746068543),
        ("NORM.DIST(42,40,1.5,TRUE)", 0.90878878027413213),
        ("NORM.DIST(42,40,1.5,FALSE)", 0.10934004978399575),
        ("NORMDIST(42,40,1.5,TRUE)", 0.90878878027413213),
        ("NORM.INV(0.908789,40,1.5)", 42.000002009566161),
        ("NORMINV(0.908789,40,1.5)", 42.000002009566161),
        ("NORM.S.INV(0.908789)", 1.3333346730441075),
        ("NORMSINV(0.908789)", 1.3333346730441075),
        ("NORM.S.INV(0.001)", -3.0902323061678135),
        ("NORM.S.INV(1E-20)", -9.2623400897984076),
        ("NORM.S.INV(0.5)", 0.0),
        ("T.DIST.2T(2,10)", 0.0733880347707404),
        ("T.DIST(60,1,TRUE)", 0.99469532636737673),
        ("T.DIST(8,3,FALSE)", 0.00073690652094692633),
        ("T.DIST(-1.5,7,TRUE)", 0.088649243494985017),
        ("T.DIST.RT(1.959999998,60)", 0.027322464987960436),
        ("TDIST(1.5,7,1)", 0.088649243494985017),
        ("TDIST(2,10,2)", 0.0733880347707404),
        ("T.DIST.2T(0.001,1000)", 0.99920231501845273),
        ("T.INV(0.75,2)", 0.81649658092772603),
        ("T.INV(0.1,5)", -1.4758840488244811),
        ("T.INV.2T(0.546449,60)", 0.60653307582575504),
        ("TINV(0.546449,60)", 0.60653307582575504),
        ("CHISQ.DIST.RT(3,2)", 0.22313016014843),
        ("CHIDIST(3,2)", 0.22313016014843),
        ("CHISQ.DIST.RT(400,300)", 9.6786219949335771e-5),
        ("CHISQ.DIST(0.5,1,TRUE)", 0.52049987781304654),
        ("CHISQ.DIST(2,3,FALSE)", 0.20755374871029735),
        ("CHISQ.INV(0.93,1)", 3.2830202867595355),
        ("CHISQ.INV.RT(0.050001,10)", 18.306973456961056),
        ("CHIINV(0.050001,10)", 18.306973456961056),
        ("F.DIST(15.2069,6,4,TRUE)", 0.99000004300276269),
        ("F.DIST(15.2069,6,4,FALSE)", 0.0012237917087831732),
        ("F.DIST.RT(15.2068649,6,4)", 0.009999999952464601),
        ("FDIST(15.2068649,6,4)", 0.009999999952464601),
        ("F.INV(0.01,6,4)", 0.10930991412457854),
        ("F.INV.RT(0.01,6,4)", 15.20686486115753),
        ("FINV(0.01,6,4)", 15.20686486115753),
        ("BETA.DIST(2,8,10,TRUE,1,3)", 0.6854705810546875),
        ("BETA.DIST(2,8,10,FALSE,1,3)", 1.4837646484375),
        ("BETADIST(2,8,10,1,3)", 0.6854705810546875),
        ("BETA.DIST(0.3,0.5,0.5,TRUE)", 0.36901011956554538),
        ("BETA.INV(0.685470581,8,10,1,3)", 1.9999999999631427),
        ("BETAINV(0.685470581,8,10,1,3)", 1.9999999999631427),
        ("BINOM.DIST(3,10,0.5,FALSE)", 0.1171875),
        ("BINOM.DIST(6,10,0.5,TRUE)", 0.828125),
        ("BINOMDIST(6,10,0.5,TRUE)", 0.828125),
        ("BINOM.DIST(50,100,0.3,FALSE)", 1.3026227131445356e-5),
        ("BINOM.DIST(30,100,0.3,TRUE)", 0.54912360076879044),
        ("BINOM.DIST.RANGE(60,0.75,48)", 0.083974967429047502),
        ("BINOM.DIST.RANGE(60,0.75,45,50)", 0.5236297934718872),
        ("BINOM.INV(6,0.5,0.75)", 4.0),
        ("CRITBINOM(6,0.5,0.75)", 4.0),
        ("GAMMA.DIST(10.00001131,9,2,FALSE)", 0.032639130418294002),
        ("GAMMA.DIST(10.00001131,9,2,TRUE)", 0.068094003869787331),
        ("GAMMADIST(10.00001131,9,2,TRUE)", 0.068094003869787331),
        ("GAMMA.INV(0.068094,9,2)", 10.000011191437178),
        ("GAMMAINV(0.068094,9,2)", 10.000011191437178),
        ("GAMMA(2.5)", 1.329340388179137),
        ("GAMMA(-0.75)", -4.8341465442958777),
        ("GAMMA(0.1)", 9.5135076986687318),
        ("GAMMA(30.5)", 4.8226969334909086e+31),
        ("GAMMA(5)", 24.0),
        ("GAMMALN(4)", 1.791759469228055),
        ("GAMMALN(0.3)", 1.0957979948180755),
        ("GAMMALN(123.456)", 469.60554712992947),
        ("GAMMALN.PRECISE(4)", 1.791759469228055),
        ("EXPON.DIST(0.2,10,TRUE)", 0.86466471676338731),
        ("EXPON.DIST(0.2,10,FALSE)", 1.3533528323661269),
        ("EXPONDIST(0.2,10,TRUE)", 0.86466471676338731),
        ("HYPGEOM.DIST(1,4,8,20,TRUE)", 0.46542827657378741),
        ("HYPGEOM.DIST(1,4,8,20,FALSE)", 0.36326109391124871),
        ("HYPGEOMDIST(1,4,8,20)", 0.36326109391124871),
        ("LOGNORM.DIST(4,3.5,1.2,TRUE)", 0.039083555706800473),
        ("LOGNORM.DIST(4,3.5,1.2,FALSE)", 0.017617596681819224),
        ("LOGNORMDIST(4,3.5,1.2)", 0.039083555706800473),
        ("LOGNORM.INV(0.039084,3.5,1.2)", 4.0000252186806351),
        ("LOGINV(0.039084,3.5,1.2)", 4.0000252186806351),
        ("NEGBINOM.DIST(10,5,0.25,TRUE)", 0.31351405847817659),
        ("NEGBINOM.DIST(10,5,0.25,FALSE)", 0.05504866037517786),
        ("NEGBINOMDIST(10,5,0.25)", 0.05504866037517786),
        ("POISSON.DIST(2,5,TRUE)", 0.12465201948308114),
        ("POISSON.DIST(2,5,FALSE)", 0.084224337488568339),
        ("POISSON(2,5,FALSE)", 0.084224337488568339),
        ("WEIBULL.DIST(105,20,100,TRUE)", 0.92958139006927678),
        ("WEIBULL.DIST(105,20,100,FALSE)", 0.035588864024504382),
        ("WEIBULL(105,20,100,TRUE)", 0.92958139006927678),
        ("CONFIDENCE.NORM(0.05,2.5,50)", 0.69295191217483897),
        ("CONFIDENCE(0.05,2.5,50)", 0.69295191217483897),
        ("CONFIDENCE.T(0.05,1,50)", 0.28419685549572993),
        ("FISHER(0.75)", 0.97295507452765665),
        ("FISHERINV(0.972955)", 0.74999996739414839),
        ("GAUSS(2)", 0.47724986805182079),
        ("PHI(0.75)", 0.3011374321548044),
        ("STANDARDIZE(42,40,1.5)", 4.0 / 3.0),
        ("PERMUT(100,3)", 970200.0),
        ("PERMUT(3,2)", 6.0),
        ("PERMUTATIONA(3,2)", 9.0),
        ("PERMUTATIONA(2,2)", 4.0),
    ];
    check_all(&mut wb, cases, 1e-9);
    check_err(
        &mut wb,
        &[
            ("NORM.DIST(1,0,0,TRUE)", ErrorKind::Num),
            ("NORM.S.INV(0)", ErrorKind::Num),
            ("NORM.INV(1,0,1)", ErrorKind::Num),
            ("T.DIST(1,0,TRUE)", ErrorKind::Num),
            ("T.DIST.2T(-1,2)", ErrorKind::Num),
            ("TDIST(1,2,3)", ErrorKind::Num),
            ("CHISQ.DIST(-1,2,TRUE)", ErrorKind::Num),
            ("CHISQ.INV(1.5,2)", ErrorKind::Num),
            ("F.DIST(1,0,2,TRUE)", ErrorKind::Num),
            ("BETA.DIST(4,8,10,TRUE,1,3)", ErrorKind::Num),
            ("BETA.INV(0,8,10)", ErrorKind::Num),
            ("BINOM.DIST(11,10,0.5,FALSE)", ErrorKind::Num),
            ("BINOM.DIST(3,10,1.5,FALSE)", ErrorKind::Num),
            ("GAMMA(0)", ErrorKind::Num),
            ("GAMMA(-2)", ErrorKind::Num),
            ("GAMMALN(0)", ErrorKind::Num),
            ("GAMMA.DIST(-1,9,2,TRUE)", ErrorKind::Num),
            ("EXPON.DIST(1,0,TRUE)", ErrorKind::Num),
            ("HYPGEOM.DIST(5,4,8,20,TRUE)", ErrorKind::Num),
            ("LOGNORM.DIST(0,1,1,TRUE)", ErrorKind::Num),
            ("POISSON.DIST(-1,5,TRUE)", ErrorKind::Num),
            ("WEIBULL.DIST(1,0,1,TRUE)", ErrorKind::Num),
            ("CONFIDENCE.NORM(1,2.5,50)", ErrorKind::Num),
            ("CONFIDENCE.T(0.05,1,1)", ErrorKind::Div0),
            ("FISHER(1)", ErrorKind::Num),
            ("STANDARDIZE(1,1,0)", ErrorKind::Num),
            ("PERMUT(2,3)", ErrorKind::Num),
            ("NEGBINOM.DIST(1,0,0.5,TRUE)", ErrorKind::Num),
            ("NORM.S.DIST(\"x\",TRUE)", ErrorKind::Value),
        ],
    );
    // Inverses round-trip.
    check_all(
        &mut wb,
        &[
            ("NORM.S.DIST(NORM.S.INV(0.025),TRUE)", 0.025),
            ("NORM.S.DIST(NORM.S.INV(0.999999),TRUE)", 0.999999),
            ("T.DIST(T.INV(0.01,3),3,TRUE)", 0.01),
            ("CHISQ.DIST(CHISQ.INV(0.3,7),7,TRUE)", 0.3),
            ("CHISQ.DIST.RT(CHISQ.INV.RT(1E-10,4),4)", 1e-10),
            ("F.DIST(F.INV(0.7,3,9),3,9,TRUE)", 0.7),
            ("GAMMA.DIST(GAMMA.INV(0.5,0.3,4),0.3,4,TRUE)", 0.5),
            ("BETA.DIST(BETA.INV(0.2,0.5,2),0.5,2,TRUE)", 0.2),
        ],
        1e-9,
    );
}

#[test]
fn array_lifting_and_references() {
    let mut wb = data();
    let r = array_of(&eval_array(&mut wb, "=ABS({-1,2;-3,4})"));
    assert_eq!(r, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
    let r = array_of(&eval_array(&mut wb, "=ROUND(A1:A3/3,1)"));
    assert_eq!(r, vec![vec![0.7], vec![1.3], vec![1.3]]);
    let r = array_of(&eval_array(&mut wb, "=NORM.S.DIST({0,1},TRUE)"));
    assert!((r[0][0] - 0.5).abs() < 1e-15 && (r[0][1] - 0.841344746068543).abs() < 1e-14);
    // Whole-column references
    check_all(&mut wb, &[("SUM(A:A)", 40.0), ("AVERAGE(A:A)", 5.0), ("COUNTIF(A:A,4)", 3.0), ("SUMIFS(D:D,C:C,\"apple\")", 40.0)], 1e-12);
}

#[test]
fn registry_metadata() {
    let names = [
        "ABS", "ACOS", "ACOSH", "ACOT", "ACOTH", "AGGREGATE", "ARABIC", "ASIN", "ASINH", "ATAN", "ATAN2", "ATANH", "BASE", "CEILING",
        "CEILING.MATH", "CEILING.PRECISE", "ISO.CEILING", "COMBIN", "COMBINA", "COS", "COSH", "COT", "COTH", "CSC", "CSCH", "DECIMAL",
        "DEGREES", "EVEN", "EXP", "FACT", "FACTDOUBLE", "FLOOR", "FLOOR.MATH", "FLOOR.PRECISE", "GCD", "INT", "LCM", "LN", "LOG", "LOG10",
        "MDETERM", "MINVERSE", "MMULT", "MOD", "MROUND", "MULTINOMIAL", "MUNIT", "ODD", "PI", "POWER", "PRODUCT", "QUOTIENT", "RADIANS",
        "RAND", "RANDBETWEEN", "ROMAN", "ROUND", "ROUNDDOWN", "ROUNDUP", "SEC", "SECH", "SERIESSUM", "SIGN", "SIN", "SINH", "SQRT",
        "SQRTPI", "SUBTOTAL", "SUM", "SUMIF", "SUMIFS", "SUMPRODUCT", "SUMSQ", "SUMX2MY2", "SUMX2PY2", "SUMXMY2", "TAN", "TANH", "TRUNC",
        "AVEDEV", "AVERAGE", "AVERAGEA", "AVERAGEIF", "AVERAGEIFS", "BETA.DIST", "BETA.INV", "BINOM.DIST", "BINOM.DIST.RANGE",
        "BINOM.INV", "CHISQ.DIST", "CHISQ.DIST.RT", "CHISQ.INV", "CHISQ.INV.RT", "CHISQ.TEST", "CONFIDENCE.NORM", "CONFIDENCE.T",
        "CORREL", "COUNT", "COUNTA", "COUNTBLANK", "COUNTIF", "COUNTIFS", "COVARIANCE.P", "COVARIANCE.S", "DEVSQ", "EXPON.DIST",
        "F.DIST", "F.DIST.RT", "F.INV", "F.INV.RT", "F.TEST", "FISHER", "FISHERINV", "FORECAST", "FORECAST.LINEAR", "FREQUENCY", "GAMMA",
        "GAMMA.DIST", "GAMMA.INV", "GAMMALN", "GAMMALN.PRECISE", "GAUSS", "GEOMEAN", "GROWTH", "HARMEAN", "HYPGEOM.DIST", "INTERCEPT",
        "KURT", "LARGE", "LINEST", "LOGEST", "LOGNORM.DIST", "LOGNORM.INV", "MAX", "MAXA", "MAXIFS", "MEDIAN", "MIN", "MINA", "MINIFS",
        "MODE.MULT", "MODE.SNGL", "NEGBINOM.DIST", "NORM.DIST", "NORM.INV", "NORM.S.DIST", "NORM.S.INV", "PEARSON", "PERCENTILE.EXC",
        "PERCENTILE.INC", "PERCENTRANK.EXC", "PERCENTRANK.INC", "PERMUT", "PERMUTATIONA", "PHI", "POISSON.DIST", "PROB", "QUARTILE.EXC",
        "QUARTILE.INC", "RANK.AVG", "RANK.EQ", "RSQ", "SKEW", "SKEW.P", "SLOPE", "SMALL", "STANDARDIZE", "STDEV.P", "STDEV.S", "STDEVA",
        "STDEVPA", "STEYX", "T.DIST", "T.DIST.2T", "T.DIST.RT", "T.INV", "T.INV.2T", "T.TEST", "TREND", "TRIMMEAN", "VAR.P", "VAR.S",
        "VARA", "VARPA", "WEIBULL.DIST", "Z.TEST", "BETADIST", "BETAINV", "BINOMDIST", "CHIDIST", "CHIINV", "CHITEST", "CONFIDENCE",
        "COVAR", "CRITBINOM", "EXPONDIST", "FDIST", "FINV", "FTEST", "GAMMADIST", "GAMMAINV", "HYPGEOMDIST", "LOGINV", "LOGNORMDIST",
        "MODE", "NEGBINOMDIST", "NORMDIST", "NORMINV", "NORMSDIST", "NORMSINV", "PERCENTILE", "PERCENTRANK", "POISSON", "QUARTILE",
        "RANK", "STDEV", "STDEVP", "TDIST", "TINV", "TTEST", "VAR", "VARP", "WEIBULL", "ZTEST",
    ];
    for n in names {
        let d = engine::functions::lookup(n).unwrap_or_else(|| panic!("{n} not registered"));
        assert!(d.syntax.starts_with(&format!("{n}(")), "{n} syntax: {}", d.syntax);
        assert!(!d.description.is_empty());
        assert!(d.category == "Math & Trig" || d.category == "Statistical", "{n}");
    }
}

#[test]
fn large_parameter_cases() {
    let mut wb = Workbook::new();
    check_all(&mut wb, &[("BINOM.INV(1000000,0.5,0.5)", 500000.0), ("BINOM.INV(1000,0.5,0.5)", 500.0)], 1e-12);
}
