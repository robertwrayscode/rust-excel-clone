//! Sending pages to the operating system's printing.
//!
//! - macOS: the pages are written to a PDF and printed with PDFKit, which shows
//!   the standard Mac print window (printer, copies, preview).
//! - Windows: the standard Print dialog, then the pages are drawn with GDI.
//! - Linux: the PDF is sent to the default printer with `lp`.

use wordcore::layout::Page;

use crate::print::PageSetup;

#[allow(dead_code)]
pub enum Outcome {
    Printed,
    Cancelled,
    Failed(String),
}

#[allow(dead_code)]
fn temp_pdf(pages: &[Page], title: &str) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("rexcel-print");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let path = dir.join(format!("{}.pdf", if safe.trim().is_empty() { "Book1" } else { safe.trim() }));
    std::fs::write(&path, crate::print::pdf_bytes(pages, title)).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(target_os = "macos")]
pub fn print(pages: &[Page], title: &str, setup: &PageSetup) -> Outcome {
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSPaperOrientation, NSPrintInfo};
    use objc2_foundation::{NSSize, NSString, NSURL};
    use objc2_pdf_kit::{PDFDocument, PDFPrintScalingMode};

    let path = match temp_pdf(pages, title) {
        Ok(p) => p,
        Err(e) => return Outcome::Failed(e),
    };
    let Some(mtm) = MainThreadMarker::new() else { return Outcome::Failed("printing must start from the main window".into()) };
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let doc: Option<Retained<PDFDocument>> = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) };
    let Some(doc) = doc else { return Outcome::Failed("couldn't prepare the pages".into()) };
    let info = NSPrintInfo::sharedPrintInfo();
    let (w, h) = setup.page_size();
    info.setOrientation(if setup.landscape { NSPaperOrientation::Landscape } else { NSPaperOrientation::Portrait });
    info.setPaperSize(NSSize::new(w as f64, h as f64));
    info.setTopMargin(0.0);
    info.setBottomMargin(0.0);
    info.setLeftMargin(0.0);
    info.setRightMargin(0.0);
    let op = unsafe { doc.printOperationForPrintInfo_scalingMode_autoRotate(Some(&info), PDFPrintScalingMode::PageScaleDownToFit, true, mtm) };
    let Some(op) = op else { return Outcome::Failed("couldn't start printing".into()) };
    op.setJobTitle(Some(&NSString::from_str(title)));
    op.setShowsPrintPanel(true);
    op.setShowsProgressPanel(true);
    if op.runOperation() {
        Outcome::Printed
    } else {
        Outcome::Cancelled
    }
}

#[cfg(target_os = "windows")]
pub fn print(pages: &[Page], title: &str, setup: &PageSetup) -> Outcome {
    win::print(pages, title, setup)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn print(pages: &[Page], title: &str, _setup: &PageSetup) -> Outcome {
    let path = match temp_pdf(pages, title) {
        Ok(p) => p,
        Err(e) => return Outcome::Failed(e),
    };
    match std::process::Command::new("lp").arg("-t").arg(title).arg(&path).status() {
        Ok(s) if s.success() => Outcome::Printed,
        _ => {
            // no print system: open the PDF instead
            match std::process::Command::new("xdg-open").arg(&path).spawn() {
                Ok(_) => Outcome::Printed,
                Err(e) => Outcome::Failed(e.to_string()),
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    use std::collections::HashMap;

    use windows_sys::Win32::Foundation::{GlobalFree, COLORREF, RECT};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use wordcore::fonts::FontDb;
    use wordcore::layout::{LineStyle, Page, Shape};

    use super::Outcome;
    use crate::print::{PageSetup, Paper};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn colorref(c: wordcore::Color) -> COLORREF {
        c.r as u32 | (c.g as u32) << 8 | (c.b as u32) << 16
    }

    fn paper_code(p: Paper) -> i16 {
        (match p {
            Paper::Letter => DMPAPER_LETTER,
            Paper::Legal => DMPAPER_LEGAL,
            Paper::Tabloid => DMPAPER_TABLOID,
            Paper::A4 => DMPAPER_A4,
            Paper::A3 => DMPAPER_A3,
        }) as i16
    }

    pub fn print(pages: &[Page], title: &str, setup: &PageSetup) -> Outcome {
        unsafe {
            let mut pd: PRINTDLGW = std::mem::zeroed();
            pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
            pd.hwndOwner = GetActiveWindow();
            // Start from the default printer's settings with our paper and orientation.
            pd.Flags = PD_RETURNDEFAULT;
            if PrintDlgW(&mut pd) == 0 {
                return if CommDlgExtendedError() == 0 { Outcome::Cancelled } else { Outcome::Failed("no printer is installed".into()) };
            }
            if !pd.hDevMode.is_null() {
                let dm = GlobalLock(pd.hDevMode) as *mut DEVMODEW;
                if !dm.is_null() {
                    (*dm).dmFields |= DM_ORIENTATION | DM_PAPERSIZE;
                    (*dm).Anonymous1.Anonymous1.dmOrientation = (if setup.landscape { DMORIENT_LANDSCAPE } else { DMORIENT_PORTRAIT }) as i16;
                    (*dm).Anonymous1.Anonymous1.dmPaperSize = paper_code(setup.paper);
                    GlobalUnlock(pd.hDevMode);
                }
            }
            pd.Flags = PD_RETURNDC | PD_NOPAGENUMS | PD_NOSELECTION | PD_USEDEVMODECOPIESANDCOLLATE | PD_HIDEPRINTTOFILE;
            pd.nCopies = 1;
            let shown = PrintDlgW(&mut pd);
            let free = |pd: &PRINTDLGW| {
                if !pd.hDevMode.is_null() {
                    GlobalFree(pd.hDevMode);
                }
                if !pd.hDevNames.is_null() {
                    GlobalFree(pd.hDevNames);
                }
            };
            if shown == 0 || pd.hDC.is_null() {
                free(&pd);
                return Outcome::Cancelled;
            }
            let hdc = pd.hDC;
            let result = draw_document(hdc, pages, title);
            DeleteDC(hdc);
            free(&pd);
            result
        }
    }

    unsafe fn draw_document(hdc: HDC, pages: &[Page], title: &str) -> Outcome {
        let name = wide(title);
        let di = DOCINFOW { cbSize: std::mem::size_of::<DOCINFOW>() as i32, lpszDocName: name.as_ptr(), lpszOutput: std::ptr::null(), lpszDatatype: std::ptr::null(), fwType: 0 };
        if StartDocW(hdc, &di) <= 0 {
            return Outcome::Failed("the printer didn't accept the document".into());
        }
        let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX as i32) as f32;
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY as i32) as f32;
        let off_x = GetDeviceCaps(hdc, PHYSICALOFFSETX as i32) as f32;
        let off_y = GetDeviceCaps(hdc, PHYSICALOFFSETY as i32) as f32;
        let phys_w = GetDeviceCaps(hdc, PHYSICALWIDTH as i32) as f32;
        let phys_h = GetDeviceCaps(hdc, PHYSICALHEIGHT as i32) as f32;
        let db = FontDb::global();
        let mut fonts: HashMap<(String, bool, bool, i32), HFONT> = HashMap::new();
        SetBkMode(hdc, TRANSPARENT as i32);
        for page in pages {
            if StartPage(hdc) <= 0 {
                AbortDoc(hdc);
                return Outcome::Failed("the printer stopped responding".into());
            }
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextAlign(hdc, TA_BASELINE | TA_LEFT);
            // shrink to fit if the paper in the printer is smaller than the page
            let k = if phys_w > 0.0 && phys_h > 0.0 { (phys_w / (page.width * dpi_x / 72.0)).min(phys_h / (page.height * dpi_y / 72.0)).min(1.0) } else { 1.0 };
            let px = |x: f32| (x * k * dpi_x / 72.0 - off_x).round() as i32;
            let py = |y: f32| (y * k * dpi_y / 72.0 - off_y).round() as i32;
            let len = |w: f32| ((w * k * dpi_x / 72.0).round() as i32).max(1);
            for s in &page.shapes {
                match s {
                    Shape::Rect { x, y, w, h, color } => {
                        let brush = CreateSolidBrush(colorref(*color));
                        let rc = RECT { left: px(*x), top: py(*y), right: px(x + w), bottom: py(y + h) };
                        FillRect(hdc, &rc, brush);
                        DeleteObject(brush as HGDIOBJ);
                    }
                    Shape::Line { x1, y1, x2, y2, width, color, style } => {
                        let pen_style = match style {
                            LineStyle::Dotted => PS_DOT,
                            LineStyle::Dashed => PS_DASH,
                            _ => PS_SOLID,
                        };
                        let draw = |dx: f32, dy: f32, w: f32| {
                            let pen = CreatePen(pen_style, if pen_style == PS_SOLID { len(w) } else { 1 }, colorref(*color));
                            let old = SelectObject(hdc, pen as HGDIOBJ);
                            MoveToEx(hdc, px(x1 + dx), py(y1 + dy), std::ptr::null_mut());
                            LineTo(hdc, px(x2 + dx), py(y2 + dy));
                            SelectObject(hdc, old);
                            DeleteObject(pen as HGDIOBJ);
                        };
                        if *style == LineStyle::Double {
                            let horizontal = (y2 - y1).abs() < (x2 - x1).abs();
                            let d = width / 3.0;
                            let (dx, dy) = if horizontal { (0.0, d) } else { (d, 0.0) };
                            draw(-dx, -dy, d);
                            draw(dx, dy, d);
                        } else {
                            draw(0.0, 0.0, *width);
                        }
                    }
                    Shape::Image { x, y, w, h, image } => {
                        // chart pictures are 24-bit BMPs: header (54 bytes) then pixels
                        let b = &image.bytes;
                        if b.len() > 54 && &b[0..2] == b"BM" {
                            let iw = i32::from_le_bytes([b[18], b[19], b[20], b[21]]);
                            let ih = i32::from_le_bytes([b[22], b[23], b[24], b[25]]);
                            let mut bmi: BITMAPINFO = std::mem::zeroed();
                            std::ptr::copy_nonoverlapping(b[14..54].as_ptr(), &mut bmi.bmiHeader as *mut BITMAPINFOHEADER as *mut u8, 40);
                            SetStretchBltMode(hdc, HALFTONE);
                            StretchDIBits(hdc, px(*x), py(*y), px(x + w) - px(*x), py(y + h) - py(*y), 0, 0, iw, ih, b[54..].as_ptr() as *const _, &bmi, DIB_RGB_COLORS, SRCCOPY);
                        }
                    }
                    Shape::Text { x, y, text, face, size, color, .. } => {
                        let f = db.face(*face);
                        let family = if f.family == "Fallback" { "Segoe UI Symbol".to_string() } else { f.family.clone() };
                        let height = -((size * k * dpi_y / 72.0).round() as i32).max(1);
                        let key = (family.clone(), f.bold, f.italic, height);
                        let font = *fonts.entry(key).or_insert_with(|| {
                            let fname = wide(&family);
                            CreateFontW(height, 0, 0, 0, if f.bold { FW_BOLD as i32 } else { FW_NORMAL as i32 }, f.italic as u32, 0, 0, DEFAULT_CHARSET as u32, OUT_TT_PRECIS as u32, 0, PROOF_QUALITY as u32, 0, fname.as_ptr())
                        });
                        let old = SelectObject(hdc, font as HGDIOBJ);
                        SetTextColor(hdc, colorref(*color));
                        let w: Vec<u16> = text.encode_utf16().collect();
                        TextOutW(hdc, px(*x), py(*y), w.as_ptr(), w.len() as i32);
                        SelectObject(hdc, old);
                    }
                    _ => {}
                }
            }
            if EndPage(hdc) <= 0 {
                AbortDoc(hdc);
                return Outcome::Failed("the printer stopped responding".into());
            }
        }
        EndDoc(hdc);
        for (_, f) in fonts {
            DeleteObject(f as HGDIOBJ);
        }
        Outcome::Printed
    }
}
