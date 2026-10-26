//! Modo FET 76, inspirado en un compresor/limitador FET clásico (no es una réplica).
//!
//! * Umbral fijo: la perilla Input empuja la señal contra él, como en el hardware.
//! * Detector de pico, topología feed-back resuelta de forma implícita (ver [`crate::gain`]).
//! * Attack 20 µs–800 µs y Release 50 ms–1.1 s con perillas **invertidas** 1..7
//!   (7 = más rápido), interpolación logarítmica entre extremos.
//! * Ratios 4/8/12/20 y "todos los botones": ratio más alta, umbral más bajo, tiempos más
//!   cortos y más saturación. Es una aproximación de carácter, para afinar de oído.
//! * Saturación FET asimétrica cuya cantidad crece con la reducción de ganancia.

use crate::gain::{static_gr_db, PeakGainSmoother};
use crate::saturation::Shaper;
use crate::{CompressorMode, Ratio, Settings};
use dsp_core::db_to_lin;

/// Umbral fijo (dBFS de pico tras la ganancia de entrada).
pub const THRESHOLD_DB: f64 = -10.0;
/// Ancho del knee (dB).
pub const KNEE_DB: f64 = 6.0;
/// Nivel de referencia (dBFS) para el auto-gain.
pub const REFERENCE_DB: f64 = -18.0;

const DRIVE_BASE: f64 = 0.8;
const DRIVE_PER_DB_GR: f64 = 0.04;
const BIAS: f64 = 0.08;

/// Ataque en µs para la perilla `knob` (1 = 800 µs … 7 = 20 µs).
pub fn attack_time_us(knob: f64) -> f64 {
    let t = ((knob - 1.0) / 6.0).clamp(0.0, 1.0);
    800.0 * (20.0f64 / 800.0).powf(t)
}

/// Release en ms para la perilla `knob` (1 = 1100 ms … 7 = 50 ms).
pub fn release_time_ms(knob: f64) -> f64 {
    let t = ((knob - 1.0) / 6.0).clamp(0.0, 1.0);
    1100.0 * (50.0f64 / 1100.0).powf(t)
}

pub struct Fet76 {
    sample_rate: f64,
    ratio: f64,
    threshold_db: f64,
    /// Nivel lineal bajo el cual no hay reducción (evita un `log10` por muestra en silencio).
    floor_lin: f64,
    drive_scale: f64,
    smoother: PeakGainSmoother,
}

impl Fet76 {
    pub fn new() -> Self {
        Self {
            sample_rate: 44_100.0,
            ratio: 4.0,
            threshold_db: THRESHOLD_DB,
            floor_lin: db_to_lin(THRESHOLD_DB - KNEE_DB * 0.5),
            drive_scale: 1.0,
            smoother: PeakGainSmoother::new(),
        }
    }

    /// Reducción de ganancia actual (dB), para medidores.
    pub fn current_gr_db(&self) -> f64 {
        self.smoother.value()
    }
}

impl Default for Fet76 {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressorMode for Fet76 {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn configure(&mut self, s: &Settings) {
        let (ratio, threshold, atk_scale, rel_scale, drive_scale) = match s.ratio {
            Ratio::R4 => (4.0, THRESHOLD_DB, 1.0, 1.0, 1.0),
            Ratio::R8 => (8.0, THRESHOLD_DB, 1.0, 1.0, 1.0),
            Ratio::R12 => (12.0, THRESHOLD_DB, 1.0, 1.0, 1.0),
            Ratio::R20 => (20.0, THRESHOLD_DB, 1.0, 1.0, 1.0),
            Ratio::AllButtons => (30.0, THRESHOLD_DB - 3.0, 0.6, 0.7, 1.5),
        };
        self.ratio = ratio;
        self.threshold_db = threshold;
        self.floor_lin = db_to_lin(threshold - KNEE_DB * 0.5);
        self.drive_scale = drive_scale;
        self.smoother.configure(
            self.sample_rate,
            attack_time_us(s.attack_knob) * 1e-6 * atk_scale,
            release_time_ms(s.release_knob) * 1e-3 * rel_scale,
            ratio,
        );
    }

    fn reset(&mut self) {
        self.smoother.reset();
    }

    fn warm_start(&mut self, gain_reduction_db: f64) {
        self.smoother.set_value(gain_reduction_db);
    }

    #[inline]
    fn gain_reduction_db(&mut self, detector_level: f64) -> f64 {
        let target = if detector_level <= self.floor_lin {
            0.0
        } else {
            let level_db = 20.0 * detector_level.log10();
            static_gr_db(level_db, self.threshold_db, self.ratio, KNEE_DB)
        };
        self.smoother.process(target)
    }

    #[inline]
    fn shaper(&self, gain_reduction_db: f64) -> Shaper {
        let drive = DRIVE_BASE * (1.0 + DRIVE_PER_DB_GR * gain_reduction_db) * self.drive_scale;
        Shaper::new(drive, BIAS)
    }

    fn auto_gain_db(&self, input_db: f64) -> f64 {
        let gr_at_reference = static_gr_db(
            REFERENCE_DB + input_db,
            self.threshold_db,
            self.ratio,
            KNEE_DB,
        );
        -input_db + gr_at_reference
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knob_mapping_hits_the_documented_extremes_and_is_inverted() {
        assert!((attack_time_us(1.0) - 800.0).abs() < 1e-9);
        assert!((attack_time_us(7.0) - 20.0).abs() < 1e-9);
        assert!((release_time_ms(1.0) - 1100.0).abs() < 1e-9);
        assert!((release_time_ms(7.0) - 50.0).abs() < 1e-9);
        let mut prev = f64::MAX;
        for i in 0..=60 {
            let t = attack_time_us(1.0 + i as f64 * 0.1);
            assert!(t < prev);
            prev = t;
        }
    }

    #[test]
    fn auto_gain_removes_input_boost_and_adds_back_static_reduction() {
        let mut m = Fet76::new();
        m.prepare(48_000.0);
        m.configure(&Settings::default());
        // Entrada bajo el knee: sólo se resta el boost de entrada.
        assert!((m.auto_gain_db(0.0) - 0.0).abs() < 0.2);
        // Mucha entrada: -input + GR(ref+input) = -input + 0.75·(ref+input−T) para ratio 4.
        let comp = m.auto_gain_db(30.0);
        let expected = -30.0 + 0.75 * (REFERENCE_DB + 30.0 - THRESHOLD_DB);
        assert!((comp - expected).abs() < 1e-9, "{comp} vs {expected}");
    }

    #[test]
    fn shaper_gets_more_aggressive_with_gain_reduction() {
        let m = Fet76::new();
        let (lo, hi) = (m.shaper(0.0), m.shaper(15.0));
        // A nivel alto la señal se aplana más con más drive.
        assert!(hi.apply(0.9) < lo.apply(0.9));
    }
}
