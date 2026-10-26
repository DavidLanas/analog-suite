//! Detector: HPF de side-chain y nivel de pico enlazado.

use dsp_core::biquad::{Biquad, BiquadCoeffs};

/// HPF de 2º orden (Butterworth) sólo para la señal de detección; no toca el audio.
#[derive(Clone, Copy, Debug)]
pub struct SidechainHpf {
    filters: [Biquad; 2],
    enabled: bool,
}

impl SidechainHpf {
    pub fn new() -> Self {
        Self {
            filters: [Biquad::default(); 2],
            enabled: false,
        }
    }

    /// `None` desactiva el filtro. Al reactivarlo se limpian los estados para evitar
    /// arrastrar energía vieja.
    pub fn configure(&mut self, sample_rate: f64, freq_hz: Option<f64>) {
        match freq_hz {
            Some(f) => {
                let c = BiquadCoeffs::highpass(sample_rate, f, std::f64::consts::FRAC_1_SQRT_2);
                for filt in &mut self.filters {
                    filt.set_coeffs(c);
                }
                if !self.enabled {
                    self.reset();
                }
                self.enabled = true;
            }
            None => self.enabled = false,
        }
    }

    pub fn reset(&mut self) {
        self.filters.iter_mut().for_each(Biquad::reset);
    }

    /// Nivel de pico enlazado (máximo del valor absoluto entre canales, post-HPF).
    #[inline]
    pub fn linked_peak(&mut self, frame: &[f64; 2], channels: usize) -> f64 {
        let mut peak: f64 = 0.0;
        for (ch, &x) in frame.iter().enumerate().take(channels) {
            let d = if self.enabled {
                self.filters[ch].process(x)
            } else {
                x
            };
            peak = peak.max(d.abs());
        }
        peak
    }
}

impl Default for SidechainHpf {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const FS: f64 = 48_000.0;

    fn peak_of_sine(hpf: &mut SidechainHpf, freq: f64) -> f64 {
        let mut p: f64 = 0.0;
        for i in 0..48_000 {
            let x = (2.0 * PI * freq * i as f64 / FS).sin();
            let lvl = hpf.linked_peak(&[x, 0.0], 1);
            if i > 24_000 {
                p = p.max(lvl);
            }
        }
        p
    }

    #[test]
    fn hpf_removes_lows_from_detection_only_when_enabled() {
        let mut off = SidechainHpf::new();
        assert!((peak_of_sine(&mut off, 40.0) - 1.0).abs() < 1e-3);

        let mut on = SidechainHpf::new();
        on.configure(FS, Some(150.0));
        let low = peak_of_sine(&mut on, 40.0);
        assert!(low < 0.1, "40 Hz con HPF 150 Hz debería caer mucho: {low}");
        let mut on2 = SidechainHpf::new();
        on2.configure(FS, Some(150.0));
        assert!((peak_of_sine(&mut on2, 3000.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn linked_peak_takes_the_louder_channel() {
        let mut d = SidechainHpf::new();
        assert_eq!(d.linked_peak(&[0.2, -0.7], 2), 0.7);
        // En mono sólo cuenta el canal 0.
        assert_eq!(d.linked_peak(&[0.2, -0.7], 1), 0.2);
    }
}
