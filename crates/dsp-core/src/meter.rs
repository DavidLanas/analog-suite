//! Medidores y valor atómico para compartir lecturas con la GUI sin locks.

use crate::{denormal, lin_to_db};
use std::sync::atomic::{AtomicU32, Ordering};

/// `f32` atómico (para pasar lecturas del hilo de audio a la GUI sin locks).
#[derive(Debug)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    #[inline]
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

impl Default for AtomicF32 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// Medidor de pico con caída exponencial.
#[derive(Clone, Copy, Debug)]
pub struct PeakMeter {
    value: f64,
    release: f64,
}

impl PeakMeter {
    pub fn new(sample_rate: f64, release_ms: f64) -> Self {
        Self {
            value: 0.0,
            release: crate::envelope::time_to_coeff(sample_rate, release_ms),
        }
    }

    #[inline]
    pub fn push(&mut self, x: f64) {
        let a = x.abs();
        self.value = if a > self.value {
            a
        } else {
            denormal::flush(a + self.release * (self.value - a))
        };
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }

    #[inline]
    pub fn linear(&self) -> f64 {
        self.value
    }

    #[inline]
    pub fn db(&self) -> f64 {
        lin_to_db(self.value)
    }
}

/// Vúmetro con balística VU: integra el valor rectificado (≈ 300 ms al 99 %).
///
/// Referencia: 0 VU = `reference_dbfs` (típico −18 dBFS). El factor 1.1107 compensa que el
/// promedio rectificado de un seno es 0.9003 de su valor RMS.
#[derive(Clone, Copy, Debug)]
pub struct VuMeter {
    avg: f64,
    coeff: f64,
    reference_db: f64,
}

impl VuMeter {
    pub fn new(sample_rate: f64, reference_dbfs: f64) -> Self {
        // Tiempo de subida al 99 % = 300 ms  =>  tau ≈ 65 ms.
        let tau_ms = 300.0 / (100f64).ln();
        Self {
            avg: 0.0,
            coeff: crate::envelope::time_to_coeff(sample_rate, tau_ms),
            reference_db: reference_dbfs,
        }
    }

    #[inline]
    pub fn push(&mut self, x: f64) {
        let a = x.abs();
        self.avg = denormal::flush(a + self.coeff * (self.avg - a));
    }

    pub fn reset(&mut self) {
        self.avg = 0.0;
    }

    /// Lectura en VU (0 VU = nivel de referencia).
    #[inline]
    pub fn vu(&self) -> f64 {
        lin_to_db(self.avg * 1.1107) - self.reference_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const FS: f64 = 48_000.0;

    #[test]
    fn atomic_roundtrip() {
        let a = AtomicF32::new(1.5);
        assert_eq!(a.load(), 1.5);
        a.store(-3.25);
        assert_eq!(a.load(), -3.25);
    }

    #[test]
    fn peak_meter_holds_and_decays() {
        let mut m = PeakMeter::new(FS, 100.0);
        m.push(0.8);
        assert_eq!(m.linear(), 0.8);
        for _ in 0..(0.1 * FS) as usize {
            m.push(0.0);
        }
        assert!((m.linear() - 0.8 * (-1f64).exp()).abs() < 0.01);
    }

    #[test]
    fn vu_reads_zero_for_reference_sine() {
        // Seno con RMS = -18 dBFS  ->  0 VU
        let rms = 10f64.powf(-18.0 / 20.0);
        let amp = rms * 2f64.sqrt();
        let mut m = VuMeter::new(FS, -18.0);
        for i in 0..(FS as usize) {
            m.push(amp * (2.0 * PI * 1000.0 * i as f64 / FS).sin());
        }
        assert!(m.vu().abs() < 0.1, "vu = {}", m.vu());
    }
}
