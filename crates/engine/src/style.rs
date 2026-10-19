//! Cell formatting.

use serde::{Deserialize, Serialize};

/// An RGB color, 0xRRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Color = Color(0x000000);
    pub const WHITE: Color = Color(0xFFFFFF);
    pub const RED: Color = Color(0xFF0000);
    pub const GREEN: Color = Color(0x00B050);
    pub const BLUE: Color = Color(0x0070C0);
    pub const YELLOW: Color = Color(0xFFFF00);
    pub fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }
    pub fn r(&self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub fn g(&self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn b(&self) -> u8 {
        self.0 as u8
    }
    pub fn to_hex(&self) -> String {
        format!("{:06X}", self.0 & 0xFFFFFF)
    }
    /// Parse "RRGGBB", "#RRGGBB" or "AARRGGBB".
    pub fn from_hex(s: &str) -> Option<Color> {
        let s = s.trim_start_matches('#');
        let s = if s.len() == 8 { &s[2..] } else { s };
        if s.len() != 6 {
            return None;
        }
        u32::from_str_radix(s, 16).ok().map(Color)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum HAlign {
    #[default]
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterAcross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VAlign {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BorderKind {
    #[default]
    Thin,
    Medium,
    Thick,
    Dashed,
    Dotted,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BorderLine {
    pub kind: BorderKind,
    pub color: Color,
}

impl Default for BorderLine {
    fn default() -> Self {
        BorderLine { kind: BorderKind::Thin, color: Color::BLACK }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Borders {
    pub left: Option<BorderLine>,
    pub right: Option<BorderLine>,
    pub top: Option<BorderLine>,
    pub bottom: Option<BorderLine>,
}

impl Borders {
    pub fn is_empty(&self) -> bool {
        self.left.is_none() && self.right.is_none() && self.top.is_none() && self.bottom.is_none()
    }
}

pub const DEFAULT_FONT: &str = "Calibri";
pub const DEFAULT_FONT_SIZE: f32 = 11.0;

/// Full formatting for one cell. Interned in [`crate::workbook::Workbook::styles`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Font size in tenths of a point (110 = 11pt).
    pub font_size_tenths: u16,
    pub font_name: Option<String>,
    pub font_color: Option<Color>,
    pub fill: Option<Color>,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub wrap: bool,
    pub indent: u8,
    /// Text rotation in degrees (-90..90).
    pub rotation: i16,
    /// Number format code, e.g. "General", "0.00", "m/d/yyyy".
    pub num_fmt: String,
    pub borders: Borders,
    pub locked: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            font_size_tenths: 110,
            font_name: None,
            font_color: None,
            fill: None,
            h_align: HAlign::General,
            v_align: VAlign::Bottom,
            wrap: false,
            indent: 0,
            rotation: 0,
            num_fmt: "General".into(),
            borders: Borders::default(),
            locked: true,
        }
    }
}

impl Style {
    pub fn font_size(&self) -> f32 {
        self.font_size_tenths as f32 / 10.0
    }
    pub fn set_font_size(&mut self, pt: f32) {
        self.font_size_tenths = (pt * 10.0).round().clamp(10.0, 4090.0) as u16;
    }
}

pub type StyleId = u32;
