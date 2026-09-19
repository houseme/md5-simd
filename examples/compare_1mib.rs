//! 1 MiB internal comparison: active vs in-tree/portable vs RustCrypto `md-5`.
//!
//! Diagnostic timings only; use the ABBA workflow for performance decisions.
//! RustCrypto is the correctness and timing reference.
//!
//! ```bash
//! cargo run --release --example compare_1mib
//! ```

use md5_simd::{Md5, Md5Engine, digest, digest_opt_scalar, digest_portable, hex_encode};
use std::io::Write;
use std::time::Instant;

fn pattern(len: usize, salt: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(17).wrapping_add(salt))
        .collect()
}

fn median(xs: &mut [f64]) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn bench_median<F: FnMut() -> [u8; 16]>(rounds: u32, iters: u32, mut f: F) -> f64 {
    let mut samples = Vec::with_capacity(rounds as usize);
    for _ in 0..rounds {
        let t0 = Instant::now();
        let mut last = [0u8; 16];
        for _ in 0..iters {
            last = std::hint::black_box(f());
        }
        std::hint::black_box(last);
        samples.push(t0.elapsed().as_secs_f64() / f64::from(iters));
    }
    median(&mut samples)
}

fn fmt_time(t: f64) -> String {
    if t >= 1e-3 {
        format!("{:.3} ms", t * 1e3)
    } else if t >= 1e-6 {
        format!("{:.2} µs", t * 1e6)
    } else {
        format!("{:.1} ns", t * 1e9)
    }
}

fn main() {
    let engine = Md5Engine::new();
    println!(
        "host={} backend={} lanes={} simd={}",
        std::env::consts::ARCH,
        engine.backend_name(),
        engine.lanes(),
        engine.simd_name()
    );

    const LEN: usize = 1024 * 1024;
    let data = pattern(LEN, 3);
    let iters = 100u32;
    let rounds = 5u32;

    let d = digest(std::hint::black_box(&data));
    assert_eq!(
        hex_encode(&d),
        hex_encode(&digest_opt_scalar(std::hint::black_box(&data)))
    );
    assert_eq!(
        hex_encode(&d),
        hex_encode(&digest_portable(std::hint::black_box(&data)))
    );
    assert_eq!(
        hex_encode(&d),
        hex_encode(&{
            use md5::Digest as _;
            let o = md5::Md5::digest(std::hint::black_box(&data));
            let mut a = [0u8; 16];
            a.copy_from_slice(&o);
            a
        })
    );
    println!("digest={}", hex_encode(&d));

    let t_active = bench_median(rounds, iters, || digest(std::hint::black_box(&data)));
    let t_tree = bench_median(rounds, iters, || {
        digest_opt_scalar(std::hint::black_box(&data))
    });
    let t_port = bench_median(rounds, iters, || {
        digest_portable(std::hint::black_box(&data))
    });
    let t_md5 = bench_median(rounds, iters, || {
        use md5::Digest as _;
        let o = md5::Md5::digest(std::hint::black_box(&data));
        let mut a = [0u8; 16];
        a.copy_from_slice(&o);
        a
    });

    println!("\n=== 1 MiB oneshot ===");
    println!("{:<22} {:>12} {:>10}", "backend", "time", "vs md-5");
    for (name, t) in [
        ("md5-simd active", t_active),
        ("md5-simd in-tree", t_tree),
        ("md5-simd portable", t_port),
        ("md-5 (RustCrypto)", t_md5),
    ] {
        println!("{:<22} {:>12} {:>9.2}x", name, fmt_time(t), t_md5 / t);
    }

    let t_stream = bench_median(rounds, iters / 2, || {
        let mut h = Md5::new();
        for chunk in std::hint::black_box(&data).chunks(4096) {
            h.update(chunk);
        }
        h.finalize()
    });
    let t_stream_md5 = bench_median(rounds, iters / 2, || {
        use md5::Digest as _;
        let mut h = md5::Md5::new();
        for chunk in std::hint::black_box(&data).chunks(4096) {
            h.update(chunk);
        }
        let o = h.finalize();
        let mut a = [0u8; 16];
        a.copy_from_slice(&o);
        a
    });
    let t_write = bench_median(rounds, iters / 2, || {
        let mut h = Md5::new();
        for chunk in std::hint::black_box(&data).chunks(4096) {
            h.write_all(chunk).unwrap();
        }
        h.finalize()
    });
    println!("\n=== 1 MiB stream 4 KiB ===");
    println!(
        "md5-simd stream  {}  vs md-5 {:.2}x",
        fmt_time(t_stream),
        t_stream_md5 / t_stream
    );
    println!("md5-simd Write   {}", fmt_time(t_write));
    println!("md-5      stream {}", fmt_time(t_stream_md5));

    println!("\n=== hash_many equal-length vs sequential active ===");
    for (count, len) in [
        (4usize, 1024usize),
        (8, 1024),
        (8, 65536),
        (16, 1024),
        (32, 1024),
        (8, 1024 * 1024),
    ] {
        let storage: Vec<Vec<u8>> = (0..count).map(|l| pattern(len, l as u8)).collect();
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        let mut out = vec![[0u8; 16]; count];
        let mut out_ref = vec![[0u8; 16]; count];
        let iters_b = if len >= LEN {
            20
        } else if len >= 65536 {
            50
        } else {
            200
        };
        let t_many = bench_median(rounds, iters_b, || {
            engine.hash_many(std::hint::black_box(&inputs), &mut out);
            std::hint::black_box(&out);
            out[0]
        });
        let t_seq = bench_median(rounds, iters_b, || {
            for (i, inp) in inputs.iter().enumerate() {
                out_ref[i] = engine.hash_one(std::hint::black_box(inp));
            }
            std::hint::black_box(&out_ref);
            out_ref[0]
        });
        assert_eq!(out, out_ref, "batch mismatch {count}x{len}");
        println!(
            "{:<12} {:>12} {:>12} seq/many={:.2}x",
            format!(
                "{count}x{}",
                if len >= LEN {
                    format!("{}MiB", len / LEN)
                } else if len >= 1024 {
                    format!("{}KiB", len / 1024)
                } else {
                    format!("{len}B")
                }
            ),
            fmt_time(t_many),
            fmt_time(t_seq),
            t_seq / t_many
        );
    }

    println!("\ncorrectness: active == in-tree == portable == md-5");
}
