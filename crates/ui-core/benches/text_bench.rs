use criterion::{criterion_group, criterion_main, Criterion};
use ui_core::text::TextBuffer;

fn bench_insert_append(c: &mut Criterion) {
    c.bench_function("text_insert_append_1000", |b| {
        b.iter(|| {
            let mut buf = TextBuffer::new("");
            for _ in 0..1_000 {
                buf.insert_text("a");
            }
            buf
        });
    });
}

fn bench_insert_middle(c: &mut Criterion) {
    c.bench_function("text_insert_middle_1000", |b| {
        b.iter(|| {
            let mut buf = TextBuffer::new("x".repeat(500));
            buf.set_caret(250);
            for _ in 0..1_000 {
                buf.insert_text("a");
            }
            buf
        });
    });
}

fn bench_select_all_delete(c: &mut Criterion) {
    c.bench_function("text_select_all_delete", |b| {
        let initial = "a".repeat(10_000);
        b.iter(|| {
            let mut buf = TextBuffer::new(initial.clone());
            buf.select_all();
            buf.delete_backward();
            buf
        });
    });
}

fn bench_undo_redo_cycle(c: &mut Criterion) {
    c.bench_function("text_undo_redo_100", |b| {
        b.iter_batched(
            || {
                let mut buf = TextBuffer::new("");
                for _ in 0..100 {
                    buf.insert_text("hello ");
                }
                buf
            },
            |mut buf| {
                for _ in 0..100 {
                    buf.undo();
                }
                for _ in 0..100 {
                    buf.redo();
                }
                buf
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_grapheme_unicode(c: &mut Criterion) {
    c.bench_function("text_insert_unicode_500", |b| {
        b.iter(|| {
            let mut buf = TextBuffer::new("");
            for _ in 0..500 {
                buf.insert_text("\u{1F600}"); // emoji
            }
            buf
        });
    });
}

criterion_group!(
    benches,
    bench_insert_append,
    bench_insert_middle,
    bench_select_all_delete,
    bench_undo_redo_cycle,
    bench_grapheme_unicode,
);
criterion_main!(benches);
