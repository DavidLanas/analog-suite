//! Paleta y estilos compartidos.
//!
//! Tipografías: se usan las incluidas en egui (feature `default_fonts`) para no depender
//! de archivos externos ni de licencias de fuentes. Para añadir una TTF propia, cargarla
//! en [`apply`] con `ctx.set_fonts(...)`.

use nih_plug_egui::egui::{self, Color32, FontFamily, FontId, TextStyle};

pub const PANEL: Color32 = Color32::from_rgb(34, 31, 28);
pub const PANEL_LIGHT: Color32 = Color32::from_rgb(52, 47, 42);
pub const INK: Color32 = Color32::from_rgb(232, 222, 200);
pub const INK_DIM: Color32 = Color32::from_rgb(150, 140, 122);
pub const BRASS: Color32 = Color32::from_rgb(204, 160, 84);
pub const NEEDLE: Color32 = Color32::from_rgb(226, 80, 56);
pub const SCALE_GREEN: Color32 = Color32::from_rgb(120, 170, 110);

/// Acentos por familia de plugin, para distinguirlos sin romper la coherencia.
#[derive(Clone, Copy)]
pub enum Accent {
    Compressor,
    /// Variante del compresor para el modo VARI-MU (carátula más fría, verde salvia).
    VariMu,
    /// Variante del compresor para el modo OPTO 3A (plata sobre gris azulado).
    Opto,
    Equalizer,
    Tape,
}

impl Accent {
    pub fn color(self) -> Color32 {
        match self {
            Accent::Compressor => Color32::from_rgb(214, 120, 60),
            Accent::VariMu => Color32::from_rgb(142, 168, 150),
            Accent::Opto => Color32::from_rgb(176, 184, 196),
            Accent::Equalizer => Color32::from_rgb(88, 150, 170),
            Accent::Tape => Color32::from_rgb(176, 90, 80),
        }
    }

    /// Color de fondo de la carátula: cada modo/familia tiene su propio tono.
    pub fn panel(self) -> Color32 {
        match self {
            Accent::VariMu => Color32::from_rgb(29, 34, 33),
            Accent::Opto => Color32::from_rgb(31, 33, 38),
            _ => PANEL,
        }
    }
}

/// Llamar una vez desde el `build` del editor.
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.override_text_color = Some(INK);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(10.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
    ctx.set_style(style);
}
