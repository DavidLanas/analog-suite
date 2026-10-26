//! Widgets y estilo compartidos por los tres plugins (egui, render en CPU/iGPU).
//!
//! * [`theme`]: paleta y estilos de texto coherentes.
//! * [`knob`]: perilla con pasos (detents) enlazada a un parámetro de nih-plug.
//! * [`selector`]: selector de posiciones tipo conmutador rotativo/botonera.
//! * [`vu`]: vúmetro de aguja (VU o reducción de ganancia).
//! * [`faceplate`]: carátula/panel con tornillos y la leyenda "inspirado en".

pub mod faceplate;
pub mod knob;
pub mod selector;
pub mod theme;
pub mod vu;

pub use nih_plug_egui::egui;
