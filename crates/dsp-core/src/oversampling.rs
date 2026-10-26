//! Oversampling 1x/2x/4x/8x con etapas half-band FIR de fase lineal.
//!
//! Cada etapa duplica la frecuencia de muestreo. El filtro half-band (sinc con ventana
//! Kaiser) sólo tiene taps no nulos en posiciones impares respecto al centro, lo que
//! permite una implementación polifásica con la mitad de multiplicaciones.
//!
//! Latencia: cada etapa `k` (1-based) aporta `C` muestras a la frecuencia `2^k · fs`,
//! es decir `C / 2^(k-1)` muestras a la frecuencia base, donde `C` es el índice del tap
//! central. Se informa con [`Oversampler::latency_samples`].

use std::f64::consts::PI;

/// Índice del tap central. Debe ser impar (ver derivación polifásica en `HalfbandUp`).
/// `C = 23` → 47 taps, ≈ 80 dB de rechazo con Kaiser β = 8.
const CENTER: usize = 23;
const KAISER_BETA: f64 = 8.0;

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x * 0.25;
    for k in 1..60 {
        term *= q / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Taps pares `h[2j]` (j = 0..=C) del half-band completo (son todos los no nulos salvo el
/// central, que vale 0.5 y se trata aparte). Son simétricos: `h[2j] = h[2(C-j)]`.
fn halfband_even_taps() -> Vec<f64> {
    let n = 2 * CENTER + 1;
    let c = CENTER as f64;
    let i0_beta = bessel_i0(KAISER_BETA);
    let mut h = vec![0.0; n];
    for (k, hk) in h.iter_mut().enumerate() {
        let m = k as f64 - c;
        if k == CENTER {
            *hk = 0.5;
        } else if (k as isize - CENTER as isize).rem_euclid(2) == 1 {
            let sinc = (PI * m * 0.5).sin() / (PI * m * 0.5);
            let r = m / c;
            let w = bessel_i0(KAISER_BETA * (1.0 - r * r).max(0.0).sqrt()) / i0_beta;
            *hk = 0.5 * sinc * w;
        }
    }
    // Ganancia DC = 1: los taps no centrales deben sumar 0.5.
    let side: f64 = h
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != CENTER)
        .map(|(_, v)| *v)
        .sum();
    let scale = 0.5 / side;
    for (k, v) in h.iter_mut().enumerate() {
        if k != CENTER {
            *v *= scale;
        }
    }
    // CENTER es impar => los taps no nulos no centrales están en índices pares 0, 2, .., 2C.
    (0..=CENTER).map(|j| h[2 * j]).collect()
}

const HALF: usize = CENTER + 1; // nº de taps pares (k = 0, 2, .., 2C)

/// Ring buffer fijo con acceso `x[n - j]`.
#[derive(Clone)]
struct History {
    buf: [f64; 32],
    pos: usize,
}

impl History {
    fn new() -> Self {
        Self {
            buf: [0.0; 32],
            pos: 0,
        }
    }
    #[inline]
    fn push(&mut self, x: f64) {
        self.pos = (self.pos + 1) & 31;
        self.buf[self.pos] = x;
    }
    /// `x[n - j]`, con `x[n]` el último valor empujado.
    #[inline]
    fn get(&self, j: usize) -> f64 {
        self.buf[(self.pos.wrapping_sub(j)) & 31]
    }
}

#[derive(Clone)]
struct Stage {
    taps: [f64; HALF],
    up_hist: History,
    down_even: History,
    down_odd: History,
}

impl Stage {
    fn new(taps: &[f64]) -> Self {
        let mut t = [0.0; HALF];
        t.copy_from_slice(taps);
        Self {
            taps: t,
            up_hist: History::new(),
            down_even: History::new(),
            down_odd: History::new(),
        }
    }

    /// 1 muestra -> 2 muestras. Fase par: FIR sobre taps pares. Fase impar: retardo puro.
    #[inline]
    fn up(&mut self, x: f64) -> (f64, f64) {
        self.up_hist.push(x);
        let mut even = 0.0;
        for (j, t) in self.taps.iter().enumerate() {
            even += t * self.up_hist.get(j);
        }
        let odd = self.up_hist.get((CENTER - 1) / 2);
        (2.0 * even, odd) // 2·h[center]=1 en la fase impar
    }

    /// 2 muestras -> 1 muestra.
    #[inline]
    fn down(&mut self, even_in: f64, odd_in: f64) -> f64 {
        self.down_even.push(even_in);
        self.down_odd.push(odd_in);
        let mut y = 0.0;
        for (j, t) in self.taps.iter().enumerate() {
            y += t * self.down_even.get(j);
        }
        y + 0.5 * self.down_odd.get(CENTER.div_ceil(2))
    }

    fn reset(&mut self) {
        self.up_hist = History::new();
        self.down_even = History::new();
        self.down_odd = History::new();
    }
}

/// Oversampler mono. Crea uno por canal.
#[derive(Clone)]
pub struct Oversampler {
    stages: Vec<Stage>,
}

impl Oversampler {
    /// `factor`: 1, 2, 4 u 8. Asigna memoria: llamar fuera del hilo de audio.
    pub fn new(factor: usize) -> Self {
        assert!(
            matches!(factor, 1 | 2 | 4 | 8),
            "factor debe ser 1, 2, 4 u 8"
        );
        let taps = halfband_even_taps();
        let stages = (0..factor.trailing_zeros())
            .map(|_| Stage::new(&taps))
            .collect();
        Self { stages }
    }

    pub fn factor(&self) -> usize {
        1 << self.stages.len()
    }

    pub fn reset(&mut self) {
        self.stages.iter_mut().for_each(Stage::reset);
    }

    /// Latencia total (subida + bajada) en muestras a la frecuencia base (puede ser fraccional).
    pub fn latency_samples(&self) -> f64 {
        (0..self.stages.len())
            .map(|k| CENTER as f64 / (1usize << k) as f64)
            .sum()
    }

    /// Procesa una muestra: sube, aplica `f` a cada muestra sobremuestreada y baja.
    #[inline]
    pub fn process<F: FnMut(f64) -> f64>(&mut self, x: f64, f: &mut F) -> f64 {
        self.process_level(0, x, f)
    }

    #[inline]
    fn process_level<F: FnMut(f64) -> f64>(&mut self, level: usize, x: f64, f: &mut F) -> f64 {
        if level == self.stages.len() {
            return f(x);
        }
        let (a, b) = self.stages[level].up(x);
        let ya = self.process_level(level + 1, a, f);
        let yb = self.process_level(level + 1, b, f);
        self.stages[level].down(ya, yb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_have_unit_dc_gain() {
        let even = halfband_even_taps();
        assert_eq!(even.len(), HALF);
        // Taps pares + tap central (0.5) deben sumar 1, y ser simétricos.
        let sum = 0.5 + even.iter().sum::<f64>();
        assert!((sum - 1.0).abs() < 1e-12, "sum = {sum}");
        for j in 0..=CENTER {
            assert!((even[j] - even[CENTER - j]).abs() < 1e-12);
        }
    }

    #[test]
    fn dc_passes_with_unity_gain() {
        for factor in [1, 2, 4, 8] {
            let mut os = Oversampler::new(factor);
            let mut y = 0.0;
            for _ in 0..200 {
                y = os.process(1.0, &mut |x| x);
            }
            assert!((y - 1.0).abs() < 1e-9, "factor {factor}: y = {y}");
        }
    }

    #[test]
    fn impulse_peak_appears_at_reported_latency() {
        for factor in [2, 4, 8] {
            let mut os = Oversampler::new(factor);
            let latency = os.latency_samples().round() as usize;
            let mut best = (0usize, 0.0f64);
            for n in 0..200 {
                let x = if n == 0 { 1.0 } else { 0.0 };
                let y = os.process(x, &mut |v| v);
                if y.abs() > best.1 {
                    best = (n, y.abs());
                }
            }
            assert!(
                (best.0 as isize - latency as isize).abs() <= 1,
                "factor {factor}: pico en {} vs latencia {latency}",
                best.0
            );
            assert!(best.1 > 0.3);
        }
    }

    #[test]
    fn passband_sine_keeps_unity_gain() {
        let fs = 48_000.0;
        let f0 = 1000.0;
        for factor in [2, 4] {
            let mut os = Oversampler::new(factor);
            let mut peak: f64 = 0.0;
            for n in 0..4000 {
                let x = (2.0 * PI * f0 * n as f64 / fs).sin();
                let y = os.process(x, &mut |v| v);
                if n > 2000 {
                    peak = peak.max(y.abs());
                }
            }
            assert!((peak - 1.0).abs() < 0.01, "factor {factor}: peak = {peak}");
        }
    }

    #[test]
    fn oversampling_reduces_aliasing_of_a_hard_nonlinearity() {
        // Seno de 9 kHz muy saturado a 48 kHz: sus armónicos impares se pliegan.
        let fs = 48_000.0;
        let f0 = 9_000.0;
        let n = 8192;
        let drive = |x: f64| (x * 8.0).tanh();

        let mut alias_plain = 0.0;
        let mut alias_os = 0.0;

        let mut os = Oversampler::new(4);
        let mut plain = Vec::with_capacity(n);
        let mut over = Vec::with_capacity(n);
        for i in 0..n {
            let x = (2.0 * PI * f0 * i as f64 / fs).sin();
            plain.push(drive(x));
            over.push(os.process(x, &mut |v| drive(v)));
        }

        // Los armónicos de 9 kHz plegados a 48 kHz caen en múltiplos de 3 kHz. 3 kHz no es
        // armónico real (< Nyquist sólo existe 9 kHz y 27 kHz > Nyquist), así que su energía
        // es alias puro (45 kHz -> 3 kHz). Se mide con Goertzel.
        let probe = 3_000.0;
        let g = |sig: &[f64]| {
            let w = 2.0 * PI * probe / fs;
            let coeff = 2.0 * w.cos();
            let (mut s1, mut s2) = (0.0, 0.0);
            for v in &sig[1024..] {
                let s = v + coeff * s1 - s2;
                s2 = s1;
                s1 = s;
            }
            (s1 * s1 + s2 * s2 - coeff * s1 * s2).sqrt() / (sig.len() - 1024) as f64
        };
        alias_plain += g(&plain);
        alias_os += g(&over);
        assert!(
            alias_os < alias_plain * 0.1,
            "alias sin OS {alias_plain:.3e}, con OS {alias_os:.3e}"
        );
    }
}
