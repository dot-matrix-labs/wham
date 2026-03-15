use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use ui_core::batch::{Batch, Material, Quad};
use ui_core::types::{Color, Rect};

fn make_quad(i: u32) -> Quad {
    Quad {
        rect: Rect::new(i as f32 * 2.0, i as f32 * 2.0, 50.0, 20.0),
        uv: Rect::new(0.0, 0.0, 1.0, 1.0),
        color: Color::rgba(1.0, 1.0, 1.0, 1.0),
        flags: 0,
    }
}

fn bench_push_quads(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_push_quads");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                let mut batch = Batch::default();
                for i in 0..n {
                    batch.push_quad(make_quad(i), Material::Solid, None);
                }
                batch
            });
        });
    }
    group.finish();
}

fn bench_clear_refill(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_clear_refill");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            let mut batch = Batch::default();
            // Pre-fill once so buffers are pre-allocated.
            for i in 0..n {
                batch.push_quad(make_quad(i), Material::Solid, None);
            }
            b.iter(|| {
                batch.clear();
                for i in 0..n {
                    batch.push_quad(make_quad(i), Material::Solid, None);
                }
            });
        });
    }
    group.finish();
}

fn bench_mixed_materials(c: &mut Criterion) {
    c.bench_function("batch_mixed_materials_1000", |b| {
        b.iter(|| {
            let mut batch = Batch::default();
            for i in 0..1_000u32 {
                let material = if i % 3 == 0 {
                    Material::TextAtlas
                } else {
                    Material::Solid
                };
                batch.push_quad(make_quad(i), material, None);
            }
            batch
        });
    });
}

criterion_group!(benches, bench_push_quads, bench_clear_refill, bench_mixed_materials);
criterion_main!(benches);
