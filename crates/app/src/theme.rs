//! Colors and egui styling for an Excel-like look.

use egui::{Color32, CornerRadius, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(0x21, 0x73, 0x46);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x18, 0x5C, 0x37);
pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0xE3, 0xF1, 0xE8);
pub const RIBBON_BG: Color32 = Color32::from_rgb(0xF3, 0xF3, 0xF3);
pub const PANEL_BG: Color32 = Color32::from_rgb(0xF8, 0xF8, 0xF8);
pub const BORDER: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD4);
pub const GRIDLINE: Color32 = Color32::from_rgb(0xE1, 0xE1, 0xE1);
pub const HEADER_BG: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF5);
pub const HEADER_SEL_BG: Color32 = Color32::from_rgb(0xD2, 0xD2, 0xD2);
pub const HEADER_FULL_SEL_BG: Color32 = Color32::from_rgb(0xC6, 0xE0, 0xCE);
pub const HEADER_TEXT: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
pub const HEADER_LINE: Color32 = Color32::from_rgb(0xC8, 0xC8, 0xC8);
pub const SEL_FILL: Color32 = Color32::from_rgba_premultiplied(0x18, 0x30, 0x20, 0x1C);
pub const TEXT: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
pub const MUTED: Color32 = Color32::from_rgb(0x60, 0x60, 0x60);

/// Colors used to highlight references while editing a formula (Excel's palette).
pub const REF_COLORS: [Color32; 8] = [
    Color32::from_rgb(0x44, 0x72, 0xC4),
    Color32::from_rgb(0xC0, 0x3B, 0x2B),
    Color32::from_rgb(0x7D, 0x3C, 0x98),
    Color32::from_rgb(0x2E, 0x8B, 0x57),
    Color32::from_rgb(0xB0, 0x60, 0x10),
    Color32::from_rgb(0x1A, 0x8C, 0xB0),
    Color32::from_rgb(0xC0, 0x35, 0x8A),
    Color32::from_rgb(0x55, 0x6B, 0x2F),
];

/// Office chart series palette.
pub const SERIES_COLORS: [Color32; 8] = [
    Color32::from_rgb(0x44, 0x72, 0xC4),
    Color32::from_rgb(0xED, 0x7D, 0x31),
    Color32::from_rgb(0xA5, 0xA5, 0xA5),
    Color32::from_rgb(0xFF, 0xC0, 0x00),
    Color32::from_rgb(0x5B, 0x9B, 0xD5),
    Color32::from_rgb(0x70, 0xAD, 0x47),
    Color32::from_rgb(0x26, 0x44, 0x78),
    Color32::from_rgb(0x9E, 0x48, 0x0E),
];

/// Theme color palette for font/fill pickers (rows of Office colors).
pub const PALETTE: [[u32; 10]; 6] = [
    [0xFFFFFF, 0x000000, 0xE7E6E6, 0x44546A, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47],
    [0xF2F2F2, 0x7F7F7F, 0xD0CECE, 0xD6DCE4, 0xD9E2F3, 0xFBE5D5, 0xEDEDED, 0xFFF2CC, 0xDEEBF6, 0xE2EFD9],
    [0xD8D8D8, 0x595959, 0xAEABAB, 0xADB9CA, 0xB4C6E7, 0xF7CBAC, 0xDBDBDB, 0xFFE599, 0xBDD7EE, 0xC5E0B3],
    [0xBFBFBF, 0x3F3F3F, 0x757070, 0x8496B0, 0x8EAADB, 0xF4B183, 0xC9C9C9, 0xFFD965, 0x9DC3E6, 0xA8D08D],
    [0xA5A5A5, 0x262626, 0x3A3838, 0x323F4F, 0x2F5496, 0xC55A11, 0x7B7B7B, 0xBF9000, 0x2E75B5, 0x538135],
    [0xC00000, 0xFF0000, 0xFFC000, 0xFFFF00, 0x92D050, 0x00B050, 0x00B0F0, 0x0070C0, 0x002060, 0x7030A0],
];

pub fn to_c32(c: engine::style::Color) -> Color32 {
    Color32::from_rgb(c.r(), c.g(), c.b())
}

pub fn from_c32(c: Color32) -> engine::style::Color {
    engine::style::Color::rgb(c.r(), c.g(), c.b())
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 2.0);
        s.spacing.interact_size.y = 22.0;
        let v = &mut s.visuals;
        v.panel_fill = PANEL_BG;
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::WHITE;
        v.selection.bg_fill = Color32::from_rgb(0xC6, 0xE0, 0xCE);
        v.selection.stroke = Stroke::new(1.0, ACCENT_DARK);
        v.hyperlink_color = Color32::from_rgb(0x05, 0x63, 0xC1);
        v.window_corner_radius = CornerRadius::same(4);
        v.menu_corner_radius = CornerRadius::same(3);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open, &mut v.widgets.noninteractive] {
            w.corner_radius = CornerRadius::same(2);
        }
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = Color32::WHITE;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xB4, 0xB4, 0xB4));
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xE6, 0xE6, 0xE6);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0xE6, 0xE6, 0xE6);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xC8, 0xC8, 0xC8));
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0xD0, 0xD0, 0xD0);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    });
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
    });
}
