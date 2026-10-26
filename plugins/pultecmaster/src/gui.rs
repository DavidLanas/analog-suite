//! Editor egui de PultecMaster: curva de respuesta en vivo, barra de presets y los controles
//! agrupados como en el equipo que lo inspira (graves / boost de agudos / atten de agudos).

use crate::params::{PultecParams, PLUGIN_ID};
use nih_plug::prelude::{BoolParam, Editor, Param, ParamSetter};
use nih_plug_egui::{create_egui_editor, egui};
use pultec_dsp::Settings;
use std::sync::Arc;
use ui_common::preset_bar::preset_bar;
use ui_common::preset_browser::Browser;
use ui_common::{faceplate, knob::ParamKnob, selector::ParamSelector, theme};

const LOW_LABELS: [&str; 4] = ["20", "30", "60", "100"];
const HIGH_LABELS: [&str; 7] = ["3", "4", "5", "8", "10", "12", "16"];
const ATTEN_LABELS: [&str; 3] = ["5", "10", "20"];
const OS_LABELS: [&str; 3] = ["Off", "2x", "4x"];

/// Frecuencia de muestreo con la que se dibuja la curva (la forma apenas cambia con ella).
const CURVE_FS: f64 = 48_000.0;
const CURVE_RANGE_DB: f32 = 20.0;
const CURVE_SIZE: egui::Vec2 = egui::vec2(820.0, 100.0);

fn toggle(ui: &mut egui::Ui, setter: &ParamSetter, param: &BoolParam) {
    let mut on = param.value();
    if ui.checkbox(&mut on, param.name()).changed() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, on);
        setter.end_set_parameter(param);
    }
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().color(theme::INK_DIM));
}

/// Curva de respuesta de la red lineal (20 Hz – 20 kHz, ±20 dB).
fn response_curve(ui: &mut egui::Ui, settings: &Settings, accent: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(CURVE_SIZE, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::PANEL_LIGHT.gamma_multiply(0.6));

    let x_of = |hz: f64| rect.left() + (hz / 20.0).ln() as f32 / 1_000f32.ln() * rect.width();
    let y_of = |db: f32| rect.center().y - db / CURVE_RANGE_DB * rect.height() * 0.5;

    let grid = egui::Stroke::new(1.0, theme::INK_DIM.gamma_multiply(0.35));
    for db in [-15.0f32, -10.0, -5.0, 5.0, 10.0, 15.0] {
        painter.line_segment(
            [
                egui::pos2(rect.left(), y_of(db)),
                egui::pos2(rect.right(), y_of(db)),
            ],
            egui::Stroke::new(1.0, theme::INK_DIM.gamma_multiply(0.15)),
        );
    }
    painter.line_segment(
        [
            egui::pos2(rect.left(), y_of(0.0)),
            egui::pos2(rect.right(), y_of(0.0)),
        ],
        grid,
    );
    for (hz, label) in [
        (50.0, "50"),
        (100.0, "100"),
        (500.0, "500"),
        (1_000.0, "1k"),
        (5_000.0, "5k"),
        (10_000.0, "10k"),
    ] {
        let x = x_of(hz);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, theme::INK_DIM.gamma_multiply(0.15)),
        );
        painter.text(
            egui::pos2(x + 3.0, rect.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            label,
            egui::FontId::proportional(9.0),
            theme::INK_DIM,
        );
    }

    let points: Vec<egui::Pos2> = (0..=240)
        .map(|i| {
            let hz = 20.0 * 1_000f64.powf(i as f64 / 240.0);
            let db =
                (settings.response_db(CURVE_FS, hz) as f32).clamp(-CURVE_RANGE_DB, CURVE_RANGE_DB);
            egui::pos2(x_of(hz), y_of(db))
        })
        .collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(2.2, accent)));
}

pub fn create(params: Arc<PultecParams>) -> Option<Box<dyn Editor>> {
    let browser = Browser::new(PLUGIN_ID, params.factory_presets(), &params.snapshot());
    create_egui_editor(
        params.editor_state.clone(),
        browser,
        |ctx, _| theme::apply(ctx),
        move |ctx, setter, browser| {
            let accent_kind = theme::Accent::Equalizer;
            let accent = accent_kind.color();

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    faceplate::draw(
                        ui,
                        "PultecMaster",
                        "EQ pasivo de tubos · inspirado en un ecualizador clásico",
                        accent_kind,
                    );
                    ui.add_space(42.0);

                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.vertical(|ui| preset_bar(ui, setter, &*params, browser));
                    });
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        response_curve(ui, &params.settings(), accent);
                    });
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.add_space(30.0);

                        ui.vertical(|ui| {
                            section_title(ui, "LOW  ·  Hz");
                            ui.horizontal(|ui| {
                                ui.add(ParamKnob::new(&params.low_boost, setter).accent(accent));
                                ui.add(ParamKnob::new(&params.low_atten, setter).accent(accent));
                            });
                            ui.add(
                                ParamSelector::new(&params.low_freq, setter, &LOW_LABELS)
                                    .button_width(48.0)
                                    .accent(accent),
                            );
                        });

                        ui.add_space(24.0);

                        ui.vertical(|ui| {
                            section_title(ui, "HIGH BOOST  ·  kHz");
                            ui.horizontal(|ui| {
                                ui.add(ParamKnob::new(&params.high_boost, setter).accent(accent));
                                ui.add(ParamKnob::new(&params.bandwidth, setter).accent(accent));
                            });
                            ui.add(
                                ParamSelector::new(&params.high_freq, setter, &HIGH_LABELS)
                                    .button_width(40.0)
                                    .accent(accent),
                            );
                        });

                        ui.add_space(24.0);

                        ui.vertical(|ui| {
                            section_title(ui, "HIGH ATTEN  ·  kHz");
                            ui.add(ParamKnob::new(&params.high_atten, setter).accent(accent));
                            ui.add(
                                ParamSelector::new(&params.atten_freq, setter, &ATTEN_LABELS)
                                    .button_width(48.0)
                                    .accent(accent),
                            );
                        });
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.add(ParamKnob::new(&params.drive, setter).accent(accent));
                        ui.add(ParamKnob::new(&params.output, setter).accent(accent));
                        ui.add(ParamKnob::new(&params.mix, setter).accent(accent));
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.add(
                                ParamSelector::new(&params.oversampling, setter, &OS_LABELS)
                                    .accent(accent),
                            );
                            toggle(ui, setter, &params.bypass);
                        });
                    });
                });
            // Para que la curva y los valores sigan a la automatización del host.
            ctx.request_repaint();
        },
    )
}
