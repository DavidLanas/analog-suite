//! Editor egui de FunkComp: carátula de hardware que cambia con el modo, barra de presets,
//! medidor de reducción de ganancia y controles propios de cada modo.

use crate::browser::{category_label, Browser, FILTERS, SAVE_CATEGORIES};
use crate::params::{FunkCompParams, ModeParam};
use dsp_core::meter::AtomicF32;
use funkcomp_dsp::varimu;
use nih_plug::prelude::{BoolParam, Editor, Param, ParamSetter};
use nih_plug_egui::{create_egui_editor, egui};
use std::sync::Arc;
use ui_common::{faceplate, knob::ParamKnob, selector::ParamSelector, theme, vu};

const MODE_LABELS: [&str; 3] = ["FET 76", "VARI-MU", "OPTO 3A"];
const RATIO_LABELS: [&str; 5] = ["4", "8", "12", "20", "ALL"];
const TIME_CONSTANT_LABELS: [&str; 6] = ["1", "2", "3", "4", "5", "6"];
const CURVE_LABELS: [&str; 2] = ["Compress", "Limit"];
const OS_LABELS: [&str; 3] = ["Off", "2x", "4x"];

fn toggle(ui: &mut egui::Ui, setter: &ParamSetter, param: &BoolParam) {
    let mut on = param.value();
    if ui.checkbox(&mut on, param.name()).changed() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, on);
        setter.end_set_parameter(param);
    }
}

/// Controles del modo FET 76: Input, Attack, Release, Output y selector de ratio.
fn fet_controls(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &FunkCompParams,
    accent: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.add(ParamKnob::new(&params.input, setter).accent(accent));
        ui.add(ParamKnob::new(&params.attack, setter).accent(accent));
        ui.add(ParamKnob::new(&params.release, setter).accent(accent));
        ui.add(ParamKnob::new(&params.output, setter).accent(accent));
    });
    ui.add_space(4.0);
    ui.add(ParamSelector::new(&params.ratio, setter, &RATIO_LABELS).accent(accent));
}

/// Controles del modo VARI-MU: Input, Threshold, Output y selector de Time Constant.
fn varimu_controls(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &FunkCompParams,
    accent: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.add(ParamKnob::new(&params.input, setter).accent(accent));
        ui.add(ParamKnob::new(&params.threshold, setter).accent(accent));
        ui.add(ParamKnob::new(&params.output, setter).accent(accent));
    });
    ui.add_space(4.0);
    ui.add(
        ParamSelector::new(&params.time_constant, setter, &TIME_CONSTANT_LABELS)
            .button_width(36.0)
            .accent(accent),
    );
    let text = varimu::describe(params.time_constant.value() as usize);
    ui.label(egui::RichText::new(text).small().color(theme::INK_DIM));
}

/// Controles del modo OPTO 3A: Peak Reduction, Gain (= Output) e interruptor Compress/Limit.
/// No hay Input: la sensibilidad la fija Peak Reduction.
fn opto_controls(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &FunkCompParams,
    accent: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.add(ParamKnob::new(&params.peak_reduction, setter).accent(accent));
        ui.add(
            ParamKnob::new(&params.output, setter)
                .label("Gain")
                .accent(accent),
        );
    });
    ui.add_space(4.0);
    ui.add(
        ParamSelector::new(&params.limit, setter, &CURVE_LABELS)
            .button_width(72.0)
            .accent(accent),
    );
    let text = if params.limit.value() {
        "Limit: 12:1, knee corto. Release en dos etapas (memoria de la célula)."
    } else {
        "Compress: 3:1, knee ancho. Release en dos etapas (memoria de la célula)."
    };
    ui.label(egui::RichText::new(text).small().color(theme::INK_DIM));
}

/// Fila del diálogo "Guardar como": nombre, categoría y confirmar/cancelar.
fn save_row(ui: &mut egui::Ui, params: &FunkCompParams, browser: &mut Browser) {
    ui.horizontal(|ui| {
        ui.label("Nombre:");
        let edit = ui.add(egui::TextEdit::singleline(&mut browser.save_name).desired_width(170.0));
        if browser.focus_name {
            edit.request_focus();
            browser.focus_name = false;
        }
        for (category, label) in SAVE_CATEGORIES {
            if ui
                .selectable_label(browser.save_category == category, label)
                .clicked()
            {
                browser.save_category = category;
            }
        }
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let text = if browser.save_would_overwrite() {
            "Sobrescribir"
        } else {
            "Guardar"
        };
        if ui.button(text).clicked() || enter {
            browser.save_as(params);
        }
        if ui.button("Cancelar").clicked() {
            browser.saving = false;
        }
    });
}

/// Barra de presets: anterior/siguiente, lista por categorías, guardar, guardar como y borrar.
fn preset_bar(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    params: &FunkCompParams,
    browser: &mut Browser,
) {
    let snapshot = params.snapshot();
    browser.tick(&snapshot);

    if browser.saving {
        save_row(ui, params, browser);
        return;
    }

    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("PRESET").small().color(theme::INK_DIM));
        if ui.button("<").clicked() {
            browser.step(-1, setter, params);
        }

        let title = browser.title(&snapshot);
        ui.menu_button(egui::RichText::new(title).strong(), |ui| {
            ui.set_min_width(300.0);
            ui.horizontal_wrapped(|ui| {
                for (filter, label) in FILTERS {
                    if ui
                        .selectable_label(browser.filter == filter, label)
                        .clicked()
                    {
                        browser.filter = filter;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    let entries = browser.entries();
                    if entries.is_empty() {
                        ui.label(
                            egui::RichText::new("No hay presets en esta categoría")
                                .small()
                                .color(theme::INK_DIM),
                        );
                    }
                    let mut last_category = String::new();
                    for source in entries {
                        let Some(preset) = browser.preset(&source) else {
                            continue;
                        };
                        let category = preset.category_or_user();
                        if category != last_category {
                            ui.label(
                                egui::RichText::new(category_label(category).to_uppercase())
                                    .small()
                                    .color(theme::INK_DIM),
                            );
                            last_category = category.to_owned();
                        }
                        let selected = browser.current() == Some(&source);
                        let mut response = ui.selectable_label(selected, &preset.name);
                        if !preset.description.is_empty() {
                            response = response.on_hover_text(&preset.description);
                        }
                        if response.clicked() {
                            chosen = Some(source);
                        }
                    }
                });
            if chosen.is_some() {
                ui.close_menu();
            }
        });

        if ui.button(">").clicked() {
            browser.step(1, setter, params);
        }

        ui.add_space(8.0);
        if ui.button("Guardar").clicked() {
            browser.save(params);
        }
        if ui.button("Guardar como...").clicked() {
            browser.begin_save_as();
        }
        if browser.current_is_user() {
            if browser.confirm_delete {
                if ui.button("Confirmar borrado").clicked() {
                    browser.delete_current();
                }
                if ui.button("No").clicked() {
                    browser.confirm_delete = false;
                }
            } else if ui.button("Borrar").clicked() {
                browser.confirm_delete = true;
            }
        }
        if !browser.message.is_empty() {
            ui.label(
                egui::RichText::new(&browser.message)
                    .small()
                    .color(theme::INK_DIM),
            );
        }
    });

    if let Some(source) = chosen {
        browser.load(source, setter, params);
    }
}

pub fn create(params: Arc<FunkCompParams>, gr_meter: Arc<AtomicF32>) -> Option<Box<dyn Editor>> {
    let browser = Browser::new(&params);
    create_egui_editor(
        params.editor_state.clone(),
        browser,
        |ctx, _| theme::apply(ctx),
        move |ctx, setter, browser| {
            let mode = params.mode.value();
            let (accent_kind, subtitle) = match mode {
                ModeParam::Fet76 => (
                    theme::Accent::Compressor,
                    "modo FET 76 · inspirado en un compresor FET clásico",
                ),
                ModeParam::VariMu => (
                    theme::Accent::VariMu,
                    "modo VARI-MU 660 · inspirado en un compresor de tubos variable-mu",
                ),
                ModeParam::Opto3a => (
                    theme::Accent::Opto,
                    "modo OPTO 3A · inspirado en un compresor óptico clásico",
                ),
            };
            let accent = accent_kind.color();

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    faceplate::draw(ui, "FunkComp", subtitle, accent_kind);
                    ui.add_space(42.0);

                    ui.horizontal(|ui| {
                        ui.add_space(30.0);
                        ui.vertical(|ui| preset_bar(ui, setter, &params, browser));
                    });
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.add_space(30.0);

                        // Columna izquierda: selector de modo, medidor y conmutadores.
                        ui.vertical(|ui| {
                            ui.add(
                                ParamSelector::new(&params.mode, setter, &MODE_LABELS)
                                    .accent(accent),
                            );
                            ui.add_space(4.0);
                            vu::vu_meter(ui, gr_meter.load(), &vu::GR_SCALE, "GAIN REDUCTION · dB");
                            ui.add_space(6.0);
                            toggle(ui, setter, &params.bypass);
                            toggle(ui, setter, &params.auto_gain);
                        });

                        ui.add_space(18.0);

                        // Controles del modo activo.
                        ui.vertical(|ui| match mode {
                            ModeParam::Fet76 => fet_controls(ui, setter, &params, accent),
                            ModeParam::VariMu => varimu_controls(ui, setter, &params, accent),
                            ModeParam::Opto3a => opto_controls(ui, setter, &params, accent),
                        });

                        ui.add_space(18.0);

                        // Extras comunes a todos los modos.
                        ui.vertical(|ui| {
                            ui.add(ParamKnob::new(&params.mix, setter).accent(accent));
                            toggle(ui, setter, &params.sidechain_hpf_on);
                            ui.add(
                                ParamKnob::new(&params.sidechain_hpf, setter)
                                    .diameter(40.0)
                                    .accent(accent),
                            );
                            ui.add(
                                ParamSelector::new(&params.oversampling, setter, &OS_LABELS)
                                    .accent(accent),
                            );
                        });
                    });
                });
            // Medidores animados: repintar mientras la GUI esté abierta.
            ctx.request_repaint();
        },
    )
}
