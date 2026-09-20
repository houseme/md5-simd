# Performance evidence

Performance depends on the CPU, compiler, message size, and batch shape.
Ratios below use **baseline time / candidate time**; a value above 1 means faster.

## Backend configuration

Defaults are `std,opt,digest,simd`. x86_64 single streams use scalar assembly;
little-endian aarch64 single streams use the `opt` `single-aarch` kernel
(fast-md5-adapted). Eligible adjacent equal-length runs use SIMD in
`hash_many` and, for live streams, `update_many` (see *Incremental multi-stream*).

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
ABBA order with identical benchmark IDs and features:

```bash
ABBA_BASELINE_BINARY=/path/to/before-bench \
ABBA_CANDIDATE_BINARY=/path/to/after-bench \
ABBA_BASELINE_FILTER='^oneshot/md5-simd\[single-aarch\(aarch64\)\]/1048576$' \
ABBA_CANDIDATE_FILTER='^oneshot/md5-simd\[single-aarch\(aarch64\)\]/1048576$' \
scripts/run_throughput_abba.sh "$PWD"
```

Both binary variables must be provided together; this mode skips Cargo. The
script copies each executable into the fresh artifact directory before timing
and records both hashes. Retain the original source revision, patch, compiler,
features, flags, and lockfile for each external build; the runner's current
compiler and lockfile do not establish how a prebuilt executable was produced.
Avoid rebuilding or changing source between measured cells. A stable comparison
against `md-5` does not establish an improvement over the previous implementation;
that requires the before/after comparison above.

`compare_1mib`, `checksum_profile`, and `probe` are diagnostic examples. Their
sequential timing output has no ABBA drift gate and is not promotion evidence.

## 2026-09-19 optimization pass

Same-host measurements after scalar G/prefetch work, SIMD scheduling policy,
and `pick_batch` crossover updates. Diagnostic examples are not promotion
evidence; only ABBA cells that passed the 3% gates are claimed.

### x86_64 AMD EPYC (AVX2 + AVX-512), rustc 1.98.1, default features

Backend: `single-asm(x86_64)`, batch `simd-avx512-fused`, lanes=16.
Full-arch ABBA (2026-09-20) on a native Linux x86_64 host.

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

### 2026-09-20 candidate — two-step rodata-K lookahead (rejected)

The candidate kept `a/b/c/d` across a multi-block loop, loaded `K` through a
read-only table pointer, and kept two `message + K` values in scratch registers.
Its benchmark-only wrapper was never connected to production dispatch. It passed
the registered boundary differential checks before timing, including all current
oneshot lengths and the independent RustCrypto `md-5` oracle.

Two same-host ABBA campaigns used three rounds per cell, 20 Criterion samples,
three seconds of measurement, one second warm-up, two seconds cooldown, and a
3% stability gate. Both campaigns were stable, but the candidate regressed:

| Comparison | A1/A2 median | B1/B2 median | Baseline / candidate | Decision |
| --- | ---: | ---: | ---: | --- |
| `md-5` → candidate | 1.067 ms / 1.067 ms | 1.156 ms / 1.157 ms | **0.923×** | reject |
| current `single-aarch` → candidate | 1.056 ms / 1.056 ms | 1.157 ms / 1.156 ms | **0.913×** | reject |

The detailed, sanitized measurements are in
[`2026-09-20-aarch64-candidate-abba.json`](validation/2026-09-20-aarch64-candidate-abba.json).
The result confirms that removing per-round immediate materialization in the
source is insufficient by itself; the extra loads and register pressure cost
more than the candidate saves. No aarch64 1 MiB speedup is claimed.

### ISA feasibility check — no AArch64 MD5 instruction backend

The requested `md5h`/`md5m`/`md5p`/`md5im` route was checked against the current
AArch64 toolchain and host. Rust's target-feature inventory exposes no `md5`
feature, and the Apple/LLVM assembler rejects all four mnemonics. The official
A64 cryptography instruction set documents SHA1/SHA2 and related extensions,
not an MD5 extension. The probe is recorded in
[`2026-09-20-aarch64-md5-isa-probe.json`](validation/2026-09-20-aarch64-md5-isa-probe.json).

No raw instruction encodings were guessed or added. A real hardware/vendor
extension, independent-message NEON/SVE batching, or another architecture is
required before an ISA-level MD5 backend can be implemented honestly.

### 2026-09-20 NEON8 batch ABBA — accepted

The supported AArch64 route for independent messages already provides a large
lead. On the same native host, eight independent equal-length 1 MiB messages
were measured with A1→B1→B2→A2, three rounds per cell, 20 samples per round,
five seconds measurement, one second warm-up, two seconds cooldown, and a 3%
stability gate:

| Baseline | Candidate | Baseline / candidate | A drift | B variation |
| --- | --- | ---: | ---: | ---: |
| sequential RustCrypto `md-5` | fused NEON8 `hash_many` | **3.104×** | 0.61% | 0.46% |

The baseline cell medians were 8.81/8.86 ms and candidate medians were
2.84/2.85 ms. This is an accepted throughput result for eight independent
messages; it is not a single-message 1 MiB latency claim. The sanitized data
is in [`2026-09-20-neon8-8x1mib-abba.json`](validation/2026-09-20-neon8-8x1mib-abba.json).

### Mixed lengths, tail batches, and irregular object schedules

The same ABBA procedure was applied to scheduler-shaped inputs:

| Workload | Shape | Baseline / NEON candidate | Gate |
| --- | --- | ---: | --- |
| `tail_7x1mib` | seven messages, below NEON8 width | **2.670×** | accepted |
| `tail_9x1mib` | eight-message vector group plus one scalar tail | **1.733×** | accepted |
| `grouped_mix` | four 1 MiB, four 64 KiB, four 32 KiB, four 128 KiB | **1.602×** | accepted |
| `object_mix_irregular` | realistic non-repeating object sizes | **1.013×** | accepted parity |

The result shows the scheduling boundary clearly: adjacent equal-length runs
retain a large NEON lead, while mostly singleton lengths cannot be fused and
remain near sequential `md-5`. Full sanitized measurements are in
[`2026-09-20-schedule-abba.json`](validation/2026-09-20-schedule-abba.json).

The explicit `hash_many_grouped` scheduler recovers that batching opportunity
for non-adjacent repeated sizes: an object sequence with four repeated 1 MiB,
64 KiB, and 32 KiB groups reached **1.632×** versus sequential `md-5` after
sorting temporary indices and scattering outputs back to their original order.
This is an opt-in allocation/scheduling tradeoff, not a change to the
allocation-free `hash_many` contract.

### NEON multi-group root cause: repeated K broadcasts

The wide AArch64 step previously loaded and broadcast `K[step]` separately for
each active SIMD group. That cost is invisible for one NEON8 group, but appears
when a grouped scheduler produces 12–32 messages. The kernel now computes one
vector K value per step and reuses it across groups; the shared-K path applies
to two or more AArch64 groups.

The repaired path passed a fresh 5-second ABBA campaign at **1.593×** versus
sequential `md-5` for the reorderable object schedule. This confirms stable
batch performance, but the campaign does not isolate the K-sharing delta from
the previous grouped campaign, so no independent percentage improvement is
claimed. See [`2026-09-20-neon-k-broadcast.json`](validation/2026-09-20-neon-k-broadcast.json).

The multi-group step loop is now statically expanded to match the single-group
kernel. A fresh ABBA for the same reorderable object schedule measured **1.624×**
versus sequential `md-5` with both drift gates accepted. This is directional
evidence for removing runtime step control flow; it is not an isolated delta
against the prior campaign. See
[`2026-09-20-neon-multigroup-unrolled.json`](validation/2026-09-20-neon-multigroup-unrolled.json).

A direct two-group workload, 16 independent 1 MiB messages, passed a fresh
5-second ABBA at **4.216×** versus sequential `md-5` after specializing the
multi-group function for const 2/3/4 group counts while keeping it out of the
caller (A drift 0.059%, B variation 0.400%). Evidence:
[`2026-09-20-neon16-constgroups-abba.json`](validation/2026-09-20-neon16-constgroups-abba.json).

### Root-cause matrix: why SIMD can appear to have no advantage

The focused AArch64 matrix shows that NEON is faster whenever work reaches the
fused kernel: 4×64 B is about **1.55×**, 8×64 B about **2.42×**, 8×1 KiB about
**2.98×**, 8×64 KiB about **3.03×**, and 16×1 KiB about **3.50×** versus
sequential `md5-simd` digest. The complete diagnostic medians are in
[`2026-09-20-neon-root-cause-matrix.json`](validation/2026-09-20-neon-root-cause-matrix.json).

The apparent no-advantage cases have separate causes: a single message is
serial by definition; `pick_batch` rejects too-small or too-short work; the
original scheduler only fused adjacent equal lengths; underfull tails waste
lanes or run scalar; and gather/transpose/padding costs dominate tiny inputs.
The grouped scheduler and shared-K multi-group repair address the two software
causes without changing the zero-allocation default API.

### Single-message endpoint path diagnosis

A fresh release diagnostic measured 1 MiB oneshot at 1.085 ms for the active
backend and 1.082 ms for RustCrypto `md-5`; 4 KiB streaming measured 1.082 ms
and 1.084 ms respectively. The active path is only about 0.8% faster than the
in-tree kernel, and its complete-block loop already calls the monomorphic
single-stream transform directly. There is no remaining generic `FnMut` call in
that hot loop, and a 1 MiB input needs only one final padding block.

This rules out another padding or wrapper fast path as the main fix. The
remaining single-message gap is the instruction schedule/code generation of two
similar scalar AArch64 MD5 kernels. Future single-message work needs a new
compiler/backend or real ISA direction; the measured endpoint evidence is in
[`2026-09-20-aarch64-endpoint-path.json`](validation/2026-09-20-aarch64-endpoint-path.json).

### R3 follow-up: message preload and API bulk paths

A state-retaining multi-block loop was rejected after a diagnostic 1 MiB result
of about 1.196 ms versus about 1.143 ms for the original path; the larger
inlined loop increased register pressure. A second candidate preloaded all 16
message words once per block. Its accepted ABBA cell was **1.0113×** against
`md-5`, while the same-window original path was **1.0126×**; this is not a
promotion and the candidate was reverted.

The retained code change routes `Md5State` through the existing `update_opt`
state machine and lets `DigestMd5` flatten its complete block slice before
calling the shared block entry point. A new 1 MiB Digest streaming diagnostic
measured 1.1222 ms for `md5-simd` versus 1.1227 ms for `md-5`, which is parity,
not a performance claim. Detailed sanitized data is in
[`2026-09-20-r3-aarch64.json`](validation/2026-09-20-r3-aarch64.json).

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

## Single-message critical-path investigation (2026-09-20)

The reported **0.997×** came from a sequential diagnostic: 1.085 ms for
`md5-simd` and 1.082 ms for `md-5`. It was not an accepted ABBA regression.
The ratio is `md-5 latency / md5-simd latency`, so that observation means a
roughly 0.3% difference, not a missing SIMD backend.

This investigation used the unmodified `8619868` production kernel on native
Apple M5 Max, rustc 1.98.1 / LLVM 22.1.8, default features, release optimization,
thin LTO, and one codegen unit. Each formal cell had three runs, 20 Criterion
samples, 3 s measurement, 1 s warm-up, and 2 s cooldown. Before formal timing,
the limits were fixed at 3% A1/A2 drift and B1/B2 variation, with at least a 2%
before/after gain required to retain an additional assembly constraint.

| Formal comparison | A1 / A2 | B1 / B2 | A / B | A drift / B variation |
| --- | ---: | ---: | ---: | ---: |
| Original kernel → H XOR candidate | 1.0935 / 1.0901 ms | 1.0816 / 1.0834 ms | **1.0086×** | 0.311% / 0.164% |
| `md-5` → original kernel, same executable | 1.1037 / 1.1051 ms | 1.0916 / 1.0900 ms | **1.0125×** | 0.132% / 0.141% |

Both stability gates passed. The candidate did **not** meet the promotion
threshold and was reverted. The second row describes the original code; it is
not a speedup introduced by this investigation. These are interactive desktop
measurements without exclusive cores; the gates do not eliminate every source
of interference or establish results for other AArch64 CPUs.

Disassembly of the actual Criterion one-shot loop explains the narrow margin:

- A 1 MiB message runs 16,384 complete blocks and one final padding block. Each
  block depends on the preceding chaining state. The batch NEON kernel's
  independent-message parallelism cannot remove that dependency.
- The complete-block compression body is inlined; there is no per-block
  indirect call. Removing final padding alone would save only approximately
  1/16,385 of equal-cost compression work, about 0.006%.
- LLVM already hoists most `old accumulator + message + K` additions away
  from the newest round result. In G, BIC feeds the independent partial sum,
  leaving AND, ADD, ROR, ADD on the newest-result path. A blanket assembly
  boundary cannot recover work that is already scheduled independently.
- In part of H, LLVM shares `b XOR c` with the following step, making the
  latest `b` pass through two XORs. The candidate forced `c XOR d` first and
  used a small EOR assembly boundary to prevent reassociation. That trades
  fewer dependent XORs for less compiler freedom and different register/code
  layout; the measured aggregate benefit was only 0.86%.

Other candidates were screened in separate two-second diagnostic runs and
were not promoted: a three-instruction ADD/ROR/ADD island took about 1.130 ms;
an ADD-only boundary with split G terms took 1.071 ms; forced F XOR-select plus
the H change took 1.072 ms. The original screen was 1.071 ms. These sequential
screens are not controlled before/after gains and are not interchangeable with
the formal ABBA timings above. The H candidate also passed the default suite
and release differential tests, including the new long-message cases.

The compiler reports `native` as `apple-m4` on this M5 Max and accepts an
explicit `apple-m5`. A separate original-source build with
`-C target-cpu=apple-m5` took about 1.091 ms in the diagnostic screen and did
not justify changing default build flags. Hardware naming alone is not a
performance result.

Production compression remains unchanged. The retained changes are the
two-executable ABBA runner and independent RustCrypto differential coverage for
1 MiB messages at every u32 alignment, padding tails 0/55/56/63, and streaming
through `Md5`, `Md5State`, and `DigestMd5`. Raw experiments remain local; the
curated cell and round medians are in
[`2026-09-20-aarch64-critical-path.json`](validation/2026-09-20-aarch64-critical-path.json).
Further single-message candidates must beat this same-source before/after gate;
batch throughput gains cannot be substituted for that evidence.

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

## Incremental multi-stream (`update_many`)

`hash_many` needs complete messages. A streaming upload never has one: the server
holds only the current chunk of each stream, and MD5 chaining values must survive
between calls. `update_many` covers that shape. The benchmark feeds N live 1 MiB
streams one chunk per round; the baseline updates the same `Md5State`s one by one
on the active single-stream backend (assembly on x86_64).

| Host | Streams × chunk | Sequential / `update_many` | Stability |
| --- | --- | ---: | --- |
| Apple Silicon, NEON8 | 8 × 64 KiB | **3.11×** | accepted |
| Apple Silicon, NEON8 | 16 × 64 KiB | **4.10×** | accepted |
| Intel Core i7-9700, AVX2 | 4 × 64 KiB | **2.92×** | accepted |
| Intel Core i7-9700, AVX2 | 8 × 64 KiB | **5.64×** | accepted |
| Intel Core i7-9700, AVX2 | 16 × 64 KiB | **4.69×** | accepted |

All cells: A1→B1→B2→A2, 3 rounds × 20 samples, 3 s measurement, 1 s warm-up, 2 s
cooldown, 3% drift gate; the i7-9700 runs were pinned to one core. Extracting the
shared full-block loop left `hash_many_equal_16/1048576` unchanged (1.005× and
1.001× before/after on the two hosts). AVX-512 was cross-compiled and is covered by
the same generic kernel, but this change was **not executed on AVX-512 hardware**.

These are aggregate-throughput results for independent streams. They say nothing
about single-message latency, and the gain disappears when fewer than four streams
hold a complete block at the same time.

Evidence: `docs/validation/2026-09-21-update-many-abba.json`. Reproduce with

```bash
ABBA_BASELINE_FILTER='^update_many_16x1mib/sequential-update/65536$' \
ABBA_CANDIDATE_FILTER='^update_many_16x1mib/md5-simd lanes=8/65536$' \
scripts/run_throughput_abba.sh .
```
