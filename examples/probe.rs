//! Diagnostic CPU probe; use ABBA for performance decisions.

use md5_simd::{Md5Engine, digest};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let engine = Md5Engine::new();
    println!(
        "backend={} lanes={} simd={}",
        engine.backend_name(),
        engine.lanes(),
        engine.simd_name()
    );
    for (count, len) in [
        (4usize, 16usize),
        (4, 32),
        (4, 64),
        (4, 1024),
        (8, 16),
        (8, 32),
        (8, 64),
        (8, 256),
        (8, 1024),
        (16, 1024),
        (24, 1024),
        (32, 1024),
        (48, 1024),
        (64, 1024),
        (16, 65536),
        (32, 4096),
    ] {
        let storage: Vec<Vec<u8>> = (0..count)
            .map(|l| (0..len).map(|i| (i as u8).wrapping_add(l as u8)).collect())
            .collect();
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        let mut out = vec![[0u8; 16]; count];
        let mut sequential = vec![[0u8; 16]; count];
        let iters = 200;
        let t0 = Instant::now();
        for _ in 0..iters {
            engine.hash_many(black_box(&inputs), &mut out);
            black_box(&out);
        }
        let many = t0.elapsed() / iters;
        let t1 = Instant::now();
        for _ in 0..iters {
            for (i, inp) in inputs.iter().enumerate() {
                sequential[i] = digest(black_box(inp));
            }
            black_box(&sequential);
        }
        let seq = t1.elapsed() / iters;
        assert_eq!(out, sequential, "batch mismatch");
        for (inp, o) in inputs.iter().zip(out.iter()) {
            assert_eq!(*o, digest(inp), "digest mismatch");
        }
        println!(
            "count={count:2} len={len:5}  hash_many={many:?}  seq={seq:?}  many/seq={:.2}x",
            many.as_secs_f64() / seq.as_secs_f64()
        );
    }
}
