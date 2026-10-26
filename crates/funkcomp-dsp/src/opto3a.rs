//! Modo OPTO 3A, inspirado en un compresor/limitador óptico clásico (no es una réplica).
//!
//! * **Peak Reduction** (0–100) fija la sensibilidad del side-chain: sube la reducción para el
//!   mismo nivel de entrada, desplazando el umbral ([`threshold_db`]). No hay control Input.
//! * **Compress / Limit**: dos curvas. Compress es suave (3:1, knee ancho); Limit es más
//!   dura (12:1, knee corto).
//! * **Célula óptica con memoria**: la reducción sigue al detector con un ataque de ~10 ms
//!   (la célula tarda en "encenderse") y el release tiene **dos etapas**: tras una señal
//!   sostenida cae rápido (~60 ms para el primer 50 %) y el resto se suelta despacio (unos
//!   segundos); tras un pico aislado se suelta todo rápido. La etapa lenta es la "memoria" de
//!   la célula: se carga con lo que lleva comprimiendo y se descarga despacio
//!   ([`TwoStageSmoother::set_slow_fraction`](crate::gain::TwoStageSmoother::set_slow_fraction)).
//! * **Detector**: seguidor de pico con caída de 8 ms antes de la curva, para que la reducción
//!   siga a la envolvente y no a cada ciclo de la onda.
//! * Saturación muy ligera (etapa de salida casi lineal).

use crate::gain::{static_gr_db, TwoStageSmoother};
use crate::saturation::Shaper;
use crate::{CompressorMode, Settings};
use dsp_core::db_to_lin;
use dsp_core::envelope::EnvelopeFollower;

/// Nivel de referencia (dBFS) para el auto-gain.
pub const REFERENCE_DB: f64 = -18.0;
/// Ataque de la célula (s).
pub const ATTACK_S: f64 = 0.010;
/// Constante de tiempo de la etapa rápida: 50 % en 60 ms => τ = 60 ms / ln 2.
pub const RELEASE_FAST_S: f64 = 0.060 / std::f64::consts::LN_2;
/// Constante de tiempo de la etapa lenta (memoria) (s).
pub const RELEASE_SLOW_S: f64 = 3.0;
/// Constante de carga de la memoria (s).
pub const MEMORY_CHARGE_S: f64 = 1.2;
/// Fracción de la reducción que se "recuerda" en la etapa lenta.
pub const MEMORY_FRACTION: f64 = 0.5;

/// Ratio y ancho del knee de cada interruptor.
pub const COMPRESS: (f64, f64) = (3.0, 12.0);
pub const LIMIT: (f64, f64) = (12.0, 6.0);

/// Umbral (dB) para un valor de Peak Reduction 0..=100: 0 → 0 dBFS, 100 → −40 dBFS.
pub fn threshold_db(peak_reduction: f64) -> f64 {
    -0.4 * peak_reduction.clamp(0.0, 100.0)
}

/// Reducción estática (dB) para un nivel de entrada, Peak Reduction e interruptor.
pub fn static_reduction_db(level_db: f64, peak_reduction: f64, limit: bool) -> f64 {
    let (ratio, knee) = if limit { LIMIT } else { COMPRESS };
    static_gr_db(level_db, threshold_db(peak_reduction), ratio, knee)
}

pub struct Opto3a {
    sample_rate: f64,
    peak_reduction: f64,
    limit: bool,
    floor_lin: f64,
    env: EnvelopeFollower,
    smoother: TwoStageSmoother,
}

impl Opto3a {
    pub fn new() -> Self {
        let mut m = Self {
            sample_rate: 44_100.0,
            peak_reduction: 50.0,
            limit: false,
            floor_lin: 0.0,
            env: EnvelopeFollower::new(),
            smoother: TwoStageSmoother::new(),
        };
        m.smoother.set_slow_fraction(MEMORY_FRACTION);
        m
    }

    fn knee(&self) -> f64 {
        if self.limit {
            LIMIT.1
        } else {
            COMPRESS.1
        }
    }
}

impl Default for Opto3a {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressorMode for Opto3a {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.env.set_times_ms(sample_rate, 0.05, 8.0);
    }

    fn configure(&mut self, s: &Settings) {
        self.peak_reduction = s.peak_reduction;
        self.limit = s.limit;
        self.floor_lin = db_to_lin(threshold_db(s.peak_reduction) - self.knee() * 0.5);
        self.smoother.configure(
            self.sample_rate,
            ATTACK_S,
            RELEASE_FAST_S,
            Some(RELEASE_SLOW_S),
            MEMORY_CHARGE_S,
        );
    }

    fn reset(&mut self) {
        self.env.reset();
        self.smoother.reset();
    }

    fn warm_start(&mut self, gain_reduction_db: f64) {
        self.smoother.set_value(gain_reduction_db);
    }

    #[inline]
    fn gain_reduction_db(&mut self, detector_level: f64) -> f64 {
        let level = self.env.process(detector_level);
        let target = if level <= self.floor_lin {
            0.0
        } else {
            static_reduction_db(20.0 * level.log10(), self.peak_reduction, self.limit)
        };
        self.smoother.process(target)
    }

    #[inline]
    fn shaper(&self, _gain_reduction_db: f64) -> Shaper {
        Shaper::new(0.6, 0.04)
    }

    fn auto_gain_db(&self, _input_db: f64) -> f64 {
        static_reduction_db(REFERENCE_DB, self.peak_reduction, self.limit)
    }

    fn uses_input(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_reduction_lowers_the_threshold_monotonically() {
        assert_eq!(threshold_db(0.0), 0.0);
        assert_eq!(threshold_db(100.0), -40.0);
        assert_eq!(threshold_db(250.0), -40.0);
        let mut prev = 1.0;
        for pr in 0..=100 {
            let t = threshold_db(pr as f64);
            assert!(t <= prev);
            prev = t;
        }
    }

    #[test]
    fn more_peak_reduction_means_more_reduction_for_the_same_level() {
        let mut prev = -1.0;
        for pr in (0..=100).step_by(10) {
            let gr = static_reduction_db(-10.0, pr as f64, false);
            assert!(gr >= prev - 1e-12, "pr {pr}: {gr} < {prev}");
            prev = gr;
        }
        assert!(prev > 8.0);
    }

    #[test]
    fn limit_is_harder_than_compress_well_above_the_threshold() {
        let (c, l) = (
            static_reduction_db(-5.0, 60.0, false),
            static_reduction_db(-5.0, 60.0, true),
        );
        assert!(l > c + 3.0, "compress {c}, limit {l}");
        // Muy por debajo del umbral ninguno comprime.
        assert_eq!(static_reduction_db(-80.0, 60.0, true), 0.0);
    }

    #[test]
    fn fast_stage_constant_gives_a_60_ms_half_life() {
        // El primer 50 % se suelta en 60 ms.
        let half = (-(0.060) / RELEASE_FAST_S).exp();
        assert!((half - 0.5).abs() < 1e-12);
    }

    #[test]
    fn mode_has_no_input_control_and_neutral_auto_gain_at_low_peak_reduction() {
        let mut m = Opto3a::new();
        m.prepare(48_000.0);
        m.configure(&Settings {
            peak_reduction: 0.0,
            ..Settings::default()
        });
        assert!(!m.uses_input());
        assert_eq!(m.auto_gain_db(12.0), 0.0);
    }
}
