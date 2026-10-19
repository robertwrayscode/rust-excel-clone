//! Font loading: use the real Office fonts (or metric-compatible substitutes)
//! when the system has them, falling back to egui's built-in fonts.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// Font families offered in the font picker.
pub const FONT_NAMES: [&str; 10] = [
    "Calibri",
    "Arial",
    "Times New Roman",
    "Courier New",
    "Verdana",
    "Georgia",
    "Segoe UI",
    "Tahoma",
    "Consolas",
    "Cambria",
];

struct Candidates {
    name: &'static str,
    regular: &'static [&'static str],
    bold: &'static [&'static str],
}

const CANDIDATES: &[Candidates] = &[
    Candidates {
        name: "Calibri",
        regular: &[
            "C:\\Windows\\Fonts\\calibri.ttf",
            "/Library/Fonts/Microsoft/Calibri.ttf",
            "/Applications/Microsoft Excel.app/Contents/Resources/DFonts/Calibri.ttf",
            "/usr/share/fonts/truetype/crosextra/Carlito-Regular.ttf",
            "/usr/share/fonts/truetype/carlito/Carlito-Regular.ttf",
            "/usr/share/fonts/carlito/Carlito-Regular.ttf",
        ],
        bold: &[
            "C:\\Windows\\Fonts\\calibrib.ttf",
            "/Library/Fonts/Microsoft/Calibri Bold.ttf",
            "/Applications/Microsoft Excel.app/Contents/Resources/DFonts/Calibrib.ttf",
            "/usr/share/fonts/truetype/crosextra/Carlito-Bold.ttf",
            "/usr/share/fonts/truetype/carlito/Carlito-Bold.ttf",
            "/usr/share/fonts/carlito/Carlito-Bold.ttf",
        ],
    },
    Candidates {
        name: "Arial",
        regular: &[
            "C:\\Windows\\Fonts\\arial.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/Library/Fonts/Arial.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
        ],
        bold: &[
            "C:\\Windows\\Fonts\\arialbd.ttf",
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            "/Library/Fonts/Arial Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
        ],
    },
    Candidates {
        name: "Times New Roman",
        regular: &[
            "C:\\Windows\\Fonts\\times.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSerif-Regular.ttf",
        ],
        bold: &[
            "C:\\Windows\\Fonts\\timesbd.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf",
            "/usr/share/fonts/liberation/LiberationSerif-Bold.ttf",
        ],
    },
    Candidates {
        name: "Courier New",
        regular: &[
            "C:\\Windows\\Fonts\\cour.ttf",
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
        ],
        bold: &[
            "C:\\Windows\\Fonts\\courbd.ttf",
            "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
            "/usr/share/fonts/liberation/LiberationMono-Bold.ttf",
        ],
    },
    Candidates {
        name: "Verdana",
        regular: &["C:\\Windows\\Fonts\\verdana.ttf", "/System/Library/Fonts/Supplemental/Verdana.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"],
        bold: &["C:\\Windows\\Fonts\\verdanab.ttf", "/System/Library/Fonts/Supplemental/Verdana Bold.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"],
    },
    Candidates {
        name: "Georgia",
        regular: &["C:\\Windows\\Fonts\\georgia.ttf", "/System/Library/Fonts/Supplemental/Georgia.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf"],
        bold: &["C:\\Windows\\Fonts\\georgiab.ttf", "/System/Library/Fonts/Supplemental/Georgia Bold.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"],
    },
    Candidates {
        name: "Segoe UI",
        regular: &["C:\\Windows\\Fonts\\segoeui.ttf", "/System/Library/Fonts/SFNS.ttf", "/usr/share/fonts/opentype/inter/Inter-Regular.otf"],
        bold: &["C:\\Windows\\Fonts\\segoeuib.ttf", "/usr/share/fonts/opentype/inter/Inter-Bold.otf"],
    },
    Candidates {
        name: "Tahoma",
        regular: &["C:\\Windows\\Fonts\\tahoma.ttf", "/System/Library/Fonts/Supplemental/Tahoma.ttf"],
        bold: &["C:\\Windows\\Fonts\\tahomabd.ttf", "/System/Library/Fonts/Supplemental/Tahoma Bold.ttf"],
    },
    Candidates {
        name: "Consolas",
        regular: &["C:\\Windows\\Fonts\\consola.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"],
        bold: &["C:\\Windows\\Fonts\\consolab.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"],
    },
    Candidates {
        name: "Cambria",
        regular: &["C:\\Windows\\Fonts\\cambria.ttc", "/usr/share/fonts/truetype/crosextra/Caladea-Regular.ttf"],
        bold: &["C:\\Windows\\Fonts\\cambriab.ttf", "/usr/share/fonts/truetype/crosextra/Caladea-Bold.ttf"],
    },
];

/// Which families were loaded: name (lower-case) -> has bold face.
static LOADED: OnceLock<HashMap<String, bool>> = OnceLock::new();
static UI_FONT: OnceLock<bool> = OnceLock::new();

fn read_first(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

fn family_name(name: &str, bold: bool) -> String {
    format!("cell:{}{}", name.to_lowercase(), if bold { ":b" } else { "" })
}

/// Load fonts and register them with egui.
pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let fallback: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut loaded = HashMap::new();
    for c in CANDIDATES {
        let Some(reg) = read_first(c.regular) else { continue };
        let key = format!("{}-regular", c.name);
        defs.font_data.insert(key.clone(), Arc::new(FontData::from_owned(reg)));
        let mut fam = vec![key.clone()];
        fam.extend(fallback.iter().cloned());
        defs.families.insert(FontFamily::Name(family_name(c.name, false).into()), fam);
        let has_bold = if let Some(b) = read_first(c.bold) {
            let bkey = format!("{}-bold", c.name);
            defs.font_data.insert(bkey.clone(), Arc::new(FontData::from_owned(b)));
            let mut fam = vec![bkey];
            fam.extend(fallback.iter().cloned());
            defs.families.insert(FontFamily::Name(family_name(c.name, true).into()), fam);
            true
        } else {
            false
        };
        loaded.insert(c.name.to_lowercase(), has_bold);
    }
    // Use Segoe UI (Windows) / Inter for the interface when available.
    let mut ui_font = false;
    if let Some(key) = ["Segoe UI-regular"].iter().find(|k| defs.font_data.contains_key(**k)) {
        if let Some(p) = defs.families.get_mut(&FontFamily::Proportional) {
            p.insert(0, key.to_string());
            ui_font = true;
        }
    }
    let _ = UI_FONT.set(ui_font);
    let _ = LOADED.set(loaded);
    ctx.set_fonts(defs);
}

/// Font for cell text. Returns the font and whether bold must be simulated.
pub fn cell_font(name: Option<&str>, bold: bool, size: f32) -> (FontId, bool) {
    let loaded = LOADED.get();
    let name = name.unwrap_or(engine::style::DEFAULT_FONT);
    let lookup = |n: &str| loaded.and_then(|l| l.get(&n.to_lowercase()).copied());
    let (family_key, has_bold) = match lookup(name) {
        Some(b) => (name.to_string(), b),
        None => {
            let lower = name.to_lowercase();
            let alt = if lower.contains("mono") || lower.contains("courier") || lower.contains("consol") {
                "Courier New"
            } else if lower.contains("times") || lower.contains("serif") || lower.contains("cambria") || lower.contains("georgia") {
                "Times New Roman"
            } else if lower.contains("arial") || lower.contains("helvet") {
                "Arial"
            } else {
                "Calibri"
            };
            match lookup(alt) {
                Some(b) => (alt.to_string(), b),
                None => {
                    let fam = if alt == "Courier New" { FontFamily::Monospace } else { FontFamily::Proportional };
                    return (FontId::new(size, fam), bold);
                }
            }
        }
    };
    if bold && has_bold {
        (FontId::new(size, FontFamily::Name(family_name(&family_key, true).into())), false)
    } else {
        (FontId::new(size, FontFamily::Name(family_name(&family_key, false).into())), bold)
    }
}
