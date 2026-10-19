//! File open/save wrappers around the engine's I/O module, native dialogs and
//! the recent-files list.

use std::path::{Path, PathBuf};

use engine::Workbook;

pub const OPEN_EXTENSIONS: [&str; 6] = ["xlsx", "xlsm", "xls", "ods", "csv", "tsv"];

pub fn open(path: &Path) -> Result<Workbook, String> {
    let mut wb = engine::io::open(path)?;
    if wb.sheets.is_empty() {
        wb.add_sheet(None);
    }
    wb.recalculate();
    Ok(wb)
}

pub fn save(wb: &Workbook, sheet: usize, path: &Path) -> Result<(), String> {
    engine::io::save(wb, sheet, path)
}

/// True when saving to this path keeps only the active sheet (text formats).
pub fn is_single_sheet_format(path: &Path) -> bool {
    matches!(ext(path).as_str(), "csv" | "tsv" | "txt")
}

pub fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn pick_open() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open")
        .add_filter("All Excel Files", &OPEN_EXTENSIONS)
        .add_filter("Excel Workbook", &["xlsx", "xlsm"])
        .add_filter("Excel 97-2003 Workbook", &["xls"])
        .add_filter("OpenDocument Spreadsheet", &["ods"])
        .add_filter("CSV / Text", &["csv", "tsv", "txt"])
        .add_filter("All Files", &["*"])
        .pick_file()
}

pub fn pick_save(current: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .set_title("Save As")
        .add_filter("Excel Workbook (*.xlsx)", &["xlsx"])
        .add_filter("CSV (Comma delimited) (*.csv)", &["csv"])
        .add_filter("Text (Tab delimited) (*.tsv)", &["tsv"]);
    let name = current
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Book1".into());
    d = d.set_file_name(format!("{}.xlsx", name));
    if let Some(dir) = current.and_then(|p| p.parent()) {
        d = d.set_directory(dir);
    }
    let mut p = d.save_file()?;
    if p.extension().is_none() {
        p.set_extension("xlsx");
    }
    Some(p)
}

fn recent_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rexcel").join("recent.txt"))
}

pub fn load_recent() -> Vec<PathBuf> {
    let Some(p) = recent_path() else { return vec![] };
    std::fs::read_to_string(p)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).map(PathBuf::from).collect())
        .unwrap_or_default()
}

pub fn add_recent(list: &mut Vec<PathBuf>, path: &Path) {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    list.retain(|p| p != &abs);
    list.insert(0, abs);
    list.truncate(10);
    if let Some(p) = recent_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: Vec<String> = list.iter().map(|p| p.to_string_lossy().to_string()).collect();
        let _ = std::fs::write(p, text.join("\n"));
    }
}
