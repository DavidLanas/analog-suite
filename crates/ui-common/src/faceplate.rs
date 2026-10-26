//! Carátula (panel frontal) común a los tres plugins.
//!
//! Regla de marca: el nombre del producto es propio; las referencias a hardware clásico
//! sólo aparecen como texto "inspirado en …" y no se usan logos ni gráficos de terceros.

use crate::theme::{self, Accent};
use nih_plug_egui::egui::{self, pos2, vec2, Align2, FontId, Stroke, Ui};

/// Pinta el fondo del panel con borde, tornillos, título y leyenda. Llamar antes de los widgets.
pub fn draw(ui: &mut Ui, title: &str, subtitle: &str, accent: Accent) {
    let rect = ui.max_rect();
    let painter = ui.painter();

    painter.rect_filled(rect, 0.0, accent.panel());
    let inner = rect.shrink(6.0);
    painter.rect_stroke(
        inner,
        8.0,
        Stroke::new(2.0, accent.color()),
        egui::StrokeKind::Inside,
    );

    // Tornillos en las esquinas.
    for corner in [
        inner.left_top() + vec2(12.0, 12.0),
        inner.right_top() + vec2(-12.0, 12.0),
        inner.left_bottom() + vec2(12.0, -12.0),
        inner.right_bottom() + vec2(-12.0, -12.0),
    ] {
        painter.circle_filled(corner, 4.5, theme::PANEL_LIGHT);
        painter.line_segment(
            [corner + vec2(-3.0, 1.0), corner + vec2(3.0, -1.0)],
            Stroke::new(1.2, theme::INK_DIM),
        );
    }

    painter.text(
        pos2(inner.left() + 30.0, inner.top() + 22.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(22.0),
        accent.color(),
    );
    painter.text(
        pos2(inner.right() - 30.0, inner.top() + 22.0),
        Align2::RIGHT_CENTER,
        subtitle,
        FontId::proportional(11.0),
        theme::INK_DIM,
    );
}
