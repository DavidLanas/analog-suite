use criterion::{black_box, criterion_group, criterion_main, Criterion};
use funkcomp_dsp::{Engine, Ratio, Settings};

const BLOCK: usize = 512;

fn bench_engine(c: &mut Criterion) {
    for (mode, name) in [
        (funkcomp_dsp::MODE_FET76, "fet76"),
        (funkcomp_dsp::MODE_VARIMU, "varimu"),
        (funkcomp_dsp::MODE_OPTO3A, "opto3a"),
    ] {
        for os in [1u8, 2, 4] {
            let mut e = Engine::new();
            e.prepare(48_000.0);
            e.set_settings(&Settings {
                mode,
                ratio: Ratio::R12,
                oversampling: os,
                ..Settings::default()
            });
            let mut l: Vec<f32> = (0..BLOCK).map(|i| (i as f32 * 0.05).sin() * 0.7).collect();
            let mut r = l.clone();
            c.bench_function(&format!("funkcomp_{name}_stereo_{os}x_512"), |b| {
                b.iter(|| {
                    e.process_block(&mut [black_box(&mut l[..]), black_box(&mut r[..])]);
                })
            });
        }
    }
}

criterion_group!(benches, bench_engine);
criterion_main!(benches);
