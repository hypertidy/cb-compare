#!/bin/bash
# The full timing plan on one exclusive Setonix CPU node (2 x 64-core AMD
# EPYC), inside the read-only cb-compare container. Nothing is built or
# downloaded on the node. See pawsey/RUNSHEET.md.
#
#   sbatch --account=<project> pawsey/setonix.slurm                 # whole plan
#   sbatch --account=<project> --array=0-7 pawsey/setonix.slurm     # plan split in 8
#   sbatch --account=<project> --time=00:30:00 --export=ALL,PLAN_ARGS=--quick pawsey/setonix.slurm
#
# Env: SIF (default ./cb-compare.sif), RESULTS (default ./results),
#      PLAN_ARGS (extra args for plan.py).
#SBATCH --job-name=cb-compare
#SBATCH --partition=work
#SBATCH --nodes=1
#SBATCH --exclusive
#SBATCH --time=12:00:00
#SBATCH --output=slurm-%A_%a.log
set -euo pipefail
cd "$SLURM_SUBMIT_DIR"
SIF=${SIF:-$PWD/cb-compare.sif}
RESULTS=${RESULTS:-$PWD/results}
mkdir -p "$RESULTS"
# Setonix provides Singularity as a module; the exact name may change:
# `module avail singularity`.
command -v singularity >/dev/null || module load singularity/4.1.0-slurm

# Variables for inside the container (SINGULARITYENV_/APPTAINERENV_ win over
# the image's own environment).
for p in SINGULARITYENV APPTAINERENV; do
  export ${p}_PLAN_ARGS="--threads 1,2,4,8,16,32,64,128 ${PLAN_ARGS:-}"
  export ${p}_PLAN_NAME=plan-setonix.tsv
  export ${p}_CBCMP_MACHINE=setonix-work
  export ${p}_CBCMP_IMAGE=$(basename "$SIF")
done

# One configuration at a time on an exclusive node.
srun -n 1 -c 128 --cpu-bind=none \
  singularity exec --bind "$RESULTS":/results "$SIF" /opt/cb-compare/scripts/job.sh
