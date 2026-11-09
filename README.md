# cb-compare

A reproducible comparison of two Rust crates for sparse polygon rasterization:
[controlledburn-rs](https://github.com/hypertidy/controlledburn-rs) and
[burnscars](https://github.com/earth-mover/burnscars). Both burn polygons,
lines and points onto a regular grid as sparse tables (interior runs plus
boundary cells with exact coverage fractions, or the cell-centre rule), and
both come from the controlledburn C++ core.

The write-up is [WRITEUP.md](WRITEUP.md). Numbers and figures come from
`results/summary.md` and `results/figures/`, which `run_all.sh` regenerates.

## Run it

```sh
./run_all.sh --quick     # small inputs, a few minutes: a smoke test
./run_all.sh             # the full plan; meant for a dedicated node
```

Or in the container (Rust 1.99.1, GDAL, matplotlib):

```sh
docker build -t cb-compare .
docker run --rm -v "$PWD":/w -w /w cb-compare ./run_all.sh --quick
```

On Pawsey Setonix, see `pawsey/setonix.slurm`.

`run_all.sh` builds the harness, writes the synthetic inputs, downloads and
checks the rock outcrop polygons (`data/SHA256SUMS`), runs the correctness
checks, then runs the timing plan one configuration per process and writes
the summary. Finished configurations are skipped, so a run can be resumed.

## Layout

| path | what |
|---|---|
| `harness/` | Rust crate pinning both engines by commit (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`) |
| `harness/src/bin/synth.rs` | synthetic inputs |
| `harness/src/bin/verify.rs` | correctness: cell-by-cell agreement, exact oracle, area check, parallel = serial |
| `harness/src/bin/timing.rs` | one timed configuration per process |
| `scripts/prepare_rock.py` | rock outcrop polygons and the REMA v2 grids |
| `scripts/plan.py` | the experiment design: every configuration that is timed |
| `scripts/run_plan.sh` | runs a plan (or a slice of it), with peak RSS per configuration |
| `scripts/summarise.py` | tables and figures |
| `pawsey/setonix.slurm` | the full plan on an exclusive Setonix node |

## Adding a case

An input is two files in `inputs/`: `<name>.wkbs` (u32 count, then u32
length + WKB per geometry) and `<name>.grid.json` (`xmin, ymin, xmax, ymax,
ncol, nrow`). Add the name to `scripts/plan.py`. Cases that favour either
crate are welcome.
