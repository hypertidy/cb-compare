# Setonix run sheet

Everything runs from the read-only container: no modules beyond
Singularity, no rustup, no build and no network on the compute node. The
image holds the harness built at one commit and the prepared inputs.

## On a login node

1. Get the job script and pull the image (pin the tag to a commit for the
   record; `latest` follows `main`):
   ```sh
   git clone https://github.com/hypertidy/cb-compare && cd cb-compare
   singularity pull cb-compare.sif docker://ghcr.io/hypertidy/cb-compare:sha-<7-char commit>
   ```
   If `singularity` isn't on PATH: `module load singularity/4.1.0-slurm`
   (check the name with `module avail singularity`).
2. Optional check that the image runs (seconds):
   ```sh
   singularity exec cb-compare.sif /opt/cb-compare/harness/target/release/verify rects /opt/cb-compare/inputs coverage | head -c 300
   ```
3. Check `pawsey/setonix.slurm`: partition (`work`) and walltime (12 h).

## Smoke test (~30 min), in its own results directory

4. ```sh
   sbatch --account=<project> --time=00:30:00 \
     --export=ALL,PLAN_ARGS=--quick,RESULTS=$PWD/results-quick pawsey/setonix.slurm
   ```
5. Check `slurm-*.log`, and that `results-quick/verify/` and
   `results-quick/timing/` have files and `results-quick/summary.md` exists.

## Full run

6. One job:
   ```sh
   sbatch --account=<project> pawsey/setonix.slurm
   ```
   or the plan split across 8 jobs. Task 0 writes the plan and runs the
   correctness checks, and the other tasks need its plan file, so submit
   task 0 first and the rest once it has started:
   ```sh
   sbatch --account=<project> --array=0-7%1 pawsey/setonix.slurm   # one at a time, simplest
   # or, in parallel on 8 nodes:
   sbatch --account=<project> --array=0 pawsey/setonix.slurm
   sbatch --account=<project> --array=1-7 pawsey/setonix.slurm      # after task 0 starts
   ```
7. If a job dies or runs out of time, resubmit the same command: finished
   configurations are skipped and the saved plan is reused.

## Bring results back

8. Copy `results/` (timing/, verify/, env.json, plan-setonix.tsv, and the
   `slurm-*.log` files) back to the dev VM's `~/cb-compare/results/`.
   `scripts/summarise.py results` rebuilds the tables and figures (an array
   run doesn't summarise by itself).
