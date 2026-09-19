//! Exercise streaming, snapshot, Digest, and batch checksum APIs.
//!
//! Timings are diagnostic only; use the ABBA workflow for performance decisions.
//! Run with `cargo run --release --example checksum_profile`.

use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

#[cfg(feature = "digest")]
use md5_simd::{Digest, DigestMd5};
use md5_simd::{Md5, Md5Engine, digest, digest_opt_scalar, hex_encode, hex_encode_digest};

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn bench<F: FnMut()>(rounds: usize, iters: usize, mut f: F) -> f64 {
    let mut samples = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let t0 = Instant::now();
        for _ in 0..iters {
            f();
        }
        samples.push(t0.elapsed().as_secs_f64() / iters as f64);
    }
    median(samples)
}

fn fmt_time(t: f64) -> String {
    if t < 1e-6 {
        format!("{:.1} ns", t * 1e9)
    } else if t < 1e-3 {
        format!("{:.2} µs", t * 1e6)
    } else {
        format!("{:.3} ms", t * 1e3)
    }
}

fn main() {
    let engine = Md5Engine::new();
    println!(
        "backend={} simd={} lanes={} digest_api={}",
        engine.backend_name(),
        engine.simd_name(),
        engine.lanes(),
        cfg!(feature = "digest")
    );

    let data: Vec<u8> = (0..(1024 * 1024))
        .map(|i| (i as u8).wrapping_mul(31))
        .collect();
    let want = hex_encode(&digest(&data));
    assert_eq!(want, hex_encode(&digest_opt_scalar(&data)));

    // Reuse the same input for every streaming interface.
    let rounds = 5;
    let iters = 200;
    let t_stream = bench(rounds, iters, || {
        let mut h = Md5::new();
        for chunk in black_box(&data).chunks(4096) {
            h.update(chunk);
        }
        let mid = h.finalize_hex();
        black_box(mid);
    });
    let t_clone = bench(rounds, iters, || {
        let mut h = Md5::new();
        for chunk in black_box(&data).chunks(4096) {
            h.update(chunk);
        }
        let mid = h.clone().finalize();
        black_box(hex_encode_digest(&mid));
    });
    let t_write = bench(rounds, iters, || {
        let mut h = Md5::new();
        for chunk in black_box(&data).chunks(4096) {
            h.write_all(chunk).unwrap();
        }
        black_box(h.finalize());
    });
    let t_oneshot = bench(rounds, iters, || {
        let d = digest(black_box(&data));
        black_box(hex_encode_digest(&d));
    });

    println!("\n=== Single-stream 1 MiB ===");
    println!("stream finalize_hex     {}", fmt_time(t_stream));
    println!("stream clone.finalize   {}", fmt_time(t_clone));
    println!("stream Write+finalize   {}", fmt_time(t_write));
    println!("oneshot digest          {}", fmt_time(t_oneshot));

    #[cfg(feature = "digest")]
    {
        let t_digest = bench(rounds, iters, || {
            let mut h = DigestMd5::new();
            for chunk in black_box(&data).chunks(4096) {
                Digest::update(&mut h, chunk);
            }
            let out = h.finalize();
            black_box(out);
        });
        let t_digest_oneshot = bench(rounds, iters, || {
            black_box(DigestMd5::digest(black_box(&data)));
        });
        println!("Digest stream           {}", fmt_time(t_digest));
        println!("Digest oneshot          {}", fmt_time(t_digest_oneshot));
    }

    // Compare batch output with independent single-message hashes.
    println!("\n=== hash_many (equal length) ===");
    println!(
        "{:<18} {:>12} {:>12} {:>12}",
        "batch", "ours", "seq-ours", "seq/ours"
    );
    for (count, len) in [
        (4usize, 1024usize),
        (8, 1024),
        (8, 64 * 1024),
        (8, 1024 * 1024),
        (32, 1024),
    ] {
        let storage: Vec<Vec<u8>> = (0..count)
            .map(|lane| {
                (0..len)
                    .map(|i| (i as u8).wrapping_add(lane as u8))
                    .collect()
            })
            .collect();
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        let mut out = vec![[0u8; 16]; count];
        let mut out_seq = vec![[0u8; 16]; count];

        let iters = if len >= 1024 * 1024 {
            20
        } else if len >= 64 * 1024 {
            50
        } else {
            200
        };
        let t_many = bench(rounds, iters, || {
            engine.hash_many(black_box(&inputs), &mut out);
            black_box(&out);
        });
        let t_seq = bench(rounds, iters, || {
            for (i, inp) in inputs.iter().enumerate() {
                out_seq[i] = engine.hash_one(black_box(inp));
            }
            black_box(&out_seq);
        });
        assert_eq!(out, out_seq, "batch {count}x{len} mismatch");
        println!(
            "{:<18} {:>12} {:>12} {:>12.2}x",
            format!("{count}x{}", fmt_size(len)),
            fmt_time(t_many),
            fmt_time(t_seq),
            t_seq / t_many
        );
    }

    // Check ordering and fallback behavior for mixed message lengths.
    let mut outs = [[0u8; 16]; 4];
    engine.hash_many(&[b"a", b"bc", b"def", b"ghij"], &mut outs);
    for (msg, o) in [b"a".as_slice(), b"bc", b"def", b"ghij"]
        .iter()
        .zip(outs.iter())
    {
        assert_eq!(*o, digest(msg));
        let _ = hex_encode(o);
    }
    println!("\nOK — digests match in-tree/opt backend.");
}

fn fmt_size(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{}MiB", n / (1024 * 1024))
    } else if n >= 1024 {
        format!("{}KiB", n / 1024)
    } else {
        format!("{n}B")
    }
}
