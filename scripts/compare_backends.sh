#!/usr/bin/env bash
# Compare md5-simd backends against RustCrypto md-5 on the same machine.
# Usage: scripts/compare_backends.sh
# Configure CRITERION_FILTER, CRITERION_SAMPLES, and CRITERION_MEASURE via the environment.
set -euo pipefail
cd "$(dirname "$0")/.."

FILTER="${CRITERION_FILTER:-oneshot}"
SAMPLES="${CRITERION_SAMPLES:-10}"
MEASURE="${CRITERION_MEASURE:-1}"

run_bench() {
  local label="$1"
  shift
  echo ""
  echo "========== ${label} =========="
  cargo bench --locked --bench throughput "$@" -- \
    --sample-size "${SAMPLES}" \
    --measurement-time "${MEASURE}" \
    --warm-up-time 0.5 \
    "${FILTER}"
}

echo "host: $(uname -sm)  rustc: $(rustc -V)"
echo "filter=${FILTER} sample_size=${SAMPLES} measurement_time=${MEASURE}s"

run_bench "default (std,opt,digest,simd)"
run_bench "portable Rust" --no-default-features --features std
run_bench "textbook oracle" --no-default-features --features std,force-portable
run_bench "optimized single-stream" --no-default-features --features std,opt

echo ""
echo "Diagnostic comparison complete; use ABBA and drift gates before attributing a speedup."
echo "Backend names:"
echo "  default:        $(cargo run -q --example basic 2>/dev/null | head -1 || true)"
