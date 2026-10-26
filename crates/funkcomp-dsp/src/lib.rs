//! Motor de compresión de FunkComp.
//!
//! Cadena: `Input -> [detector] -> modo de compresión -> saturación (oversampled)
//! -> DC blocker -> [coloración de transformador] -> Output -> Mix/Bypass`. Cada modo
//! (FET 76, VARI-MU 660; OPTO 3A en la fase siguiente) implementa [`CompressorMode`].
//!
//! Módulos:
//! * [`detector`]: HPF de side-chain y nivel de pico enlazado entre canales.
//! * [`gain`]: curva estática con knee y suavizadores de ganancia (1 y 2 etapas de release).
//! * [`saturation`]: waveshaper asimétrico normalizado.
//! * [`oversampling`]: oversamplers pre-asignados (1x/2x/4x) y su latencia.
//! * [`fet76`]: modo FET 76 ("inspirado en" un compresor FET clásico).
//! * [`opto3a`]: modo OPTO 3A ("inspirado en" un compresor óptico clásico).
//! * [`varimu`]: modo VARI-MU 660 ("inspirado en" un compresor de tubos variable-mu).
//! * [`engine`]: orquesta todo por bloques `f32` y reporta latencia.

pub mod detector;
pub mod engine;
pub mod factory;
pub mod fet76;
pub mod gain;
pub mod opto3a;
pub mod oversampling;
pub mod saturation;
pub mod varimu;

pub use engine::Engine;

/// Índices de modo en [`Settings::mode`].
pub const MODE_FET76: usize = 0;
pub const MODE_VARIMU: usize = 1;
pub const MODE_OPTO3A: usize = 2;

/// Selector de ratio del modo FET 76. `AllButtons` imita el modo "todos los botones".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ratio {
    R4,
    R8,
    R12,
    R20,
    AllButtons,
}

/// Ajustes de usuario que recibe el motor. Barato de copiar y comparar: el plugin lo
/// reconstruye en cada bloque y el motor sólo reacciona si cambió algo.
///
/// Cada modo usa sólo los campos que le corresponden (p. ej. `attack_knob`, `release_knob`
/// y `ratio` son del FET 76; `threshold_db` y `time_constant` son del VARI-MU).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Índice de modo ([`MODE_FET76`], [`MODE_VARIMU`]).
    pub mode: usize,
    pub input_db: f64,
    pub output_db: f64,
    /// Posición de la perilla 1..=7 (7 = más rápido), como en el hardware.
    pub attack_knob: f64,
    pub release_knob: f64,
    pub ratio: Ratio,
    /// Umbral (dB, centro del knee) del VARI-MU.
    pub threshold_db: f64,
    /// Posición 1..=6 del selector Time Constant del VARI-MU.
    pub time_constant: usize,
    /// Peak Reduction del OPTO 3A (0..=100): sensibilidad del side-chain.
    pub peak_reduction: f64,
    /// OPTO 3A: `false` = Compress, `true` = Limit.
    pub limit: bool,
    /// `Some(hz)` activa el HPF de side-chain.
    pub sidechain_hpf_hz: Option<f64>,
    /// 0 = sólo seco, 1 = sólo procesado.
    pub mix: f64,
    pub auto_gain: bool,
    /// 1, 2 o 4.
    pub oversampling: u8,
    pub bypass: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: MODE_FET76,
            input_db: 6.0,
            output_db: 0.0,
            attack_knob: 4.0,
            release_knob: 5.0,
            ratio: Ratio::R4,
            threshold_db: -12.0,
            time_constant: 2,
            peak_reduction: 50.0,
            limit: false,
            sidechain_hpf_hz: None,
            mix: 1.0,
            auto_gain: false,
            oversampling: 2,
            bypass: false,
        }
    }
}

/// Waveshaper normalizado (ver [`saturation::Shaper`]).
pub use saturation::Shaper;

/// Coloración lineal que un modo aplica a la señal procesada (p. ej. un transformador de
/// salida: algo más de cuerpo en graves y agudos algo recortados). Ganancias en dB; con
/// `0.0` en ambas no se ejecuta ningún filtro.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coloration {
    pub low_shelf_db: f64,
    pub low_shelf_hz: f64,
    pub high_shelf_db: f64,
    pub high_shelf_hz: f64,
}

impl Coloration {
    pub const NONE: Self = Self {
        low_shelf_db: 0.0,
        low_shelf_hz: 100.0,
        high_shelf_db: 0.0,
        high_shelf_hz: 10_000.0,
    };

    pub fn is_none(&self) -> bool {
        self.low_shelf_db == 0.0 && self.high_shelf_db == 0.0
    }
}

/// Contrato común de los modos de compresión.
///
/// El motor llama a [`CompressorMode::gain_reduction_db`] una vez por muestra con el nivel
/// de pico (lineal, ya multiplicado por la ganancia de entrada) y aplica la reducción
/// devuelta. La saturación no lleva estado, así que se entrega como [`Shaper`] por valor y
/// puede ejecutarse dentro del oversampler sin pelear con el borrow checker.
pub trait CompressorMode: Send {
    fn prepare(&mut self, sample_rate: f64);
    fn configure(&mut self, settings: &Settings);
    fn reset(&mut self);
    /// Arranca el modo con una reducción de ganancia ya aplicada (dB). El motor lo llama al
    /// cambiar de modo, después de `reset`, para que la ganancia no dé un salto: el modo
    /// nuevo evoluciona desde la reducción vigente hacia la suya con sus propios tiempos.
    fn warm_start(&mut self, gain_reduction_db: f64);
    /// Reducción de ganancia en dB (>= 0) a aplicar en esta muestra.
    fn gain_reduction_db(&mut self, detector_level: f64) -> f64;
    /// Forma de la saturación para la reducción actual.
    fn shaper(&self, gain_reduction_db: f64) -> Shaper;
    /// Compensación estática de nivel (dB) para el auto-gain, según la ganancia de entrada.
    fn auto_gain_db(&self, input_db: f64) -> f64;
    /// `false` si el modo no tiene control Input (el OPTO 3A usa Peak Reduction): el motor
    /// ignora entonces `input_db`.
    fn uses_input(&self) -> bool {
        true
    }
    /// Coloración lineal de la etapa de salida (ninguna por defecto).
    fn coloration(&self) -> Coloration {
        Coloration::NONE
    }
}
