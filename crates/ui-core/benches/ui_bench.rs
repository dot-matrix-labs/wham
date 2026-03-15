use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use ui_core::text::TextBuffer;
use ui_core::theme::Theme;
use ui_core::ui::Ui;

fn bench_build_form(c: &mut Criterion) {
    let mut group = c.benchmark_group("ui_build_form");
    for widget_count in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(widget_count),
            &widget_count,
            |b, &n| {
                let theme = Theme::default_light();
                let mut buffers: Vec<TextBuffer> = (0..n)
                    .map(|i| TextBuffer::new(format!("value {}", i)))
                    .collect();
                b.iter(|| {
                    let mut ui = Ui::new(800.0, 600.0, theme.clone());
                    ui.begin_frame(Vec::new(), 800.0, 600.0, 1.0, 0.0);
                    for i in 0..n {
                        match i % 4 {
                            0 => {
                                ui.label(&format!("Label {}", i));
                            }
                            1 => {
                                ui.button(&format!("Button {}", i));
                            }
                            2 => {
                                let mut checked = i % 2 == 0;
                                ui.checkbox(&format!("Check {}", i), &mut checked);
                            }
                            3 => {
                                ui.text_input(
                                    &format!("Input {}", i),
                                    &mut buffers[i],
                                    "placeholder",
                                );
                            }
                            _ => unreachable!(),
                        }
                    }
                    ui.end_frame()
                });
            },
        );
    }
    group.finish();
}

fn bench_id_stack(c: &mut Criterion) {
    c.bench_function("ui_id_stack_push_pop_hash_1000", |b| {
        let theme = Theme::default_light();
        b.iter(|| {
            let mut ui = Ui::new(800.0, 600.0, theme.clone());
            ui.begin_frame(Vec::new(), 800.0, 600.0, 1.0, 0.0);
            for i in 0u64..1_000 {
                ui.push_id(i);
                ui.label(&format!("item {}", i));
                ui.pop_id();
            }
            ui.end_frame()
        });
    });
}

fn bench_hash_id_standalone(c: &mut Criterion) {
    c.bench_function("hash_id_depth_10", |b| {
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            // Simulate an ID stack of depth 10
            for i in 0u64..10 {
                i.hash(&mut hasher);
            }
            "widget_label".hash(&mut hasher);
            hasher.finish()
        });
    });
}

criterion_group!(benches, bench_build_form, bench_id_stack, bench_hash_id_standalone);
criterion_main!(benches);
