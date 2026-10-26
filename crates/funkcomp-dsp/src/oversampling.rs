//! Oversamplers pre-asignados para 2 canales y factores 2x/4x.
//!
//! Se construyen todos en `new` (fuera del hilo de audio) para poder cambiar de factor en
//! caliente sin asignar memoria.

use dsp_core::oversampling::Oversampler;

pub struct StereoOversampler {
    x2: [Oversampler; 2],
    x4: [Oversampler; 2],
}

impl StereoOversampler {
    pub fn new() -> Self {
        Self {
            x2: [Oversampler::new(2), Oversampler::new(2)],
            x4: [Oversampler::new(4), Oversampler::new(4)],
        }
    }

    pub fn reset(&mut self) {
        self.x2.iter_mut().for_each(Oversampler::reset);
        self.x4.iter_mut().for_each(Oversampler::reset);
    }

    /// Latencia (muestras a la frecuencia base) del factor dado. 1x => 0.
    ///
    /// En 4x es fraccional (`C + C/2` con `C` impar): el motor la redondea para alinear la
    /// señal seca, lo que deja ≤ 0.5 muestras de desalineación (sólo afecta a agudos con Mix
    /// parcial).
    pub fn latency_samples(&self, factor: u8) -> f64 {
        match factor {
            2 => self.x2[0].latency_samples(),
            4 => self.x4[0].latency_samples(),
            _ => 0.0,
        }
    }

    #[inline]
    pub fn process<F: FnMut(f64) -> f64>(
        &mut self,
        channel: usize,
        factor: u8,
        x: f64,
        f: &mut F,
    ) -> f64 {
        match factor {
            2 => self.x2[channel].process(x, f),
            4 => self.x4[channel].process(x, f),
            _ => f(x),
        }
    }
}

impl Default for StereoOversampler {
    fn default() -> Self {
        Self::new()
    }
}
