//! Presets de fábrica de FunkComp, como datos puros (sin dependencias de nih-plug).
//!
//! Cada preset declara el nivel de pico con el que se afinó (`ref_peak_db`) y la reducción de
//! ganancia que busca con ese material (`target_gr_db`). Los tests lo verifican de verdad:
//! pasan un programa de referencia por el motor y comprueban que la reducción máxima cae en el
//! rango declarado y que el nivel de salida queda cerca del de entrada (los presets fijan el
//! Output para compensar). Son **puntos de partida**: afinar de oído y según el material.
//!
//! Los nombres que evocan equipos o épocas son sólo una guía de carácter ("inspirado en"); no
//! son réplicas. Las etapas Preamp y Tape opcionales todavía no existen, así que ningún preset
//! las usa.

#[cfg(test)]
use crate::Engine;
use crate::{Ratio, Settings, MODE_FET76, MODE_OPTO3A, MODE_VARIMU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Voice,
    Instruments,
    Mastering,
}

impl Category {
    pub const ALL: [Category; 3] = [Category::Voice, Category::Instruments, Category::Mastering];

    /// Nombre mostrado y guardado en los archivos de preset.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Voice => "Voice",
            Category::Instruments => "Instruments",
            Category::Mastering => "Mastering",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FactoryPreset {
    pub name: &'static str,
    pub category: Category,
    pub description: &'static str,
    /// Pico (dBFS) del material de referencia con el que se afinó.
    pub ref_peak_db: f64,
    /// Reducción de ganancia máxima esperada (dB) con ese material: (mínimo, máximo).
    pub target_gr_db: (f64, f64),
    pub settings: Settings,
}

fn with(f: impl FnOnce(&mut Settings)) -> Settings {
    let mut s = Settings::default();
    f(&mut s);
    s
}

/// Los presets de fábrica, agrupados por categoría.
pub fn presets() -> Vec<FactoryPreset> {
    use Category::*;
    vec![
        // ------------------------------------------------------------------ Voz
        FactoryPreset {
            name: "MJ Lead Vocal",
            category: Voice,
            description:
                "FET 76, 12:1, ataque medio, release medio-rápido. Voz principal presente \
                          y controlada, estilo pop/funk (inspirado en un 1176 clásico).",
            ref_peak_db: -10.0,
            target_gr_db: (4.0, 8.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 9.5;
                s.ratio = Ratio::R12;
                s.attack_knob = 3.5;
                s.release_knob = 5.0;
                s.output_db = -3.5;
            }),
        },
        FactoryPreset {
            name: "Motown 60s Lead",
            category: Voice,
            description: "VARI-MU, Time Constant 2. Compresión suave y redondeada, 2–4 dB, \
                          con algo de cuerpo (inspirado en un compresor de tubos de los 60).",
            ref_peak_db: -10.0,
            target_gr_db: (2.0, 4.0),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -11.0;
                s.time_constant = 2;
                s.output_db = 3.0;
            }),
        },
        FactoryPreset {
            name: "Jackson 5 Light",
            category: Voice,
            description: "OPTO 3A en Compress, compresión ligera y muy natural \
                          (inspirado en un compresor óptico clásico).",
            ref_peak_db: -10.0,
            target_gr_db: (1.5, 4.0),
            settings: with(|s| {
                s.mode = MODE_OPTO3A;
                s.peak_reduction = 43.0;
                s.limit = false;
                s.output_db = 2.5;
            }),
        },
        FactoryPreset {
            name: "Cincinnati Soul 1965",
            category: Voice,
            description: "VARI-MU, compresión muy ligera que respeta la dinámica y los \
                          transientes (0.5–2 dB).",
            ref_peak_db: -10.0,
            target_gr_db: (0.5, 2.0),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -7.5;
                s.time_constant = 2;
                s.output_db = 1.0;
            }),
        },
        FactoryPreset {
            name: "Funk Shout",
            category: Voice,
            description: "FET 76, 8:1, ataque rápido, mezcla paralela ~60 %. Voz agresiva y \
                          al frente sin perder el ataque.",
            ref_peak_db: -10.0,
            target_gr_db: (8.0, 14.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 15.5;
                s.ratio = Ratio::R8;
                s.attack_knob = 6.0;
                s.release_knob = 5.5;
                s.mix = 0.6;
                s.output_db = -5.0;
            }),
        },
        FactoryPreset {
            name: "Vocal Glue",
            category: Voice,
            description: "VARI-MU, Time Constant 5 (release dependiente del programa), ratio \
                          bajo: une la voz con suavidad (2–4 dB).",
            ref_peak_db: -10.0,
            target_gr_db: (2.0, 4.0),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -11.0;
                s.time_constant = 5;
                s.output_db = 3.0;
            }),
        },
        FactoryPreset {
            name: "Neo-Soul Vocal Smooth",
            category: Voice,
            description: "OPTO 3A en Compress, reducción moderada y release lento que respeta \
                          los melismas y la respiración.",
            ref_peak_db: -10.0,
            target_gr_db: (3.0, 6.0),
            settings: with(|s| {
                s.mode = MODE_OPTO3A;
                s.peak_reduction = 50.0;
                s.limit = false;
                s.output_db = 4.0;
            }),
        },
        // ---------------------------------------------------------- Instrumentos
        FactoryPreset {
            name: "Funk Bass Tight",
            category: Instruments,
            description: "FET 76, 8:1, ataque medio y release rápido: bajo firme y parejo sin \
                          perder el golpe.",
            ref_peak_db: -10.0,
            target_gr_db: (4.0, 8.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 7.0;
                s.ratio = Ratio::R8;
                s.attack_knob = 4.0;
                s.release_knob = 5.5;
                s.sidechain_hpf_hz = Some(80.0);
                s.output_db = -1.5;
            }),
        },
        FactoryPreset {
            name: "Slap Bass Control",
            category: Instruments,
            description: "FET 76, 12:1, ataque y release rápidos: doma los picos del slap.",
            ref_peak_db: -10.0,
            target_gr_db: (6.0, 11.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 9.5;
                s.ratio = Ratio::R12;
                s.attack_knob = 6.0;
                s.release_knob = 6.0;
                s.sidechain_hpf_hz = Some(100.0);
                s.output_db = -1.5;
            }),
        },
        FactoryPreset {
            name: "Rhodes Smooth",
            category: Instruments,
            description: "OPTO 3A en Compress, 2–4 dB: piano eléctrico parejo y sedoso.",
            ref_peak_db: -10.0,
            target_gr_db: (2.0, 4.5),
            settings: with(|s| {
                s.mode = MODE_OPTO3A;
                s.peak_reduction = 45.0;
                s.limit = false;
                s.output_db = 3.0;
            }),
        },
        FactoryPreset {
            name: "Piano Neo-Soul Glue",
            category: Instruments,
            description: "VARI-MU, Time Constant 3: unifica acordes y notas sueltas con 1–3 dB \
                          de reducción, conservando el cuerpo.",
            ref_peak_db: -10.0,
            target_gr_db: (1.0, 3.0),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -9.0;
                s.time_constant = 3;
                s.output_db = 2.0;
            }),
        },
        FactoryPreset {
            name: "Funk Guitar Tight",
            category: Instruments,
            description: "FET 76, 4:1, ataque lento para dejar pasar el rasgueo y release \
                          rápido: guitarra rítmica firme.",
            ref_peak_db: -10.0,
            target_gr_db: (3.0, 6.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 6.0;
                s.ratio = Ratio::R4;
                s.attack_knob = 2.5;
                s.release_knob = 6.0;
                s.sidechain_hpf_hz = Some(120.0);
                s.output_db = -2.0;
            }),
        },
        FactoryPreset {
            name: "Acoustic Guitar Even",
            category: Instruments,
            description: "OPTO 3A en Compress: iguala la dinámica de la guitarra acústica sin \
                          apagarla.",
            ref_peak_db: -10.0,
            target_gr_db: (2.0, 4.0),
            settings: with(|s| {
                s.mode = MODE_OPTO3A;
                s.peak_reduction = 44.0;
                s.limit = false;
                s.output_db = 3.0;
            }),
        },
        FactoryPreset {
            name: "Horn Section Leveler",
            category: Instruments,
            description: "OPTO 3A en Limit: nivela la sección de vientos y controla los picos.",
            ref_peak_db: -10.0,
            target_gr_db: (3.0, 7.0),
            settings: with(|s| {
                s.mode = MODE_OPTO3A;
                s.peak_reduction = 47.0;
                s.limit = true;
                s.output_db = 4.5;
            }),
        },
        FactoryPreset {
            name: "Drum Bus Glue",
            category: Instruments,
            description: "VARI-MU, Time Constant 2, filtro de side-chain a 100 Hz: pega el \
                          kit sin que el bombo dispare toda la compresión.",
            ref_peak_db: -10.0,
            target_gr_db: (2.0, 5.0),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -9.0;
                s.time_constant = 2;
                s.sidechain_hpf_hz = Some(100.0);
                s.output_db = 3.5;
            }),
        },
        FactoryPreset {
            name: "Drum Crush (Parallel)",
            category: Instruments,
            description: "FET 76 con todos los botones, mezcla paralela 40 %: batería \
                          agresiva y con cuerpo.",
            ref_peak_db: -10.0,
            target_gr_db: (10.0, 20.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 15.5;
                s.ratio = Ratio::AllButtons;
                s.attack_knob = 3.0;
                s.release_knob = 6.0;
                s.mix = 0.4;
                s.output_db = -0.5;
            }),
        },
        // ------------------------------------------------------------ Mastering
        FactoryPreset {
            name: "Master Glue Gentle",
            category: Mastering,
            description: "VARI-MU, Time Constant 4, side-chain a 90 Hz, 4x: pegamento \
                          suave de 1–2 dB en la mezcla final.",
            ref_peak_db: -6.0,
            target_gr_db: (1.0, 2.5),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -2.0;
                s.time_constant = 4;
                s.sidechain_hpf_hz = Some(90.0);
                s.oversampling = 4;
                s.output_db = 1.5;
            }),
        },
        FactoryPreset {
            name: "Master Opto Smooth",
            category: Mastering,
            description: "OPTO 3A en Compress, reducción mínima (0.5–2 dB): suaviza la \
                          dinámica sin colorear.",
            ref_peak_db: -6.0,
            target_gr_db: (0.5, 2.0),
            settings: with(|s| {
                s.mode = MODE_OPTO3A;
                s.peak_reduction = 25.0;
                s.limit = false;
                s.oversampling = 4;
                s.output_db = 1.5;
            }),
        },
        FactoryPreset {
            name: "Master Transparent Touch",
            category: Mastering,
            description: "VARI-MU, Time Constant 6 (release dependiente del programa), \
                          0.5–1.5 dB: apenas se nota, sólo da cohesión.",
            ref_peak_db: -6.0,
            target_gr_db: (0.5, 1.5),
            settings: with(|s| {
                s.mode = MODE_VARIMU;
                s.input_db = 0.0;
                s.threshold_db = -0.5;
                s.time_constant = 6;
                s.sidechain_hpf_hz = Some(80.0);
                s.oversampling = 4;
                s.output_db = 1.0;
            }),
        },
        FactoryPreset {
            name: "Mix Bus Punch",
            category: Mastering,
            description: "FET 76, 4:1, ataque lento y release rápido, side-chain a 100 Hz: \
                          conserva los transientes y da empuje a la mezcla.",
            ref_peak_db: -6.0,
            target_gr_db: (2.0, 4.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = 0.5;
                s.ratio = Ratio::R4;
                s.attack_knob = 2.0;
                s.release_knob = 5.0;
                s.sidechain_hpf_hz = Some(100.0);
                s.oversampling = 4;
                s.output_db = 2.5;
            }),
        },
        FactoryPreset {
            name: "Peak Tamer",
            category: Mastering,
            description: "FET 76, 20:1, ataque rápido: recorta los picos sueltos antes del \
                          limitador final (1–3 dB).",
            ref_peak_db: -6.0,
            target_gr_db: (1.0, 3.0),
            settings: with(|s| {
                s.mode = MODE_FET76;
                s.input_db = -1.5;
                s.ratio = Ratio::R20;
                s.attack_knob = 7.0;
                s.release_knob = 3.5;
                s.sidechain_hpf_hz = Some(100.0);
                s.oversampling = 4;
                s.output_db = 3.5;
            }),
        },
    ]
}

/// Programa de referencia de los tests: 8 repeticiones de "frase" (250 ms de tono con
/// armónico al pico indicado, 120 ms 10 dB más bajos, 130 ms de silencio). Devuelve muestras
/// mono a `sample_rate`.
pub fn reference_program(peak_db: f64, sample_rate: f64) -> Vec<f32> {
    use std::f64::consts::PI;
    let amp = 10f64.powf(peak_db / 20.0);
    let ms = |t: f64| (t * 1e-3 * sample_rate) as usize;
    let mut out = Vec::new();
    for _ in 0..8 {
        for (len, level) in [(ms(250.0), 1.0), (ms(120.0), 0.316), (ms(130.0), 0.0)] {
            for i in 0..len {
                let t = i as f64 / sample_rate;
                // 220 Hz + 3er armónico: pico normalizado a 1/1.3.
                let v = ((2.0 * PI * 220.0 * t).sin() + 0.3 * (2.0 * PI * 660.0 * t).sin()) / 1.3;
                out.push((amp * level * v) as f32);
            }
        }
    }
    out
}

#[cfg(test)]
const FS: f64 = 48_000.0;

#[cfg(test)]
/// (reducción máxima en la segunda mitad del programa, diferencia de nivel RMS en dB).
pub(crate) fn measure(settings: &Settings, ref_peak_db: f64) -> (f64, f64) {
    let mut e = Engine::new();
    e.prepare(FS);
    e.set_settings(settings);
    // Dejar que los suavizadores de parámetros lleguen a su valor antes de medir.
    let mut warm = vec![0.0f32; 24_000];
    e.process_block(&mut [&mut warm[..]]);

    let input = reference_program(ref_peak_db, FS);
    let mut output = input.clone();
    let mut max_gr: f64 = 0.0;
    let half = input.len() / 2;
    for (i, chunk) in output.chunks_mut(256).enumerate() {
        e.process_block(&mut [chunk]);
        if i * 256 >= half {
            max_gr = max_gr.max(e.block_gr_db());
        }
    }
    let rms =
        |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    let lat = e.latency_samples() as usize;
    let (a, b) = (&input[half..input.len() - lat], &output[half + lat..]);
    (max_gr, 20.0 * (rms(b) / rms(a)).log10())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_is_well_formed() {
        let all = presets();
        assert!(all.len() >= 20);
        let mut names = HashSet::new();
        for p in &all {
            assert!(names.insert(p.name), "nombre repetido: {}", p.name);
            assert!(!p.description.is_empty(), "{} sin descripción", p.name);
            assert!(p.target_gr_db.0 < p.target_gr_db.1);
            let s = &p.settings;
            assert!((-24.0..=36.0).contains(&s.input_db), "{}: input", p.name);
            assert!((-24.0..=24.0).contains(&s.output_db), "{}: output", p.name);
            assert!((1.0..=7.0).contains(&s.attack_knob), "{}: attack", p.name);
            assert!((1.0..=7.0).contains(&s.release_knob), "{}: release", p.name);
            assert!(
                (-40.0..=0.0).contains(&s.threshold_db),
                "{}: threshold",
                p.name
            );
            assert!(
                (1..=6).contains(&s.time_constant),
                "{}: time constant",
                p.name
            );
            assert!(
                (0.0..=100.0).contains(&s.peak_reduction),
                "{}: peak red.",
                p.name
            );
            assert!((0.0..=1.0).contains(&s.mix), "{}: mix", p.name);
            assert!(
                [1u8, 2, 4].contains(&s.oversampling),
                "{}: oversampling",
                p.name
            );
            if let Some(hz) = s.sidechain_hpf_hz {
                assert!((60.0..=200.0).contains(&hz), "{}: HPF", p.name);
            }
            assert!(!s.bypass && !s.auto_gain, "{}: bypass/auto-gain", p.name);
        }
        for c in Category::ALL {
            assert!(
                all.iter().filter(|p| p.category == c).count() >= 5,
                "pocos presets en {:?}",
                c
            );
        }
    }

    #[test]
    fn the_original_voice_presets_exist_with_their_documented_character() {
        let all = presets();
        let find = |n: &str| {
            all.iter()
                .find(|p| p.name == n)
                .unwrap_or_else(|| panic!("falta {n}"))
        };
        let mj = find("MJ Lead Vocal").settings;
        assert_eq!((mj.mode, mj.ratio), (MODE_FET76, Ratio::R12));
        assert!((3.0..=4.0).contains(&mj.attack_knob));
        assert_eq!(find("Motown 60s Lead").settings.mode, MODE_VARIMU);
        assert!((2..=3).contains(&find("Motown 60s Lead").settings.time_constant));
        assert_eq!(find("Jackson 5 Light").settings.mode, MODE_OPTO3A);
        assert_eq!(find("Cincinnati Soul 1965").settings.mode, MODE_VARIMU);
        let shout = find("Funk Shout").settings;
        assert_eq!(shout.mode, MODE_FET76);
        assert!(matches!(shout.ratio, Ratio::R8 | Ratio::R12));
        assert!(shout.attack_knob >= 5.0 && (shout.mix - 0.6).abs() < 1e-9);
        let glue = find("Vocal Glue").settings;
        assert_eq!(glue.mode, MODE_VARIMU);
        assert!(glue.time_constant >= 4);
    }

    #[test]
    fn every_preset_hits_its_declared_reduction_and_keeps_the_level() {
        let mut report = String::new();
        let mut failures = Vec::new();
        for p in presets() {
            let (gr, level) = super::measure(&p.settings, p.ref_peak_db);
            let (lo, hi) = p.target_gr_db;
            report += &format!(
                "{:<26} GR {gr:5.1} dB (objetivo {lo:.1}–{hi:.1})  nivel {level:+5.1} dB\n",
                p.name
            );
            if gr < lo - 0.01 || gr > hi + 0.01 {
                failures.push(format!("{}: GR {gr:.2} fuera de {lo}–{hi}", p.name));
            }
            if level.abs() > 1.0 {
                failures.push(format!("{}: nivel {level:+.2} dB (máx ±1)", p.name));
            }
        }
        println!("{report}");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
