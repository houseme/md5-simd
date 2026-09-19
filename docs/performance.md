# Performance evidence

Performance depends on the CPU, compiler, message size, and batch shape.
Ratios below use **baseline time / candidate time**; a value above 1 means faster.

## Backend configuration

Defaults are `std,opt,digest,simd`. x86_64 single streams use scalar assembly;
aarch64 single streams use portable Rust. Eligible adjacent equal-length runs
use SIMD in `hash_many`. Incremental multi-stream helpers remain scalar.

## Refactor validation (2026-09-19)

The review retains a pre-edit source snapshot and independent before/after
release benchmark executables. Both use Rust 1.98.1, the same native
`aarch64-apple-darwin` host, default features, thin LTO, and one codegen unit.
Benchmarks run serially; no concurrent builds or tests were run during timing.
This desktop environment is not a pinned, isolated deployment benchmark host.

Each cell runs three rounds of 20 Criterion samples, with a 0.5-second warm-up
and 1-second measurement window. A1 → B1 → B2 → A2 compares the pre-review source
against the refactor. The predeclared acceptance limits are:

- `abs(A2 / A1 - 1) <= 3%` baseline drift;
- `abs(B2 / B1 - 1) <= 3%` candidate repeatability.

Cell values are medians of each round's estimated median. Ratios use the mean
of the two baseline cell medians divided by the mean of the two candidate cell
medians. Failed cells do not support speedup or regression attribution. Small
accepted differences are not proof of a universal performance improvement.

An early auxiliary-code cleanup candidate showed an accepted approximately 5%
NEON 8×1 KiB regression. Isolating pointer preparation showed that replacing its
bounded loop with variable-size `copy_from_slice` caused the observed loss.
Restoring the loop returned both 8×1 KiB and 32×1 KiB probes to baseline levels.
The gather adapters retain this small, documented lint exception; the kernel,
framing, and schedule are not duplicated. Constant-load broadcasting uses a safe
shared-reference argument, and unused lane-packing/reference gather code is gone.
Final ABBA measurements follow below.

## Final refactor comparison (2026-09-19)

All eight workloads passed both predeclared stability gates. The measured
changes are small (about −1.2% to +0.6% in the time ratio): treat them as
**performance parity**, not a new throughput claim. The initial gather regression
was removed while retaining the correctness fixes and shared framing.

| Workload | Before | After | Before / after | A2 drift | B2 variation |
| --- | ---: | ---: | ---: | ---: | ---: |
| 32 × 1 KiB batch | 10.9290 µs | 10.8682 µs | 1.006× | 0.99% | 0.02% |
| 8 × 1 KiB batch | 3.2992 µs | 3.2802 µs | 1.006× | 0.20% | 0.41% |
| One-shot 1 MiB | 1.2123 ms | 1.2112 ms | 1.001× | 0.37% | 0.17% |
| One-shot 32 B | 60.08 ns | 60.82 ns | 0.988× | 0.20% | 0.01% |
| One-shot 55 B | 60.77 ns | 61.33 ns | 0.991× | 0.04% | 0.69% |
| One-shot 56 B | 135.20 ns | 136.00 ns | 0.994× | 0.37% | 0.42% |
| One-shot 64 B | 133.47 ns | 133.43 ns | 1.000× | 0.28% | 0.70% |
| 1 MiB stream / 4 KiB chunks | 1.2156 ms | 1.2113 ms | 1.004× | 0.86% | 0.05% |

The [machine-readable results](validation/2026-09-19-abba.json) include every
round's median and both cell medians. The full local run also retains raw
Criterion estimates, logs, source snapshots, and binary fingerprints outside
the package. These measurements compare this refactor to its input snapshot,
not to RustCrypto or to a native x86 deployment. Subsequent compile-time
endianness selection and diagnostic-label cleanup do not change the measured
little-endian hashing kernels.

## Reproduce

The checked-in Criterion suite covers one-shot boundaries and larger messages,
streaming in 4 KiB chunks, and equal-length batches of 4/8/16/32/64 messages.
Each operation has input/output optimization barriers where needed.

```bash
cargo test --locked
cargo bench --locked --bench throughput -- --sample-size 20 --measurement-time 3 --warm-up-time 1 'oneshot/.*/1048576'
cargo bench --locked --bench throughput -- --sample-size 20 --measurement-time 3 --warm-up-time 1 'hash_many_equal_(8|32)/.*/1024$'
```

For a same-build reference comparison:

```bash
scripts/run_throughput_abba.sh "$PWD"
```

The script builds the default-profile binary before timing, validates that each
filter selects exactly one benchmark, runs the complete ABBA sequence, preserves
logs and Criterion estimates in a fresh artifact directory, and writes
`summary.json`. It rejects stability failures with a nonzero exit status.
Default A is RustCrypto `md-5` 1 MiB one-shot; B is the active backend on the same
input. `ABBA_BASELINE_FILTER` and `ABBA_CANDIDATE_FILTER` select other workloads;
`ABBA_ROUNDS`, `ABBA_SAMPLES`, `ABBA_MEASUREMENT_SECS`, `ABBA_WARMUP_SECS`, and
`ABBA_DRIFT_LIMIT` must be chosen before a run.

For a source refactor, build two immutable executables first and run those in
ABBA order with identical benchmark IDs and features, as in this review. Avoid
rebuilding or changing source between measured cells.

`compare_1mib`, `checksum_profile`, and `probe` are diagnostic examples. Their
sequential timing output has no ABBA drift gate and is not promotion evidence.

## Remaining limits

- No current native x86_64/AVX2/AVX-512 performance measurement is claimed.
- Rosetta execution can check scalar x86 correctness, but does not establish
  native performance or exercise SIMD when runtime detection reports `none`.
- No universal crossover is established for 32-byte messages, partial vectors,
  mixed workloads, or different CPU families.
- The wide kernel processes multiple groups per block; this is not proof of
  round-level dual/triple interleaving. Further changes require new measurements.
- Microbenchmarks do not establish application throughput, tail latency, or
  end-to-end behavior. Measure those in the consuming application.
