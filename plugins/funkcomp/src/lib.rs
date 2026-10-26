//! FunkComp — compresor multimodo para voces de funk, soul y R&B.
//!
//! Este crate es sólo el *glue* con el host: parámetros ([`params`]), GUI ([`gui`]) y el
//! ciclo `initialize/process`. Todo el DSP vive en `funkcomp-dsp`.
//!
//! Modos FET 76, VARI-MU 660 y OPTO 3A; extras comunes (side-chain HPF, Mix paralelo,
//! auto-gain, oversampling, bypass sin clicks), latencia reportada al host y navegador de
//! presets ([`browser`]) con presets de fábrica y de usuario.

mod browser;
mod gui;
mod params;

use dsp_core::meter::AtomicF32;
use funkcomp_dsp::Engine;
use nih_plug::prelude::*;
use params::FunkCompParams;
use std::sync::Arc;

/// Caída del medidor de GR en la GUI (constante de tiempo, segundos).
const GR_METER_FALL_S: f64 = 0.25;

pub struct FunkComp {
    params: Arc<FunkCompParams>,
    engine: Engine,
    sample_rate: f64,
    last_latency: u32,
    /// Reducción de ganancia mostrada (dB); se comparte con la GUI sin locks.
    gr_display: f64,
    gr_meter: Arc<AtomicF32>,
}

impl Default for FunkComp {
    fn default() -> Self {
        Self {
            params: Arc::new(FunkCompParams::default()),
            engine: Engine::new(),
            sample_rate: 44_100.0,
            last_latency: 0,
            gr_display: 0.0,
            gr_meter: Arc::new(AtomicF32::new(0.0)),
        }
    }
}

impl Plugin for FunkComp {
    const NAME: &'static str = "FunkComp";
    const VENDOR: &'static str = "analog-suite";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gui::create(self.params.clone(), self.gr_meter.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.engine.prepare(self.sample_rate);
        self.engine.set_settings(&self.params.settings());
        self.last_latency = self.engine.latency_samples();
        context.set_latency_samples(self.last_latency);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.gr_display = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let samples = buffer.samples();

        self.engine.set_settings(&self.params.settings());

        // El factor de oversampling cambia la latencia: avisar al host al instante.
        let latency = self.engine.latency_samples();
        if latency != self.last_latency {
            context.set_latency_samples(latency);
            self.last_latency = latency;
        }

        self.engine.process_block(buffer.as_slice());

        if self.params.editor_state.is_open() {
            let fall = (-(samples as f64) / (GR_METER_FALL_S * self.sample_rate)).exp();
            self.gr_display = self.engine.block_gr_db().max(self.gr_display * fall);
            self.gr_meter.store(self.gr_display as f32);
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FunkComp {
    const CLAP_ID: &'static str = "com.analog-suite.funkcomp";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Compresor multimodo para voces de funk, soul y R&B");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Compressor,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

nih_export_clap!(FunkComp);
