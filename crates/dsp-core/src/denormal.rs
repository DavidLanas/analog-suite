//! Protección anti-denormales.
//!
//! nih-plug activa FTZ/DAZ del procesador durante `process()`, pero los estados de los
//! filtros IIR pueden decaer a valores subnormales en `f64` si el host no lo hace
//! (o en tests). Esta función los barre a cero de forma explícita y barata.

/// Umbral por debajo del cual un estado se considera cero (-600 dB).
pub const THRESHOLD: f64 = 1e-30;

/// Devuelve `0.0` si `x` es subnormal o despreciable; en otro caso `x`.
#[inline(always)]
pub fn flush(x: f64) -> f64 {
    if x.abs() < THRESHOLD {
        0.0
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flushes_tiny_values() {
        assert_eq!(flush(1e-40), 0.0);
        assert_eq!(flush(-1e-310), 0.0); // subnormal
        assert_eq!(flush(0.5), 0.5);
        assert_eq!(flush(-1e-20), -1e-20);
    }
}
