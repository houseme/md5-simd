#!/usr/bin/env bash
# Same-host Criterion ABBA; one benchmark per filter, three rounds per cell.
# Builds once before timing. Rejects missing samples, baseline drift, and
# inconsistent candidate cells. Raw logs/estimates remain in the artifact dir.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <crate-dir>" >&2
  exit 2
fi
crate_dir=$(cd "$1" && pwd)
cargo_bin=${CARGO_BIN:-cargo}
rounds=${ABBA_ROUNDS:-3}
samples=${ABBA_SAMPLES:-20}
measurement=${ABBA_MEASUREMENT_SECS:-3}
warmup=${ABBA_WARMUP_SECS:-1}
cooldown=${ABBA_COOLDOWN_SECS:-2}
drift_limit=${ABBA_DRIFT_LIMIT:-0.03}
artifact_dir=${ABBA_ARTIFACT_DIR:-"$crate_dir/abba-$(date -u +%Y%m%dT%H%M%SZ)"}
baseline_filter=${ABBA_BASELINE_FILTER:-'^oneshot/md-5/1048576$'}
candidate_filter=${ABBA_CANDIDATE_FILTER:-'^oneshot/md5-simd\[(single-asm\(x86_64\)|in-tree\(aarch64\)|in-tree\(x86_64\)|in-tree)\]/1048576$'}

# Validate numeric gates before building or collecting any measurements.
python3 - "$rounds" "$samples" "$measurement" "$warmup" "$cooldown" "$drift_limit" <<'PYCFG'
import math, sys
rounds, samples = map(int, sys.argv[1:3])
measurement, warmup, cooldown, limit = map(float, sys.argv[3:])
if rounds < 1 or samples < 10 or not all(map(math.isfinite, (measurement, warmup, cooldown, limit))):
    raise SystemExit('invalid ABBA rounds, samples, or non-finite settings')
if measurement <= 0 or warmup <= 0 or cooldown < 0 or not 0 < limit < 1:
    raise SystemExit('invalid ABBA timing or drift limit')
PYCFG

# Never overwrite a prior run or accidentally summarize stale Criterion output.
mkdir "$artifact_dir"
artifact_dir=$(cd "$artifact_dir" && pwd)
cd "$crate_dir"
"$cargo_bin" bench --locked --bench throughput --no-run --message-format=json > "$artifact_dir/build.jsonl"
binary=$(python3 - "$artifact_dir/build.jsonl" <<'PY'
import json, sys
paths = [p['executable'] for line in open(sys.argv[1])
         if (p := json.loads(line)).get('executable') and p.get('target', {}).get('name') == 'throughput']
if len(paths) != 1:
    raise SystemExit('expected exactly one throughput benchmark binary')
print(paths[0])
PY
)
for filter in "$baseline_filter" "$candidate_filter"; do
  "$binary" --bench --list "$filter" > "$artifact_dir/selected.txt"
  python3 - "$artifact_dir/selected.txt" <<'PY'
import sys
selected = [line for line in open(sys.argv[1]) if line.rstrip().endswith(': benchmark')]
if len(selected) != 1:
    raise SystemExit(f'filter must select exactly one benchmark; selected {len(selected)}')
PY
done
{
  uname -sm
  "$cargo_bin" -V
  rustc -Vv
  printf 'features=default\nbaseline_filter=%s\ncandidate_filter=%s\n' "$baseline_filter" "$candidate_filter"
  printf 'rounds=%s samples=%s measurement=%ss warmup=%ss cooldown=%ss drift_limit=%s\n' \
    "$rounds" "$samples" "$measurement" "$warmup" "$cooldown" "$drift_limit"
  python3 - "$binary" Cargo.lock <<'PY'
import hashlib, pathlib, sys
for name in sys.argv[1:]:
    print(pathlib.Path(name).name, hashlib.sha256(pathlib.Path(name).read_bytes()).hexdigest())
PY
} > "$artifact_dir/provenance.txt"

for cell in A1 B1 B2 A2; do
  case "$cell" in
    A1|A2) filter=$baseline_filter ;;
    B1|B2) filter=$candidate_filter ;;
  esac
  for ((round = 1; round <= rounds; round++)); do
    CRITERION_HOME="$artifact_dir/$cell-$round" "$binary" --bench --noplot \
      --sample-size "$samples" --measurement-time "$measurement" \
      --warm-up-time "$warmup" "$filter" > "$artifact_dir/$cell-$round.log" 2>&1
    printf '%s round %s complete\n' "$cell" "$round"
    sleep "$cooldown"
  done
done

python3 - "$artifact_dir" "$rounds" "$drift_limit" <<'PY'
import json, pathlib, statistics, sys
root, rounds, limit = pathlib.Path(sys.argv[1]), int(sys.argv[2]), float(sys.argv[3])
if rounds < 1 or not 0 < limit < 1:
    raise SystemExit('rounds must be positive; drift limit must be between 0 and 1')
cells = {}
for cell in ('A1', 'B1', 'B2', 'A2'):
    values = []
    for round in range(1, rounds + 1):
        files = list((root / f'{cell}-{round}').glob('**/new/estimates.json'))
        if len(files) != 1:
            raise SystemExit(f'{cell}-{round}: expected one result, found {len(files)}')
        values.append(json.loads(files[0].read_text())['median']['point_estimate'])
    cells[cell] = statistics.median(values)
drift = abs(cells['A2'] / cells['A1'] - 1)
variation = abs(cells['B2'] / cells['B1'] - 1)
summary = dict(cell_medians_ns=cells, baseline_drift=drift, candidate_variation=variation,
               limit=limit, accepted=drift <= limit and variation <= limit)
if summary['accepted']:
    summary['baseline_over_candidate'] = statistics.mean([cells['A1'], cells['A2']]) / statistics.mean([cells['B1'], cells['B2']])
(root / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
if not summary['accepted']:
    raise SystemExit('ABBA stability gate failed; do not attribute a speedup')
(root / 'complete').touch()
PY
printf 'artifacts=%s\n' "$artifact_dir"
