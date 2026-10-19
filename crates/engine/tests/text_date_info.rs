use engine::{CellPos, ErrorKind, Value, Workbook};

fn p(a1: &str) -> CellPos {
    CellPos::parse_a1(a1).unwrap()
}

fn eval(wb: &mut Workbook, f: &str) -> Value {
    wb.set_input(0, p("Z100"), f).unwrap();
    wb.recalculate();
    wb.value(0, p("Z100"))
}

#[track_caller]
fn num(wb: &mut Workbook, f: &str, expected: f64) {
    match eval(wb, f) {
        Value::Number(n) => assert!((n - expected).abs() < 1e-9, "{f}: got {n}, expected {expected}"),
        v => panic!("{f}: got {v:?}, expected {expected}"),
    }
}

#[track_caller]
fn txt(wb: &mut Workbook, f: &str, expected: &str) {
    assert_eq!(eval(wb, f), Value::Text(expected.into()), "{f}");
}

#[track_caller]
fn boolean(wb: &mut Workbook, f: &str, expected: bool) {
    assert_eq!(eval(wb, f), Value::Bool(expected), "{f}");
}

#[track_caller]
fn err(wb: &mut Workbook, f: &str, expected: ErrorKind) {
    assert_eq!(eval(wb, f), Value::Error(expected), "{f}");
}

/// Evaluate a spilling formula at H1 and read a rows x cols block.
fn spill(wb: &mut Workbook, f: &str, rows: u32, cols: u32) -> Vec<Vec<Value>> {
    wb.set_input(0, p("H1"), f).unwrap();
    wb.recalculate();
    let h = p("H1");
    (0..rows).map(|r| (0..cols).map(|c| wb.value(0, CellPos::new(h.row + r, h.col + c))).collect()).collect()
}

fn t(s: &str) -> Value {
    Value::Text(s.into())
}

fn setup() -> Workbook {
    let mut wb = Workbook::new();
    wb.set_input(0, p("A1"), "hello").unwrap();
    wb.set_input(0, p("A2"), "").unwrap();
    wb.set_input(0, p("A3"), "world").unwrap();
    wb.set_input(0, p("B1"), "1").unwrap();
    wb.set_input(0, p("B2"), "2").unwrap();
    wb.set_input(0, p("C1"), "=B1+B2").unwrap();
    wb.set_input(0, p("D1"), "2024-01-01").unwrap(); // a holiday
    wb.set_input(0, p("D2"), "2024-01-15").unwrap();
    wb
}

#[test]
fn text_basics() {
    let mut wb = setup();
    txt(&mut wb, "=TRIM(\"  a   b \")", "a b");
    txt(&mut wb, "=PROPER(\"hello wORLD\")", "Hello World");
    txt(&mut wb, "=PROPER(\"2-way it's\")", "2-Way It'S");
    num(&mut wb, "=FIND(\"b\",\"abcb\",3)", 4.0);
    num(&mut wb, "=FIND(\"\",\"abc\",2)", 2.0);
    err(&mut wb, "=FIND(\"B\",\"abc\")", ErrorKind::Value);
    err(&mut wb, "=FIND(\"a\",\"abc\",5)", ErrorKind::Value);
    num(&mut wb, "=FINDB(\"c\",\"abc\")", 3.0);
    txt(&mut wb, "=SUBSTITUTE(\"aaa\",\"a\",\"b\",2)", "aba");
    txt(&mut wb, "=SUBSTITUTE(\"aaa\",\"a\",\"bb\")", "bbbbbb");
    txt(&mut wb, "=SUBSTITUTE(\"aaa\",\"x\",\"b\")", "aaa");
    err(&mut wb, "=SUBSTITUTE(\"aaa\",\"a\",\"b\",0)", ErrorKind::Value);
    txt(&mut wb, "=LEFT(\"abc\")", "a");
    txt(&mut wb, "=LEFT(\"abc\",5)", "abc");
    txt(&mut wb, "=LEFTB(\"abc\",2)", "ab");
    err(&mut wb, "=LEFT(\"abc\",-1)", ErrorKind::Value);
    txt(&mut wb, "=RIGHT(\"abc\",2)", "bc");
    txt(&mut wb, "=RIGHTB(\"abc\")", "c");
    txt(&mut wb, "=MID(\"abcdef\",3,2)", "cd");
    txt(&mut wb, "=MIDB(\"abcdef\",5,10)", "ef");
    txt(&mut wb, "=MID(\"abc\",10,2)", "");
    err(&mut wb, "=MID(\"abc\",0,1)", ErrorKind::Value);
    num(&mut wb, "=LEN(\"héllo\")", 5.0);
    num(&mut wb, "=LENB(123.5)", 5.0);
    num(&mut wb, "=LEN(A2)", 0.0);
    txt(&mut wb, "=LOWER(\"AbC\")", "abc");
    txt(&mut wb, "=UPPER(\"AbC\")", "ABC");
    boolean(&mut wb, "=EXACT(\"a\",\"A\")", false);
    boolean(&mut wb, "=EXACT(\"a\",\"a\")", true);
    txt(&mut wb, "=REPLACE(\"abcdef\",2,3,\"X\")", "aXef");
    txt(&mut wb, "=REPLACEB(\"abc\",4,0,\"d\")", "abcd");
    err(&mut wb, "=REPLACE(\"abc\",0,1,\"x\")", ErrorKind::Value);
    txt(&mut wb, "=REPT(\"ab\",3)", "ababab");
    err(&mut wb, "=REPT(\"ab\",-1)", ErrorKind::Value);
    err(&mut wb, "=REPT(\"ab\",20000)", ErrorKind::Value);
    txt(&mut wb, "=T(\"x\")", "x");
    txt(&mut wb, "=T(1)", "");
    err(&mut wb, "=T(NA())", ErrorKind::NA);
    txt(&mut wb, "=CHAR(65)", "A");
    txt(&mut wb, "=CHAR(128)", "€");
    err(&mut wb, "=CHAR(0)", ErrorKind::Value);
    num(&mut wb, "=CODE(\"Abc\")", 65.0);
    num(&mut wb, "=CODE(\"€\")", 128.0);
    err(&mut wb, "=CODE(\"\")", ErrorKind::Value);
    txt(&mut wb, "=CLEAN(CHAR(7)&\"a\"&CHAR(10))", "a");
    txt(&mut wb, "=UNICHAR(9731)", "☃");
    err(&mut wb, "=UNICHAR(0)", ErrorKind::Value);
    num(&mut wb, "=UNICODE(\"☃x\")", 9731.0);
    txt(&mut wb, "=CONCATENATE(\"a\",1,TRUE)", "a1TRUE");
    err(&mut wb, "=CONCATENATE(\"a\",1/0)", ErrorKind::Div0);
    txt(&mut wb, "=ASC(\"ＡＢＣ　１\")", "ABC 1");
    txt(&mut wb, "=DBCS(\"AB 1\")", "ＡＢ　１");
    // array lifting of scalar functions
    let r = spill(&mut wb, "=LEN(A1:A3)", 3, 1);
    assert_eq!(r, vec![vec![Value::Number(5.0)], vec![Value::Number(0.0)], vec![Value::Number(5.0)]]);
}

#[test]
fn text_numbers() {
    let mut wb = setup();
    txt(&mut wb, "=DOLLAR(1234.567)", "$1,234.57");
    txt(&mut wb, "=DOLLAR(-1234.567,1)", "($1,234.6)");
    txt(&mut wb, "=DOLLAR(1234.567,-2)", "$1,200");
    txt(&mut wb, "=FIXED(1234.567,1)", "1,234.6");
    txt(&mut wb, "=FIXED(1234.567)", "1,234.57");
    txt(&mut wb, "=FIXED(1234.567,-2,TRUE)", "1200");
    txt(&mut wb, "=FIXED(-0.5,0)", "-1");
    txt(&mut wb, "=FIXED(1000000,0,FALSE)", "1,000,000");
    num(&mut wb, "=NUMBERVALUE(\"2.500,27\",\",\",\".\")", 2500.27);
    num(&mut wb, "=NUMBERVALUE(\"3.5%\")", 0.035);
    num(&mut wb, "=NUMBERVALUE(\" 1 000 \")", 1000.0);
    num(&mut wb, "=NUMBERVALUE(\"\")", 0.0);
    err(&mut wb, "=NUMBERVALUE(\"1.2.3\")", ErrorKind::Value);
    num(&mut wb, "=VALUE(\"$1,000\")", 1000.0);
    num(&mut wb, "=VALUE(\"50%\")", 0.5);
    num(&mut wb, "=VALUE(\"12:00\")", 0.5);
    err(&mut wb, "=VALUE(\"abc\")", ErrorKind::Value);
    txt(&mut wb, "=TEXT(1234,\"General\")", "1234");
    txt(&mut wb, "=TEXT(\"abc\",\"@\")", "abc");
    err(&mut wb, "=TEXT(1/0,\"0\")", ErrorKind::Div0);
    txt(&mut wb, "=VALUETOTEXT(\"a\",1)", "\"a\"");
    txt(&mut wb, "=VALUETOTEXT(\"a\")", "a");
    txt(&mut wb, "=VALUETOTEXT(TRUE)", "TRUE");
    txt(&mut wb, "=VALUETOTEXT(NA())", "#N/A");
    txt(&mut wb, "=ARRAYTOTEXT({1,\"a\";TRUE,2},1)", "{1,\"a\";TRUE,2}");
    txt(&mut wb, "=ARRAYTOTEXT({1,\"a\";TRUE,2})", "1, a, TRUE, 2");
    txt(&mut wb, "=ARRAYTOTEXT(A1:A3)", "hello, , world");
    err(&mut wb, "=ARRAYTOTEXT(A1:A3,2)", ErrorKind::Value);
    txt(&mut wb, "=BAHTTEXT(1234)", "หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน");
    txt(&mut wb, "=BAHTTEXT(21.5)", "ยี่สิบเอ็ดบาทห้าสิบสตางค์");
    txt(&mut wb, "=BAHTTEXT(0)", "ศูนย์บาทถ้วน");
    txt(&mut wb, "=BAHTTEXT(1000000)", "หนึ่งล้านบาทถ้วน");
}

#[test]
fn search_wildcards() {
    let mut wb = setup();
    num(&mut wb, "=SEARCH(\"B\",\"abcb\",3)", 4.0);
    num(&mut wb, "=SEARCH(\"b*d\",\"abcd\")", 2.0);
    num(&mut wb, "=SEARCH(\"?c\",\"abcd\")", 2.0);
    num(&mut wb, "=SEARCH(\"~*\",\"a*b\")", 2.0);
    num(&mut wb, "=SEARCH(\"c*\",\"abcd\")", 3.0);
    num(&mut wb, "=SEARCHB(\"D\",\"abcd\")", 4.0);
    err(&mut wb, "=SEARCH(\"x\",\"abc\")", ErrorKind::Value);
    err(&mut wb, "=SEARCH(\"a\",\"abc\",0)", ErrorKind::Value);
}

#[test]
fn concat_join_split() {
    let mut wb = setup();
    txt(&mut wb, "=CONCAT(A1:A3)", "helloworld");
    txt(&mut wb, "=CONCAT(A1:B2,\"!\")", "hello12!");
    txt(&mut wb, "=CONCAT({\"a\",\"b\";\"c\",\"d\"})", "abcd");
    txt(&mut wb, "=TEXTJOIN(\", \",TRUE,A1:A3)", "hello, world");
    txt(&mut wb, "=TEXTJOIN(\"-\",FALSE,A1:A3)", "hello--world");
    txt(&mut wb, "=TEXTJOIN({\"+\",\"=\"},TRUE,1,2,3)", "1+2=3");
    txt(&mut wb, "=TEXTJOIN(\"\",TRUE,\"a\",\"\",\"b\")", "ab");
    err(&mut wb, "=TEXTJOIN(\",\",TRUE,\"a\",NA())", ErrorKind::NA);

    let r = spill(&mut wb, "=TEXTSPLIT(\"a,b;c,d\",\",\",\";\")", 2, 2);
    assert_eq!(r, vec![vec![t("a"), t("b")], vec![t("c"), t("d")]]);
    let r = spill(&mut wb, "=TEXTSPLIT(\"a,b;c\",\",\",\";\")", 2, 2);
    assert_eq!(r, vec![vec![t("a"), t("b")], vec![t("c"), Value::Error(ErrorKind::NA)]]);
    let r = spill(&mut wb, "=TEXTSPLIT(\"a,b;c\",\",\",\";\",FALSE,0,\"-\")", 2, 2);
    assert_eq!(r, vec![vec![t("a"), t("b")], vec![t("c"), t("-")]]);
    let r = spill(&mut wb, "=TEXTSPLIT(\"a,,b\",\",\",,TRUE)", 1, 3);
    assert_eq!(r, vec![vec![t("a"), t("b"), Value::Empty]]);
    let r = spill(&mut wb, "=TEXTSPLIT(\"a b-c\",{\" \",\"-\"})", 1, 3);
    assert_eq!(r, vec![vec![t("a"), t("b"), t("c")]]);
    let r = spill(&mut wb, "=TEXTSPLIT(\"aXbxc\",\"x\",,,1)", 1, 3);
    assert_eq!(r, vec![vec![t("a"), t("b"), t("c")]]);
    let r = spill(&mut wb, "=TEXTSPLIT(\"a;b\",,\";\")", 2, 1);
    assert_eq!(r, vec![vec![t("a")], vec![t("b")]]);
}

#[test]
fn text_before_after() {
    let mut wb = setup();
    txt(&mut wb, "=TEXTAFTER(\"a-b-c\",\"-\")", "b-c");
    txt(&mut wb, "=TEXTAFTER(\"a-b-c\",\"-\",-1)", "c");
    txt(&mut wb, "=TEXTAFTER(\"a-b-c\",\"-\",2)", "c");
    txt(&mut wb, "=TEXTBEFORE(\"a-b-c\",\"-\")", "a");
    txt(&mut wb, "=TEXTBEFORE(\"a-b-c\",\"-\",2)", "a-b");
    txt(&mut wb, "=TEXTBEFORE(\"a-b-c\",\"-\",-1)", "a-b");
    err(&mut wb, "=TEXTBEFORE(\"abc\",\"x\")", ErrorKind::NA);
    txt(&mut wb, "=TEXTBEFORE(\"abc\",\"x\",,,,\"none\")", "none");
    err(&mut wb, "=TEXTBEFORE(\"abc\",\"b\",0)", ErrorKind::Value);
    txt(&mut wb, "=TEXTAFTER(\"aXb\",\"x\",1,1)", "b");
    err(&mut wb, "=TEXTAFTER(\"aXb\",\"x\")", ErrorKind::NA);
    txt(&mut wb, "=TEXTBEFORE(\"a-b\",\"-\",2,0,1)", "a-b");
    txt(&mut wb, "=TEXTAFTER(\"a-b\",\"-\",-2,0,1)", "a-b");
    txt(&mut wb, "=TEXTAFTER(\"a b-c\",{\"-\",\" \"})", "b-c");
    txt(&mut wb, "=TEXTAFTER(\"abc\",\"\")", "abc");
    txt(&mut wb, "=TEXTBEFORE(\"abc\",\"\")", "");
}

#[test]
fn regex() {
    let mut wb = setup();
    boolean(&mut wb, "=REGEXTEST(\"abc123\",\"\\d+\")", true);
    boolean(&mut wb, "=REGEXTEST(\"abc\",\"^\\d\")", false);
    boolean(&mut wb, "=REGEXTEST(\"ABC\",\"^abc$\")", false);
    boolean(&mut wb, "=REGEXTEST(\"ABC\",\"^abc$\",1)", true);
    boolean(&mut wb, "=REGEXTEST(\"hot dog\",\"cat|dog\")", true);
    boolean(&mut wb, "=REGEXTEST(\"aaab\",\"^(a*)*b$\")", true);
    boolean(&mut wb, "=REGEXTEST(\"x-9\",\"[^a-z][0-9]\")", true);
    boolean(&mut wb, "=REGEXTEST(\"ab\",\"^a{2,3}\")", false);
    boolean(&mut wb, "=REGEXTEST(\"aab\",\"^a{2,3}b\")", true);
    boolean(&mut wb, "=REGEXTEST(\"foo bar\",\"\\bbar\\b\")", true);
    err(&mut wb, "=REGEXTEST(\"a\",\"(\")", ErrorKind::Value);
    txt(&mut wb, "=REGEXEXTRACT(\"abc123def\",\"[0-9]+\")", "123");
    txt(&mut wb, "=REGEXEXTRACT(\"<a><b>\",\"<.+?>\")", "<a>");
    txt(&mut wb, "=REGEXEXTRACT(\"<a><b>\",\"<.+>\")", "<a><b>");
    err(&mut wb, "=REGEXEXTRACT(\"x\",\"y\")", ErrorKind::NA);
    let r = spill(&mut wb, "=REGEXEXTRACT(\"a1b22c333\",\"\\d+\",1)", 3, 1);
    assert_eq!(r, vec![vec![t("1")], vec![t("22")], vec![t("333")]]);
    let r = spill(&mut wb, "=REGEXEXTRACT(\"John Smith\",\"(\\w+) (\\w+)\",2)", 1, 2);
    assert_eq!(r, vec![vec![t("John"), t("Smith")]]);
    txt(&mut wb, "=REGEXREPLACE(\"a1b22c\",\"\\d+\",\"#\")", "a#b#c");
    txt(&mut wb, "=REGEXREPLACE(\"a1b22c\",\"\\d+\",\"#\",2)", "a1b#c");
    txt(&mut wb, "=REGEXREPLACE(\"a1b22c3\",\"\\d+\",\"#\",-1)", "a1b22c#");
    txt(&mut wb, "=REGEXREPLACE(\"John Smith\",\"(\\w+) (\\w+)\",\"$2, $1\")", "Smith, John");
    txt(&mut wb, "=REGEXREPLACE(\"ABC\",\"b\",\"x\",0,1)", "AxC");
}

#[test]
fn dates() {
    let mut wb = setup();
    num(&mut wb, "=DATE(2024,1,1)", 45292.0);
    num(&mut wb, "=DATE(2024,14,1)", 45689.0);
    num(&mut wb, "=DATE(2024,3,0)", 45351.0);
    num(&mut wb, "=DATE(124,1,1)", 45292.0);
    err(&mut wb, "=DATE(-1,1,1)", ErrorKind::Num);
    err(&mut wb, "=DATE(10000,1,1)", ErrorKind::Num);
    num(&mut wb, "=YEAR(45292)", 2024.0);
    num(&mut wb, "=MONTH(\"2024-03-05\")", 3.0);
    num(&mut wb, "=DAY(DATE(2024,3,5))", 5.0);
    err(&mut wb, "=YEAR(-1)", ErrorKind::Num);
    num(&mut wb, "=DATEVALUE(\"2024-01-01\")", 45292.0);
    num(&mut wb, "=DATEVALUE(\"1/1/2024 10:00\")", 45292.0);
    err(&mut wb, "=DATEVALUE(5)", ErrorKind::Value);
    err(&mut wb, "=DATEVALUE(\"nope\")", ErrorKind::Value);
    num(&mut wb, "=TIMEVALUE(\"6:00 PM\")", 0.75);
    num(&mut wb, "=TIMEVALUE(\"2024-01-01 12:00\")", 0.5);
    num(&mut wb, "=TIME(25,0,0)", 1.0 / 24.0);
    num(&mut wb, "=TIME(1,2,3)", (3600.0 + 120.0 + 3.0) / 86400.0);
    err(&mut wb, "=TIME(0,0,-1)", ErrorKind::Num);
    num(&mut wb, "=HOUR(0.75)", 18.0);
    num(&mut wb, "=MINUTE(\"12:45\")", 45.0);
    num(&mut wb, "=SECOND(TIME(1,2,3))", 3.0);
    num(&mut wb, "=DAYS(DATE(2024,3,1),DATE(2024,2,1))", 29.0);
    num(&mut wb, "=DAYS(\"2024-03-01\",\"2024-03-11\")", -10.0);
    num(&mut wb, "=EDATE(DATE(2024,1,31),1)", 45351.0);
    num(&mut wb, "=EDATE(DATE(2024,3,31),-1)", 45351.0);
    num(&mut wb, "=EOMONTH(DATE(2024,1,15),1)", 45351.0);
    num(&mut wb, "=EOMONTH(DATE(2024,1,15),-1)", 45291.0);
    boolean(&mut wb, "=TODAY()<=NOW()", true);
    boolean(&mut wb, "=NOW()-TODAY()<1", true);
}

#[test]
fn datedif_and_days360() {
    let mut wb = setup();
    num(&mut wb, "=DATEDIF(DATE(2020,1,31),DATE(2020,3,1),\"MD\")", -1.0);
    num(&mut wb, "=DATEDIF(DATE(2020,1,15),DATE(2023,1,14),\"Y\")", 2.0);
    num(&mut wb, "=DATEDIF(DATE(2020,1,15),DATE(2023,1,14),\"M\")", 35.0);
    num(&mut wb, "=DATEDIF(DATE(2020,1,15),DATE(2023,1,14),\"YM\")", 11.0);
    num(&mut wb, "=DATEDIF(DATE(2020,1,15),DATE(2023,1,14),\"YD\")", 364.0);
    num(&mut wb, "=DATEDIF(DATE(2020,1,15),DATE(2023,1,14),\"md\")", 30.0);
    num(&mut wb, "=DATEDIF(DATE(2020,1,1),DATE(2020,12,31),\"D\")", 365.0);
    err(&mut wb, "=DATEDIF(DATE(2021,1,1),DATE(2020,1,1),\"D\")", ErrorKind::Num);
    err(&mut wb, "=DATEDIF(DATE(2020,1,1),DATE(2021,1,1),\"X\")", ErrorKind::Num);
    num(&mut wb, "=DAYS360(DATE(2024,1,31),DATE(2024,3,31))", 60.0);
    num(&mut wb, "=DAYS360(DATE(2024,2,29),DATE(2024,3,31))", 30.0);
    num(&mut wb, "=DAYS360(DATE(2024,1,15),DATE(2024,3,31))", 76.0);
    num(&mut wb, "=DAYS360(DATE(2024,1,15),DATE(2024,3,31),TRUE)", 75.0);
    num(&mut wb, "=DAYS360(DATE(2024,3,31),DATE(2024,1,31))", -60.0);
}

#[test]
fn yearfrac() {
    let mut wb = setup();
    num(&mut wb, "=YEARFRAC(DATE(2024,1,1),DATE(2024,12,31),1)", 365.0 / 366.0);
    num(&mut wb, "=YEARFRAC(DATE(2023,1,1),DATE(2024,1,1))", 1.0);
    num(&mut wb, "=YEARFRAC(DATE(2024,1,31),DATE(2024,2,29),0)", 29.0 / 360.0);
    num(&mut wb, "=YEARFRAC(DATE(2024,2,29),DATE(2025,2,28),0)", 1.0);
    num(&mut wb, "=YEARFRAC(DATE(2020,1,1),DATE(2022,7,1),1)", 912.0 / (1096.0 / 3.0));
    num(&mut wb, "=YEARFRAC(DATE(2023,3,1),DATE(2024,2,29),1)", 365.0 / 366.0);
    num(&mut wb, "=YEARFRAC(DATE(2023,1,1),DATE(2023,7,1),1)", 181.0 / 365.0);
    num(&mut wb, "=YEARFRAC(DATE(2024,1,1),DATE(2024,7,1),2)", 182.0 / 360.0);
    num(&mut wb, "=YEARFRAC(DATE(2024,7,1),DATE(2024,1,1),3)", 182.0 / 365.0);
    num(&mut wb, "=YEARFRAC(DATE(2024,1,31),DATE(2024,3,31),4)", 60.0 / 360.0);
    err(&mut wb, "=YEARFRAC(DATE(2024,1,1),DATE(2024,3,31),5)", ErrorKind::Num);
}

#[test]
fn weekdays() {
    let mut wb = setup();
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1))", 2.0);
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1),2)", 1.0);
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1),3)", 0.0);
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1),11)", 1.0);
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1),12)", 7.0);
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1),16)", 3.0);
    num(&mut wb, "=WEEKDAY(DATE(2024,1,1),17)", 2.0);
    err(&mut wb, "=WEEKDAY(DATE(2024,1,1),4)", ErrorKind::Num);
    num(&mut wb, "=WEEKNUM(DATE(2024,1,1))", 1.0);
    num(&mut wb, "=WEEKNUM(DATE(2024,1,7))", 2.0);
    num(&mut wb, "=WEEKNUM(DATE(2024,1,7),2)", 1.0);
    num(&mut wb, "=WEEKNUM(DATE(2024,1,7),11)", 1.0);
    num(&mut wb, "=WEEKNUM(DATE(2024,12,31),1)", 53.0);
    num(&mut wb, "=WEEKNUM(DATE(2021,1,1),21)", 53.0);
    err(&mut wb, "=WEEKNUM(DATE(2021,1,1),3)", ErrorKind::Num);
    num(&mut wb, "=ISOWEEKNUM(DATE(2021,1,1))", 53.0);
    num(&mut wb, "=ISOWEEKNUM(DATE(2024,12,30))", 1.0);
    num(&mut wb, "=ISOWEEKNUM(DATE(2024,6,15))", 24.0);
}

#[test]
fn workdays() {
    let mut wb = setup();
    num(&mut wb, "=WORKDAY(DATE(2024,1,5),1)", 45299.0);
    num(&mut wb, "=WORKDAY(DATE(2024,1,5),0)", 45296.0);
    num(&mut wb, "=WORKDAY(DATE(2024,1,8),-1)", 45296.0);
    num(&mut wb, "=WORKDAY(DATE(2024,1,5),1,DATE(2024,1,8))", 45300.0);
    num(&mut wb, "=WORKDAY(DATE(2023,12,29),1,D1:D2)", 45293.0);
    num(&mut wb, "=WORKDAY(DATE(2024,1,1),10)", 45306.0);
    num(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,7)", 45298.0);
    num(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,\"0000011\")", 45299.0);
    num(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,11)", 45297.0);
    num(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,1,{45299})", 45300.0);
    err(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,\"1111111\")", ErrorKind::Value);
    err(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,\"0011\")", ErrorKind::Value);
    err(&mut wb, "=WORKDAY.INTL(DATE(2024,1,5),1,8)", ErrorKind::Num);
    num(&mut wb, "=NETWORKDAYS(DATE(2024,1,1),DATE(2024,1,31))", 23.0);
    num(&mut wb, "=NETWORKDAYS(DATE(2024,1,1),DATE(2024,1,31),D1:D2)", 21.0);
    num(&mut wb, "=NETWORKDAYS(DATE(2024,1,31),DATE(2024,1,1))", -23.0);
    num(&mut wb, "=NETWORKDAYS(DATE(2024,1,6),DATE(2024,1,7))", 0.0);
    num(&mut wb, "=NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,31),11)", 27.0);
    num(&mut wb, "=NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,31),\"0000000\")", 31.0);
    num(&mut wb, "=NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,31),1,D1)", 22.0);
}

#[test]
fn information() {
    let mut wb = setup();
    txt(&mut wb, "=CELL(\"address\",B3)", "$B$3");
    num(&mut wb, "=CELL(\"row\",B3)", 3.0);
    num(&mut wb, "=CELL(\"col\",B3:C9)", 2.0);
    txt(&mut wb, "=CELL(\"contents\",A1)", "hello");
    num(&mut wb, "=CELL(\"contents\",C1)", 3.0);
    txt(&mut wb, "=CELL(\"type\",Z50)", "b");
    txt(&mut wb, "=CELL(\"type\",A1)", "l");
    txt(&mut wb, "=CELL(\"type\",B1)", "v");
    num(&mut wb, "=CELL(\"width\",A1)", 8.0);
    txt(&mut wb, "=CELL(\"format\",A1)", "G");
    txt(&mut wb, "=CELL(\"format\",D1)", "D4");
    txt(&mut wb, "=CELL(\"filename\")", "");
    txt(&mut wb, "=CELL(\"sheet\")", "Sheet1");
    num(&mut wb, "=CELL(\"row\")", 100.0);
    err(&mut wb, "=CELL(\"bogus\",A1)", ErrorKind::Value);
    num(&mut wb, "=ERROR.TYPE(1/0)", 2.0);
    num(&mut wb, "=ERROR.TYPE(NA())", 7.0);
    err(&mut wb, "=ERROR.TYPE(1)", ErrorKind::NA);
    num(&mut wb, "=INFO(\"numfile\")", 1.0);
    txt(&mut wb, "=INFO(\"recalc\")", "Automatic");
    txt(&mut wb, "=INFO(\"release\")", "16.0");
    err(&mut wb, "=INFO(\"bogus\")", ErrorKind::Value);
    boolean(&mut wb, "=ISBLANK(Z50)", true);
    boolean(&mut wb, "=ISBLANK(A1)", false);
    boolean(&mut wb, "=ISBLANK(\"\")", false);
    boolean(&mut wb, "=ISERR(NA())", false);
    boolean(&mut wb, "=ISERR(1/0)", true);
    boolean(&mut wb, "=ISERROR(NA())", true);
    boolean(&mut wb, "=ISERROR(1)", false);
    boolean(&mut wb, "=ISEVEN(2.5)", true);
    boolean(&mut wb, "=ISEVEN(-3)", false);
    boolean(&mut wb, "=ISODD(3)", true);
    boolean(&mut wb, "=ISODD(Z50)", false);
    err(&mut wb, "=ISEVEN(\"x\")", ErrorKind::Value);
    boolean(&mut wb, "=ISFORMULA(C1)", true);
    boolean(&mut wb, "=ISFORMULA(B1)", false);
    boolean(&mut wb, "=ISLOGICAL(TRUE)", true);
    boolean(&mut wb, "=ISLOGICAL(1)", false);
    boolean(&mut wb, "=ISNA(NA())", true);
    boolean(&mut wb, "=ISNA(1/0)", false);
    boolean(&mut wb, "=ISNONTEXT(1)", true);
    boolean(&mut wb, "=ISNONTEXT(\"a\")", false);
    boolean(&mut wb, "=ISNONTEXT(Z50)", true);
    boolean(&mut wb, "=ISNUMBER(\"1\")", false);
    boolean(&mut wb, "=ISNUMBER(B1)", true);
    boolean(&mut wb, "=ISNUMBER(1/0)", false);
    boolean(&mut wb, "=ISREF(A1)", true);
    boolean(&mut wb, "=ISREF(A1:B2)", true);
    boolean(&mut wb, "=ISREF(1)", false);
    boolean(&mut wb, "=ISTEXT(\"a\")", true);
    boolean(&mut wb, "=ISTEXT(A1)", true);
    boolean(&mut wb, "=ISTEXT(1/0)", false);
    num(&mut wb, "=N(TRUE)", 1.0);
    num(&mut wb, "=N(\"a\")", 0.0);
    num(&mut wb, "=N(5)", 5.0);
    err(&mut wb, "=N(1/0)", ErrorKind::Div0);
    err(&mut wb, "=NA()", ErrorKind::NA);
    num(&mut wb, "=TYPE(1)", 1.0);
    num(&mut wb, "=TYPE(\"a\")", 2.0);
    num(&mut wb, "=TYPE(TRUE)", 4.0);
    num(&mut wb, "=TYPE(NA())", 16.0);
    num(&mut wb, "=TYPE({1,2})", 64.0);
    num(&mut wb, "=TYPE(A1:A2)", 64.0);
    num(&mut wb, "=TYPE(A1)", 2.0);
    num(&mut wb, "=SHEET()", 1.0);
    num(&mut wb, "=SHEETS()", 1.0);
    let s2 = wb.add_sheet(Some("Sheet2"));
    wb.set_input(s2, p("A1"), "x").unwrap();
    num(&mut wb, "=SHEET(\"Sheet2\")", 2.0);
    num(&mut wb, "=SHEET(Sheet2!A1)", 2.0);
    err(&mut wb, "=SHEET(\"Nope\")", ErrorKind::NA);
    num(&mut wb, "=SHEETS()", 2.0);
    num(&mut wb, "=SHEETS(A1)", 1.0);
    txt(&mut wb, "=CELL(\"address\",Sheet2!A1)", "Sheet2!$A$1");
    txt(&mut wb, "=CELL(\"sheet\",Sheet2!A1)", "Sheet2");
    num(&mut wb, "=INFO(\"numfile\")", 2.0);
}

#[test]
fn registry_metadata() {
    for name in [
        "ARRAYTOTEXT", "ASC", "BAHTTEXT", "CHAR", "CLEAN", "CODE", "CONCAT", "CONCATENATE", "DBCS", "DOLLAR", "EXACT",
        "FIND", "FINDB", "FIXED", "LEFT", "LEFTB", "LEN", "LENB", "LOWER", "MID", "MIDB", "NUMBERVALUE", "PROPER",
        "REGEXEXTRACT", "REGEXREPLACE", "REGEXTEST", "REPLACE", "REPLACEB", "REPT", "RIGHT", "RIGHTB", "SEARCH",
        "SEARCHB", "SUBSTITUTE", "T", "TEXT", "TEXTAFTER", "TEXTBEFORE", "TEXTJOIN", "TEXTSPLIT", "TRIM", "UNICHAR",
        "UNICODE", "UPPER", "VALUE", "VALUETOTEXT", "DATE", "DATEDIF", "DATEVALUE", "DAY", "DAYS", "DAYS360", "EDATE",
        "EOMONTH", "HOUR", "ISOWEEKNUM", "MINUTE", "MONTH", "NETWORKDAYS", "NETWORKDAYS.INTL", "NOW", "SECOND", "TIME",
        "TIMEVALUE", "TODAY", "WEEKDAY", "WEEKNUM", "WORKDAY", "WORKDAY.INTL", "YEAR", "YEARFRAC", "CELL",
        "ERROR.TYPE", "INFO", "ISBLANK", "ISERR", "ISERROR", "ISEVEN", "ISFORMULA", "ISLOGICAL", "ISNA", "ISNONTEXT",
        "ISNUMBER", "ISODD", "ISREF", "ISTEXT", "N", "NA", "SHEET", "SHEETS", "TYPE",
    ] {
        let def = engine::functions::lookup(name).unwrap_or_else(|| panic!("{name} not registered"));
        assert!(def.syntax.starts_with(&format!("{name}(")), "{name} syntax");
        assert!(!def.description.is_empty());
    }
}
