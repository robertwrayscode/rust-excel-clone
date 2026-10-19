//! File import and export (xlsx, csv/tsv, and read-only xls/xlsb/ods).

pub mod csv;
mod other;
pub mod xlsx;
mod xlsx_read;
mod xlsx_write;
mod xml;

use std::path::Path;

use crate::workbook::Workbook;

pub use self::csv::{read_csv, write_csv};
pub use self::other::read_other;
pub use self::xlsx::{read_xlsx, write_xlsx};

/// File extensions [`open`] understands.
pub const OPEN_EXTENSIONS: &[&str] = &["xlsx", "xlsm", "xls", "xlsb", "ods", "csv", "tsv", "txt"];
/// File extensions [`save`] can write.
pub const SAVE_EXTENSIONS: &[&str] = &["xlsx", "csv", "tsv", "txt"];

fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// Open a workbook, choosing the format from the file extension, then recalculate it.
pub fn open(path: &Path) -> Result<Workbook, String> {
    let mut wb = match ext_of(path).as_str() {
        "xlsx" | "xlsm" | "xltx" | "xltm" => read_xlsx(path)?,
        "xls" | "xlsb" | "ods" | "xla" | "xlam" => read_other(path)?,
        "csv" => read_csv(path, b',')?,
        "tsv" | "tab" | "txt" => read_csv(path, b'\t')?,
        e => return Err(format!("Unsupported file type: .{}", e)),
    };
    wb.recalculate();
    Ok(wb)
}

/// Save a workbook, choosing the format from the file extension. Delimited
/// formats write only sheet `sheet_idx`.
pub fn save(wb: &Workbook, sheet_idx: usize, path: &Path) -> Result<(), String> {
    match ext_of(path).as_str() {
        "xlsx" => write_xlsx(wb, path),
        "xlsm" => Err("Saving macro-enabled workbooks (.xlsm) is not supported; save as .xlsx instead.".into()),
        "csv" => write_csv(wb, sheet_idx, path, b','),
        "tsv" | "tab" | "txt" => write_csv(wb, sheet_idx, path, b'\t'),
        e => Err(format!("Cannot save as .{}; use .xlsx, .csv or .tsv", e)),
    }
}
