use engine::{CellPos, ErrorKind, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}

fn n(x: f64) -> Value {
    Value::Number(x)
}

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn er(e: ErrorKind) -> Value {
    Value::Error(e)
}

fn set(wb: &mut Workbook, a1: &str, v: &str) {
    wb.set_input(0, p(a1), v).unwrap();
}

/// Evaluate a formula in Z100 and return its (top-left) value.
fn eval(wb: &mut Workbook, f: &str) -> Value {
    wb.set_input(0, p("Z100"), f).unwrap();
    wb.recalculate();
    wb.value(0, p("Z100"))
}

/// Evaluate a formula in AA1 and return the spilled block (rows x cols).
fn spill(wb: &mut Workbook, f: &str, rows: u32, cols: u32) -> Vec<Vec<Value>> {
    let anchor = p("AA1");
    wb.set_input(0, anchor, f).unwrap();
    wb.recalculate();
    let out = (0..rows)
        .map(|r| (0..cols).map(|c| wb.value(0, CellPos::new(anchor.row + r, anchor.col + c))).collect())
        .collect();
    wb.set_input(0, anchor, "").unwrap();
    wb.recalculate();
    out
}

fn nums(rows: &[&[f64]]) -> Vec<Vec<Value>> {
    rows.iter().map(|r| r.iter().map(|x| n(*x)).collect()).collect()
}

/// A small table in A1:C5: Name | Qty | Price
fn table() -> Workbook {
    let mut wb = Workbook::new();
    for (i, row) in [["Name", "Qty", "Price"], ["apple", "10", "1.5"], ["banana", "20", "0.5"], ["cherry", "5", "4"], ["date", "8", "3"]]
        .iter()
        .enumerate()
    {
        for (j, v) in row.iter().enumerate() {
            wb.set_input(0, CellPos::new(i as u32, j as u32), v).unwrap();
        }
    }
    wb
}

#[test]
fn vlookup_hlookup() {
    let mut wb = table();
    assert_eq!(eval(&mut wb, "=VLOOKUP(\"banana\",A2:C5,2,FALSE)"), n(20.0));
    assert_eq!(eval(&mut wb, "=VLOOKUP(\"CHERRY\",A2:C5,3,FALSE)"), n(4.0));
    assert_eq!(eval(&mut wb, "=VLOOKUP(\"ch*\",A2:C5,2,FALSE)"), n(5.0));
    assert_eq!(eval(&mut wb, "=VLOOKUP(\"zzz\",A2:C5,2,FALSE)"), er(ErrorKind::NA));
    assert_eq!(eval(&mut wb, "=VLOOKUP(\"apple\",A2:C5,4,FALSE)"), er(ErrorKind::Ref));
    assert_eq!(eval(&mut wb, "=VLOOKUP(\"apple\",A2:C5,0,FALSE)"), er(ErrorKind::Value));
    // approximate match on sorted numbers
    set(&mut wb, "E1", "0");
    set(&mut wb, "E2", "50");
    set(&mut wb, "E3", "70");
    set(&mut wb, "E4", "90");
    set(&mut wb, "F1", "F");
    set(&mut wb, "F2", "C");
    set(&mut wb, "F3", "B");
    set(&mut wb, "F4", "A");
    assert_eq!(eval(&mut wb, "=VLOOKUP(75,E1:F4,2)"), t("B"));
    assert_eq!(eval(&mut wb, "=VLOOKUP(90,E1:F4,2,TRUE)"), t("A"));
    assert_eq!(eval(&mut wb, "=VLOOKUP(-1,E1:F4,2,TRUE)"), er(ErrorKind::NA));
    assert_eq!(eval(&mut wb, "=VLOOKUP(1000,E:F,2,TRUE)"), t("A"));
    assert_eq!(eval(&mut wb, "=HLOOKUP(\"Qty\",A1:C5,3,FALSE)"), n(20.0));
    assert_eq!(eval(&mut wb, "=HLOOKUP(\"Price\",A1:C5,5,FALSE)"), n(3.0));
}

#[test]
fn index_match() {
    let mut wb = table();
    assert_eq!(eval(&mut wb, "=MATCH(\"cherry\",A1:A5,0)"), n(4.0));
    assert_eq!(eval(&mut wb, "=INDEX(C1:C5,MATCH(\"cherry\",A1:A5,0))"), n(4.0));
    assert_eq!(eval(&mut wb, "=INDEX(A1:C5,3,2)"), n(20.0));
    assert_eq!(eval(&mut wb, "=INDEX(A1:C5,6,1)"), er(ErrorKind::Ref));
    assert_eq!(eval(&mut wb, "=SUM(INDEX(A1:C5,0,2))"), n(43.0));
    assert_eq!(eval(&mut wb, "=SUM(INDEX(A1:C5,2,0))"), n(11.5));
    assert_eq!(eval(&mut wb, "=INDEX({1,2;3,4},2,1)"), n(3.0));
    assert_eq!(eval(&mut wb, "=INDEX({1,2,3},3)"), n(3.0));
    assert_eq!(eval(&mut wb, "=INDEX((A1:A5,C1:C5),2,1,2)"), n(1.5));
    assert_eq!(eval(&mut wb, "=MATCH(7,{1,3,5,7,9},1)"), n(4.0));
    assert_eq!(eval(&mut wb, "=MATCH(6,{1,3,5,7,9})"), n(3.0));
    assert_eq!(eval(&mut wb, "=MATCH(6,{9,7,5,3,1},-1)"), n(2.0));
    assert_eq!(eval(&mut wb, "=MATCH(\"b?nana\",A1:A5,0)"), n(3.0));
    assert_eq!(eval(&mut wb, "=MATCH(0,{1,3},1)"), er(ErrorKind::NA));
    // INDEX returns a reference usable in a range
    for i in 1..=10 {
        wb.set_input(0, CellPos::new(i - 1, 7), &i.to_string()).unwrap();
    }
    assert_eq!(eval(&mut wb, "=SUM(H1:INDEX(H1:H10,3))"), n(6.0));
    assert_eq!(eval(&mut wb, "=SUM(INDEX(H1:H10,2):INDEX(H1:H10,4))"), n(9.0));
    assert_eq!(spill(&mut wb, "=INDEX(H1:H10,{1;3;5})", 3, 1), nums(&[&[1.0], &[3.0], &[5.0]]));
}

#[test]
fn lookup_fn() {
    let mut wb = Workbook::new();
    assert_eq!(eval(&mut wb, "=LOOKUP(4,{1,3,5},{\"a\",\"b\",\"c\"})"), t("b"));
    assert_eq!(eval(&mut wb, "=LOOKUP(5,{1,2;3,4;5,6})"), n(6.0));
    assert_eq!(eval(&mut wb, "=LOOKUP(0,{1,3,5})"), er(ErrorKind::NA));
}

#[test]
fn xlookup_xmatch() {
    let mut wb = table();
    assert_eq!(eval(&mut wb, "=XLOOKUP(\"date\",A2:A5,B2:B5)"), n(8.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(\"kiwi\",A2:A5,B2:B5)"), er(ErrorKind::NA));
    assert_eq!(eval(&mut wb, "=XLOOKUP(\"kiwi\",A2:A5,B2:B5,\"none\")"), t("none"));
    // returning a whole row
    assert_eq!(spill(&mut wb, "=XLOOKUP(\"banana\",A2:A5,B2:C5)", 1, 2), nums(&[&[20.0, 0.5]]));
    // horizontal
    assert_eq!(spill(&mut wb, "=XLOOKUP(\"Price\",A1:C1,A2:C5)", 4, 1), nums(&[&[1.5], &[0.5], &[4.0], &[3.0]]));
    // reverse search finds the last duplicate
    set(&mut wb, "E1", "x");
    set(&mut wb, "E2", "y");
    set(&mut wb, "E3", "x");
    set(&mut wb, "F1", "1");
    set(&mut wb, "F2", "2");
    set(&mut wb, "F3", "3");
    assert_eq!(eval(&mut wb, "=XLOOKUP(\"x\",E1:E3,F1:F3)"), n(1.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(\"x\",E1:E3,F1:F3,,0,-1)"), n(3.0));
    // match modes
    assert_eq!(eval(&mut wb, "=XLOOKUP(12,{5,10,15,20},{1,2,3,4},,-1)"), n(2.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(12,{5,10,15,20},{1,2,3,4},,1)"), n(3.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(\"b*\",A2:A5,B2:B5,,2)"), n(20.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(12,{5,10,15,20},{1,2,3,4},,1,2)"), n(3.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(12,{5,10,15,20},{1,2,3,4},,-1,2)"), n(2.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(15,{5,10,15,20},{1,2,3,4},,0,2)"), n(3.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(12,{20,15,10,5},{1,2,3,4},,1,-2)"), n(2.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(12,{20,15,10,5},{1,2,3,4},,-1,-2)"), n(3.0));
    assert_eq!(eval(&mut wb, "=XLOOKUP(1,{1,2},{1,2},,5)"), er(ErrorKind::Value));
    // XLOOKUP returns a reference
    assert_eq!(eval(&mut wb, "=SUM(XLOOKUP(\"apple\",A2:A5,B2:B5):XLOOKUP(\"cherry\",A2:A5,B2:B5))"), n(35.0));
    assert_eq!(eval(&mut wb, "=XMATCH(\"cherry\",A2:A5)"), n(3.0));
    assert_eq!(eval(&mut wb, "=XMATCH(12,{5,10,15,20},1)"), n(3.0));
    assert_eq!(eval(&mut wb, "=XMATCH(\"x\",E1:E3,0,-1)"), n(3.0));
    assert_eq!(spill(&mut wb, "=XMATCH({\"date\";\"apple\"},A2:A5)", 2, 1), nums(&[&[4.0], &[1.0]]));
}

#[test]
fn offset_indirect() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "2");
    set(&mut wb, "A3", "3");
    set(&mut wb, "A4", "4");
    set(&mut wb, "B2", "hello");
    assert_eq!(eval(&mut wb, "=SUM(OFFSET(A1,1,0,2,1))"), n(5.0));
    assert_eq!(eval(&mut wb, "=OFFSET(A1,3,0)"), n(4.0));
    assert_eq!(eval(&mut wb, "=SUM(OFFSET(A4,0,0,-3,1))"), n(9.0));
    assert_eq!(eval(&mut wb, "=OFFSET(A1,-1,0)"), er(ErrorKind::Ref));
    assert_eq!(eval(&mut wb, "=ROWS(OFFSET(A1,0,0,3,2))"), n(3.0));
    assert_eq!(eval(&mut wb, "=INDIRECT(\"B\"&2)"), t("hello"));
    assert_eq!(eval(&mut wb, "=SUM(INDIRECT(\"A1:A3\"))"), n(6.0));
    assert_eq!(eval(&mut wb, "=INDIRECT(\"R2C2\",FALSE)"), t("hello"));
    assert_eq!(eval(&mut wb, "=SUM(INDIRECT(\"R1C1:R4C1\",FALSE))"), n(10.0));
    assert_eq!(eval(&mut wb, "=INDIRECT(\"!!bad\")"), er(ErrorKind::Ref));
    let s2 = wb.add_sheet(None);
    wb.set_input(s2, p("A1"), "42").unwrap();
    assert_eq!(eval(&mut wb, "=INDIRECT(\"Sheet2!A1\")"), n(42.0));
    assert_eq!(eval(&mut wb, "=INDIRECT(\"Sheet2!R1C1\",FALSE)"), n(42.0));
    // relative R1C1 from Z100 (row 100, col 26)
    assert_eq!(eval(&mut wb, "=INDIRECT(\"R[-98]C[-24]\",FALSE)"), t("hello"));
}

#[test]
fn row_column_address() {
    let mut wb = Workbook::new();
    assert_eq!(eval(&mut wb, "=ROW()"), n(100.0));
    assert_eq!(eval(&mut wb, "=COLUMN()"), n(26.0));
    assert_eq!(eval(&mut wb, "=ROW(C5)"), n(5.0));
    assert_eq!(eval(&mut wb, "=COLUMN(D1)"), n(4.0));
    assert_eq!(spill(&mut wb, "=ROW(A1:A3)", 3, 1), nums(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(spill(&mut wb, "=COLUMN(A1:C1)", 1, 3), nums(&[&[1.0, 2.0, 3.0]]));
    assert_eq!(eval(&mut wb, "=ROWS(A1:C5)"), n(5.0));
    assert_eq!(eval(&mut wb, "=COLUMNS(A1:C5)"), n(3.0));
    assert_eq!(eval(&mut wb, "=COLUMNS({1,2,3;4,5,6})"), n(3.0));
    assert_eq!(eval(&mut wb, "=AREAS((A1:B2,C3,D4))"), n(3.0));
    assert_eq!(eval(&mut wb, "=ADDRESS(2,3)"), t("$C$2"));
    assert_eq!(eval(&mut wb, "=ADDRESS(2,3,2)"), t("C$2"));
    assert_eq!(eval(&mut wb, "=ADDRESS(2,3,3)"), t("$C2"));
    assert_eq!(eval(&mut wb, "=ADDRESS(2,3,4)"), t("C2"));
    assert_eq!(eval(&mut wb, "=ADDRESS(2,3,1,FALSE)"), t("R2C3"));
    assert_eq!(eval(&mut wb, "=ADDRESS(2,3,4,FALSE)"), t("R[2]C[3]"));
    assert_eq!(eval(&mut wb, "=ADDRESS(1,1,1,TRUE,\"My Sheet\")"), t("'My Sheet'!$A$1"));
    assert_eq!(eval(&mut wb, "=ADDRESS(1,1,1,TRUE,\"Data\")"), t("Data!$A$1"));
    assert_eq!(eval(&mut wb, "=ADDRESS(0,1)"), er(ErrorKind::Value));
}

#[test]
fn misc_reference() {
    let mut wb = Workbook::new();
    set(&mut wb, "A1", "=1+2");
    assert_eq!(eval(&mut wb, "=FORMULATEXT(A1)"), t("=1+2"));
    assert_eq!(eval(&mut wb, "=FORMULATEXT(B1)"), er(ErrorKind::NA));
    assert_eq!(eval(&mut wb, "=HYPERLINK(\"http://x.com\",\"X\")"), t("X"));
    assert_eq!(eval(&mut wb, "=HYPERLINK(\"http://x.com\")"), t("http://x.com"));
    assert_eq!(eval(&mut wb, "=GETPIVOTDATA(\"Sales\",A1)"), er(ErrorKind::Ref));
    assert_eq!(eval(&mut wb, "=RTD(\"a\",\"b\",\"c\")"), er(ErrorKind::NA));
}

#[test]
fn filter_sort_unique() {
    let mut wb = table();
    assert_eq!(
        spill(&mut wb, "=FILTER(A2:B5,B2:B5>8)", 2, 2),
        vec![vec![t("apple"), n(10.0)], vec![t("banana"), n(20.0)]]
    );
    assert_eq!(eval(&mut wb, "=FILTER(A2:B5,B2:B5>100)"), er(ErrorKind::Calc));
    assert_eq!(eval(&mut wb, "=FILTER(A2:B5,B2:B5>100,\"none\")"), t("none"));
    assert_eq!(eval(&mut wb, "=FILTER(A2:B5,{1;0})"), er(ErrorKind::Value));
    assert_eq!(
        spill(&mut wb, "=SORT(A2:C5,2,-1)", 4, 2),
        vec![vec![t("banana"), n(20.0)], vec![t("apple"), n(10.0)], vec![t("date"), n(8.0)], vec![t("cherry"), n(5.0)]]
    );
    assert_eq!(spill(&mut wb, "=SORT({3;1;2})", 3, 1), nums(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(spill(&mut wb, "=SORT({3,1,2},1,1,TRUE)", 1, 3), nums(&[&[1.0, 2.0, 3.0]]));
    assert_eq!(
        spill(&mut wb, "=SORTBY(A2:A5,C2:C5,1)", 4, 1),
        vec![vec![t("banana")], vec![t("apple")], vec![t("date")], vec![t("cherry")]]
    );
    assert_eq!(spill(&mut wb, "=UNIQUE({1;2;1;3;2})", 3, 1), nums(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(spill(&mut wb, "=UNIQUE({1;2;1;3;2},,TRUE)", 1, 1), nums(&[&[3.0]]));
    assert_eq!(spill(&mut wb, "=UNIQUE({\"a\";\"A\";\"b\"})", 2, 1), vec![vec![t("a")], vec![t("b")]]);
    assert_eq!(spill(&mut wb, "=UNIQUE({1,1,2},TRUE)", 1, 2), nums(&[&[1.0, 2.0]]));
}

#[test]
fn reshaping() {
    let mut wb = Workbook::new();
    assert_eq!(spill(&mut wb, "=TRANSPOSE({1,2,3})", 3, 1), nums(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(spill(&mut wb, "=CHOOSECOLS({1,2,3;4,5,6},3,1)", 2, 2), nums(&[&[3.0, 1.0], &[6.0, 4.0]]));
    assert_eq!(spill(&mut wb, "=CHOOSECOLS({1,2,3;4,5,6},-1)", 2, 1), nums(&[&[3.0], &[6.0]]));
    assert_eq!(eval(&mut wb, "=CHOOSECOLS({1,2,3},4)"), er(ErrorKind::Value));
    assert_eq!(spill(&mut wb, "=CHOOSEROWS({1,2;3,4;5,6},3,1)", 2, 2), nums(&[&[5.0, 6.0], &[1.0, 2.0]]));
    assert_eq!(spill(&mut wb, "=TAKE({1,2;3,4;5,6},2)", 2, 2), nums(&[&[1.0, 2.0], &[3.0, 4.0]]));
    assert_eq!(spill(&mut wb, "=TAKE({1,2;3,4;5,6},-1,1)", 1, 1), nums(&[&[5.0]]));
    assert_eq!(spill(&mut wb, "=TAKE({1,2;3,4;5,6},,-1)", 3, 1), nums(&[&[2.0], &[4.0], &[6.0]]));
    assert_eq!(spill(&mut wb, "=DROP({1,2;3,4;5,6},1)", 2, 2), nums(&[&[3.0, 4.0], &[5.0, 6.0]]));
    assert_eq!(spill(&mut wb, "=DROP({1,2;3,4;5,6},-2,1)", 1, 1), nums(&[&[2.0]]));
    assert_eq!(eval(&mut wb, "=DROP({1,2},,2)"), er(ErrorKind::Calc));
    assert_eq!(
        spill(&mut wb, "=EXPAND({1,2},2,3,0)", 2, 3),
        nums(&[&[1.0, 2.0, 0.0], &[0.0, 0.0, 0.0]])
    );
    assert_eq!(spill(&mut wb, "=EXPAND({1},2)", 2, 1), vec![vec![n(1.0)], vec![er(ErrorKind::NA)]]);
    assert_eq!(
        spill(&mut wb, "=HSTACK({1;2},{3;4;5})", 3, 2),
        vec![vec![n(1.0), n(3.0)], vec![n(2.0), n(4.0)], vec![er(ErrorKind::NA), n(5.0)]]
    );
    assert_eq!(spill(&mut wb, "=VSTACK({1,2},{3,4})", 2, 2), nums(&[&[1.0, 2.0], &[3.0, 4.0]]));
    assert_eq!(spill(&mut wb, "=TOCOL({1,2;3,4})", 4, 1), nums(&[&[1.0], &[2.0], &[3.0], &[4.0]]));
    assert_eq!(spill(&mut wb, "=TOCOL({1,2;3,4},0,TRUE)", 4, 1), nums(&[&[1.0], &[3.0], &[2.0], &[4.0]]));
    assert_eq!(spill(&mut wb, "=TOROW({1,#N/A;3,4},2)", 1, 3), nums(&[&[1.0, 3.0, 4.0]]));
    assert_eq!(
        spill(&mut wb, "=WRAPROWS({1,2,3,4,5},2,0)", 3, 2),
        nums(&[&[1.0, 2.0], &[3.0, 4.0], &[5.0, 0.0]])
    );
    assert_eq!(
        spill(&mut wb, "=WRAPCOLS({1,2,3,4,5},2)", 2, 3),
        vec![vec![n(1.0), n(3.0), n(5.0)], vec![n(2.0), n(4.0), er(ErrorKind::NA)]]
    );
}

#[test]
fn trimrange() {
    let mut wb = Workbook::new();
    set(&mut wb, "B2", "1");
    set(&mut wb, "B3", "2");
    assert_eq!(eval(&mut wb, "=ROWS(TRIMRANGE(A1:C10))"), n(2.0));
    assert_eq!(eval(&mut wb, "=SUM(TRIMRANGE(A:C))"), n(3.0));
}

#[test]
fn dynamic_arrays() {
    let mut wb = Workbook::new();
    assert_eq!(
        spill(&mut wb, "=SEQUENCE(3,2,10,5)", 3, 2),
        nums(&[&[10.0, 15.0], &[20.0, 25.0], &[30.0, 35.0]])
    );
    assert_eq!(spill(&mut wb, "=SEQUENCE(3)", 3, 1), nums(&[&[1.0], &[2.0], &[3.0]]));
    assert_eq!(eval(&mut wb, "=SEQUENCE(0)"), er(ErrorKind::Calc));
    assert_eq!(eval(&mut wb, "=ROWS(RANDARRAY(4,2))"), n(4.0));
    assert_eq!(eval(&mut wb, "=AND(RANDARRAY(5,5,1,6,TRUE)>=1,RANDARRAY(5,5,1,6,TRUE)<=6)"), Value::Bool(true));
    let r = eval(&mut wb, "=RANDARRAY()");
    assert!(matches!(r, Value::Number(x) if (0.0..1.0).contains(&x)));
    assert_eq!(spill(&mut wb, "=MAP({1,2,3},LAMBDA(x,x*x))", 1, 3), nums(&[&[1.0, 4.0, 9.0]]));
    assert_eq!(spill(&mut wb, "=MAP({1,2},{10,20},LAMBDA(a,b,a+b))", 1, 2), nums(&[&[11.0, 22.0]]));
    assert_eq!(eval(&mut wb, "=REDUCE(0,{1,2,3,4},LAMBDA(acc,v,acc+v))"), n(10.0));
    assert_eq!(eval(&mut wb, "=REDUCE(1,{1,2,3,4},LAMBDA(acc,v,acc*v))"), n(24.0));
    assert_eq!(spill(&mut wb, "=SCAN(0,{1,2,3},LAMBDA(a,v,a+v))", 1, 3), nums(&[&[1.0, 3.0, 6.0]]));
    assert_eq!(spill(&mut wb, "=BYROW({1,2;3,4},LAMBDA(r,SUM(r)))", 2, 1), nums(&[&[3.0], &[7.0]]));
    assert_eq!(spill(&mut wb, "=BYCOL({1,2;3,4},LAMBDA(c,SUM(c)))", 1, 2), nums(&[&[4.0, 6.0]]));
    assert_eq!(spill(&mut wb, "=BYROW({1,2;3,4},SUM)", 2, 1), nums(&[&[3.0], &[7.0]]));
    assert_eq!(
        spill(&mut wb, "=MAKEARRAY(2,3,LAMBDA(r,c,r*10+c))", 2, 3),
        nums(&[&[11.0, 12.0, 13.0], &[21.0, 22.0, 23.0]])
    );
    assert_eq!(eval(&mut wb, "=LET(f,LAMBDA(a,b,IF(ISOMITTED(b),a,a+b)),f(1))"), n(1.0));
    assert_eq!(eval(&mut wb, "=LAMBDA(a,b,ISOMITTED(b))(1)"), Value::Bool(true));
    assert_eq!(eval(&mut wb, "=LAMBDA(a,b,ISOMITTED(b))(1,2)"), Value::Bool(false));
    // MAP over a range
    set(&mut wb, "A1", "1");
    set(&mut wb, "A2", "2");
    assert_eq!(spill(&mut wb, "=MAP(A1:A2,LAMBDA(x,x+1))", 2, 1), nums(&[&[2.0], &[3.0]]));
}

#[test]
fn groupby_pivotby() {
    let mut wb = Workbook::new();
    for (i, (r, c, v)) in [("East", "Q1", "10"), ("West", "Q1", "5"), ("East", "Q2", "7"), ("West", "Q2", "1")].iter().enumerate() {
        wb.set_input(0, CellPos::new(i as u32, 0), r).unwrap();
        wb.set_input(0, CellPos::new(i as u32, 1), c).unwrap();
        wb.set_input(0, CellPos::new(i as u32, 2), v).unwrap();
    }
    assert_eq!(
        spill(&mut wb, "=GROUPBY(A1:A4,C1:C4,SUM)", 3, 2),
        vec![vec![t("East"), n(17.0)], vec![t("West"), n(6.0)], vec![t("Total"), n(23.0)]]
    );
    assert_eq!(
        spill(&mut wb, "=PIVOTBY(A1:A4,B1:B4,C1:C4,SUM)", 4, 4),
        vec![
            vec![t(""), t("Q1"), t("Q2"), t("Total")],
            vec![t("East"), n(10.0), n(7.0), n(17.0)],
            vec![t("West"), n(5.0), n(1.0), n(6.0)],
            vec![t("Total"), n(15.0), n(8.0), n(23.0)],
        ]
    );
}

#[test]
fn database_functions() {
    let mut wb = Workbook::new();
    let data = [
        ["Tree", "Height", "Age", "Yield"],
        ["Apple", "18", "20", "14"],
        ["Pear", "12", "12", "10"],
        ["Cherry", "13", "14", "9"],
        ["Apple", "14", "15", "10"],
        ["Pear", "9", "8", "8"],
        ["Apple", "8", "9", "6"],
    ];
    for (i, row) in data.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            wb.set_input(0, CellPos::new(i as u32, j as u32), v).unwrap();
        }
    }
    // Criteria in F1:G3: (Tree=Apple AND Height>10) OR (Tree=Pear)
    set(&mut wb, "F1", "Tree");
    set(&mut wb, "G1", "Height");
    set(&mut wb, "F2", "Apple");
    set(&mut wb, "G2", ">10");
    set(&mut wb, "F3", "Pear");
    assert_eq!(eval(&mut wb, "=DSUM(A1:D7,\"Yield\",F1:G3)"), n(42.0));
    assert_eq!(eval(&mut wb, "=DSUM(A1:D7,4,F1:G2)"), n(24.0));
    assert_eq!(eval(&mut wb, "=DCOUNT(A1:D7,\"Age\",F1:G3)"), n(4.0));
    assert_eq!(eval(&mut wb, "=DCOUNTA(A1:D7,\"Tree\",F1:G3)"), n(4.0));
    assert_eq!(eval(&mut wb, "=DAVERAGE(A1:D7,\"Yield\",F1:G3)"), n(10.5));
    assert_eq!(eval(&mut wb, "=DMAX(A1:D7,\"Height\",F1:G3)"), n(18.0));
    assert_eq!(eval(&mut wb, "=DMIN(A1:D7,\"Height\",F1:G3)"), n(9.0));
    assert_eq!(eval(&mut wb, "=DPRODUCT(A1:D7,\"Yield\",F1:G2)"), n(140.0));
    assert_eq!(eval(&mut wb, "=DGET(A1:D7,\"Yield\",F1:G2)"), er(ErrorKind::Num));
    set(&mut wb, "H1", "Tree");
    set(&mut wb, "H2", "Plum");
    assert_eq!(eval(&mut wb, "=DGET(A1:D7,\"Yield\",H1:H2)"), er(ErrorKind::Value));
    set(&mut wb, "H2", "Cherry");
    assert_eq!(eval(&mut wb, "=DGET(A1:D7,\"Yield\",H1:H2)"), n(9.0));
    assert_eq!(eval(&mut wb, "=DSUM(A1:D7,\"Nope\",H1:H2)"), er(ErrorKind::Value));
    // variance/stdev of Yield for Apple trees: 14, 10, 6
    set(&mut wb, "H2", "Apple");
    assert_eq!(eval(&mut wb, "=DVAR(A1:D7,\"Yield\",H1:H2)"), n(16.0));
    assert_eq!(eval(&mut wb, "=DSTDEV(A1:D7,\"Yield\",H1:H2)"), n(4.0));
    let vp = eval(&mut wb, "=DVARP(A1:D7,\"Yield\",H1:H2)");
    assert!(matches!(vp, Value::Number(x) if (x - 32.0 / 3.0).abs() < 1e-9));
    let sp = eval(&mut wb, "=DSTDEVP(A1:D7,\"Yield\",H1:H2)");
    assert!(matches!(sp, Value::Number(x) if (x - (32.0f64 / 3.0).sqrt()).abs() < 1e-9));
    // text criteria match "begins with"; "=" forces exact
    set(&mut wb, "H2", "App");
    assert_eq!(eval(&mut wb, "=DCOUNTA(A1:D7,\"Tree\",H1:H2)"), n(3.0));
    set(&mut wb, "H2", "'=App");
    assert_eq!(eval(&mut wb, "=DCOUNTA(A1:D7,\"Tree\",H1:H2)"), n(0.0));
}
