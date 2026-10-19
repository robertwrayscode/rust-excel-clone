//! xlsx import/export.
//!
//! Reading uses a built-in zip + XML parser so styles, conditional formats,
//! validations, comments, hyperlinks, names and charts are preserved.
//! Writing uses `rust_xlsxwriter` (Excel-compatible output; `_xlfn.` prefixes
//! for newer functions are added automatically).

pub use super::xlsx_read::{read_xlsx, width_chars_to_px};
pub use super::xlsx_write::write_xlsx;
