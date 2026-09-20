//! Criterion throughput suite for md5-simd.
//!
//! Compare backends and RustCrypto `md-5` on the **same machine only**.
//! Use `--save-baseline` / `--baseline` for before/after (see docs/performance.md).
//!
//! ```bash
//! # Active backend vs md-5
//! cargo bench --bench throughput
//! # Portable oracle baseline
//! cargo bench --bench throughput --features force-portable
//! # Explicit optimized single-stream profile
//! cargo bench --bench throughput --no-default-features --features std,opt
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md5_simd::{DigestMd5, Md5, Md5Engine, backend_name, digest};
use std::hint::black_box;

fn ref_md5(data: &[u8]) -> [u8; 16] {
    use md5::Digest;
    let out = md5::Md5::digest(data);
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&out);
    arr
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(7))
        .collect()
}

fn bench_oneshot(c: &mut Criterion) {
    let active = format!("md5-simd[{}]", backend_name());
    let mut group = c.benchmark_group("oneshot");
    for &len in &[0usize, 32, 55, 56, 64, 1024, 64 * 1024, 1024 * 1024] {
        let data = pattern(len);
        group.throughput(Throughput::Bytes(len as u64));
        group.bench_with_input(BenchmarkId::new(active.clone(), len), &data, |b, data| {
            b.iter(|| digest(black_box(data)))
        });
        group.bench_with_input(
            BenchmarkId::new("md5-simd[oracle]", len),
            &data,
            |b, data| b.iter(|| md5_simd::digest_portable(black_box(data))),
        );
        group.bench_with_input(BenchmarkId::new("md-5", len), &data, |b, data| {
            b.iter(|| ref_md5(black_box(data)))
        });
    }
    group.finish();
}

fn bench_streaming(c: &mut Criterion) {
    let active = format!("md5-simd[{}]", backend_name());
    let mut group = c.benchmark_group("streaming_4k_chunks");
    for &len in &[64usize, 1024, 64 * 1024, 1024 * 1024] {
        let data = pattern(len);
        group.throughput(Throughput::Bytes(len as u64));
        group.bench_with_input(BenchmarkId::new(active.clone(), len), &data, |b, data| {
            b.iter(|| {
                let mut h = Md5::new();
                for chunk in black_box(data).chunks(4096) {
                    h.update(chunk);
                }
                h.finalize()
            })
        });
        group.bench_with_input(BenchmarkId::new("md-5", len), &data, |b, data| {
            b.iter(|| {
                use md5::Digest;
                let mut h = md5::Md5::new();
                for chunk in black_box(data).chunks(4096) {
                    h.update(chunk);
                }
                h.finalize()
            })
        });
    }
    group.finish();
}

fn bench_digest_streaming(c: &mut Criterion) {
    use md5_simd::Digest as _;
    let mut group = c.benchmark_group("digest_streaming_4k_chunks");
    for &len in &[64usize, 1024, 64 * 1024, 1024 * 1024] {
        let data = pattern(len);
        group.throughput(Throughput::Bytes(len as u64));
        group.bench_with_input(BenchmarkId::new("md5-simd", len), &data, |b, data| {
            b.iter(|| {
                let mut h = DigestMd5::new();
                for chunk in black_box(data).chunks(4096) {
                    h.update(chunk);
                }
                black_box(h.finalize())
            })
        });
        group.bench_with_input(BenchmarkId::new("md-5", len), &data, |b, data| {
            b.iter(|| {
                use md5::Digest;
                let mut h = md5::Md5::new();
                for chunk in black_box(data).chunks(4096) {
                    h.update(chunk);
                }
                black_box(h.finalize())
            })
        });
    }
    group.finish();
}

fn bench_hash_many_n<const N: usize>(c: &mut Criterion) {
    let active = format!("md5-simd[{}]", backend_name());
    let mut group = c.benchmark_group(format!("hash_many_equal_{N}"));
    let engine = Md5Engine::new();
    for &len in &[64usize, 1024, 64 * 1024, 1024 * 1024] {
        let storage: Vec<Vec<u8>> = (0..N)
            .map(|lane| {
                pattern(len)
                    .into_iter()
                    .map(|b| b.wrapping_add(lane as u8))
                    .collect()
            })
            .collect();
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        group.throughput(Throughput::Bytes((len * N) as u64));
        group.bench_with_input(
            BenchmarkId::new(format!("{active} lanes={}", engine.lanes()), len),
            &inputs,
            |b, inputs| {
                b.iter(|| {
                    let mut outputs = [[0u8; 16]; N];
                    engine.hash_many(black_box(inputs), &mut outputs);
                    outputs
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("sequential-digest", len),
            &inputs,
            |b, inputs| {
                b.iter(|| {
                    let mut outputs = [[0u8; 16]; N];
                    for (input, out) in inputs.iter().zip(outputs.iter_mut()) {
                        *out = digest(black_box(input));
                    }
                    outputs
                })
            },
        );
        // md-5 sequential reference
        group.bench_with_input(
            BenchmarkId::new("sequential-md-5", len),
            &inputs,
            |b, inputs| {
                b.iter(|| {
                    let mut outputs = [[0u8; 16]; N];
                    for (input, out) in inputs.iter().zip(outputs.iter_mut()) {
                        *out = ref_md5(black_box(input));
                    }
                    outputs
                })
            },
        );
    }
    group.finish();
}

fn bench_hash_many(c: &mut Criterion) {
    bench_hash_many_n::<4>(c);
    bench_hash_many_n::<8>(c);
    bench_hash_many_n::<16>(c);
    bench_hash_many_n::<32>(c);
    bench_hash_many_n::<64>(c);
}

fn bench_hash_many_schedules(c: &mut Criterion) {
    let engine = Md5Engine::new();
    let schedules: &[(&str, &[usize])] = &[
        ("tail_7x1mib", &[1024 * 1024; 7]),
        ("tail_9x1mib", &[1024 * 1024; 9]),
        (
            "grouped_mix",
            &[
                1024 * 1024,
                1024 * 1024,
                1024 * 1024,
                1024 * 1024,
                64 * 1024,
                64 * 1024,
                64 * 1024,
                64 * 1024,
                32 * 1024,
                32 * 1024,
                32 * 1024,
                32 * 1024,
                128 * 1024,
                128 * 1024,
                128 * 1024,
                128 * 1024,
            ],
        ),
        (
            "object_mix_irregular",
            &[
                8 * 1024,
                32 * 1024,
                128 * 1024,
                1024 * 1024,
                64 * 1024,
                256 * 1024,
                1024 * 1024,
                32 * 1024,
                1024 * 1024,
                128 * 1024,
                64 * 1024,
                512 * 1024,
            ],
        ),
        (
            "object_mix_reorderable",
            &[
                1024 * 1024,
                64 * 1024,
                1024 * 1024,
                32 * 1024,
                64 * 1024,
                1024 * 1024,
                32 * 1024,
                1024 * 1024,
                64 * 1024,
                32 * 1024,
                64 * 1024,
                32 * 1024,
            ],
        ),
    ];

    let mut group = c.benchmark_group("hash_many_schedules");
    for &(name, lengths) in schedules {
        let storage: Vec<Vec<u8>> = lengths
            .iter()
            .enumerate()
            .map(|(index, &len)| {
                pattern(len)
                    .into_iter()
                    .map(|byte| byte.wrapping_add(index as u8))
                    .collect()
            })
            .collect();
        let inputs: Vec<&[u8]> = storage.iter().map(Vec::as_slice).collect();
        let mut outputs = vec![[0u8; 16]; inputs.len()];
        let total_bytes: usize = lengths.iter().sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));

        group.bench_with_input(
            BenchmarkId::new(format!("md5-simd/{name}"), total_bytes),
            &inputs,
            |b, inputs| {
                b.iter(|| {
                    engine.hash_many(black_box(inputs), &mut outputs);
                    black_box(outputs[0])
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new(format!("sequential-md-5/{name}"), total_bytes),
            &inputs,
            |b, inputs| {
                b.iter(|| {
                    for (input, output) in inputs.iter().zip(outputs.iter_mut()) {
                        *output = ref_md5(black_box(input));
                    }
                    black_box(outputs[0])
                })
            },
        );
        if name == "object_mix_reorderable" {
            group.bench_with_input(
                BenchmarkId::new("md5-simd-grouped/object_mix_reorderable", total_bytes),
                &inputs,
                |b, inputs| {
                    b.iter(|| {
                        engine.hash_many_grouped(black_box(inputs), &mut outputs);
                        black_box(outputs[0])
                    })
                },
            );
        }
    }
    group.finish();
}

fn bench_pair(c: &mut Criterion) {
    if !md5_simd::pair_path_active() {
        return;
    }
    let mut group = c.benchmark_group("hash_pair_equal");
    for &len in &[1024usize, 64 * 1024] {
        let a = pattern(len);
        let b: Vec<u8> = a.iter().map(|x| x.wrapping_add(3)).collect();
        group.throughput(Throughput::Bytes((len * 2) as u64));
        group.bench_with_input(
            BenchmarkId::new("pair-interleave", len),
            &(&a, &b),
            |b, (a, bb)| b.iter(|| md5_simd::hash_pair(black_box(a), black_box(bb))),
        );
        group.bench_with_input(
            BenchmarkId::new("two-sequential-digest", len),
            &(&a, &b),
            |b, (a, bb)| {
                b.iter(|| {
                    let x = digest(black_box(a));
                    let y = digest(black_box(bb));
                    (x, y)
                })
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_oneshot,
    bench_streaming,
    bench_digest_streaming,
    bench_hash_many,
    bench_hash_many_schedules,
    bench_pair
);
criterion_main!(benches);
