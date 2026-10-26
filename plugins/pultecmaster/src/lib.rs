//! PultecMaster — EQ pasivo con etapa de tubo, inspirado en un ecualizador clásico de graves y
//! agudos (no es una réplica).
//!
//! Este crate es sólo el *glue* con el host: parámetros ([`params`]), GUI ([`gui`]) y el ciclo
//! `initialize/process`. Todo el DSP vive en `pultec-dsp`.

mod gui;
mod params;

use nih_plug::prelude::*;
use params::PultecParams;
use pultec_dsp::Engine;
use std::sync::Arc;

pub struct PultecMaster {
    params: Arc<PultecParams>,
    engine: Engine,
    last_latency: u32,
}

impl Default for PultecMaster {
    fn default() -> Self {
        Self {
            params: Arc::new(PultecParams::default()),
            engine: Engine::new(),
            last_latency: 0,
        }
    }
}

impl Plugin for PultecMaster {
    const NAME: &'static str = "PultecMaster";
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
        gui::create(self.params.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.engine.prepare(buffer_config.sample_rate as f64);
        self.engine.set_settings(&self.params.settings());
        self.last_latency = self.engine.latency_samples();
        context.set_latency_samples(self.last_latency);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.engine.set_settings(&self.params.settings());

        // El factor de oversampling cambia la latencia: avisar al host al instante.
        let latency = self.engine.latency_samples();
        if latency != self.last_latency {
            context.set_latency_samples(latency);
            self.last_latency = latency;
        }

        self.engine.process_block(buffer.as_slice());
        ProcessStatus::Normal
    }
}

impl ClapPlugin for PultecMaster {
    const CLAP_ID: &'static str = "com.analog-suite.pultecmaster";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("EQ pasivo de tubos para graves y agudos, inspirado en un clásico");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Equalizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

nih_export_clap!(PultecMaster);
