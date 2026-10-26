//! Biquads RBJ en `f64` (forma transpuesta directa II).

use crate::denormal;
use std::f64::consts::PI;

/// Coeficientes normalizados (`a0 = 1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    pub const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn normalized(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        let inv = 1.0 / a0;
        Self {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
        }
    }

    fn trig(sample_rate: f64, freq: f64, q: f64) -> (f64, f64, f64) {
        // Limitar la frecuencia evita inestabilidad al acercarse a Nyquist.
        let f = freq.clamp(1.0, sample_rate * 0.499);
        let w0 = 2.0 * PI * f / sample_rate;
        let (sin, cos) = w0.sin_cos();
        (cos, sin, sin / (2.0 * q.max(1e-3)))
    }

    pub fn lowpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (cos, _, alpha) = Self::trig(sample_rate, freq, q);
        Self::normalized(
            (1.0 - cos) * 0.5,
            1.0 - cos,
            (1.0 - cos) * 0.5,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    pub fn highpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (cos, _, alpha) = Self::trig(sample_rate, freq, q);
        Self::normalized(
            (1.0 + cos) * 0.5,
            -(1.0 + cos),
            (1.0 + cos) * 0.5,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    /// Campana (peaking EQ).
    pub fn bell(sample_rate: f64, freq: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let (cos, _, alpha) = Self::trig(sample_rate, freq, q);
        Self::normalized(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    pub fn low_shelf(sample_rate: f64, freq: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let (cos, _, alpha) = Self::trig(sample_rate, freq, q);
        let t = 2.0 * a.sqrt() * alpha;
        Self::normalized(
            a * ((a + 1.0) - (a - 1.0) * cos + t),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - t),
            (a + 1.0) + (a - 1.0) * cos + t,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - t,
        )
    }

    pub fn high_shelf(sample_rate: f64, freq: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let (cos, _, alpha) = Self::trig(sample_rate, freq, q);
        let t = 2.0 * a.sqrt() * alpha;
        Self::normalized(
            a * ((a + 1.0) + (a - 1.0) * cos + t),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - t),
            (a + 1.0) - (a - 1.0) * cos + t,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - t,
        )
    }

    /// Módulo de la respuesta en frecuencia (lineal) en `freq` Hz.
    /// Útil para tests y para dibujar curvas en la GUI.
    pub fn magnitude(&self, sample_rate: f64, freq: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
    }
}

/// Filtro biquad con estado (un canal).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    c: BiquadCoeffs,
    z1: f64,
    z2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new(BiquadCoeffs::IDENTITY)
    }
}

impl Biquad {
    pub fn new(c: BiquadCoeffs) -> Self {
        Self {
            c,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    pub fn set_coeffs(&mut self, c: BiquadCoeffs) {
        self.c = c;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = denormal::flush(self.c.b1 * x - self.c.a1 * y + self.z2);
        self.z2 = denormal::flush(self.c.b2 * x - self.c.a2 * y);
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    #[test]
    fn lowpass_is_minus_3db_at_cutoff_for_butterworth_q() {
        let c = BiquadCoeffs::lowpass(FS, 1000.0, std::f64::consts::FRAC_1_SQRT_2);
        assert!((db(c.magnitude(FS, 1000.0)) + 3.0103).abs() < 0.01);
        assert!((c.magnitude(FS, 1.0) - 1.0).abs() < 1e-3);
        assert!(db(c.magnitude(FS, 10_000.0)) < -35.0);
    }

    #[test]
    fn highpass_blocks_dc() {
        let c = BiquadCoeffs::highpass(FS, 100.0, 0.707);
        assert!(c.magnitude(FS, 1.0) < 1e-4);
        assert!((c.magnitude(FS, 10_000.0) - 1.0).abs() < 1e-2);
    }

    #[test]
    fn bell_hits_requested_gain_at_center() {
        for gain in [-12.0, -3.0, 6.0, 12.0] {
            let c = BiquadCoeffs::bell(FS, 2000.0, 1.0, gain);
            assert!((db(c.magnitude(FS, 2000.0)) - gain).abs() < 1e-6);
        }
    }

    #[test]
    fn shelves_reach_gain_in_their_band() {
        let ls = BiquadCoeffs::low_shelf(FS, 200.0, 0.707, 6.0);
        assert!((db(ls.magnitude(FS, 1.0)) - 6.0).abs() < 0.05);
        assert!(db(ls.magnitude(FS, 15_000.0)).abs() < 0.05);
        let hs = BiquadCoeffs::high_shelf(FS, 5000.0, 0.707, -6.0);
        assert!((db(hs.magnitude(FS, 20_000.0)) + 6.0).abs() < 0.2);
        assert!(db(hs.magnitude(FS, 20.0)).abs() < 0.05);
    }

    #[test]
    fn time_domain_matches_analytic_magnitude() {
        let freq = 3000.0;
        let c = BiquadCoeffs::bell(FS, 3000.0, 2.0, 9.0);
        let mut f = Biquad::new(c);
        let n = 20_000;
        let mut peak: f64 = 0.0;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f64 / FS).sin();
            let y = f.process(x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - c.magnitude(FS, freq)).abs() < 1e-3, "peak = {peak}");
    }

    #[test]
    fn state_decays_without_denormals() {
        let mut f = Biquad::new(BiquadCoeffs::lowpass(FS, 100.0, 0.707));
        f.process(1.0);
        for _ in 0..2_000_000 {
            f.process(0.0);
        }
        assert_eq!(f.z1, 0.0);
        assert_eq!(f.z2, 0.0);
    }
}
