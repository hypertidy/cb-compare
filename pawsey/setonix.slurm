#!/bin/bash
# Full timing plan on one exclusive Setonix CPU node (2 x 64-core AMD EPYC).
# Inputs are prepared elsewhere (run_all.sh up to the "inputs" step, or
# rsync inputs/ from a machine with GDAL), so the node needs only Rust.
#
#   sbatch --account=<project> pawsey/setonix.slurm            # whole plan
#   sbatch --account=<project> --array=0-7 pawsey/setonix.slurm # plan split in 8
#
# Check before the first run: partition name, walltime, and that ~/.cargo or
# $MYSOFTWARE holds the 1.99.1 toolchain (rustup toolchain install 1.99.1).
#SBATCH --job-name=cb-compare
#SBATCH --partition=work
#SBATCH --nodes=1
#SBATCH --exclusive
#SBATCH --time=12:00:00
#SBATCH --output=results/slurm-%A_%a.log
set -euo pipefail
cd "$SLURM_SUBMIT_DIR"
export CBCMP_MACHINE=setonix-work
(cd harness && cargo build --release --locked)
mkdir -p results
scripts/env.sh > /dev/null
PLAN=results/plan-setonix.tsv
[ -s "$PLAN" ] || python3 scripts/plan.py --threads 1,2,4,8,16,32,64,128 ${PLAN_ARGS:-} > "$PLAN"
# correctness first, on the node that times (array task 0 only)
[ "${SLURM_ARRAY_TASK_ID:-0}" = 0 ] && for i in $(cut -f1 "$PLAN" | sort -u); do for m in coverage approx; do
  mkdir -p results/verify; f=results/verify/$i.$m.json
  [ -s "$f" ] || harness/target/release/verify "$i" inputs "$m" > "$f"
done; done
N=$(wc -l < "$PLAN")
if [ -n "${SLURM_ARRAY_TASK_COUNT:-}" ]; then
  per=$(( (N + SLURM_ARRAY_TASK_COUNT - 1) / SLURM_ARRAY_TASK_COUNT ))
  first=$(( SLURM_ARRAY_TASK_ID * per + 1 )); last=$(( first + per - 1 ))
else
  first=1; last=$N
fi
# one configuration at a time: the node is exclusive, nothing else competes
srun --cpu-bind=none -n 1 -c 128 scripts/run_plan.sh "$PLAN" "$first" "$last"
