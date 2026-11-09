#!/usr/bin/env bash
# Regenerate every number and figure of the comparison.
#   ./run_all.sh            full plan (meant for a dedicated node, see pawsey/)
#   ./run_all.sh --quick    small inputs, a smoke test (minutes)
# Env: PLAN_ARGS (extra args for scripts/plan.py, e.g. "--threads 1,8,64"),
#      CBCMP_MACHINE (a label for results/env.json).
# Needs: the Rust toolchain in harness/rust-toolchain.toml (rustup installs it),
# python3 with GDAL bindings (only to prepare the rock outcrop input) and
# matplotlib (figures). Or run it inside the container, see Dockerfile.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); cd "$HERE"
QUICK=""; [ "${1:-}" = "--quick" ] && QUICK="--quick"
mkdir -p results inputs

echo "== build" >&2
(cd harness && cargo build --release --locked)
B=harness/target/release

echo "== inputs" >&2
[ -s inputs/small-1e5.wkbs ] || $B/synth inputs
[ -s inputs/rock.wkbs ] || python3 -I scripts/prepare_rock.py data inputs

scripts/env.sh > /dev/null

echo "== correctness (before any timing)" >&2
python3 scripts/plan.py $QUICK ${PLAN_ARGS:-} > results/plan.tsv
for i in $(cut -f1 results/plan.tsv | sort -u); do
  for m in coverage approx; do
    f=results/verify/$i.$m.json; mkdir -p results/verify
    [ -s "$f" ] || $B/verify "$i" inputs "$m" > "$f"
  done
done

echo "== timing ($(wc -l < results/plan.tsv) configurations)" >&2
scripts/run_plan.sh results/plan.tsv

echo "== summary" >&2
python3 scripts/summarise.py results
echo "done: results/summary.md, results/figures/" >&2
