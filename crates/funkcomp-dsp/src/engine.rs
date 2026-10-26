//! Motor: procesa bloques `f32` de 1 o 2 canales.
//!
//! Flujo por muestra:
//!
//! ```text
//! x ──┬─ retardo de latencia ─────────────────────────────────┐ (seco)
//!     │                                                        ▼
//!     └─ HPF sc ─ pico enlazado ─ ×Input ─ modo.GR ─┐       Mix ─ Bypass ─ y
//!     └─ ×Input ─ ×10^(−GR/20) ─ [OS: saturación] ─ DC ─ ×Output ┘ (procesado)
//! ```
//!
//! * La ganancia se aplica a la frecuencia base; sólo la saturación (la única etapa no
//!   lineal) corre sobremuestreada. Por eso el detector ve la señal sin retardar y no hay
//!   desfase entre detección y ganancia.
//! * La señal seca (Mix y Bypass) se retarda exactamente la latencia del oversampler, que es
//!   la misma que se reporta al host.
//! * Sin asignaciones tras `new`: todo es de tamaño fijo o pre-asignado.

use crate::detector::SidechainHpf;
use crate::fet76::Fet76;
use crate::opto3a::Opto3a;
use crate::oversampling::StereoOversampler;
use crate::varimu::VariMu;
use crate::{CompressorMode, Settings};
use dsp_core::biquad::{Biquad, BiquadCoeffs};
use dsp_core::db_to_lin;
use dsp_core::smoothing::{BypassFader, Smoother};

const DELAY_LEN: usize = 64;
const DELAY_MASK: usize = DELAY_LEN - 1;
const SMOOTH_MS: f64 = 20.0;
const BYPASS_MS: f64 = 10.0;
const DC_BLOCK_HZ: f64 = 5.0;

#[derive(Clone, Copy)]
struct DelayLine {
    buf: [f64; DELAY_LEN],
    pos: usize,
}

impl DelayLine {
    fn new() -> Self {
        Self {
            buf: [0.0; DELAY_LEN],
            pos: 0,
        }
    }
    #[inline]
    fn push(&mut self, x: f64) {
        self.pos = (self.pos + 1) & DELAY_MASK;
        self.buf[self.pos] = x;
    }
    /// Muestra empujada hace `delay` pasos (`delay = 0` es la última).
    #[inline]
    fn get(&self, delay: usize) -> f64 {
        self.buf[self.pos.wrapping_sub(delay) & DELAY_MASK]
    }
}

pub struct Engine {
    sample_rate: f64,
    settings: Settings,
    modes: Vec<Box<dyn CompressorMode>>,
    sidechain: SidechainHpf,
    os: StereoOversampler,
    dc_block: [Biquad; 2],
    dry: [DelayLine; 2],
    delay: usize,
    input: Smoother,
    output: Smoother,
    mix: Smoother,
    bypass: [BypassFader; 2],
    block_gr_db: f64,
    /// Modo en uso (índice en `modes`).
    active_mode: usize,
    /// Última reducción de ganancia aplicada (dB); se hereda al cambiar de modo.
    last_gr_db: f64,
    /// Coloración de transformador por canal: [low shelf, high shelf].
    color: [[Biquad; 2]; 2],
    /// 0 = sin coloración, 1 = coloración completa (cross-fade de 20 ms al cambiar de modo).
    color_mix: Smoother,
}

impl Engine {
    /// Asigna toda la memoria. Llamar fuera del hilo de audio.
    pub fn new() -> Self {
        let mut e = Self {
            sample_rate: 44_100.0,
            settings: Settings::default(),
            modes: vec![
                Box::new(Fet76::new()),
                Box::new(VariMu::new()),
                Box::new(Opto3a::new()),
            ],
            sidechain: SidechainHpf::new(),
            os: StereoOversampler::new(),
            dc_block: [Biquad::default(); 2],
            dry: [DelayLine::new(); 2],
            delay: 0,
            input: Smoother::new(1.0),
            output: Smoother::new(1.0),
            mix: Smoother::new(1.0),
            bypass: [BypassFader::new(true); 2],
            block_gr_db: 0.0,
            active_mode: 0,
            last_gr_db: 0.0,
            color: [[Biquad::default(); 2]; 2],
            color_mix: Smoother::new(0.0),
        };
        e.prepare(44_100.0);
        e
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        for m in &mut self.modes {
            m.prepare(sample_rate);
        }
        let dc = BiquadCoeffs::highpass(sample_rate, DC_BLOCK_HZ, std::f64::consts::FRAC_1_SQRT_2);
        self.dc_block = [Biquad::new(dc); 2];
        for s in [
            &mut self.input,
            &mut self.output,
            &mut self.mix,
            &mut self.color_mix,
        ] {
            s.set_time_ms(sample_rate, SMOOTH_MS);
        }
        for b in &mut self.bypass {
            b.set_time_ms(sample_rate, BYPASS_MS);
        }
        let s = self.settings;
        self.apply(&s, true);
        self.reset();
    }

    pub fn reset(&mut self) {
        for m in &mut self.modes {
            m.reset();
        }
        self.sidechain.reset();
        self.os.reset();
        self.dc_block.iter_mut().for_each(Biquad::reset);
        self.dry = [DelayLine::new(); 2];
        self.color.iter_mut().flatten().for_each(Biquad::reset);
        self.block_gr_db = 0.0;
        self.last_gr_db = 0.0;
    }

    /// Latencia que debe reportarse al host (muestras).
    pub fn latency_samples(&self) -> u32 {
        self.delay as u32
    }

    /// Mayor reducción de ganancia (dB) del último bloque procesado.
    pub fn block_gr_db(&self) -> f64 {
        self.block_gr_db
    }

    /// Aplica ajustes nuevos; no hace nada si no cambiaron.
    pub fn set_settings(&mut self, s: &Settings) {
        if *s != self.settings {
            self.apply(s, false);
        }
    }

    fn apply(&mut self, s: &Settings, snap: bool) {
        let prev_os = self.settings.oversampling;
        self.settings = *s;
        let idx = s.mode.min(self.modes.len() - 1);
        if idx != self.active_mode {
            // Arranque en caliente: el modo nuevo parte de la reducción vigente, así que la
            // ganancia no da saltos al cambiar (evoluciona hacia la suya con sus tiempos).
            self.modes[idx].reset();
            self.modes[idx].warm_start(self.last_gr_db);
            self.active_mode = idx;
        }
        self.modes[idx].configure(s);
        self.configure_coloration(idx, snap);
        self.sidechain
            .configure(self.sample_rate, s.sidechain_hpf_hz);

        let factor = Self::sanitize_factor(s.oversampling);
        if !snap && factor != Self::sanitize_factor(prev_os) {
            self.os.reset();
            self.dry = [DelayLine::new(); 2];
        }
        self.delay = self.os.latency_samples(factor).round() as usize;

        // Los modos sin control Input (OPTO 3A) ignoran `input_db`.
        let input_db = if self.modes[idx].uses_input() {
            s.input_db
        } else {
            0.0
        };
        let auto = if s.auto_gain {
            self.modes[idx].auto_gain_db(input_db)
        } else {
            0.0
        };
        self.input.set_target(db_to_lin(input_db));
        self.output.set_target(db_to_lin(s.output_db + auto));
        self.mix.set_target(s.mix.clamp(0.0, 1.0));
        for b in &mut self.bypass {
            if snap {
                b.snap(!s.bypass);
            } else {
                b.set_active(!s.bypass);
            }
        }
        if snap {
            self.input.snap_to(db_to_lin(input_db));
            self.output.snap_to(db_to_lin(s.output_db + auto));
            self.mix.snap_to(s.mix.clamp(0.0, 1.0));
        }
    }

    /// Ajusta la coloración del modo activo. Al pasar a un modo sin coloración se conservan los
    /// coeficientes y sólo se baja `color_mix` (cross-fade), para no introducir un escalón.
    fn configure_coloration(&mut self, idx: usize, snap: bool) {
        let c = self.modes[idx].coloration();
        let target = if c.is_none() { 0.0 } else { 1.0 };
        if !c.is_none() {
            let q = std::f64::consts::FRAC_1_SQRT_2;
            let low = BiquadCoeffs::low_shelf(self.sample_rate, c.low_shelf_hz, q, c.low_shelf_db);
            let high =
                BiquadCoeffs::high_shelf(self.sample_rate, c.high_shelf_hz, q, c.high_shelf_db);
            if self.color_mix.target() == 0.0 {
                // Estaba apagada: empezar con estados limpios.
                self.color.iter_mut().flatten().for_each(Biquad::reset);
            }
            for ch in &mut self.color {
                ch[0].set_coeffs(low);
                ch[1].set_coeffs(high);
            }
        }
        self.color_mix.set_target(target);
        if snap {
            self.color_mix.snap_to(target);
        }
    }

    fn sanitize_factor(f: u8) -> u8 {
        match f {
            0 | 1 => 1,
            2 | 3 => 2,
            _ => 4,
        }
    }

    /// Procesa un bloque in-place. `bufs` tiene 1 o 2 canales de igual longitud.
    #[allow(clippy::needless_range_loop)] // `i` recorre varios canales a la vez
    pub fn process_block(&mut self, bufs: &mut [&mut [f32]]) {
        let channels = bufs.len().min(2);
        if channels == 0 {
            return;
        }
        let n = bufs[0].len();
        let factor = Self::sanitize_factor(self.settings.oversampling);
        let idx = self.active_mode;
        let mut block_gr: f64 = 0.0;

        for i in 0..n {
            let g_in = self.input.next_value();
            let g_out = self.output.next_value();
            let mix = self.mix.next_value();
            let color = self.color_mix.next_value();

            let mut x = [0.0f64; 2];
            let mut dry = [0.0f64; 2];
            for ch in 0..channels {
                x[ch] = bufs[ch][i] as f64;
                self.dry[ch].push(x[ch]);
                dry[ch] = self.dry[ch].get(self.delay);
            }

            // Bypass total y estable: no gastar CPU; los estados dinámicos quedan congelados.
            if self.bypass[0].is_fully_bypassed() {
                for ch in 0..channels {
                    let y = self.bypass[ch].mix(dry[ch], dry[ch]);
                    bufs[ch][i] = y as f32;
                }
                continue;
            }

            let detector = self.sidechain.linked_peak(&x, channels) * g_in;
            let mode = &mut self.modes[idx];
            let gr = mode.gain_reduction_db(detector);
            block_gr = block_gr.max(gr);
            let shaper = mode.shaper(gr);
            self.last_gr_db = gr;
            let gain = g_in * (-gr * std::f64::consts::LN_10 / 20.0).exp();

            for ch in 0..channels {
                let pre = x[ch] * gain;
                let sat = self.os.process(ch, factor, pre, &mut |v| shaper.apply(v));
                let mut wet = self.dc_block[ch].process(sat);
                if color > 0.0 {
                    let low = self.color[ch][0].process(wet);
                    let tinted = self.color[ch][1].process(low);
                    wet += (tinted - wet) * color;
                }
                let wet = wet * g_out;
                let blend = dry[ch] * (1.0 - mix) + wet * mix;
                bufs[ch][i] = self.bypass[ch].mix(dry[ch], blend) as f32;
            }
        }
        self.block_gr_db = block_gr;
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fet76::THRESHOLD_DB;
    use crate::gain::static_gr_db;
    use crate::Ratio;
    use std::f64::consts::PI;

    const FS: f64 = 48_000.0;

    fn engine(s: Settings) -> Engine {
        let mut e = Engine::new();
        e.prepare(FS);
        e.set_settings(&s);
        e
    }

    fn sine(freq: f64, amp: f64, n: usize, offset: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (amp * (2.0 * PI * freq * (i + offset) as f64 / FS).sin()) as f32)
            .collect()
    }

    /// Procesa en bloques de 256 muestras (mono) y devuelve la salida completa.
    fn run_mono(e: &mut Engine, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(input.len());
        for chunk in input.chunks(256) {
            let mut buf = chunk.to_vec();
            e.process_block(&mut [&mut buf[..]]);
            out.extend_from_slice(&buf);
        }
        out
    }

    /// Amplitud de la componente `k·f0` por proyección (ventana de periodos enteros).
    fn harmonic(sig: &[f32], f0: f64, k: usize) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &v) in sig.iter().enumerate() {
            let w = 2.0 * PI * f0 * k as f64 * i as f64 / FS;
            re += v as f64 * w.cos();
            im += v as f64 * w.sin();
        }
        2.0 * (re * re + im * im).sqrt() / sig.len() as f64
    }

    fn thd(sig: &[f32], f0: f64) -> f64 {
        let h1 = harmonic(sig, f0, 1);
        let rest: f64 = (2..=6).map(|k| harmonic(sig, f0, k).powi(2)).sum();
        rest.sqrt() / h1
    }

    #[test]
    fn mix_zero_is_a_perfect_null_against_delayed_input() {
        for os in [1u8, 2, 4] {
            let s = Settings {
                mix: 0.0,
                oversampling: os,
                ..Settings::default()
            };
            let mut e = engine(s);
            let lat = e.latency_samples() as usize;
            let x = sine(440.0, 0.8, 24_000, 0);
            run_mono(&mut e, &x); // asentar smoothers
            let y = run_mono(&mut e, &sine(440.0, 0.8, 4_800, 24_000));
            // La salida en el instante t es la entrada de t - lat.
            let reference = sine(440.0, 0.8, 4_800, 24_000 - lat);
            for i in 0..4_800 {
                assert!((y[i] - reference[i]).abs() < 1e-6, "os {os}, i {i}");
            }
        }
    }

    #[test]
    fn full_bypass_returns_delayed_input_bit_exact() {
        let s = Settings {
            bypass: true,
            ..Settings::default()
        };
        let mut e = engine(s);
        let lat = e.latency_samples() as usize;
        run_mono(&mut e, &sine(1000.0, 0.9, 24_000, 0));
        let y = run_mono(&mut e, &sine(1000.0, 0.9, 2_000, 24_000));
        let reference = sine(1000.0, 0.9, 2_000, 24_000 - lat);
        for i in 0..2_000 {
            assert_eq!(y[i], reference[i]);
        }
    }

    #[test]
    fn reported_latency_matches_the_oversampler() {
        assert_eq!(
            engine(Settings {
                oversampling: 1,
                ..Settings::default()
            })
            .latency_samples(),
            0
        );
        assert_eq!(
            engine(Settings {
                oversampling: 2,
                ..Settings::default()
            })
            .latency_samples(),
            23
        );
        assert_eq!(
            engine(Settings {
                oversampling: 4,
                ..Settings::default()
            })
            .latency_samples(),
            35
        );
    }

    #[test]
    fn wet_and_dry_are_time_aligned_so_partial_mix_does_not_comb() {
        // Señal pequeña => sin compresión y saturación lineal => procesado ≈ seco retardado.
        for (os, tol) in [(2u8, 0.01), (4, 0.05)] {
            let s = Settings {
                input_db: 0.0,
                mix: 0.5,
                oversampling: os,
                ..Settings::default()
            };
            let mut e = engine(s);
            let lat = e.latency_samples() as usize;
            let amp = 1e-3;
            run_mono(&mut e, &sine(1000.0, amp, 24_000, 0));
            let y = run_mono(&mut e, &sine(1000.0, amp, 4_800, 24_000));
            let reference = sine(1000.0, amp, 4_800, 24_000 - lat);
            let err = (0..4_800)
                .map(|i| (y[i] - reference[i]).abs() as f64)
                .fold(0.0, f64::max);
            assert!(err < amp * tol, "os {os}: error {err:.3e}");
        }
    }

    #[test]
    fn steady_state_reduction_follows_the_static_law_for_every_ratio() {
        let peak_db = -4.0; // 6 dB sobre el umbral, fuera del knee
        for (ratio, r) in [
            (Ratio::R4, 4.0),
            (Ratio::R8, 8.0),
            (Ratio::R12, 12.0),
            (Ratio::R20, 20.0),
        ] {
            let s = Settings {
                input_db: 0.0,
                ratio,
                attack_knob: 4.0,
                release_knob: 1.0, // el más lento: el rizado del detector es mínimo
                ..Settings::default()
            };
            let mut e = engine(s);
            let amp = 10f64.powf(peak_db / 20.0);
            run_mono(&mut e, &sine(1000.0, amp, 48_000, 0));
            run_mono(&mut e, &sine(1000.0, amp, 4_800, 48_000));
            let expected = static_gr_db(peak_db, THRESHOLD_DB, r, crate::fet76::KNEE_DB);
            assert!(
                (e.block_gr_db() - expected).abs() < 0.3,
                "ratio {r}: GR {:.2} dB vs {expected:.2} dB",
                e.block_gr_db()
            );
        }
    }

    #[test]
    fn no_reduction_well_below_the_threshold() {
        let mut e = engine(Settings {
            input_db: 0.0,
            ..Settings::default()
        });
        run_mono(&mut e, &sine(1000.0, 10f64.powf(-30.0 / 20.0), 24_000, 0));
        assert_eq!(e.block_gr_db(), 0.0);
    }

    #[test]
    fn all_buttons_in_compresses_harder_than_20_to_1() {
        let gr = |ratio| {
            let mut e = engine(Settings {
                input_db: 12.0,
                ratio,
                release_knob: 1.0,
                ..Settings::default()
            });
            let x = sine(1000.0, 0.25, 48_000, 0);
            run_mono(&mut e, &x);
            run_mono(&mut e, &sine(1000.0, 0.25, 4_800, 48_000));
            e.block_gr_db()
        };
        assert!(gr(Ratio::AllButtons) > gr(Ratio::R20) + 1.0);
    }

    #[test]
    fn stereo_link_applies_the_same_reduction_to_both_channels() {
        let mut e = engine(Settings {
            input_db: 0.0,
            mix: 1.0,
            ..Settings::default()
        });
        let loud = sine(1000.0, 0.7, 48_000, 0);
        let quiet: Vec<f32> = loud.iter().map(|v| v * 0.05).collect();
        let (mut l, mut r) = (loud.clone(), quiet.clone());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process_block(&mut [cl, cr]);
        }
        // Ganancia entre salida y entrada (tras latencia y asentamiento) igual en ambos.
        let gain = |out: &[f32], inp: &[f32]| {
            let s = 40_000;
            harmonic(&out[s..s + 4_800], 1000.0, 1)
                / harmonic(&inp[s - 23..s - 23 + 4_800], 1000.0, 1)
        };
        let (gl, gr) = (gain(&l, &loud), gain(&r, &quiet));
        assert!(gl < 0.9, "el canal fuerte debe comprimirse: {gl}");
        assert!(e.block_gr_db() > 3.0);
        // La saturación baja algo más (~3 %) el canal fuerte, que llega más alto al shaper.
        assert!((gl / gr - 1.0).abs() < 0.06, "gl {gl} gr {gr}");
    }

    #[test]
    fn thd_is_low_for_small_signals_and_grows_with_level() {
        let measure = |level_db: f64| {
            let mut e = engine(Settings {
                input_db: 0.0,
                oversampling: 4,
                ..Settings::default()
            });
            let amp = 10f64.powf(level_db / 20.0);
            run_mono(&mut e, &sine(1000.0, amp, 48_000, 0));
            let y = run_mono(&mut e, &sine(1000.0, amp, 24_000, 48_000));
            thd(&y, 1000.0)
        };
        let (low, mid, high) = (measure(-30.0), measure(-18.0), measure(-3.0));
        assert!(low < 0.003, "THD(-30 dBFS) = {:.3}%", low * 100.0);
        assert!(mid < 0.012, "THD(-18 dBFS) = {:.3}%", mid * 100.0);
        assert!(
            high > mid * 2.0,
            "THD alto {:.2}% vs medio {:.2}%",
            high * 100.0,
            mid * 100.0
        );
        println!(
            "THD: -30 dBFS {:.3}%  -18 dBFS {:.3}%  -3 dBFS {:.2}%",
            low * 100.0,
            mid * 100.0,
            high * 100.0
        );
    }

    #[test]
    fn mono_and_empty_blocks_are_handled() {
        let mut e = engine(Settings::default());
        e.process_block(&mut []);
        let mut empty: [f32; 0] = [];
        e.process_block(&mut [&mut empty[..]]);
        let mut m = sine(1000.0, 0.5, 512, 0);
        e.process_block(&mut [&mut m[..]]);
        assert!(m.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn changing_oversampling_and_ratio_live_never_produces_nonfinite_or_huge_output() {
        let mut e = engine(Settings::default());
        let mut x = sine(220.0, 0.9, 2_048, 0);
        let ratios = [Ratio::R4, Ratio::R20, Ratio::AllButtons, Ratio::R8];
        for (i, chunk) in x.chunks_mut(256).enumerate() {
            e.set_settings(&Settings {
                ratio: ratios[i % 4],
                oversampling: [1, 2, 4][i % 3],
                attack_knob: 1.0 + (i % 7) as f64,
                ..Settings::default()
            });
            e.process_block(&mut [chunk]);
        }
        assert!(x.iter().all(|v| v.is_finite() && v.abs() < 4.0));
    }

    // ---------------------------------------------------------------- VARI-MU 660

    fn varimu(extra: impl FnOnce(&mut Settings)) -> Settings {
        let mut s = Settings {
            mode: crate::MODE_VARIMU,
            input_db: 0.0,
            threshold_db: -12.0,
            time_constant: 1,
            ..Settings::default()
        };
        extra(&mut s);
        s
    }

    #[test]
    fn varimu_steady_state_follows_the_variable_ratio_curve() {
        for level_db in [-18.0, -12.0, -6.0, 0.0] {
            let mut e = engine(varimu(|_| {}));
            let amp = 10f64.powf(level_db / 20.0);
            run_mono(&mut e, &sine(1000.0, amp, 48_000, 0));
            run_mono(&mut e, &sine(1000.0, amp, 4_800, 48_000));
            let expected = crate::varimu::static_gr_db(level_db, -12.0);
            assert!(
                (e.block_gr_db() - expected).abs() < 0.6,
                "{level_db} dBFS: GR {:.2} dB vs {expected:.2} dB",
                e.block_gr_db()
            );
        }
    }

    #[test]
    fn varimu_compresses_gently_below_threshold_and_harder_above() {
        // La ratio efectiva crece: 6 dB más de entrada dan cada vez menos salida extra.
        let out_level = |level_db: f64| {
            let mut e = engine(varimu(|_| {}));
            let amp = 10f64.powf(level_db / 20.0);
            run_mono(&mut e, &sine(1000.0, amp, 48_000, 0));
            let y = run_mono(&mut e, &sine(1000.0, amp, 24_000, 48_000));
            20.0 * harmonic(&y, 1000.0, 1).log10()
        };
        let (a, b, c) = (out_level(-30.0), out_level(-18.0), out_level(-6.0));
        let (low_slope, high_slope) = ((b - a) / 12.0, (c - b) / 12.0);
        assert!(low_slope > 0.9, "por debajo del knee casi 1:1: {low_slope}");
        assert!(high_slope < low_slope - 0.2, "{low_slope} vs {high_slope}");
    }

    #[test]
    fn varimu_tube_adds_more_even_harmonics_than_the_fet_mode() {
        let harmonics = |mode: usize| {
            let mut e = engine(Settings {
                mode,
                input_db: 0.0,
                threshold_db: 0.0, // sin compresión: sólo saturación
                oversampling: 4,
                ..Settings::default()
            });
            run_mono(&mut e, &sine(1000.0, 0.2, 48_000, 0));
            let y = run_mono(&mut e, &sine(1000.0, 0.2, 24_000, 48_000));
            let h1 = harmonic(&y, 1000.0, 1);
            (harmonic(&y, 1000.0, 2) / h1, harmonic(&y, 1000.0, 3) / h1)
        };
        let (fet_h2, _) = harmonics(crate::MODE_FET76);
        let (tube_h2, tube_h3) = harmonics(crate::MODE_VARIMU);
        println!("H2 FET {fet_h2:.4}  H2 tubo {tube_h2:.4}  H3 tubo {tube_h3:.4}");
        assert!(tube_h2 > 2.0 * tube_h3, "H2 {tube_h2} vs H3 {tube_h3}");
        assert!(tube_h2 > fet_h2 * 1.5, "tubo {tube_h2} vs FET {fet_h2}");
        assert!((0.003..0.05).contains(&tube_h2), "H2 = {tube_h2}");
    }

    #[test]
    fn varimu_transformer_adds_low_body_and_trims_highs() {
        let rel_gain_db = |mode: usize, freq: f64| {
            let mut e = engine(Settings {
                mode,
                input_db: 0.0,
                threshold_db: 0.0,
                oversampling: 2,
                ..Settings::default()
            });
            let x = sine(freq, 0.01, 48_000, 0); // -40 dBFS: sin compresión ni saturación
            let y = run_mono(&mut e, &x);
            20.0 * (harmonic(&y[24_000..], freq, 1) / harmonic(&x[24_000..], freq, 1)).log10()
        };
        let tilt = |mode: usize, freq: f64| rel_gain_db(mode, freq) - rel_gain_db(mode, 1000.0);
        let (low, high) = (
            tilt(crate::MODE_VARIMU, 60.0),
            tilt(crate::MODE_VARIMU, 15_000.0),
        );
        println!("transformador: 60 Hz {low:+.2} dB, 15 kHz {high:+.2} dB (vs 1 kHz)");
        assert!((0.6..1.4).contains(&low), "graves: {low} dB");
        assert!((-1.8..-0.8).contains(&high), "agudos: {high} dB");
        // El FET no lleva transformador: respuesta plana.
        assert!(tilt(crate::MODE_FET76, 60.0).abs() < 0.1);
        assert!(tilt(crate::MODE_FET76, 15_000.0).abs() < 0.1);
    }

    #[test]
    fn varimu_program_dependent_positions_release_slower_after_sustained_signal() {
        // Posición 6: tras una señal sostenida la reducción tarda mucho más en soltarse que
        // tras un pulso corto.
        let decay_s = |burst_samples: usize| {
            let mut e = engine(varimu(|s| s.time_constant = 6));
            let loud = sine(1000.0, 0.8, burst_samples, 0);
            run_mono(&mut e, &loud);
            let quiet = vec![0.0f32; 256];
            let mut buf = quiet.clone();
            e.process_block(&mut [&mut buf[..]]);
            let start = e.block_gr_db();
            let mut t = 0usize;
            while e.block_gr_db() > start * 0.5 && t < 48_000 * 20 {
                let mut b = quiet.clone();
                e.process_block(&mut [&mut b[..]]);
                t += 256;
            }
            t as f64 / FS
        };
        let (short, long) = (decay_s(960), decay_s(48_000 * 4));
        println!("TC6: mitad de release tras 20 ms: {short:.2} s, tras 4 s: {long:.2} s");
        assert!(long > short * 4.0, "pulso {short} s vs sostenida {long} s");
    }

    #[test]
    fn switching_mode_mid_signal_does_not_click() {
        let fet = Settings {
            input_db: 12.0,
            ..Settings::default()
        };
        let vari = varimu(|s| {
            s.input_db = 12.0;
            s.time_constant = 2;
        });
        let x = sine(220.0, 0.5, 96_000, 0);
        let max_step = |y: &[f32]| {
            y.windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0f32, f32::max)
        };

        let mut reference = engine(fet);
        let base = run_mono(&mut reference, &x);

        let mut e = engine(fet);
        let mut y = run_mono(&mut e, &x[..48_000]);
        e.set_settings(&vari);
        y.extend(run_mono(&mut e, &x[48_000..]));

        let window = 48_000..50_400; // 50 ms tras el cambio
        let natural = max_step(&base[window.clone()]);
        let after = max_step(&y[window]);
        println!("salto máximo: natural {natural:.4}, tras el cambio {after:.4}");
        assert!(after < natural * 2.5, "click: {after} vs natural {natural}");
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 2.0));
    }

    #[test]
    fn cycling_modes_and_time_constants_live_stays_finite() {
        let mut e = engine(Settings::default());
        let mut x = sine(110.0, 0.9, 6_144, 0);
        for (i, chunk) in x.chunks_mut(128).enumerate() {
            e.set_settings(&Settings {
                mode: i % 3,
                peak_reduction: (i * 17 % 101) as f64,
                limit: i % 2 == 0,
                time_constant: 1 + i % 6,
                threshold_db: -24.0 + (i % 5) as f64 * 6.0,
                oversampling: [1, 2, 4][i % 3],
                ..Settings::default()
            });
            e.process_block(&mut [chunk]);
        }
        assert!(x.iter().all(|v| v.is_finite() && v.abs() < 4.0));
    }

    // ---------------------------------------------------------------- OPTO 3A

    fn opto(extra: impl FnOnce(&mut Settings)) -> Settings {
        let mut s = Settings {
            mode: crate::MODE_OPTO3A,
            peak_reduction: 60.0,
            limit: false,
            ..Settings::default()
        };
        extra(&mut s);
        s
    }

    /// Reducción de ganancia (dB) bloque a bloque de 48 muestras (1 ms).
    fn gr_trace(e: &mut Engine, input: &[f32]) -> Vec<f64> {
        input
            .chunks(48)
            .map(|c| {
                let mut b = c.to_vec();
                e.process_block(&mut [&mut b[..]]);
                e.block_gr_db()
            })
            .collect()
    }

    #[test]
    fn opto_steady_state_follows_the_static_law() {
        for (limit, level_db) in [(false, -12.0), (false, -6.0), (true, -12.0), (true, -6.0)] {
            let mut e = engine(opto(|s| s.limit = limit));
            let amp = 10f64.powf(level_db / 20.0);
            let trace = gr_trace(&mut e, &sine(1000.0, amp, 48_000 * 2, 0));
            let gr = trace[trace.len() - 1];
            let expected = crate::opto3a::static_reduction_db(level_db, 60.0, limit);
            assert!(
                (gr - expected).abs() < 0.9,
                "limit {limit}, {level_db} dBFS: {gr:.2} dB vs {expected:.2} dB"
            );
        }
    }

    #[test]
    fn opto_attack_takes_roughly_ten_milliseconds() {
        let mut e = engine(opto(|_| {}));
        // El motor arranca con los ajustes por defecto (Input +6 dB); dejar que los
        // suavizadores de 20 ms lleguen a los del OPTO antes de medir el ataque.
        run_mono(&mut e, &vec![0.0f32; 24_000]);
        let trace = gr_trace(&mut e, &sine(1000.0, 0.5, 48_000, 0));
        let final_gr = trace[trace.len() - 1];
        let t63 = trace.iter().position(|&g| g >= 0.632 * final_gr).unwrap();
        println!("OPTO: final {final_gr:.2} dB, 63 % en {t63} ms");
        assert!((7..=25).contains(&t63), "t63 = {t63} ms");
    }

    #[test]
    fn opto_release_is_fast_for_half_then_slow_after_sustained_signal() {
        let release_times = |burst_ms: usize| {
            let mut e = engine(opto(|_| {}));
            let loud = sine(1000.0, 0.5, 48 * burst_ms, 0);
            let trace = gr_trace(&mut e, &loud);
            let start = trace[trace.len() - 1];
            let silence = vec![0.0f32; 48_000 * 12];
            let tail = gr_trace(&mut e, &silence);
            let first_below = |frac: f64| {
                tail.iter()
                    .position(|&g| g <= start * frac)
                    .unwrap_or(tail.len())
            };
            (start, first_below(0.5), first_below(0.1))
        };
        let (g_long, t50_long, t10_long) = release_times(4_000);
        let (g_short, t50_short, t10_short) = release_times(30);
        println!(
            "sostenida ({g_long:.1} dB): 50 % en {t50_long} ms, 10 % en {t10_long} ms; \
             pulso 30 ms ({g_short:.1} dB): 50 % en {t50_short} ms, 10 % en {t10_short} ms"
        );
        // Tras señal sostenida: el primer 50 % cae rápido y el resto tarda segundos.
        assert!(t50_long < 120, "50 % en {t50_long} ms");
        assert!(t10_long > 1_500, "10 % en {t10_long} ms");
        // Tras un pulso corto se suelta todo mucho antes.
        assert!(t10_short * 3 < t10_long, "{t10_short} vs {t10_long}");
    }

    #[test]
    fn opto_ignores_the_input_knob_and_responds_to_peak_reduction() {
        let x = sine(1000.0, 0.4, 24_000, 0);
        let mut a = engine(opto(|s| s.input_db = 0.0));
        let mut b = engine(opto(|s| s.input_db = 20.0));
        let (ya, yb) = (run_mono(&mut a, &x), run_mono(&mut b, &x));
        assert_eq!(ya, yb, "Input no debe afectar al OPTO 3A");

        let gr_at = |pr: f64| {
            let mut e = engine(opto(|s| s.peak_reduction = pr));
            *gr_trace(&mut e, &sine(1000.0, 0.4, 48_000, 0))
                .last()
                .unwrap()
        };
        let (low, mid, high) = (gr_at(20.0), gr_at(50.0), gr_at(80.0));
        assert!(low < mid && mid < high, "{low} {mid} {high}");
    }

    #[test]
    fn opto_limit_compresses_harder_than_compress() {
        let gr = |limit: bool| {
            let mut e = engine(opto(|s| {
                s.limit = limit;
                s.peak_reduction = 70.0;
            }));
            *gr_trace(&mut e, &sine(1000.0, 0.7, 48_000, 0))
                .last()
                .unwrap()
        };
        assert!(gr(true) > gr(false) + 2.0);
    }
}
