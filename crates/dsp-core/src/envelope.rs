//! Detectores y envolventes.

use crate::denormal;

/// Coeficiente de un polo para una constante de tiempo en ms.
///
/// `ms <= 0` devuelve `0.0` (respuesta instantánea).
#[inline]
pub fn time_to_coeff(sample_rate: f64, ms: f64) -> f64 {
    if ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (ms * 1e-3 * sample_rate)).exp()
    }
}

/// Seguidor de envolvente con ataque y release independientes (detector tipo peak).
///
/// Entrada: valor absoluto de la señal (o de su versión en dB si se quiere
/// detectar en el dominio logarítmico).
#[derive(Clone, Copy, Debug)]
pub struct EnvelopeFollower {
    attack: f64,
    release: f64,
    env: f64,
}

impl EnvelopeFollower {
    pub fn new() -> Self {
        Self {
            attack: 0.0,
            release: 0.0,
            env: 0.0,
        }
    }

    pub fn set_times_ms(&mut self, sample_rate: f64, attack_ms: f64, release_ms: f64) {
        self.attack = time_to_coeff(sample_rate, attack_ms);
        self.release = time_to_coeff(sample_rate, release_ms);
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    #[inline]
    pub fn value(&self) -> f64 {
        self.env
    }

    #[inline]
    pub fn process(&mut self, input: f64) -> f64 {
        let coeff = if input > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = denormal::flush(input + coeff * (self.env - input));
        self.env
    }
}

impl Default for EnvelopeFollower {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;

    #[test]
    fn attack_reaches_63_percent_in_one_time_constant() {
        let mut e = EnvelopeFollower::new();
        e.set_times_ms(FS, 5.0, 100.0);
        let n = (0.005 * FS) as usize;
        let mut v = 0.0;
        for _ in 0..n {
            v = e.process(1.0);
        }
        assert!((v - 0.632).abs() < 0.01, "v = {v}");
    }

    #[test]
    fn release_decays_to_37_percent_in_one_time_constant() {
        let mut e = EnvelopeFollower::new();
        e.set_times_ms(FS, 0.0, 50.0);
        e.process(1.0);
        let n = (0.050 * FS) as usize;
        let mut v = 1.0;
        for _ in 0..n {
            v = e.process(0.0);
        }
        assert!((v - 0.368).abs() < 0.01, "v = {v}");
    }

    #[test]
    fn zero_time_is_instantaneous() {
        let mut e = EnvelopeFollower::new();
        e.set_times_ms(FS, 0.0, 0.0);
        assert_eq!(e.process(0.7), 0.7);
        assert_eq!(e.process(0.2), 0.2);
    }
}
