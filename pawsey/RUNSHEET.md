# Setonix run sheet (by hand)

Everything that runs is in the read-only image
`ghcr.io/hypertidy/cb-compare:sha-<commit>`: the harness built at that commit
and the prepared inputs. The git checkout only supplies `pawsey/go.sh` and
the Slurm script. You don't need rustup, a build, or network on the compute
node. On the login node you only need Singularity; if it isn't on PATH,
`go.sh` loads `singularity/4.3.5-slurm` itself (falling back to `4.1.0-slurm`, then the default).

The image, the Singularity cache and the results all go under
`$MYSCRATCH/cb-compare` (set `WORK=` to put them elsewhere), never in `$HOME`.
The account defaults to `$PAWSEY_PROJECT` (set `ACCOUNT=` to override).

## 1. Get the code (login node)

```sh
git clone https://github.com/hypertidy/cb-compare && cd cb-compare   # first time
git pull                                                            # after that
```

## 2. Check the image (minutes, login node)

```sh
pawsey/go.sh check
```

This pulls the image for the checked-out commit (about 0.9 GB) and runs the
exact-answer check on `rects`. It should print JSON beginning
`{"area_check":…`. If CI hasn't built that commit's image yet (it takes
about 3 minutes after a push), `go.sh` falls back to `latest`.

## 3. Smoke test (about 30 min, 1 node)

```sh
pawsey/go.sh quick
pawsey/go.sh status      # while it runs, and after
```

Check `$MYSCRATCH/cb-compare/slurm-*.log` and
`$MYSCRATCH/cb-compare/results-quick-sha-<commit>/summary.md`. In `verify/*.json`, the
only `differ` or `only_*` cells should be the known ones in WRITEUP §4.

## 4. Full run (up to 12 h, 1 exclusive node)

```sh
pawsey/go.sh full                    # one job
pawsey/go.sh full --array=0-7%1      # or 8 shorter jobs, one after another
```

The results go in `$MYSCRATCH/cb-compare/results-sha-<commit>`, one
directory per image, so a run at a new commit never reuses an older image's
timings. `full` needs the image for the checked-out commit and won't fall
back to `latest`; if CI hasn't built it yet, wait a few minutes. Set
`RESULTS=` to choose the directory yourself.

Extra arguments are passed to `sbatch`. If a job dies or times out, run the
same command again from the same commit: finished configurations are
skipped and the saved plan is reused. An array run doesn't write `summary.md`. Summarise it when it is
brought back (step 5).

## 5. Bring the results back

```sh
pawsey/go.sh pack        # prints the path of a .tar.gz with results*/ and slurm-*.log
```

Copy the tarball to wherever you have the repo checked out and unpack it
somewhere outside the repo. It holds every `results*` directory on scratch,
older runs included. Copy the one for this commit, `results-sha-<commit>`,
over the repo's `results/`, then run `python3 scripts/summarise.py results`
and commit.
Alternatively, commit the results from a branch on Setonix and push, then
`git pull` them elsewhere.

## If something goes wrong

- **`singularity pull` fails on the cache or with quota errors:** check that
  `$MYSCRATCH` is set. `go.sh` points `SINGULARITY_CACHEDIR` and the tmp dir
  there.
- **Wrong partition or walltime:** edit the `#SBATCH` lines in
  `pawsey/setonix.slurm`, or pass `--partition=… --time=…` to `go.sh full`.
- **A failure inside the container:** fix it in the repo and push. Once CI
  has rebuilt the image, `git pull` and rerun. Don't patch on the cluster:
  the image tag is the record of what ran.
