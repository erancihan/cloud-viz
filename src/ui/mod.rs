pub mod canvas;
pub mod glyphs;

use crate::theme::Rgb;
use eframe::egui::Color32;

pub fn c32(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

pub fn c32a(rgb: Rgb, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(rgb.0, rgb.1, rgb.2, alpha)
}
