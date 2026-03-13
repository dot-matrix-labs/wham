use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use ui_core::batch::{Batch, Material, Quad};
use ui_core::hit_test::HitTestGrid;
use ui_core::types::{Color, Rect, Vec2};

fn bench_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch");
    for widgets in [100usize, 500, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(widgets), &widgets, |b, &count| {
            b.iter(|| {
                let mut batch = Batch::default();
                for i in 0..count {
                    let x = (i % 10) as f32 * 80.0;
                    let y = (i / 10) as f32 * 32.0;
                    batch.push_quad(
                        Quad {
                            rect: Rect::new(x, y, 70.0, 28.0),
                            uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                            color: Color::rgba(0.2, 0.4, 0.9, 1.0),
                            flags: 0,
                        },
                        Material::Solid,
                        None,
                    );
                }
            });
        });
    }
    group.finish();
}

fn bench_hit_test(c: &mut Criterion) {
    c.bench_function("hit_test", |b| {
        b.iter(|| {
            let mut grid = HitTestGrid::new(1024.0, 768.0, 48.0);
            for i in 0..200 {
                let x = (i % 20) as f32 * 48.0;
                let y = (i / 20) as f32 * 48.0;
                grid.insert(ui_core::hit_test::HitTestEntry {
                    id: i as u64,
                    rect: Rect::new(x, y, 40.0, 40.0),
                });
            }
            let _ = grid.hit_test(Vec2::new(120.0, 120.0));
        })
    });
}

criterion_group!(benches, bench_batch, bench_hit_test);
criterion_main!(benches);

