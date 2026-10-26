//! Núcleo de DSP compartido por FunkComp, PultecMaster y TapeMaster.
//!
//! Reglas de diseño:
//! * Todo en `f64`; la conversión desde/hacia `f32` ocurre en el borde del plugin.
//! * Ninguna ruta de `process`/`push`/`next` reserva memoria, bloquea o hace I/O.
//!   Las asignaciones (buffers, tablas de coeficientes) ocurren sólo en `new`/`set_*`
//!   que se llaman fuera del hilo de audio (`initialize`).
//! * Este crate no depende de `nih-plug` para poder testearse y medirse aislado.

pub mod biquad;
pub mod denormal;
pub mod envelope;
pub mod meter;
pub mod oversampling;
pub mod saturation;
pub mod smoothing;

/// dB -> ganancia lineal de amplitud.
#[inline]
pub fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db * 0.05)
}

/// Ganancia lineal de amplitud -> dB (con suelo en -240 dB para evitar `-inf`).
#[inline]
pub fn lin_to_db(lin: f64) -> f64 {
    20.0 * lin.abs().max(1e-12).log10()
}
