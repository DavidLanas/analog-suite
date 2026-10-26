//! Perilla enlazada a un `Param` de nih-plug, con pasos (detents) opcionales.

use crate::theme;
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    self, pos2, vec2, Align2, FontId, Response, Sense, Shape, Stroke, Ui, Widget,
};
use std::f32::consts::PI;

/// Barrido de la perilla: de -135° a +135° respecto a la vertical.
const SWEEP: f32 = 0.75 * PI;
/// Píxeles de arrastre vertical para recorrer todo el rango.
const DRAG_RANGE_PX: f32 = 180.0;

pub struct ParamKnob<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    steps: Option<usize>,
    diameter: f32,
    accent: egui::Color32,
    label: Option<&'a str>,
}

impl<'a, P: Param> ParamKnob<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>) -> Self {
        Self {
            param,
            setter,
            steps: None,
            diameter: 56.0,
            accent: theme::BRASS,
            label: None,
        }
    }

    /// Etiqueta distinta del nombre del parámetro (p. ej. "Gain" para Output en el OPTO 3A).
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Número de posiciones fijas (detents). Para `IntParam`/`EnumParam` con `n` valores,
    /// pasar `n`.
    pub fn steps(mut self, n: usize) -> Self {
        self.steps = Some(n.max(2));
        self
    }

    pub fn diameter(mut self, d: f32) -> Self {
        self.diameter = d;
        self
    }

    pub fn accent(mut self, c: egui::Color32) -> Self {
        self.accent = c;
        self
    }

    fn snap(&self, norm: f32) -> f32 {
        match self.steps {
            Some(n) => {
                let last = (n - 1) as f32;
                (norm.clamp(0.0, 1.0) * last).round() / last
            }
            None => norm.clamp(0.0, 1.0),
        }
    }
}

impl<P: Param> Widget for ParamKnob<'_, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let label_h = 30.0;
        let size = vec2(self.diameter.max(70.0), self.diameter + label_h);
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());

        // --- interacción ---
        // Posición continua del arrastre, guardada en la memoria temporal de egui: con pasos
        // hay que acumular sobre ella y cuantizar sólo al escribir, o los deltas pequeños
        // nunca cruzarían un detent.
        let drag_id = response.id.with("drag_norm");
        if response.drag_started() {
            self.setter.begin_set_parameter(self.param);
            let start = self.param.unmodulated_normalized_value();
            ui.memory_mut(|m| m.data.insert_temp(drag_id, start));
        }
        if response.dragged() {
            let delta = -response.drag_delta().y / DRAG_RANGE_PX;
            let continuous = ui
                .memory(|m| m.data.get_temp::<f32>(drag_id))
                .unwrap_or_else(|| self.param.unmodulated_normalized_value());
            let continuous = (continuous + delta).clamp(0.0, 1.0);
            ui.memory_mut(|m| m.data.insert_temp(drag_id, continuous));

            let current = self.param.unmodulated_normalized_value();
            let next = self.snap(continuous);
            if (next - current).abs() > f32::EPSILON {
                self.setter.set_parameter_normalized(self.param, next);
                response.mark_changed();
            }
        }
        if response.drag_stopped() {
            self.setter.end_set_parameter(self.param);
        }
        if response.double_clicked() {
            self.setter.begin_set_parameter(self.param);
            self.setter
                .set_parameter_normalized(self.param, self.param.default_normalized_value());
            self.setter.end_set_parameter(self.param);
        }

        // --- dibujo ---
        let painter = ui.painter_at(rect);
        let center = pos2(rect.center().x, rect.top() + self.diameter * 0.5);
        let r = self.diameter * 0.5;
        let norm = self.param.unmodulated_normalized_value();
        let angle = -SWEEP + 2.0 * SWEEP * norm;

        // Marcas de pasos / escala.
        let ticks = self.steps.unwrap_or(11);
        for i in 0..ticks {
            let t = i as f32 / (ticks - 1) as f32;
            let a = -SWEEP + 2.0 * SWEEP * t;
            let dir = vec2(a.sin(), -a.cos());
            painter.line_segment(
                [center + dir * (r + 3.0), center + dir * (r + 7.0)],
                Stroke::new(1.2, theme::INK_DIM),
            );
        }

        // Cuerpo.
        painter.circle_filled(center, r, theme::PANEL_LIGHT);
        painter.circle_stroke(center, r, Stroke::new(1.5, theme::INK_DIM));

        // Arco de valor.
        let n_pts = 32;
        let arc: Vec<_> = (0..=n_pts)
            .map(|i| {
                let a = -SWEEP + (angle + SWEEP) * (i as f32 / n_pts as f32);
                center + vec2(a.sin(), -a.cos()) * (r - 4.0)
            })
            .collect();
        painter.add(Shape::line(arc, Stroke::new(3.0, self.accent)));

        // Indicador.
        let dir = vec2(angle.sin(), -angle.cos());
        painter.line_segment(
            [center + dir * (r * 0.25), center + dir * (r - 6.0)],
            Stroke::new(2.5, theme::INK),
        );

        // Etiqueta y valor.
        painter.text(
            pos2(rect.center().x, rect.top() + self.diameter + 12.0),
            Align2::CENTER_CENTER,
            self.label.unwrap_or_else(|| self.param.name()),
            FontId::proportional(11.0),
            theme::INK_DIM,
        );
        painter.text(
            pos2(rect.center().x, rect.top() + self.diameter + 25.0),
            Align2::CENTER_CENTER,
            self.param.normalized_value_to_string(norm, true),
            FontId::monospace(11.0),
            theme::INK,
        );

        response
    }
}
