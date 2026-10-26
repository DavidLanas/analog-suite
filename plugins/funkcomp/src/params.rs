//! Parámetros de FunkComp y su traducción a los ajustes del motor.
//!
//! Todo el suavizado de parámetros vive en el motor (en `f64`, por muestra), por eso los
//! `FloatParam` no usan el suavizador de nih-plug.

use funkcomp_dsp::fet76::{attack_time_us, release_time_ms};
use funkcomp_dsp::{Ratio, Settings, MODE_FET76, MODE_OPTO3A, MODE_VARIMU};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use presets::Preset;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Identificador del plugin en los archivos de preset.
pub const PLUGIN_ID: &str = "funkcomp";

pub const EDITOR_SIZE: (u32, u32) = (800, 460);

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeParam {
    #[id = "fet76"]
    #[name = "FET 76"]
    Fet76,
    #[id = "varimu"]
    #[name = "VARI-MU 660"]
    VariMu,
    #[id = "opto3a"]
    #[name = "OPTO 3A"]
    Opto3a,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatioParam {
    #[id = "r4"]
    #[name = "4:1"]
    R4,
    #[id = "r8"]
    #[name = "8:1"]
    R8,
    #[id = "r12"]
    #[name = "12:1"]
    R12,
    #[id = "r20"]
    #[name = "20:1"]
    R20,
    #[id = "all"]
    #[name = "All"]
    AllButtons,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OversamplingParam {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "x2"]
    #[name = "2x"]
    X2,
    #[id = "x4"]
    #[name = "4x"]
    X4,
}

#[derive(Params)]
pub struct FunkCompParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "bypass"]
    pub bypass: BoolParam,

    #[id = "mode"]
    pub mode: EnumParam<ModeParam>,

    /// Empuja la señal contra el umbral fijo; es la perilla que decide cuánto se comprime.
    #[id = "input"]
    pub input: FloatParam,

    #[id = "output"]
    pub output: FloatParam,

    /// Perilla 1..7, invertida como en el hardware: 7 = más rápido (20 µs).
    #[id = "attack"]
    pub attack: FloatParam,

    /// Perilla 1..7, invertida: 7 = más rápido (50 ms).
    #[id = "release"]
    pub release: FloatParam,

    #[id = "ratio"]
    pub ratio: EnumParam<RatioParam>,

    /// VARI-MU: centro del knee (dB).
    #[id = "threshold"]
    pub threshold: FloatParam,

    /// VARI-MU: posición 1..=6 del selector Time Constant (5 y 6 dependen del programa).
    #[id = "time_const"]
    pub time_constant: IntParam,

    /// OPTO 3A: sensibilidad del side-chain (0..100); sustituye al Input.
    #[id = "peak_red"]
    pub peak_reduction: FloatParam,

    /// OPTO 3A: `false` = Compress, `true` = Limit.
    #[id = "limit"]
    pub limit: BoolParam,

    #[id = "sc_hpf_on"]
    pub sidechain_hpf_on: BoolParam,

    #[id = "sc_hpf"]
    pub sidechain_hpf: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "auto_gain"]
    pub auto_gain: BoolParam,

    #[id = "oversampling"]
    pub oversampling: EnumParam<OversamplingParam>,
}

impl Default for FunkCompParams {
    fn default() -> Self {
        let db_range = |min, max| FloatRange::Linear { min, max };

        Self {
            editor_state: EguiState::from_size(EDITOR_SIZE.0, EDITOR_SIZE.1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            mode: EnumParam::new("Mode", ModeParam::Fet76),

            input: FloatParam::new("Input", 6.0, db_range(-24.0, 36.0))
                .with_unit(" dB")
                .with_step_size(0.1),
            output: FloatParam::new("Output", 0.0, db_range(-24.0, 24.0))
                .with_unit(" dB")
                .with_step_size(0.1),

            attack: FloatParam::new("Attack", 4.0, FloatRange::Linear { min: 1.0, max: 7.0 })
                .with_step_size(0.1)
                .with_value_to_string(Arc::new(|knob: f32| {
                    format!("{knob:.1} · {:.0} µs", attack_time_us(knob as f64))
                })),
            release: FloatParam::new("Release", 5.0, FloatRange::Linear { min: 1.0, max: 7.0 })
                .with_step_size(0.1)
                .with_value_to_string(Arc::new(|knob: f32| {
                    format!("{knob:.1} · {:.0} ms", release_time_ms(knob as f64))
                })),

            ratio: EnumParam::new("Ratio", RatioParam::R4),

            threshold: FloatParam::new("Threshold", -12.0, db_range(-40.0, 0.0))
                .with_unit(" dB")
                .with_step_size(0.1),
            time_constant: IntParam::new("Time Constant", 2, IntRange::Linear { min: 1, max: 6 }),

            peak_reduction: FloatParam::new(
                "Peak Reduction",
                50.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_step_size(0.1)
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            limit: BoolParam::new("Compress / Limit", false),

            sidechain_hpf_on: BoolParam::new("SC HPF", false),
            sidechain_hpf: FloatParam::new(
                "SC Freq",
                80.0,
                FloatRange::Skewed {
                    min: 60.0,
                    max: 200.0,
                    factor: FloatRange::skew_factor(-0.5),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            auto_gain: BoolParam::new("Auto Gain", false),
            oversampling: EnumParam::new("Oversampling", OversamplingParam::X2),
        }
    }
}

impl FunkCompParams {
    /// Instantánea de los parámetros en el formato del motor.
    pub fn settings(&self) -> Settings {
        Settings {
            mode: match self.mode.value() {
                ModeParam::Fet76 => MODE_FET76,
                ModeParam::VariMu => MODE_VARIMU,
                ModeParam::Opto3a => MODE_OPTO3A,
            },
            input_db: self.input.value() as f64,
            output_db: self.output.value() as f64,
            attack_knob: self.attack.value() as f64,
            release_knob: self.release.value() as f64,
            ratio: match self.ratio.value() {
                RatioParam::R4 => Ratio::R4,
                RatioParam::R8 => Ratio::R8,
                RatioParam::R12 => Ratio::R12,
                RatioParam::R20 => Ratio::R20,
                RatioParam::AllButtons => Ratio::AllButtons,
            },
            threshold_db: self.threshold.value() as f64,
            time_constant: self.time_constant.value() as usize,
            peak_reduction: self.peak_reduction.value() as f64,
            limit: self.limit.value(),
            sidechain_hpf_hz: self
                .sidechain_hpf_on
                .value()
                .then(|| self.sidechain_hpf.value() as f64),
            mix: self.mix.value() as f64,
            auto_gain: self.auto_gain.value(),
            oversampling: match self.oversampling.value() {
                OversamplingParam::Off => 1,
                OversamplingParam::X2 => 2,
                OversamplingParam::X4 => 4,
            },
            bypass: self.bypass.value(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Presets: instantánea de los parámetros, aplicación al host y conversión desde los ajustes
// de fábrica. Todo esto corre en el hilo de la GUI, nunca en el de audio.
// ---------------------------------------------------------------------------------------------

/// Escribe un parámetro desde un preset (si el preset lo contiene) pasando por el host, para
/// que el gesto quede registrado y se pueda automatizar/deshacer.
fn put<P: Param>(setter: &ParamSetter, param: &P, preset: &Preset, id: &str) {
    if let Some(&value) = preset.params.get(id) {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, value.clamp(0.0, 1.0));
        setter.end_set_parameter(param);
    }
}

impl FunkCompParams {
    /// Valores normalizados de todos los parámetros que forman un preset (todo menos Bypass,
    /// que es estado del canal y no del sonido). Las claves son los `#[id]` de los parámetros.
    pub fn snapshot(&self) -> BTreeMap<String, f32> {
        let mut m = BTreeMap::new();
        m.insert("mode".to_owned(), self.mode.unmodulated_normalized_value());
        m.insert(
            "input".to_owned(),
            self.input.unmodulated_normalized_value(),
        );
        m.insert(
            "output".to_owned(),
            self.output.unmodulated_normalized_value(),
        );
        m.insert(
            "attack".to_owned(),
            self.attack.unmodulated_normalized_value(),
        );
        m.insert(
            "release".to_owned(),
            self.release.unmodulated_normalized_value(),
        );
        m.insert(
            "ratio".to_owned(),
            self.ratio.unmodulated_normalized_value(),
        );
        m.insert(
            "threshold".to_owned(),
            self.threshold.unmodulated_normalized_value(),
        );
        m.insert(
            "time_const".to_owned(),
            self.time_constant.unmodulated_normalized_value(),
        );
        m.insert(
            "peak_red".to_owned(),
            self.peak_reduction.unmodulated_normalized_value(),
        );
        m.insert(
            "limit".to_owned(),
            self.limit.unmodulated_normalized_value(),
        );
        m.insert(
            "sc_hpf_on".to_owned(),
            self.sidechain_hpf_on.unmodulated_normalized_value(),
        );
        m.insert(
            "sc_hpf".to_owned(),
            self.sidechain_hpf.unmodulated_normalized_value(),
        );
        m.insert("mix".to_owned(), self.mix.unmodulated_normalized_value());
        m.insert(
            "auto_gain".to_owned(),
            self.auto_gain.unmodulated_normalized_value(),
        );
        m.insert(
            "oversampling".to_owned(),
            self.oversampling.unmodulated_normalized_value(),
        );
        m
    }

    /// Aplica un preset a los parámetros. Los que el preset no menciona quedan como están.
    pub fn apply_preset(&self, setter: &ParamSetter, preset: &Preset) {
        put(setter, &self.mode, preset, "mode");
        put(setter, &self.input, preset, "input");
        put(setter, &self.output, preset, "output");
        put(setter, &self.attack, preset, "attack");
        put(setter, &self.release, preset, "release");
        put(setter, &self.ratio, preset, "ratio");
        put(setter, &self.threshold, preset, "threshold");
        put(setter, &self.time_constant, preset, "time_const");
        put(setter, &self.peak_reduction, preset, "peak_red");
        put(setter, &self.limit, preset, "limit");
        put(setter, &self.sidechain_hpf_on, preset, "sc_hpf_on");
        put(setter, &self.sidechain_hpf, preset, "sc_hpf");
        put(setter, &self.mix, preset, "mix");
        put(setter, &self.auto_gain, preset, "auto_gain");
        put(setter, &self.oversampling, preset, "oversampling");
    }

    /// Convierte unos [`Settings`] del motor (los presets de fábrica) en un preset normalizado.
    pub fn preset_from_settings(
        &self,
        name: &str,
        category: &str,
        description: &str,
        s: &Settings,
    ) -> Preset {
        let mode = match s.mode {
            MODE_VARIMU => ModeParam::VariMu,
            MODE_OPTO3A => ModeParam::Opto3a,
            _ => ModeParam::Fet76,
        };
        let ratio = match s.ratio {
            Ratio::R4 => RatioParam::R4,
            Ratio::R8 => RatioParam::R8,
            Ratio::R12 => RatioParam::R12,
            Ratio::R20 => RatioParam::R20,
            Ratio::AllButtons => RatioParam::AllButtons,
        };
        let oversampling = match s.oversampling {
            1 => OversamplingParam::Off,
            4 => OversamplingParam::X4,
            _ => OversamplingParam::X2,
        };
        // Sin HPF de side-chain se deja la frecuencia por defecto (80 Hz).
        let hpf_hz = s.sidechain_hpf_hz.unwrap_or(80.0) as f32;

        Preset::new(PLUGIN_ID, name)
            .with_category(category)
            .with_description(description)
            .with_param("mode", self.mode.preview_normalized(mode))
            .with_param("input", self.input.preview_normalized(s.input_db as f32))
            .with_param("output", self.output.preview_normalized(s.output_db as f32))
            .with_param(
                "attack",
                self.attack.preview_normalized(s.attack_knob as f32),
            )
            .with_param(
                "release",
                self.release.preview_normalized(s.release_knob as f32),
            )
            .with_param("ratio", self.ratio.preview_normalized(ratio))
            .with_param(
                "threshold",
                self.threshold.preview_normalized(s.threshold_db as f32),
            )
            .with_param(
                "time_const",
                self.time_constant
                    .preview_normalized(s.time_constant as i32),
            )
            .with_param(
                "peak_red",
                self.peak_reduction
                    .preview_normalized(s.peak_reduction as f32),
            )
            .with_param("limit", self.limit.preview_normalized(s.limit))
            .with_param(
                "sc_hpf_on",
                self.sidechain_hpf_on
                    .preview_normalized(s.sidechain_hpf_hz.is_some()),
            )
            .with_param("sc_hpf", self.sidechain_hpf.preview_normalized(hpf_hz))
            .with_param("mix", self.mix.preview_normalized(s.mix as f32))
            .with_param("auto_gain", self.auto_gain.preview_normalized(s.auto_gain))
            .with_param(
                "oversampling",
                self.oversampling.preview_normalized(oversampling),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_covers_every_param_except_bypass_and_editor_state() {
        let params = FunkCompParams::default();
        let mut ids: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        ids.retain(|id| id.as_str() != "bypass");
        ids.sort();
        let keys: Vec<String> = params.snapshot().into_keys().collect();
        assert_eq!(keys, ids, "snapshot() y los #[id] deben coincidir");
    }

    #[test]
    fn factory_conversion_roundtrips_through_the_param_ranges() {
        let params = FunkCompParams::default();
        let s = Settings {
            mode: MODE_OPTO3A,
            output_db: 3.0,
            peak_reduction: 62.5,
            limit: true,
            mix: 0.5,
            sidechain_hpf_hz: Some(120.0),
            oversampling: 4,
            ..Settings::default()
        };
        let p = params.preset_from_settings("t", "Voice", "d", &s);
        assert_eq!(p.params.len(), params.snapshot().len());
        let n = |id: &str| p.params[id];
        assert_eq!(params.mode.preview_plain(n("mode")), ModeParam::Opto3a);
        assert_eq!(
            params.oversampling.preview_plain(n("oversampling")),
            OversamplingParam::X4
        );
        assert!((params.output.preview_plain(n("output")) - 3.0).abs() < 0.06);
        assert!((params.peak_reduction.preview_plain(n("peak_red")) - 62.5).abs() < 0.06);
        assert!((params.mix.preview_plain(n("mix")) - 0.5).abs() < 1e-3);
        assert!((params.sidechain_hpf.preview_plain(n("sc_hpf")) - 120.0).abs() < 0.5);
        assert!(params.limit.preview_plain(n("limit")));
        assert!(params.sidechain_hpf_on.preview_plain(n("sc_hpf_on")));
    }

    #[test]
    fn default_settings_map_to_the_engine_defaults() {
        let s = FunkCompParams::default().settings();
        assert_eq!(s.mode, MODE_FET76);
        assert_eq!(s.peak_reduction, 50.0);
        assert!(!s.limit);
    }
}
