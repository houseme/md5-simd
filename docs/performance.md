# Performance evidence

Performance depends on the CPU, compiler, message size, and batch shape.
Ratios below use **baseline time / candidate time**; a value above 1 means faster.

## Backend configuration

Defaults are `std,opt,digest,simd`. x86_64 single streams use scalar assembly;
little-endian aarch64 single streams use the `opt` `single-aarch` kernel
(fast-md5-adapted). Eligible adjacent equal-length runs use SIMD in
`hash_many`. Incremental multi-stream helpers remain scalar.

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

## 2026-09-19 optimization pass

Same-host measurements after scalar G/prefetch work, SIMD scheduling policy,
and `pick_batch` crossover updates. Diagnostic examples are not promotion
evidence; only ABBA cells that passed the 3% gates are claimed.

### x86_64 AMD EPYC (AVX2 + AVX-512), rustc 1.98.1, default features

Backend: `single-asm(x86_64)`, batch `simd-avx512-fused`, lanes=16.
Full-arch ABBA (2026-09-20) on `azure-401246254:/data/rustfs/md5-simd`.

| Workload | Baseline | Candidate | Baseline / candidate | Gate |
| --- | ---: | ---: | ---: | --- |
| oneshot 1 MiB: md-5 vs single-asm | ~1.44 ms | ~1.13 ms | **1.267–1.277×** | accepted (2 runs) |
| `hash_many` 8×1 KiB: sequential-digest vs SIMD | 9.5 µs | 3.2 µs | **2.97×** | accepted |
| `hash_many` 16×1 KiB: sequential-digest vs SIMD | 18.9 µs | 3.2 µs | **5.84×** | accepted |
| `hash_many` 32×1 KiB: sequential-digest vs SIMD | ~37.9 µs | ~6.4 µs | **5.90–5.98×** | accepted (2 runs) |

Diagnostic (not gated): 16×1 KiB ≈ 5.8× sequential; 48×/64×1 KiB ≈ 6.2×.
`pick_batch` falls back to scalar when `n < 8` and `msg_len < 64` (4×32 B
measured at parity/slower than sequential on both hosts).

### aarch64 Apple Silicon, rustc 1.98.1, default features

With `opt`, the single-stream path is `single-aarch(aarch64)` (fast-md5-adapted
kernel in `src/simd/single_aarch.rs`); batch remains `simd-neon8-fused`, lanes=8.
Full-arch ABBA (2026-09-20).

| Workload | Observation |
| --- | --- |
| oneshot 1 MiB vs md-5 | Accepted ABBA cells **0.973–1.010×**（含 single-aarch **0.988 / 1.009 / 1.010**）；噪声轮次未晋级 |
| `digest` vs `digest_opt_scalar` | diagnostic: active ≈ in-tree within ~1–2% |
| `hash_many` 8×1 KiB | diagnostic ≈ 3.0× sequential；桌面批量 ABBA 未过 3% 噪声门限 |
| `hash_many` 32×1 KiB | diagnostic ≈ 4.3× sequential；同上，不作 ABBA 晋级 |

`single_aarch` is selected under default `opt` for little-endian aarch64
(correctness: RFC + in-tree + md-5). Measured oneshot remains **parity** with
RustCrypto `md-5`, not a throughput win.

### External reference diagnostic (fast-md5 1.0.0, aarch64, not an ABBA gate)

Standalone probe crate (not a package dependency) compared kernels on the
same host / thin LTO / rustc 1.98.1:

| Kernel | Time | vs `fast_md5::transform` |
| --- | ---: | ---: |
| `fast-md5` aarch64 `transform` | ~50.5–50.7 ns | 1.00× |
| `md5-simd` `single-aarch` / hazmat | ~50.9–51.3 ns | **~1.01×** |
| RustCrypto `md-5` `block_api::compress` | ~54.8–55.8 ns | **~1.08–1.12×** |

End-to-end oneshot 1 MiB (same probe): `single-aarch` ≈ `md-5` within ~1–2%;
both trail `fast-md5` by ~1–2%. Trial code changes (unaligned `mi`, drop
prefetch, `rotate_left` instead of per-round `ror` asm, local `K` copy) did
**not** produce a stable win over the shipped kernel — production code is
unchanged after that experiment set.

Interpretation: aarch64 “parity with md-5” is **not** caused by a missing
`single_aarch` path (md-5 soft is *slower* at the compress kernel). The
residual gap to `fast-md5` is small codegen/framing difference, not a broken
schedule. `fast-md5` is **not** a crates.io dependency of this package.

### Phase C experiment — monolithic AArch64 `asm!` (not shipped)

A fully unrolled AArch64 `asm!` schedule (GOpt G / BIC+AND, `movz`/`movk` for
each RFC K, per-round `ldr` of message words) was implemented and **correct**
(RFC + in-tree match), but measured **slower** than the shipped Rust+`ror`
kernel:

| Metric | Shipped Rust+`ror` | Monolithic naive asm |
| --- | ---: | ---: |
| compress×1 | ~51 ns | **~81 ns** |
| oneshot 1 MiB vs md-5 | ~0.99× | **~0.71×** |

Cause: each round pays extra `movz`/`movk`/`ldr`/`add` materializations that
lengthen the MD5 critical path versus LLVM’s register-allocated Rust schedule
with immediate `K` folds. The experiment was **reverted**; default aarch64
`opt` remains the fast-md5-adapted Rust + per-round `ror` kernel.

### What changed in the kernels

- Scalar production `mix_g` uses `(x&z)+(y&!z)`; multi-block loops prefetch
  the next 1–2 blocks (aarch64 `prfm`, x86 `_mm_prefetch`).
- `wide_step` G remains the xor identity (the add form regressed NEON8
  single-group codegen).
- aarch64 `ngroups >= 3`: step interleave + gather pipeline. x86 and
  `ngroups <= 2`: sequential per-group compress inside the fused kernel.
- `pick_batch`: SIMD requires `msg_len >= 32`, and `msg_len >= 64` when
  `n < 8`.

## Path optimizations (2026-09-20)

Shipped after same-host probes (not all are ABBA-promoted):

- Removed manual `prfm` from multi-block `compress_blocks_with` / `hash_with`
  (sequential HW prefetch; no stable win from software prefetch).
- Tight `for block in chunks` framing; `hash_with` / `finalize_with` /
  `compress_blocks_with` marked `#[inline(always)]`.
- `build_final_blocks` avoids zeroing the second padding block when unused.
- `single_aarch` `mi` uses unaligned LE loads (upstream fast-md5 form).
- `digest(b"")` returns the RFC empty digest without entering compress.

aarch64 oneshot vs md-5 after these changes remains in the **parity band**
(ABBA accepted cell **0.989×**; probe medians often 0.99–1.04×). Empty-message
oneshot improved toward md-5/fast-md5. x86 `single-asm` 1 MiB still **~1.29×**
md-5 on the Azure host.

## Short-message digest fast paths (2026-09-20, after path opts)

`backend::hash` / `digest`: empty → RFC constant; `1..=55` bytes → one padded
block through the active compressor (no generic multi-block framing).

Probe medians vs RustCrypto `md-5` (aarch64, diagnostic):

| len | md-5 / ours |
| --- | ---: |
| 0 | **~30×** (ours ~2 ns) |
| 32 | **1.05×** |
| 55 | **1.03×** |
| 56 | **1.03×** |
| 64 | **1.02×** |
| 200 | **1.01×** |

1 MiB oneshot remains in the parity band vs md-5. x86 `single-asm` still
**~1.27×** md-5 (Azure). Each subsequent optimization round will be posted to
[rustfs/backlog#2619](https://github.com/rustfs/backlog/issues/2619).

## Round 7 — monomorphic opt loops + ABBA (2026-09-20)

**Rejected (not shipped):** monolithic AArch64 `asm!` with either per-round
`movz`/`movk` K materialization **or** rodata `ldr` for every K — both ~55%
slower than the LLVM Rust+`ror` kernel (~82 ns vs ~52 ns compress×1).

**Shipped:**
- `backend::hash` bulk path: multi-block oneshot calls
  `single_stream::transform` directly (no `FnMut` in the 1 MiB loop).
- `Md5::update` → `Raw::update_opt`: same monomorphic multi-block loop.

**ABBA vs RustCrypto `md-5` (aarch64, accepted):**

| Workload | baseline/candidate |
| --- | ---: |
| oneshot 1 MiB | **1.017×** |
| streaming 1 MiB / 4 KiB chunks | **1.003×** |

x86 Azure after the same tree: `single-asm` oneshot **~1.22×** md-5; stream
**~1.19×**. Empty/short digest fast paths remain in place (round 6).

## Remaining limits

- No current native x86_64/AVX2/AVX-512 performance measurement is claimed.
- Rosetta execution can check scalar x86 correctness, but does not establish
  native performance or exercise SIMD when runtime detection reports `none`.
- No universal crossover is established for 32-byte messages, partial vectors,
  mixed workloads, or different CPU families.
- The wide kernel processes multiple groups per block; aarch64 multi-group
  interleaving is measured, not a universal x86 claim. Further changes require
  new measurements.
- Microbenchmarks do not establish application throughput, tail latency, or
  end-to-end behavior. Measure those in the consuming application.
