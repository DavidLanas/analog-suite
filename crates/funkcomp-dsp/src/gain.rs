//! Ganancia: curva estática con knee y suavizador de reducción de ganancia.
//!
//! # Topología feed-back sin inestabilidad
//!
//! En un compresor feed-back real el detector lee la *salida*, y el lazo se comporta como
//! `GR = k·(L_out − T)` con `L_out = L_in − GR`. En estado estacionario eso equivale a una
//! ratio `R = 1 + k`. Implementarlo literalmente en digital (con un retardo de una muestra)
//! es inestable con ataques cortos y ratios altas: el polo del lazo es
//! `c − (1−c)·k`, y con `c ≈ 0.35` (20 µs a 48 kHz) y `k = 19` queda en −5.
//!
//! En su lugar se resuelve el lazo de forma **implícita** en cada muestra. Con la ratio
//! efectiva `R = 1 + k`, el valor de reposo es `g_ss = static_gr(L_in)` y la actualización
//! de ataque exacta del lazo cerrado es
//!
//! ```text
//! g' = g + w·(g_ss − g),   w = (1−c)(1+k) / (1 + (1−c)k)
//! ```
//!
//! que es una media convexa entre `g` y `g_ss` (nunca sobrepasa, incondicionalmente estable)
//! y conserva la propiedad de la topología real: **a mayor ratio, ataque efectivo más
//! rápido** (`τ_eff ≈ τ/R`).
//!
//! El release es el de un detector de pico con descarga RC: la reducción decae
//! exponencialmente hacia 0 con constante `τ_r` (lazo abierto, el diodo bloquea), y nunca
//! baja de `g_ss` porque entonces la señal vuelve a cargar el condensador.

use dsp_core::denormal;

/// Reducción de ganancia estática (dB, >= 0) para un nivel de entrada en dB.
///
/// Knee cuadrático de ancho `knee_db` centrado en el umbral (Giannoulis et al.).
pub fn static_gr_db(level_db: f64, threshold_db: f64, ratio: f64, knee_db: f64) -> f64 {
    let over = level_db - threshold_db;
    let slope = 1.0 - 1.0 / ratio.max(1.0);
    if 2.0 * over < -knee_db {
        0.0
    } else if knee_db > 0.0 && 2.0 * over.abs() <= knee_db {
        let t = over + knee_db * 0.5;
        slope * t * t / (2.0 * knee_db)
    } else {
        slope * over
    }
}

/// Peso de ataque `w` del lazo cerrado (ver documentación del módulo).
pub fn attack_weight(attack_coeff: f64, ratio: f64) -> f64 {
    let k = (ratio - 1.0).max(0.0);
    let a = 1.0 - attack_coeff;
    a * (1.0 + k) / (1.0 + a * k)
}

#[derive(Clone, Copy, Debug)]
pub struct PeakGainSmoother {
    gr: f64,
    w_attack: f64,
    release: f64,
}

impl PeakGainSmoother {
    pub fn new() -> Self {
        Self {
            gr: 0.0,
            w_attack: 1.0,
            release: 0.0,
        }
    }

    /// Tiempos en segundos (constantes de tiempo, no 10–90 %).
    pub fn configure(&mut self, sample_rate: f64, attack_s: f64, release_s: f64, ratio: f64) {
        let ca = if attack_s <= 0.0 {
            0.0
        } else {
            (-1.0 / (attack_s * sample_rate)).exp()
        };
        self.w_attack = attack_weight(ca, ratio);
        self.release = if release_s <= 0.0 {
            0.0
        } else {
            (-1.0 / (release_s * sample_rate)).exp()
        };
    }

    pub fn reset(&mut self) {
        self.gr = 0.0;
    }

    #[inline]
    pub fn value(&self) -> f64 {
        self.gr
    }

    /// Fija la reducción actual (arranque en caliente al cambiar de modo).
    pub fn set_value(&mut self, gr: f64) {
        self.gr = gr.max(0.0);
    }

    /// `target_gr_db`: reducción de reposo para el nivel actual (de [`static_gr_db`]).
    #[inline]
    pub fn process(&mut self, target_gr_db: f64) -> f64 {
        if target_gr_db > self.gr {
            self.gr += self.w_attack * (target_gr_db - self.gr);
        } else {
            self.gr = (self.gr * self.release).max(target_gr_db);
        }
        self.gr = denormal::flush(self.gr);
        self.gr
    }
}

/// Suavizador *feed-forward* con release de dos etapas dependiente del programa.
///
/// * **Etapa rápida**: ataque (sin aceleración por ratio) y release `release_s`; su salida
///   nunca baja de la reducción objetivo.
/// * **Etapa lenta** (opcional): se *carga* hacia la etapa rápida con constante
///   `charge_s` y se descarga con `release_slow_s`. Un pico aislado apenas la carga, así que
///   el release es el rápido; una señal sostenida o varios picos seguidos la cargan y
///   entonces manda ella, con un release mucho más largo. Es la forma habitual de modelar el
///   release dependiente del programa de compresores ópticos y variable-mu.
///
/// La reducción aplicada es `max(rápida, lenta)`.
#[derive(Clone, Copy, Debug)]
pub struct TwoStageSmoother {
    fast: f64,
    slow: f64,
    w_attack: f64,
    rel_fast: f64,
    slow_enabled: bool,
    rel_slow: f64,
    w_charge: f64,
    /// Fracción de la etapa rápida a la que tiende la lenta (1.0 = la iguala).
    slow_fraction: f64,
}

impl TwoStageSmoother {
    pub fn new() -> Self {
        Self {
            fast: 0.0,
            slow: 0.0,
            w_attack: 1.0,
            rel_fast: 0.0,
            slow_enabled: false,
            rel_slow: 0.0,
            w_charge: 0.0,
            slow_fraction: 1.0,
        }
    }

    /// La etapa lenta se carga hacia `fraction · rápida` (0..=1). Con `0.5`, tras una señal
    /// sostenida el release cae rápido al 50 % y el resto se suelta despacio (célula óptica).
    pub fn set_slow_fraction(&mut self, fraction: f64) {
        self.slow_fraction = fraction.clamp(0.0, 1.0);
    }

    /// Todos los tiempos son constantes de tiempo en segundos.
    pub fn configure(
        &mut self,
        sample_rate: f64,
        attack_s: f64,
        release_s: f64,
        release_slow_s: Option<f64>,
        charge_s: f64,
    ) {
        let coeff = |t: f64| {
            if t <= 0.0 {
                0.0
            } else {
                (-1.0 / (t * sample_rate)).exp()
            }
        };
        self.w_attack = 1.0 - coeff(attack_s);
        self.rel_fast = coeff(release_s);
        match release_slow_s {
            Some(t) => {
                self.slow_enabled = true;
                self.rel_slow = coeff(t);
                self.w_charge = 1.0 - coeff(charge_s);
            }
            None => {
                self.slow_enabled = false;
                self.slow = 0.0;
            }
        }
    }

    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
    }

    /// Arranque en caliente: la etapa rápida toma `gr` y la lenta empieza vacía (si no, el
    /// release largo congelaría la reducción heredada durante segundos).
    pub fn set_value(&mut self, gr: f64) {
        self.fast = gr.max(0.0);
        self.slow = 0.0;
    }

    #[inline]
    pub fn value(&self) -> f64 {
        self.fast.max(self.slow)
    }

    #[inline]
    pub fn process(&mut self, target_gr_db: f64) -> f64 {
        if target_gr_db > self.fast {
            self.fast += self.w_attack * (target_gr_db - self.fast);
        } else {
            self.fast = (self.fast * self.rel_fast).max(target_gr_db);
        }
        self.fast = denormal::flush(self.fast);

        if self.slow_enabled {
            let slow_target = self.fast * self.slow_fraction;
            if slow_target > self.slow {
                self.slow += self.w_charge * (slow_target - self.slow);
            } else {
                self.slow *= self.rel_slow;
            }
            self.slow = denormal::flush(self.slow);
        }
        self.fast.max(self.slow)
    }
}

impl Default for TwoStageSmoother {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PeakGainSmoother {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_curve_matches_ratio_above_the_knee() {
        for r in [4.0, 8.0, 12.0, 20.0] {
            // 10 dB sobre el umbral, muy por encima del knee de 6 dB.
            let gr = static_gr_db(0.0, -10.0, r, 6.0);
            assert!((gr - 10.0 * (1.0 - 1.0 / r)).abs() < 1e-12);
        }
    }

    #[test]
    fn static_curve_is_zero_below_and_continuous_through_the_knee() {
        assert_eq!(static_gr_db(-40.0, -10.0, 4.0, 6.0), 0.0);
        let eps = 1e-9;
        for edge in [-13.0, -7.0] {
            let a = static_gr_db(edge - eps, -10.0, 4.0, 6.0);
            let b = static_gr_db(edge + eps, -10.0, 4.0, 6.0);
            assert!((a - b).abs() < 1e-6, "salto en {edge}: {a} vs {b}");
        }
        // Monótona creciente.
        let mut prev = 0.0;
        for i in 0..400 {
            let g = static_gr_db(-30.0 + i as f64 * 0.1, -10.0, 8.0, 6.0);
            assert!(g >= prev - 1e-12);
            prev = g;
        }
    }

    #[test]
    fn release_decays_with_the_time_constant() {
        let fs = 48_000.0;
        let mut s = PeakGainSmoother::new();
        s.configure(fs, 0.0, 0.200, 4.0);
        s.process(10.0); // ataque instantáneo
        assert!((s.value() - 10.0).abs() < 1e-9);
        let n = (0.200 * fs) as usize;
        for _ in 0..n {
            s.process(0.0);
        }
        assert!(
            (s.value() - 10.0 * (-1f64).exp()).abs() < 0.02,
            "{}",
            s.value()
        );
    }

    #[test]
    fn release_never_drops_below_the_target() {
        let mut s = PeakGainSmoother::new();
        s.configure(48_000.0, 0.0, 0.050, 4.0);
        s.process(10.0);
        for _ in 0..100_000 {
            s.process(6.0);
        }
        assert!((s.value() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn higher_ratio_means_faster_effective_attack_and_never_overshoots() {
        let fs = 48_000.0;
        let steps_to_90 = |ratio: f64| {
            let mut s = PeakGainSmoother::new();
            s.configure(fs, 0.0008, 1.0, ratio);
            let mut n = 0;
            while s.value() < 9.0 {
                let v = s.process(10.0);
                assert!(v <= 10.0 + 1e-12, "overshoot {v}");
                n += 1;
                assert!(n < 100_000);
            }
            n
        };
        let (n4, n20) = (steps_to_90(4.0), steps_to_90(20.0));
        assert!(n20 * 3 < n4 * 2, "n4 = {n4}, n20 = {n20}");
    }

    #[test]
    fn closed_loop_is_stable_for_the_fastest_attack_and_highest_ratio() {
        // El lazo feed-back ingenuo con k = 19 y 20 µs a 48 kHz es inestable (polo en −5).
        let mut s = PeakGainSmoother::new();
        s.configure(48_000.0, 20e-6, 0.050, 20.0);
        for i in 0..10_000 {
            let target = if (i / 200) % 2 == 0 { 12.0 } else { 0.0 };
            let v = s.process(target);
            assert!(v.is_finite() && (0.0..=12.0 + 1e-9).contains(&v));
        }
    }

    /// Tiempo (s) hasta que la reducción cae por debajo de 1/e de 10 dB tras un pulso.
    fn time_to_decay(burst_s: f64, release: f64, slow: Option<f64>) -> f64 {
        let fs = 48_000.0;
        let mut s = TwoStageSmoother::new();
        s.configure(fs, 0.0002, release, slow, 0.4);
        for _ in 0..(burst_s * fs) as usize {
            s.process(10.0);
        }
        let mut n = 0usize;
        while s.process(0.0) > 10.0 * (-1f64).exp() {
            n += 1;
            assert!(n < 48_000 * 600);
        }
        n as f64 / fs
    }

    #[test]
    fn two_stage_without_slow_matches_a_plain_exponential_release() {
        let t = time_to_decay(0.5, 0.3, None);
        assert!((t - 0.3).abs() < 0.01, "t = {t}");
    }

    #[test]
    fn program_dependent_release_is_much_longer_after_sustained_signal() {
        // Pulso aislado de 20 ms: manda la etapa rápida (0.3 s).
        let single = time_to_decay(0.020, 0.3, Some(25.0));
        // Señal sostenida de 5 s: manda la etapa lenta (25 s).
        let sustained = time_to_decay(5.0, 0.3, Some(25.0));
        assert!(single < 0.5, "pulso aislado: {single} s");
        assert!(sustained > 20.0, "señal sostenida: {sustained} s");
        assert!(sustained > single * 30.0);
    }

    #[test]
    fn two_stage_never_undershoots_the_target_and_attack_never_overshoots() {
        let mut s = TwoStageSmoother::new();
        s.configure(48_000.0, 0.0004, 2.0, Some(10.0), 0.4);
        let mut prev = 0.0;
        for i in 0..200_000 {
            let target = if (i / 5_000) % 3 == 0 { 9.0 } else { 3.0 };
            let v = s.process(target);
            // Por debajo del objetivo sólo se está en pleno ataque: la reducción sube (la etapa
            // lenta puede bajar ~1e-5 dB por muestra mientras la rápida aún sube).
            assert!(
                v >= target - 1e-9 || v >= prev - 1e-3,
                "v = {v} (objetivo {target})"
            );
            assert!(v <= 9.0 + 1e-9, "sobreimpulso: {v}");
            prev = v;
        }
    }

    #[test]
    fn warm_start_resumes_from_the_given_reduction_without_a_jump() {
        let mut s = TwoStageSmoother::new();
        s.configure(48_000.0, 0.0002, 0.8, Some(10.0), 0.4);
        s.set_value(7.0);
        let first = s.process(7.0);
        assert!((first - 7.0).abs() < 0.01, "{first}");
        let mut p = PeakGainSmoother::new();
        p.set_value(5.0);
        assert_eq!(p.value(), 5.0);
        p.set_value(-3.0);
        assert_eq!(p.value(), 0.0);
    }
}
