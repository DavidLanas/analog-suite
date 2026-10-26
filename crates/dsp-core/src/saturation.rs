//! Saturación y waveshaping sin estado.
//!
//! Estas funciones generan armónicos que superan Nyquist: úsalas siempre dentro de
//! un [`crate::oversampling::Oversampler`] cuando el nivel de entrada sea alto.

/// Saturación simétrica suave (armónicos impares).
#[inline]
pub fn tanh_sat(x: f64) -> f64 {
    x.tanh()
}

/// Saturación asimétrica: `tanh(x + bias) - tanh(bias)`.
///
/// `bias != 0` introduce armónicos pares (carácter de FET/tubo). Al restar
/// `tanh(bias)` el punto de reposo queda en 0 y no se añade DC.
#[inline]
pub fn asym_tanh(x: f64, bias: f64) -> f64 {
    (x + bias).tanh() - bias.tanh()
}

/// Clipper cúbico suave, exacto en ±1 y de pendiente 1 en 0.
#[inline]
pub fn soft_clip(x: f64) -> f64 {
    if x >= 1.5 {
        1.0
    } else if x <= -1.5 {
        -1.0
    } else {
        x - (4.0 / 27.0) * x * x * x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_signal_gain_is_unity() {
        for f in [tanh_sat, soft_clip] {
            let x = 1e-4;
            assert!((f(x) / x - 1.0).abs() < 1e-3);
        }
        let x = 1e-4;
        assert!((asym_tanh(x, 0.3) / x - (1.0 - 0.3f64.tanh().powi(2))).abs() < 1e-3);
    }

    #[test]
    fn outputs_are_bounded() {
        for i in -100..=100 {
            let x = i as f64 * 0.5;
            assert!(tanh_sat(x).abs() <= 1.0);
            assert!(soft_clip(x).abs() <= 1.0);
        }
    }

    #[test]
    fn asymmetry_creates_dc_in_rectified_response_but_zero_at_rest() {
        assert_eq!(asym_tanh(0.0, 0.5), 0.0);
        assert!((asym_tanh(1.0, 0.5) + asym_tanh(-1.0, 0.5)).abs() > 0.01);
    }
}
