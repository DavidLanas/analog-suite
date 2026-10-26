//! Parámetros de PultecMaster y su traducción a los ajustes del motor.
//!
//! El suavizado vive en el motor (en `f64`), así que los `FloatParam` no usan el de nih-plug.

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use presets::Preset;
use pultec_dsp::factory;
use pultec_dsp::network::{
    bandwidth_to_q, HIGH_ATTEN_MAX_DB, HIGH_BOOST_MAX_DB, LOW_ATTEN_MAX_DB, LOW_BOOST_MAX_DB,
};
use pultec_dsp::Settings;
use std::collections::BTreeMap;
use std::sync::Arc;
use ui_common::preset_browser::PresetParams;

/// Identificador del plugin en los archivos de preset.
pub const PLUGIN_ID: &str = "pultecmaster";

pub const EDITOR_SIZE: (u32, u32) = (880, 540);

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowFreqParam {
    #[id = "f20"]
    #[name = "20 Hz"]
    F20,
    #[id = "f30"]
    #[name = "30 Hz"]
    F30,
    #[id = "f60"]
    #[name = "60 Hz"]
    F60,
    #[id = "f100"]
    #[name = "100 Hz"]
    F100,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighFreqParam {
    #[id = "k3"]
    #[name = "3 kHz"]
    K3,
    #[id = "k4"]
    #[name = "4 kHz"]
    K4,
    #[id = "k5"]
    #[name = "5 kHz"]
    K5,
    #[id = "k8"]
    #[name = "8 kHz"]
    K8,
    #[id = "k10"]
    #[name = "10 kHz"]
    K10,
    #[id = "k12"]
    #[name = "12 kHz"]
    K12,
    #[id = "k16"]
    #[name = "16 kHz"]
    K16,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttenFreqParam {
    #[id = "k5"]
    #[name = "5 kHz"]
    K5,
    #[id = "k10"]
    #[name = "10 kHz"]
    K10,
    #[id = "k20"]
    #[name = "20 kHz"]
    K20,
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
pub struct PultecParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "bypass"]
    pub bypass: BoolParam,

    #[id = "low_freq"]
    pub low_freq: EnumParam<LowFreqParam>,
    #[id = "low_boost"]
    pub low_boost: FloatParam,
    #[id = "low_atten"]
    pub low_atten: FloatParam,

    #[id = "high_freq"]
    pub high_freq: EnumParam<HighFreqParam>,
    #[id = "high_boost"]
    pub high_boost: FloatParam,
    /// 0 = agudo, 10 = ancho.
    #[id = "bandwidth"]
    pub bandwidth: FloatParam,

    #[id = "atten_freq"]
    pub atten_freq: EnumParam<AttenFreqParam>,
    #[id = "high_atten"]
    pub high_atten: FloatParam,

    /// Saturación de la etapa de tubo; 0 = lineal.
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "output"]
    pub output: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "oversampling"]
    pub oversampling: EnumParam<OversamplingParam>,
}

/// Perilla 0..10 con el valor en dB que produce: "6.0 · +8.1 dB".
fn knob(name: &str, default: f32, max_db: f64, sign: f64) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Linear {
            min: 0.0,
            max: 10.0,
        },
    )
    .with_step_size(0.1)
    .with_value_to_string(Arc::new(move |v: f32| {
        format!("{v:.1} · {:+.1} dB", sign * v as f64 / 10.0 * max_db)
    }))
}

impl Default for PultecParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(EDITOR_SIZE.0, EDITOR_SIZE.1),
            bypass: BoolParam::new("Bypass", false).make_bypass(),

            low_freq: EnumParam::new("Low Freq", LowFreqParam::F60),
            low_boost: knob("Low Boost", 0.0, LOW_BOOST_MAX_DB, 1.0),
            low_atten: knob("Low Atten", 0.0, LOW_ATTEN_MAX_DB, -1.0),

            high_freq: EnumParam::new("Boost Freq", HighFreqParam::K10),
            high_boost: knob("High Boost", 0.0, HIGH_BOOST_MAX_DB, 1.0),
            bandwidth: FloatParam::new(
                "Bandwidth",
                5.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_step_size(0.1)
            .with_value_to_string(Arc::new(|v: f32| {
                format!("{v:.1} · Q {:.2}", bandwidth_to_q(v as f64))
            })),

            atten_freq: EnumParam::new("Atten Freq", AttenFreqParam::K10),
            high_atten: knob("High Atten", 0.0, HIGH_ATTEN_MAX_DB, -1.0),

            drive: FloatParam::new(
                "Tube Drive",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_step_size(0.1)
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            output: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            oversampling: EnumParam::new("Oversampling", OversamplingParam::X2),
        }
    }
}

impl PultecParams {
    /// Instantánea de los parámetros en el formato del motor.
    pub fn settings(&self) -> Settings {
        Settings {
            low_freq: self.low_freq.value() as usize,
            low_boost: self.low_boost.value() as f64,
            low_atten: self.low_atten.value() as f64,
            high_freq: self.high_freq.value() as usize,
            high_boost: self.high_boost.value() as f64,
            bandwidth: self.bandwidth.value() as f64,
            atten_freq: self.atten_freq.value() as usize,
            high_atten: self.high_atten.value() as f64,
            drive: self.drive.value() as f64,
            output_db: self.output.value() as f64,
            mix: self.mix.value() as f64,
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
// Presets (hilo de la GUI, nunca el de audio).
// ---------------------------------------------------------------------------------------------

/// Posición normalizada de la opción `index` de un parámetro enumerado de `count` opciones.
fn enum_norm(index: usize, count: usize) -> f32 {
    index as f32 / (count.max(2) - 1) as f32
}

fn put<P: Param>(setter: &ParamSetter, param: &P, preset: &Preset, id: &str) {
    if let Some(&value) = preset.params.get(id) {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, value.clamp(0.0, 1.0));
        setter.end_set_parameter(param);
    }
}

impl PultecParams {
    /// Valores normalizados de todos los parámetros de un preset (todo menos Bypass).
    pub fn snapshot(&self) -> BTreeMap<String, f32> {
        let mut m = BTreeMap::new();
        let mut add = |id: &str, v: f32| {
            m.insert(id.to_owned(), v);
        };
        add("low_freq", self.low_freq.unmodulated_normalized_value());
        add("low_boost", self.low_boost.unmodulated_normalized_value());
        add("low_atten", self.low_atten.unmodulated_normalized_value());
        add("high_freq", self.high_freq.unmodulated_normalized_value());
        add("high_boost", self.high_boost.unmodulated_normalized_value());
        add("bandwidth", self.bandwidth.unmodulated_normalized_value());
        add("atten_freq", self.atten_freq.unmodulated_normalized_value());
        add("high_atten", self.high_atten.unmodulated_normalized_value());
        add("drive", self.drive.unmodulated_normalized_value());
        add("output", self.output.unmodulated_normalized_value());
        add("mix", self.mix.unmodulated_normalized_value());
        add(
            "oversampling",
            self.oversampling.unmodulated_normalized_value(),
        );
        m
    }

    pub fn apply_preset(&self, setter: &ParamSetter, preset: &Preset) {
        put(setter, &self.low_freq, preset, "low_freq");
        put(setter, &self.low_boost, preset, "low_boost");
        put(setter, &self.low_atten, preset, "low_atten");
        put(setter, &self.high_freq, preset, "high_freq");
        put(setter, &self.high_boost, preset, "high_boost");
        put(setter, &self.bandwidth, preset, "bandwidth");
        put(setter, &self.atten_freq, preset, "atten_freq");
        put(setter, &self.high_atten, preset, "high_atten");
        put(setter, &self.drive, preset, "drive");
        put(setter, &self.output, preset, "output");
        put(setter, &self.mix, preset, "mix");
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
        let oversampling = match s.oversampling {
            1 => OversamplingParam::Off,
            4 => OversamplingParam::X4,
            _ => OversamplingParam::X2,
        };
        Preset::new(PLUGIN_ID, name)
            .with_category(category)
            .with_description(description)
            .with_param("low_freq", enum_norm(s.low_freq, 4))
            .with_param(
                "low_boost",
                self.low_boost.preview_normalized(s.low_boost as f32),
            )
            .with_param(
                "low_atten",
                self.low_atten.preview_normalized(s.low_atten as f32),
            )
            .with_param("high_freq", enum_norm(s.high_freq, 7))
            .with_param(
                "high_boost",
                self.high_boost.preview_normalized(s.high_boost as f32),
            )
            .with_param(
                "bandwidth",
                self.bandwidth.preview_normalized(s.bandwidth as f32),
            )
            .with_param("atten_freq", enum_norm(s.atten_freq, 3))
            .with_param(
                "high_atten",
                self.high_atten.preview_normalized(s.high_atten as f32),
            )
            .with_param("drive", self.drive.preview_normalized(s.drive as f32))
            .with_param("output", self.output.preview_normalized(s.output_db as f32))
            .with_param("mix", self.mix.preview_normalized(s.mix as f32))
            .with_param(
                "oversampling",
                self.oversampling.preview_normalized(oversampling),
            )
    }

    /// Los presets de fábrica del motor, convertidos a valores normalizados.
    pub fn factory_presets(&self) -> Vec<Preset> {
        factory::presets()
            .iter()
            .map(|f| {
                self.preset_from_settings(f.name, f.category.as_str(), f.description, &f.settings)
            })
            .collect()
    }
}

impl PresetParams for PultecParams {
    fn snapshot(&self) -> BTreeMap<String, f32> {
        PultecParams::snapshot(self)
    }

    fn apply_preset(&self, setter: &ParamSetter, preset: &Preset) {
        PultecParams::apply_preset(self, setter, preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pultec_dsp::network::{ATTEN_FREQS, HIGH_FREQS, LOW_FREQS};

    #[test]
    fn default_params_are_flat_and_linear() {
        let s = PultecParams::default().settings();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn enum_order_matches_the_engine_frequency_tables() {
        let p = PultecParams::default();
        assert_eq!(LowFreqParam::F100 as usize, LOW_FREQS.len() - 1);
        assert_eq!(HighFreqParam::K16 as usize, HIGH_FREQS.len() - 1);
        assert_eq!(AttenFreqParam::K20 as usize, ATTEN_FREQS.len() - 1);
        // La posición normalizada de cada opción devuelve la variante esperada.
        for i in 0..LOW_FREQS.len() {
            assert_eq!(p.low_freq.preview_plain(enum_norm(i, 4)) as usize, i);
        }
        for i in 0..HIGH_FREQS.len() {
            assert_eq!(p.high_freq.preview_plain(enum_norm(i, 7)) as usize, i);
        }
        for i in 0..ATTEN_FREQS.len() {
            assert_eq!(p.atten_freq.preview_plain(enum_norm(i, 3)) as usize, i);
        }
    }

    #[test]
    fn snapshot_covers_every_param_except_bypass_and_editor_state() {
        let params = PultecParams::default();
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
    fn factory_presets_are_complete_and_roundtrip_through_the_param_ranges() {
        let params = PultecParams::default();
        let list = params.factory_presets();
        let engine = factory::presets();
        assert_eq!(list.len(), engine.len());
        for (p, f) in list.iter().zip(&engine) {
            assert_eq!(p.params.len(), params.snapshot().len(), "{}", p.name);
            assert!(p.params.values().all(|v| (0.0..=1.0).contains(v)));
            let n = |id: &str| p.params[id];
            let s = &f.settings;
            assert_eq!(
                params.low_freq.preview_plain(n("low_freq")) as usize,
                s.low_freq
            );
            assert_eq!(
                params.high_freq.preview_plain(n("high_freq")) as usize,
                s.high_freq
            );
            assert_eq!(
                params.atten_freq.preview_plain(n("atten_freq")) as usize,
                s.atten_freq
            );
            for (id, param, want) in [
                ("low_boost", &params.low_boost, s.low_boost),
                ("low_atten", &params.low_atten, s.low_atten),
                ("high_boost", &params.high_boost, s.high_boost),
                ("bandwidth", &params.bandwidth, s.bandwidth),
                ("high_atten", &params.high_atten, s.high_atten),
                ("drive", &params.drive, s.drive),
            ] {
                let got = param.preview_plain(n(id)) as f64;
                assert!(
                    (got - want).abs() < 0.06,
                    "{} {id}: {got} vs {want}",
                    p.name
                );
            }
        }
    }
}
