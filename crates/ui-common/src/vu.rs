//! Vúmetro de aguja. Sirve para VU de nivel y para reducción de ganancia.

use crate::theme;
use nih_plug_egui::egui::{self, pos2, vec2, Align2, FontId, Response, Sense, Shape, Stroke, Ui};

/// Rango y marcas de la escala.
pub struct VuScale {
    pub min: f32,
    pub max: f32,
    /// Posiciones con etiqueta.
    pub ticks: &'static [f32],
    /// A partir de este valor la escala se pinta en rojo (p. ej. 0 VU).
    pub red_from: Option<f32>,
}

/// Escala VU clásica: −20 … +3.
pub const VU_SCALE: VuScale = VuScale {
    min: -20.0,
    max: 3.0,
    ticks: &[-20.0, -10.0, -7.0, -5.0, -3.0, 0.0, 3.0],
    red_from: Some(0.0),
};

/// Escala de reducción de ganancia: 0 … 20 dB (la aguja crece hacia la derecha).
pub const GR_SCALE: VuScale = VuScale {
    min: 0.0,
    max: 20.0,
    ticks: &[0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0],
    red_from: None,
};

/// Dibuja el vúmetro y devuelve su `Response`. `value` en las unidades de `scale`.
pub fn vu_meter(ui: &mut Ui, value: f32, scale: &VuScale, caption: &str) -> Response {
    let size = vec2(180.0, 110.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(222, 205, 160));
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(2.0, theme::PANEL_LIGHT),
        egui::StrokeKind::Inside,
    );

    let pivot = pos2(rect.center().x, rect.bottom() + 20.0);
    let radius = 112.0;
    let span = 0.55; // semiángulo del arco en radianes (~63°)
    let to_angle = |v: f32| {
        let t = ((v - scale.min) / (scale.max - scale.min)).clamp(0.0, 1.0);
        -span + 2.0 * span * t
    };
    let point = |v: f32, r: f32| {
        let a = to_angle(v);
        pivot + vec2(a.sin(), -a.cos()) * r
    };

    // Arco de escala (verde/rojo).
    let steps = 48;
    let red_from = scale.red_from.unwrap_or(scale.max + 1.0);
    for i in 0..steps {
        let v0 = scale.min + (scale.max - scale.min) * i as f32 / steps as f32;
        let v1 = scale.min + (scale.max - scale.min) * (i + 1) as f32 / steps as f32;
        let color = if v0 >= red_from {
            theme::NEEDLE
        } else {
            egui::Color32::from_rgb(60, 50, 40)
        };
        painter.line_segment(
            [point(v0, radius - 22.0), point(v1, radius - 22.0)],
            Stroke::new(2.5, color),
        );
    }

    // Marcas y etiquetas.
    for &t in scale.ticks {
        painter.line_segment(
            [point(t, radius - 22.0), point(t, radius - 30.0)],
            Stroke::new(1.5, egui::Color32::from_rgb(60, 50, 40)),
        );
        painter.text(
            point(t, radius - 40.0),
            Align2::CENTER_CENTER,
            format!("{t:.0}"),
            FontId::proportional(9.0),
            egui::Color32::from_rgb(60, 50, 40),
        );
    }

    // Aguja.
    painter.add(Shape::line_segment(
        [point(value, 12.0), point(value, radius - 18.0)],
        Stroke::new(1.8, theme::NEEDLE),
    ));
    painter.text(
        pos2(rect.center().x, rect.bottom() - 10.0),
        Align2::CENTER_CENTER,
        caption,
        FontId::proportional(10.0),
        egui::Color32::from_rgb(60, 50, 40),
    );

    response
}
