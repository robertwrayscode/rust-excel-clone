use engine::{CellPos, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}

fn eval(wb: &mut Workbook, f: &str) -> Value {
    wb.set_input(0, p("Z100"), f).unwrap();
    wb.recalculate();
    wb.value(0, p("Z100"))
}

#[test]
fn basics() {
    let mut wb = Workbook::new();
    wb.set_input(0, p("A1"), "10").unwrap();
    wb.set_input(0, p("A2"), "20").unwrap();
    wb.set_input(0, p("A3"), "abc").unwrap();
    assert_eq!(eval(&mut wb, "=A1+A2*2"), Value::Number(50.0));
    assert_eq!(eval(&mut wb, "=SUM(A1:A3)"), Value::Number(30.0));
    assert_eq!(eval(&mut wb, "=SUM(A:A)"), Value::Number(30.0));
    assert_eq!(eval(&mut wb, "=A3+1"), Value::Error(engine::ErrorKind::Value));
    assert_eq!(eval(&mut wb, "=IF(A1>5,\"big\",\"small\")"), Value::Text("big".into()));
    assert_eq!(eval(&mut wb, "=1/0"), Value::Error(engine::ErrorKind::Div0));
    assert_eq!(eval(&mut wb, "=IFERROR(1/0,7)"), Value::Number(7.0));
    assert_eq!(eval(&mut wb, "=LET(x,5,y,x*2,x+y)"), Value::Number(15.0));
    assert_eq!(eval(&mut wb, "=LAMBDA(a,b,a*b)(3,4)"), Value::Number(12.0));
    assert_eq!(eval(&mut wb, "=LET(f,LAMBDA(n,n+1),f(1))"), Value::Number(2.0));
    assert_eq!(eval(&mut wb, "=\"a\"&\"b\"&1"), Value::Text("ab1".into()));
    assert_eq!(eval(&mut wb, "=-2^2"), Value::Number(4.0));
    assert_eq!(eval(&mut wb, "=2^3^2"), Value::Number(64.0));
    assert_eq!(eval(&mut wb, "=50%*2"), Value::Number(1.0));
    assert_eq!(eval(&mut wb, "=SUM((A1,A2))"), Value::Number(30.0));
    assert_eq!(eval(&mut wb, "=SUM(A1:A3 A2:B2)"), Value::Number(20.0));
    assert_eq!(eval(&mut wb, "=UNKNOWNFN(1)"), Value::Error(engine::ErrorKind::Name));
    assert_eq!(eval(&mut wb, "=\"abc\"=\"ABC\""), Value::Bool(true));
    assert_eq!(eval(&mut wb, "=SUM({1,2;3,4}*2)"), Value::Number(20.0));
}

#[test]
fn spill() {
    let mut wb = Workbook::new();
    wb.set_input(0, p("A1"), "1").unwrap();
    wb.set_input(0, p("A2"), "2").unwrap();
    wb.set_input(0, p("A3"), "3").unwrap();
    wb.set_input(0, p("B1"), "=A1:A3*10").unwrap();
    wb.set_input(0, p("C1"), "=SUM(B1#)").unwrap();
    wb.set_input(0, p("D1"), "=B3+1").unwrap();
    wb.recalculate();
    assert_eq!(wb.value(0, p("B2")), Value::Number(20.0));
    assert_eq!(wb.value(0, p("C1")), Value::Number(60.0));
    assert_eq!(wb.value(0, p("D1")), Value::Number(31.0));
    // block the spill
    wb.set_input(0, p("B3"), "x").unwrap();
    wb.recalculate();
    assert_eq!(wb.value(0, p("B1")), Value::Error(engine::ErrorKind::Spill));
}

#[test]
fn circular_and_deep_chain() {
    let mut wb = Workbook::new();
    wb.set_input(0, p("A1"), "=B1").unwrap();
    wb.set_input(0, p("B1"), "=A1").unwrap();
    wb.recalculate();
    assert!(!wb.circular.is_empty());
    let mut wb = Workbook::new();
    // reverse chain: A1 = A2+1, ..., last = 0
    let n = 20000;
    for i in 0..n {
        wb.set_input(0, CellPos::new(i, 0), &format!("=A{}+1", i + 2)).unwrap();
    }
    wb.recalculate();
    assert_eq!(wb.value(0, CellPos::new(0, 0)), Value::Number(n as f64));
}

#[test]
fn defined_names() {
    let mut wb = Workbook::new();
    wb.set_input(0, p("A1"), "4").unwrap();
    wb.define_name("Rate", None, "=Sheet1!$A$1").unwrap();
    wb.define_name("Double", None, "=LAMBDA(x,x*2)").unwrap();
    assert_eq!(eval(&mut wb, "=Rate*2"), Value::Number(8.0));
    assert_eq!(eval(&mut wb, "=Double(Rate)"), Value::Number(8.0));
}
