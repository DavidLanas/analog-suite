//! Selector de posiciones (botonera) para parámetros discretos: modo, ratio, velocidad…

use crate::theme;
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{self, vec2, Align2, FontId, Response, Sense, Stroke, Ui, Widget};

pub struct ParamSelector<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    labels: &'a [&'a str],
    accent: egui::Color32,
    button_width: f32,
}

impl<'a, P: Param> ParamSelector<'a, P> {
    /// `labels.len()` debe coincidir con el número de valores del parámetro
    /// (`IntParam` con rango 0..=n-1 o `EnumParam` con n variantes).
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, labels: &'a [&'a str]) -> Self {
        Self {
            param,
            setter,
            labels,
            accent: theme::BRASS,
            button_width: 54.0,
        }
    }

    /// Ancho de cada botón en píxeles (por defecto 54; menos para etiquetas cortas).
    pub fn button_width(mut self, w: f32) -> Self {
        self.button_width = w;
        self
    }

    pub fn accent(mut self, c: egui::Color32) -> Self {
        self.accent = c;
        self
    }
}

impl<P: Param> Widget for ParamSelector<'_, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let n = self.labels.len().max(1);
        let last = (n.max(2) - 1) as f32;
        let selected = (self.param.unmodulated_normalized_value() * last).round() as usize;

        let button = vec2(self.button_width, 24.0);
        let gap = 4.0;
        let total = vec2(n as f32 * button.x + (n - 1) as f32 * gap, button.y + 16.0);
        let (rect, mut response) = ui.allocate_exact_size(total, Sense::hover());
        let painter = ui.painter_at(rect);

        painter.text(
            rect.left_top() + vec2(0.0, 6.0),
            Align2::LEFT_CENTER,
            self.param.name(),
            FontId::proportional(11.0),
            theme::INK_DIM,
        );

        for (i, label) in self.labels.iter().enumerate() {
            let min = rect.left_top() + vec2(i as f32 * (button.x + gap), 16.0);
            let r = egui::Rect::from_min_size(min, button);
            let id = ui.id().with((self.param.name(), i));
            let click = ui.interact(r, id, Sense::click());
            let on = i == selected;

            painter.rect_filled(r, 3.0, if on { self.accent } else { theme::PANEL_LIGHT });
            painter.rect_stroke(
                r,
                3.0,
                Stroke::new(1.0, theme::INK_DIM),
                egui::StrokeKind::Inside,
            );
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                *label,
                FontId::proportional(11.0),
                if on { theme::PANEL } else { theme::INK },
            );

            if click.clicked() && !on {
                let norm = i as f32 / last;
                self.setter.begin_set_parameter(self.param);
                self.setter.set_parameter_normalized(self.param, norm);
                self.setter.end_set_parameter(self.param);
                response.mark_changed();
            }
        }
        response
    }
}
