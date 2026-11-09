#!/usr/bin/env bash
# The whole measurement, given built binaries and prepared inputs:
# plan -> correctness -> timing -> summary. Used by run_all.sh, by the
# container (code and inputs read-only under /opt/cb-compare) and by
# pawsey/setonix.slurm.
#
# Env:
#   CBCMP_RESULTS  writable results dir (default <repo>/results)
#   CBCMP_INPUTS   inputs dir (default <repo>/inputs)
#   CBCMP_BIN      dir holding synth/verify/timing (default <repo>/harness/target/release)
#   PLAN_ARGS      extra args for plan.py, e.g. "--quick" or "--threads 1,8,64"
#   PLAN_NAME      plan file name in the results dir (default plan.tsv)
#   SLURM_ARRAY_TASK_ID / SLURM_ARRAY_TASK_COUNT  split the plan across array tasks
set -euo pipefail
HERE=$(cd "$(dirname "$0")/.." && pwd)
export CBCMP_RESULTS=${CBCMP_RESULTS:-$HERE/results}
export CBCMP_INPUTS=${CBCMP_INPUTS:-$HERE/inputs}
export CBCMP_BIN=${CBCMP_BIN:-$HERE/harness/target/release}
R=$CBCMP_RESULTS; mkdir -p "$R/verify" "$R/timing"
PLAN=$R/${PLAN_NAME:-plan.tsv}
TASK=${SLURM_ARRAY_TASK_ID:-0}; NTASK=${SLURM_ARRAY_TASK_COUNT:-1}

"$HERE/scripts/env.sh" > /dev/null
# the plan is written once (task 0) and reused, so a resubmit continues it
if [ ! -s "$PLAN" ]; then
  [ "$TASK" = 0 ] || { echo "no plan at $PLAN; submit task 0 first" >&2; exit 1; }
  python3 "$HERE/scripts/plan.py" ${PLAN_ARGS:-} > "$PLAN.tmp" && mv "$PLAN.tmp" "$PLAN"
fi

if [ "$TASK" = 0 ]; then
  echo "== correctness (before any timing)" >&2
  for i in $(cut -f1 "$PLAN" | sort -u); do for m in coverage approx; do
    f=$R/verify/$i.$m.json
    if [ ! -s "$f" ]; then "$CBCMP_BIN/verify" "$i" "$CBCMP_INPUTS" "$m" > "$f.tmp"; mv "$f.tmp" "$f"; fi
  done; done
fi

N=$(wc -l < "$PLAN"); per=$(( (N + NTASK - 1) / NTASK ))
first=$(( TASK * per + 1 )); last=$(( first + per - 1 ))
echo "== timing: plan lines $first..$last of $N" >&2
"$HERE/scripts/run_plan.sh" "$PLAN" "$first" "$last"

if [ "$NTASK" = 1 ]; then
  python3 "$HERE/scripts/summarise.py" "$R"
else
  echo "array task $TASK done; summarise once all tasks finish: scripts/summarise.py $R" >&2
fi
