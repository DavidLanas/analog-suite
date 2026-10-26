//! Modo VARI-MU 660, inspirado en un compresor/limitador de tubos de mu variable
//! (no es una réplica).
//!
//! * **Curva de ratio creciente con knee muy amplio.** En un variable-mu la ganancia de los
//!   tubos de control cae poco a poco al aumentar la polarización, así que la ratio efectiva
//!   no es un número fijo: empieza en 1:1 muy por debajo del umbral y sube suavemente hasta
//!   [`RATIO_MAX`]. Se modela con `R(o) = 1 + (R_max − 1)·x²`, `x = (o + W)/(2W)`
//!   (`o` = dB sobre el umbral, `W` = [`KNEE_HALF_DB`]); la reducción es la integral de la
//!   pendiente `1 − 1/R`, que tiene forma cerrada (ver [`static_gr_db`]).
//! * **Time Constant de 6 posiciones** ([`TIME_CONSTANTS`]); las dos últimas tienen release de
//!   dos etapas dependiente del programa ([`crate::gain::TwoStageSmoother`]).
//! * **Feed-forward**: el ataque no se acelera con la ratio.
//! * **Saturación de tubo**: waveshaper con polarización alta (mucho 2º armónico) y drive que
//!   crece con la reducción.
//! * **Transformador**: algo de cuerpo en graves y agudos algo recortados, como filtros
//!   lineales ([`Coloration`]). No modela la saturación de flujo del núcleo a baja frecuencia
//!   (queda para el pulido).

use crate::gain::TwoStageSmoother;
use crate::saturation::Shaper;
use crate::{Coloration, CompressorMode, Settings};
use dsp_core::db_to_lin;

/// Semiancho del knee (dB): la ratio sube de 1:1 a [`RATIO_MAX`] entre `−W` y `+W`.
pub const KNEE_HALF_DB: f64 = 12.0;
/// Ratio efectiva máxima (al final del knee y por encima).
pub const RATIO_MAX: f64 = 12.0;
/// Nivel de referencia (dBFS) para el auto-gain.
pub const REFERENCE_DB: f64 = -18.0;
/// Constante de carga de la etapa lenta del release dependiente del programa (s).
pub const SLOW_CHARGE_S: f64 = 0.4;

const DRIVE_BASE: f64 = 0.7;
const DRIVE_PER_DB_GR: f64 = 0.03;
const BIAS: f64 = 0.2;

/// Una posición del selector Time Constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeConstant {
    pub attack_ms: f64,
    pub release_s: f64,
    /// `Some(s)`: release de dos etapas dependiente del programa (el segundo valor).
    pub release_slow_s: Option<f64>,
}

/// Posiciones 1..=6. Posiciones 1–5 según valores publicados de la unidad original (ataque
/// 0.2 ms en 1, 2 y 6, y 0.4 ms en 3–5; release 0.3/0.8/2/5 s en 1–4 y 2 s → 10 s con
/// dependencia del programa en 5). La posición 6 no figura en la fuente consultada: se usa
/// 0.2 ms y 0.3 s → 25 s como aproximación; afinar de oído.
pub const TIME_CONSTANTS: [TimeConstant; 6] = [
    TimeConstant {
        attack_ms: 0.2,
        release_s: 0.3,
        release_slow_s: None,
    },
    TimeConstant {
        attack_ms: 0.2,
        release_s: 0.8,
        release_slow_s: None,
    },
    TimeConstant {
        attack_ms: 0.4,
        release_s: 2.0,
        release_slow_s: None,
    },
    TimeConstant {
        attack_ms: 0.4,
        release_s: 5.0,
        release_slow_s: None,
    },
    TimeConstant {
        attack_ms: 0.4,
        release_s: 2.0,
        release_slow_s: Some(10.0),
    },
    TimeConstant {
        attack_ms: 0.2,
        release_s: 0.3,
        release_slow_s: Some(25.0),
    },
];

/// Datos de la posición `position` (1..=6, se recorta al rango).
pub fn time_constant(position: usize) -> TimeConstant {
    TIME_CONSTANTS[position.clamp(1, 6) - 1]
}

/// Texto para la GUI, p. ej. `"5 · attack 0.4 ms · release 2 s → 10 s (auto)"`.
pub fn describe(position: usize) -> String {
    let tc = time_constant(position);
    let fmt_s = |s: f64| format!("{s} s");
    match tc.release_slow_s {
        Some(slow) => format!(
            "{} · attack {} ms · release {} → {} (auto)",
            position.clamp(1, 6),
            tc.attack_ms,
            fmt_s(tc.release_s),
            fmt_s(slow)
        ),
        None => format!(
            "{} · attack {} ms · release {}",
            position.clamp(1, 6),
            tc.attack_ms,
            fmt_s(tc.release_s)
        ),
    }
}

/// Pendiente de la curva estática (dB de reducción por dB de entrada) a `o` dB del umbral.
pub fn slope(over_db: f64) -> f64 {
    let x = ((over_db + KNEE_HALF_DB) / (2.0 * KNEE_HALF_DB)).clamp(0.0, 1.0);
    let ratio = 1.0 + (RATIO_MAX - 1.0) * x * x;
    1.0 - 1.0 / ratio
}

/// Reducción de ganancia estática (dB, >= 0) para un nivel de entrada y un umbral en dB.
///
/// Integral cerrada de [`slope`]: con `a = R_max − 1` y `g(x) = x − atan(√a·x)/√a`,
/// `GR = 2W·g(x)` dentro del knee y `2W·g(1) + (1 − 1/R_max)·(o − W)` por encima.
pub fn static_gr_db(level_db: f64, threshold_db: f64) -> f64 {
    let over = level_db - threshold_db;
    let w = KNEE_HALF_DB;
    if over <= -w {
        return 0.0;
    }
    let a = RATIO_MAX - 1.0;
    let sa = a.sqrt();
    let g = |x: f64| x - (sa * x).atan() / sa;
    if over >= w {
        2.0 * w * g(1.0) + (1.0 - 1.0 / RATIO_MAX) * (over - w)
    } else {
        2.0 * w * g((over + w) / (2.0 * w))
    }
}

pub struct VariMu {
    sample_rate: f64,
    threshold_db: f64,
    /// Nivel lineal bajo el cual no hay reducción (evita `log10` por muestra en silencio).
    floor_lin: f64,
    smoother: TwoStageSmoother,
}

impl VariMu {
    pub fn new() -> Self {
        Self {
            sample_rate: 44_100.0,
            threshold_db: -12.0,
            floor_lin: db_to_lin(-12.0 - KNEE_HALF_DB),
            smoother: TwoStageSmoother::new(),
        }
    }
}

impl Default for VariMu {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressorMode for VariMu {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn configure(&mut self, s: &Settings) {
        self.threshold_db = s.threshold_db;
        self.floor_lin = db_to_lin(s.threshold_db - KNEE_HALF_DB);
        let tc = time_constant(s.time_constant);
        self.smoother.configure(
            self.sample_rate,
            tc.attack_ms * 1e-3,
            tc.release_s,
            tc.release_slow_s,
            SLOW_CHARGE_S,
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
            static_gr_db(20.0 * detector_level.log10(), self.threshold_db)
        };
        self.smoother.process(target)
    }

    #[inline]
    fn shaper(&self, gain_reduction_db: f64) -> Shaper {
        Shaper::new(
            DRIVE_BASE * (1.0 + DRIVE_PER_DB_GR * gain_reduction_db),
            BIAS,
        )
    }

    fn auto_gain_db(&self, input_db: f64) -> f64 {
        -input_db + static_gr_db(REFERENCE_DB + input_db, self.threshold_db)
    }

    fn coloration(&self) -> Coloration {
        Coloration {
            low_shelf_db: 1.2,
            low_shelf_hz: 90.0,
            high_shelf_db: -1.5,
            high_shelf_hz: 12_000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_constants_match_the_documented_table() {
        assert_eq!(time_constant(1).release_s, 0.3);
        assert_eq!(time_constant(2).release_s, 0.8);
        assert_eq!(time_constant(3).release_s, 2.0);
        assert_eq!(time_constant(4).release_s, 5.0);
        assert_eq!(time_constant(5).release_slow_s, Some(10.0));
        assert!(time_constant(6).release_slow_s.is_some());
        // Sólo las dos últimas son dependientes del programa.
        for p in 1..=4 {
            assert!(time_constant(p).release_slow_s.is_none());
        }
        // Fuera de rango se recorta.
        assert_eq!(time_constant(0), time_constant(1));
        assert_eq!(time_constant(99), time_constant(6));
        assert!(describe(5).contains("2 s → 10 s"));
    }

    #[test]
    fn static_curve_is_zero_below_the_knee_and_continuous_everywhere() {
        let t = -12.0;
        assert_eq!(static_gr_db(t - KNEE_HALF_DB - 1.0, t), 0.0);
        assert!(static_gr_db(t - KNEE_HALF_DB, t).abs() < 1e-12);
        let eps = 1e-7;
        for edge in [-KNEE_HALF_DB, KNEE_HALF_DB] {
            let a = static_gr_db(t + edge - eps, t);
            let b = static_gr_db(t + edge + eps, t);
            assert!((a - b).abs() < 1e-5, "salto en {edge}: {a} vs {b}");
        }
    }

    #[test]
    fn static_curve_is_the_integral_of_the_slope() {
        // Integración numérica (trapecios) de la pendiente frente a la forma cerrada.
        let t = 0.0;
        let mut acc = 0.0;
        let step = 0.001;
        let mut o = -KNEE_HALF_DB - 5.0;
        while o < 20.0 {
            acc += 0.5 * (slope(o) + slope(o + step)) * step;
            o += step;
            if o > -KNEE_HALF_DB && (o - step) <= 20.0 {
                let closed = static_gr_db(o, t);
                assert!((acc - closed).abs() < 2e-3, "o = {o}: {acc} vs {closed}");
            }
        }
    }

    #[test]
    fn effective_ratio_rises_smoothly_from_1_to_max() {
        let mut prev = 1.0;
        for i in 0..=48 {
            let o = -KNEE_HALF_DB + i as f64 * 0.5;
            let ratio = 1.0 / (1.0 - slope(o));
            assert!(ratio >= prev - 1e-12, "no monótona en {o}");
            prev = ratio;
        }
        assert!((1.0 / (1.0 - slope(-KNEE_HALF_DB)) - 1.0).abs() < 1e-12);
        assert!((1.0 / (1.0 - slope(KNEE_HALF_DB)) - RATIO_MAX).abs() < 1e-9);
        // Muy por encima del knee la ratio se queda en el máximo.
        assert!((1.0 / (1.0 - slope(40.0)) - RATIO_MAX).abs() < 1e-9);
    }

    #[test]
    fn output_level_never_decreases_as_input_rises() {
        // Una curva de compresión válida es monótona: más entrada nunca da menos salida.
        let mut prev = f64::MIN;
        for i in 0..800 {
            let level = -60.0 + i as f64 * 0.1;
            let out = level - static_gr_db(level, -12.0);
            assert!(out >= prev - 1e-9);
            prev = out;
        }
    }

    #[test]
    fn auto_gain_compensates_input_boost_and_static_reduction() {
        let mut m = VariMu::new();
        m.prepare(48_000.0);
        m.configure(&Settings {
            threshold_db: -20.0,
            ..Settings::default()
        });
        for input in [0.0, 6.0, 18.0] {
            let expected = -input + static_gr_db(REFERENCE_DB + input, -20.0);
            assert!((m.auto_gain_db(input) - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn tube_shaper_is_asymmetric_and_gets_harder_with_reduction() {
        let m = VariMu::new();
        let s = m.shaper(0.0);
        assert!((s.apply(0.8) + s.apply(-0.8)).abs() > 0.01);
        assert!(m.shaper(15.0).apply(0.9) < m.shaper(0.0).apply(0.9));
    }

    #[test]
    fn warm_start_keeps_the_inherited_reduction_instead_of_dropping_to_zero() {
        let mut m = VariMu::new();
        m.prepare(48_000.0);
        m.configure(&Settings::default());
        m.warm_start(8.0);
        // En silencio la reducción heredada se suelta con el release del modo, sin saltar a 0.
        let first = m.gain_reduction_db(0.0);
        assert!(first > 7.9 && first <= 8.0, "{first}");
    }
}
