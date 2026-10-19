//! File import and export (xlsx, csv).
//!
//! TEMPORARY STUBS added by the GUI work so the app compiles; the real
//! implementation replaces this file.

/// Open a workbook (dispatch on extension).
pub fn open(path: &std::path::Path) -> Result<crate::Workbook, String> {
    Err(format!("Opening {} is not implemented yet", path.display()))
}

/// Save one sheet / the workbook (dispatch on extension).
pub fn save(wb: &crate::Workbook, sheet: usize, path: &std::path::Path) -> Result<(), String> {
    let _ = (wb, sheet);
    Err(format!("Saving {} is not implemented yet", path.display()))
}
