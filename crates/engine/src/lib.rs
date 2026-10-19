//! Spreadsheet engine: workbook model, formula parser and evaluator, Excel
//! function library, number formatting and file import/export.

pub mod address;
pub mod ast;
pub mod condfmt;
pub mod criteria;
pub mod datetime;
pub mod eval;
pub mod functions;
pub mod io;
pub mod numfmt;
pub mod ops;
pub mod parser;
pub mod style;
pub mod value;
pub mod workbook;

pub use address::{CellPos, Rect};
pub use value::{Array, ErrorKind, Value};
pub use workbook::{Cell, Sheet, Workbook};
