# cb-compare

A reproducible comparison of two Rust crates for sparse polygon rasterization:
[controlledburn-rs](https://github.com/hypertidy/controlledburn-rs) and
[burnscars](https://github.com/earth-mover/burnscars). Both burn polygons,
lines and points onto a regular grid as sparse tables (interior runs plus
boundary cells with exact coverage fractions, or the cell-centre rule), and
both come from the controlledburn C++ core.

**Read the [write-up](WRITEUP.md)**, which starts with a TL;DR of the
pros and cons. Every number and figure is in the
[results summary](results/summary.md) (tables with spread, and
`results/figures/`), which `run_all.sh` regenerates.

## Run it

The container holds everything, already built, with the inputs prepared. It
runs read-only and writes only to the results directory you bind in:

```sh
docker run --rm --read-only --tmpfs /tmp -e PLAN_ARGS=--quick \
  -v "$PWD/results":/results ghcr.io/hypertidy/cb-compare     # a smoke test, minutes
docker run --rm --read-only --tmpfs /tmp \
  -v "$PWD/results":/results ghcr.io/hypertidy/cb-compare     # the full plan
```

On HPC, with Singularity/Apptainer, see [pawsey/RUNSHEET.md](pawsey/RUNSHEET.md).

From source instead (Rust 1.99.1, python3 with GDAL bindings and matplotlib):

```sh
./run_all.sh --quick
./run_all.sh
```

Each run builds or reuses the inputs, runs the correctness checks, runs the
timing plan one configuration per process and writes the summary. Finished
configurations are skipped, so a run can be resumed.

## Layout

| path | what |
|---|---|
| `harness/` | Rust crate pinning both engines by commit (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`) |
| `harness/src/bin/synth.rs` | synthetic inputs |
| `harness/src/bin/verify.rs` | correctness: cell-by-cell agreement, exact oracle, area check, parallel = serial |
| `harness/src/bin/timing.rs` | one timed configuration per process |
| `scripts/prepare_rock.py` | rock outcrop polygons and the REMA v2 grids |
| `scripts/plan.py` | the experiment design: every configuration that is timed |
| `scripts/job.sh` | plan, correctness, timing, summary (what the container runs) |
| `scripts/run_plan.sh` | runs a plan (or a slice of it), with peak RSS per configuration |
| `scripts/summarise.py` | tables and figures |
| `Dockerfile`, `.github/workflows/image.yml` | the image, published to `ghcr.io/hypertidy/cb-compare` |
| `pawsey/go.sh`, `pawsey/setonix.slurm`, `pawsey/RUNSHEET.md` | the full plan on an exclusive Setonix node, via Singularity, run by hand from a checkout |

## Adding a case

An input is two files in `inputs/`: `<name>.wkbs` (u32 count, then u32
length + WKB per geometry) and `<name>.grid.json` (`xmin, ymin, xmax, ymax,
ncol, nrow`). Add the name to `scripts/plan.py`. Cases that favour either
crate are welcome.
