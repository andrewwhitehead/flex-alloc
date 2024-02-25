#[macro_use]
extern crate criterion;

use core::mem::size_of;

use criterion::{black_box, Criterion};

use flex_vec::{aligned_byte_storage, array_storage, vec::Vec as FlexVec, Inline, Thin};

fn standard_compare(c: &mut Criterion) {
    const SMALL_COUNT: usize = 100;
    const LARGE_COUNT: usize = 1000;

    for count in [SMALL_COUNT, LARGE_COUNT] {
        c.bench_function(&format!("flexvec push {} values", count), |b| {
            b.iter(|| {
                let mut buf = FlexVec::<usize>::new();
                for value in 0..count {
                    buf.push(black_box(value));
                }
            });
        });

        c.bench_function(&format!("flexvec thin push {} values", count), |b| {
            b.iter(|| {
                let mut buf = FlexVec::<usize, Thin>::new();
                for value in 0..count {
                    buf.push(black_box(value));
                }
            });
        });

        c.bench_function(
            &format!("flexvec with_capacity({0}) push {0} values", count),
            |b| {
                b.iter(|| {
                    let mut buf = FlexVec::<usize>::with_capacity(count as usize);
                    for value in 0..count {
                        buf.push(black_box(value));
                    }
                });
            },
        );

        c.bench_function(
            &format!("flexvec thin with_capacity({0}) push {0} values", count),
            |b| {
                b.iter(|| {
                    let mut buf = FlexVec::<usize, Thin>::with_capacity(count as usize);
                    for value in 0..count {
                        buf.push(black_box(value));
                    }
                });
            },
        );

        if count == SMALL_COUNT {
            c.bench_function(
                &format!("flexvec inline({}) push {} values", SMALL_COUNT, count),
                |b| {
                    b.iter(|| {
                        let mut buf = FlexVec::<usize, Inline<SMALL_COUNT>>::new();
                        for value in 0..count {
                            buf.push(black_box(value));
                        }
                    });
                },
            );

            c.bench_function(
                &format!("flexvec fixed({}) push {} values", SMALL_COUNT, count),
                |b| {
                    b.iter(|| {
                        let mut buf = array_storage::<usize, SMALL_COUNT>();
                        let mut buf = FlexVec::new_in(&mut buf);
                        for value in 0..count {
                            buf.push(black_box(value));
                        }
                    });
                },
            );

            c.bench_function(
                &format!("flexvec byte storage push {} values", count),
                |b| {
                    b.iter(|| {
                        let mut buf =
                            aligned_byte_storage::<usize, { SMALL_COUNT * size_of::<usize>() }>();
                        let mut buf = FlexVec::new_in(&mut buf);
                        for value in 0..count {
                            buf.push(black_box(value));
                        }
                    });
                },
            );
        }

        c.bench_function(&format!("stdvec push {} values", count), |b| {
            b.iter(|| {
                let mut buf = Vec::<usize>::new();
                for value in 0..count {
                    buf.push(black_box(value));
                }
            });
        });

        c.bench_function(
            &format!("stdvec with_capacity({0}) push {0} values", count),
            |b| {
                b.iter(|| {
                    let mut buf = Vec::<usize>::with_capacity(count as usize);
                    for value in 0..count {
                        buf.push(black_box(value));
                    }
                });
            },
        );

        c.bench_function(&format!("flexvec extend {} values", count), |b| {
            b.iter(|| {
                let mut buf = FlexVec::<usize>::new();
                buf.extend(black_box(0..count));
            });
        });

        c.bench_function(&format!("stdvec extend {} values", count), |b| {
            b.iter(|| {
                let mut buf = Vec::<usize>::new();
                buf.extend(black_box(0..count));
            });
        });

        if count == SMALL_COUNT {
            c.bench_function(
                &format!("flexvec extend from slice {} values", count),
                |b| {
                    let mut data = [0usize; SMALL_COUNT];
                    for (idx, item) in data.iter_mut().enumerate() {
                        *item = idx;
                    }
                    b.iter(|| {
                        let mut buf = FlexVec::<usize>::new();
                        buf.extend_from_slice(black_box(&data[..count]));
                    });
                },
            );

            c.bench_function(&format!("stdvec extend from slice {} values", count), |b| {
                let mut data = [0usize; SMALL_COUNT];
                for (idx, item) in data.iter_mut().enumerate() {
                    *item = idx;
                }
                b.iter(|| {
                    let mut buf = Vec::<usize>::new();
                    buf.extend_from_slice(black_box(&data[..count]));
                });
            });
        }
    }
}

criterion_group!(benches, standard_compare);
criterion_main!(benches);
