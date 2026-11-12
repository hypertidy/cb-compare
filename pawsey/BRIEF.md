# Brief: run cb-compare on Setonix

A brief for picking this work up in a fresh session on a machine that can
`ssh setonix` (for example a Nectar VM in the Pawsey zone). Read this, then
`pawsey/RUNSHEET.md`, then `WRITEUP.md` (its TL;DR first).

**Manual route (preferred):** on a Setonix login node, `git pull` this repo
and follow `pawsey/RUNSHEET.md`, where `pawsey/go.sh check|quick|full|status|pack`
does each step. The steps below are the same, done by hand.

## What this is

A reproducible comparison of two Rust crates for sparse polygon
rasterization, controlledburn-rs and burnscars, run correctness-first and
then timed across inputs. Everything that runs on the cluster is in the
read-only image `ghcr.io/hypertidy/cb-compare:sha-<commit>`: the harness
built at that commit and the prepared inputs (synthetic cases, plus the SCAR
ADD rock outcrop polygons v7.3 on the REMA v2 32 m grid and coarser grids).
The image pulls anonymously. No rustup, modules beyond Singularity, build or
network are needed on the compute node.

## State at handover (2026-11-10)

- controlledburn-rs PR #25 (`slice_tiles`) is merged. The harness pins the
  crate at its head, `56ba259`, and burnscars at `1d24ef7`.
  (2026-11-12: the harness pins `e1696fc`, #30, and `cb_slice` uses
  `slice_tiles_par`. `results/` holds the Setonix run of that version, job
  52790194; the earlier run of `56ba259` was job 52637376.)
  (Later on 2026-11-12: the harness pins the released crate, `=0.3.0` from
  crates.io. `cb_par` is now the crate's `burn_par` with the WKB parsed in
  parallel, the old approach stays as `cb_par_groups` at the most threads,
  and every timing records page faults; `env.json` records THP, glibc and
  malloc settings. Not yet run on Setonix.)
- The image at `sha-2986fe0` or later is tested with `docker run --read-only`
  as a non-root user. `pawsey/setonix.slurm` and the Singularity step have
  **not** run on Setonix yet.
- `results/` in the repo holds a quick run on a 32-core dev VM. That run only
  checks the pipeline; its numbers aren't for quoting. The Setonix run will
  replace them.
- Correctness so far: both crates match the exact answer on `rects` bit for
  bit, and agree cell for cell on the rock outcrops. The exceptions are one
  invalid-polygon cell at 32 m in coverage mode and one near-tie at 32 m in
  approx mode (WRITEUP §4).

## What to do

1. On Setonix, in a scratch or software directory (keep the 0.9 GB SIF off a
   small VM disk):
   ```sh
   git clone https://github.com/hypertidy/cb-compare && cd cb-compare
   singularity pull cb-compare.sif docker://ghcr.io/hypertidy/cb-compare:sha-<latest commit on main>
   ```
   If `singularity` isn't on PATH, find the module with
   `module avail singularity` and fix the `module load` line in
   `pawsey/setonix.slurm`. Check its partition (`work`) and walltime too.
2. Smoke test, in its own results directory (about 30 min, 1 node):
   ```sh
   sbatch --account=<project> --time=00:30:00 \
     --export=ALL,PLAN_ARGS=--quick,RESULTS=$PWD/results-quick pawsey/setonix.slurm
   ```
   Check the `slurm-*.log`, `results-quick/verify/*.json` (no `differ` beyond
   the known cells) and `results-quick/summary.md`.
3. **Ask the user before the full run.** It takes up to 12 h on one exclusive
   node and is charged to their allocation.
   ```sh
   sbatch --account=<project> pawsey/setonix.slurm      # or --array=0-7%1
   ```
4. When it finishes:
   - copy `results/` and the `slurm-*.log` files back into the repo's
     `results/`;
   - run `python3 scripts/summarise.py results`, which needs matplotlib, or
     run it inside the image with `results/` bound to `/results`;
   - check that `results/env.json` names the Setonix node.

   Then update WRITEUP.md:
   - replace the provisional TL;DR numbers;
   - fill in §5 with what varies with the input. Avoid a single headline
     speedup;
   - remove the "dev VM" caveats that no longer apply.

   Commit and push.

## Cautions

- Timing must run alone on the node. The job asks for `--exclusive`; keep
  that.
- If anything fails inside the container, fix it in this repo and push. CI
  rebuilds the image. Pull the new `sha-` tag rather than patching on
  the cluster.
- The repo is public. Keep usernames, project codes and hostnames out of
  commits. `results/env.json` records only the CPU model, core counts,
  OS and the Slurm job id.

## Next, after this run

The user has more real examples and data problems to add. Each new case is a
`.wkbs` file plus a `.grid.json` file in `inputs/` (built into the image),
and one line in `scripts/plan.py`. Still to add:

- line and point inputs, so the known difference for points exactly on cell
  edges gets exercised;
- the REMA 10 m and 2 m grids, whose official extents need confirming.
