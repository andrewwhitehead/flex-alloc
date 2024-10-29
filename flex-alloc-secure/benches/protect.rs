#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion};

use flex_alloc_secure::{
    boxed::{ProtectedBox, SecureBox, ShieldedBox},
    ExposeProtected,
};

fn compare(c: &mut Criterion) {
    c.bench_function("secure box create", |b| b.iter(SecureBox::<usize>::default));

    c.bench_function("protected box create", |b| {
        b.iter(ProtectedBox::<usize>::default)
    });

    c.bench_function("protected box read uncontested", |b| {
        b.iter_batched(
            ProtectedBox::<usize>::default,
            |b| {
                b.expose_read(|_r| {
                    black_box(());
                });
                b
            },
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("protected box write", |b| {
        b.iter_batched(
            ProtectedBox::<usize>::default,
            |mut b| {
                b.expose_write(|_w| {
                    black_box(());
                });
                b
            },
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("shielded box create", |b| {
        b.iter(ShieldedBox::<usize>::default)
    });

    c.bench_function("shielded box read uncontested", |b| {
        b.iter_batched(
            ShieldedBox::<usize>::default,
            |b| {
                b.expose_read(|_r| {
                    black_box(());
                });
                b
            },
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("shielded box write", |b| {
        b.iter_batched(
            ShieldedBox::<usize>::default,
            |mut b| {
                b.expose_write(|_w| {
                    black_box(());
                });
                b
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, compare);
criterion_main!(benches);
