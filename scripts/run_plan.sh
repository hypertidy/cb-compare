#!/usr/bin/env bash
# Run lines FIRST..LAST (1-based, inclusive) of a plan; default all.
# Each configuration writes results/timing/<key>.jsonl (one line per repeat)
# and <key>.rss (peak RSS, KB, of the whole process). Existing results are
# skipped, so a run can be resumed or split across jobs.
#   run_plan.sh <plan.tsv> [FIRST [LAST]]
set -euo pipefail
HERE=$(cd "$(dirname "$0")/.." && pwd)
PLAN=$1; FIRST=${2:-1}; LAST=${3:-999999}
BIN=$HERE/harness/target/release/timing
OUT=$HERE/results/timing; mkdir -p "$OUT"
n=0
while IFS=$'\t' read -r input mode strat threads tile reps; do
  n=$((n+1)); [ "$n" -lt "$FIRST" ] && continue; [ "$n" -gt "$LAST" ] && break
  key="$input.$mode.$strat.t$threads.tile$tile"
  [ -s "$OUT/$key.jsonl" ] && continue
  echo "[$n] $key" >&2
  /usr/bin/time -f "%M" -o "$OUT/$key.rss" \
    "$BIN" "$input" "$HERE/inputs" "$mode" "$strat" "$threads" "$tile" "$reps" > "$OUT/$key.jsonl.tmp"
  mv "$OUT/$key.jsonl.tmp" "$OUT/$key.jsonl"
done < "$PLAN"
