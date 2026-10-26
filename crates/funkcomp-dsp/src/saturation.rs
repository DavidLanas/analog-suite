//! Saturación: waveshaper asimétrico normalizado.

/// `y = [tanh(d·x + b) − tanh(b)] / [d·(1 − tanh²(b))]`
///
/// * Ganancia de pequeña señal exactamente 1 (el shaper no cambia el nivel de señales suaves).
/// * `b > 0` desplaza el punto de trabajo y añade armónicos pares (carácter FET/tubo);
///   restar `tanh(b)` mantiene el reposo en 0.
/// * Un nivel alto o un `d` mayor aumentan el contenido armónico, no el nivel.
///
/// Es sin estado y `Copy`: el modo lo entrega por valor al motor, que lo ejecuta dentro del
/// oversampler.
#[derive(Clone, Copy, Debug)]
pub struct Shaper {
    drive: f64,
    bias: f64,
    bias_tanh: f64,
    norm: f64,
}

impl Shaper {
    pub fn new(drive: f64, bias: f64) -> Self {
        let drive = drive.max(1e-6);
        let bt = bias.tanh();
        Self {
            drive,
            bias,
            bias_tanh: bt,
            norm: 1.0 / (drive * (1.0 - bt * bt)),
        }
    }

    #[inline]
    pub fn apply(&self, x: f64) -> f64 {
        ((x * self.drive + self.bias).tanh() - self.bias_tanh) * self.norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_signal_gain_is_unity_for_any_drive_and_bias() {
        for (d, b) in [(0.5, 0.0), (0.8, 0.08), (2.0, 0.3), (5.0, 0.1)] {
            let s = Shaper::new(d, b);
            let x = 1e-6;
            assert!((s.apply(x) / x - 1.0).abs() < 1e-4, "d={d} b={b}");
        }
    }

    #[test]
    fn rests_at_zero_and_is_asymmetric_with_bias() {
        let s = Shaper::new(1.0, 0.1);
        assert_eq!(s.apply(0.0), 0.0);
        assert!((s.apply(0.8) + s.apply(-0.8)).abs() > 1e-3);
        let sym = Shaper::new(1.0, 0.0);
        assert!((sym.apply(0.8) + sym.apply(-0.8)).abs() < 1e-12);
    }
}
