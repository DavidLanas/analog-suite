//! Suavizado de parámetros y crossfade de bypass.

use crate::denormal;

/// Suavizador exponencial de un polo (`current` persigue a `target`).
///
/// Buen defecto para ganancias y mezclas: sin zipper noise, coste de una MAC por muestra.
#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    current: f64,
    target: f64,
    coeff: f64,
}

impl Smoother {
    pub fn new(initial: f64) -> Self {
        Self {
            current: initial,
            target: initial,
            coeff: 0.0,
        }
    }

    /// Constante de tiempo en ms (tiempo para alcanzar ~63 % del salto).
    pub fn set_time_ms(&mut self, sample_rate: f64, time_ms: f64) {
        let samples = (time_ms * 1e-3 * sample_rate).max(1.0);
        self.coeff = (-1.0 / samples).exp();
    }

    #[inline]
    pub fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    /// Salta inmediatamente al valor (por ejemplo en `reset()`).
    pub fn snap_to(&mut self, value: f64) {
        self.current = value;
        self.target = value;
    }

    #[inline]
    pub fn target(&self) -> f64 {
        self.target
    }

    #[inline]
    pub fn is_settled(&self) -> bool {
        (self.current - self.target).abs() < 1e-9
    }

    #[inline]
    pub fn next_value(&mut self) -> f64 {
        self.current = self.target + (self.current - self.target) * self.coeff;
        if (self.current - self.target).abs() < 1e-12 {
            self.current = self.target;
        }
        self.current = denormal::flush(self.current);
        self.current
    }
}

/// Potencia constante: `mix` 0..1 -> `(dry, wet)` con `dry² + wet² = 1`.
#[inline]
pub fn equal_power(mix: f64) -> (f64, f64) {
    let m = mix.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2;
    (m.cos(), m.sin())
}

/// Bypass sin clicks: rampa de `wet` entre 0 (bypass) y 1 (activo).
#[derive(Clone, Copy, Debug)]
pub struct BypassFader {
    wet: Smoother,
}

impl BypassFader {
    pub fn new(active: bool) -> Self {
        Self {
            wet: Smoother::new(if active { 1.0 } else { 0.0 }),
        }
    }

    pub fn set_time_ms(&mut self, sample_rate: f64, time_ms: f64) {
        self.wet.set_time_ms(sample_rate, time_ms);
    }

    pub fn set_active(&mut self, active: bool) {
        self.wet.set_target(if active { 1.0 } else { 0.0 });
    }

    pub fn snap(&mut self, active: bool) {
        self.wet.snap_to(if active { 1.0 } else { 0.0 });
    }

    /// `true` si ya está totalmente en bypass y se puede saltar el procesado.
    #[inline]
    pub fn is_fully_bypassed(&self) -> bool {
        self.wet.target() == 0.0 && self.wet.is_settled()
    }

    /// Mezcla `dry`/`wet` con potencia constante.
    #[inline]
    pub fn mix(&mut self, dry: f64, wet: f64) -> f64 {
        let (d, w) = equal_power(self.wet.next_value());
        dry * d + wet * w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoother_reaches_63_percent_at_time_constant() {
        let fs = 48_000.0;
        let mut s = Smoother::new(0.0);
        s.set_time_ms(fs, 10.0);
        s.set_target(1.0);
        let n = (0.010 * fs) as usize;
        let mut v = 0.0;
        for _ in 0..n {
            v = s.next_value();
        }
        assert!((v - 0.632).abs() < 0.01, "v = {v}");
    }

    #[test]
    fn smoother_settles_exactly() {
        let mut s = Smoother::new(0.0);
        s.set_time_ms(48_000.0, 5.0);
        s.set_target(2.0);
        for _ in 0..48_000 {
            s.next_value();
        }
        assert_eq!(s.next_value(), 2.0);
        assert!(s.is_settled());
    }

    #[test]
    fn equal_power_is_constant_power() {
        for i in 0..=20 {
            let (d, w) = equal_power(i as f64 / 20.0);
            assert!((d * d + w * w - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn bypass_fader_has_no_steps() {
        let fs = 48_000.0;
        let mut b = BypassFader::new(true);
        b.set_time_ms(fs, 5.0);
        b.set_active(false);
        let mut prev = b.mix(0.0, 1.0);
        let mut max_step: f64 = 0.0;
        for _ in 0..4800 {
            let y = b.mix(0.0, 1.0);
            max_step = max_step.max((y - prev).abs());
            prev = y;
        }
        assert!(max_step < 0.01, "salto máximo {max_step}");
        assert!(prev < 1e-3);
    }
}
