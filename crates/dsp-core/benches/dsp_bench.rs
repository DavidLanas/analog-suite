use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dsp_core::biquad::{Biquad, BiquadCoeffs};
use dsp_core::oversampling::Oversampler;
use dsp_core::saturation::asym_tanh;

const BLOCK: usize = 512;

fn input() -> Vec<f64> {
    (0..BLOCK).map(|i| (i as f64 * 0.05).sin() * 0.5).collect()
}

fn bench_biquad(c: &mut Criterion) {
    let x = input();
    let mut f = Biquad::new(BiquadCoeffs::bell(48_000.0, 1000.0, 1.0, 6.0));
    c.bench_function("biquad_512", |b| {
        b.iter(|| {
            let mut acc = 0.0;
            for &v in &x {
                acc += f.process(black_box(v));
            }
            acc
        })
    });
}

fn bench_oversampling(c: &mut Criterion) {
    let x = input();
    for factor in [2usize, 4, 8] {
        let mut os = Oversampler::new(factor);
        c.bench_function(&format!("oversampler_{factor}x_asym_tanh_512"), |b| {
            b.iter(|| {
                let mut acc = 0.0;
                for &v in &x {
                    acc += os.process(black_box(v), &mut |s| asym_tanh(s * 4.0, 0.2));
                }
                acc
            })
        });
    }
}

criterion_group!(benches, bench_biquad, bench_oversampling);
criterion_main!(benches);
