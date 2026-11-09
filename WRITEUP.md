# Sparse rasterization in Rust: controlledburn-rs and burnscars, compared

*Draft. The numbers come from `results/summary.md`; the machine and versions
for each run are in `results/env.json`. Numbers quoted here are from the
development VM until the Pawsey run replaces them.*

## 1. Purpose

controlledburn-rs and burnscars solve the same problem: rasterize vector
geometry onto a regular grid as sparse tables, with exact coverage fractions
or the cell-centre rule, in time and memory that grow with the perimeter
rather than the grid. They make different choices about how a large output
is split up. controlledburn-rs burns once in whole-grid indices and cuts
tiles out afterwards by integer arithmetic. burnscars can also re-burn each
fixed tile of the grid on its own. This comparison checks that the two give
the same answer, then measures where each choice costs time or memory, across
inputs that range from one huge ring to tens of thousands of small polygons.
It reports what was measured; it does not pick a winner.

## 2. Scope

**Implementations** (pinned in `harness/Cargo.toml` and `Cargo.lock`):

| crate | commit | notes |
|---|---|---|
| controlledburn-rs | `56ba259` (hypertidy/controlledburn-rs#25, `slice_tiles`) | |
| burnscars | `1d24ef7` | |

Both are built with Rust 1.99.1 (burnscars' pin) in release mode, from the
same WKB bytes. WKB parsing is inside the timings for both crates.

**The task.** Same geometries, same grid (extent and `ncol × nrow`), same
mode. Coverage mode: the fraction of each cell's area inside the polygon.
Approx mode: a cell is in if its centre is in.

**Where the semantics differ.** Timings only mean something where outputs
agree, so the differences are stated first. Each was found by reading both
sources and confirmed by running cases (the case harnesses are to be
folded into this repo):

| | controlledburn-rs | burnscars | effect here |
|---|---|---|---|
| records for a multipolygon | one per part per cell | one per geometry per cell | compare sums per cell and id; record counts differ |
| run coalescing | adjacent runs merged by default | not merged | record counts differ, cells do not |
| point exactly on an interior cell edge | `floor`, so some go to the left cell | corrected to the cell box | points only; none of the inputs here have points |
| far edge when `xmin + ncol·dx ≠ xmax` | the last cell stretches to `xmax` | lattice edge, so a point at `xmax` can fall outside | not hit by these grids (REMA edges are exact multiples of 32 m) |
| invalid rings | best effort | unspecified (validity is a precondition) | one cell in the rock outcrops, see §4 |
| errors | per-geometry notes, the burn continues | stops at the first error | the harness feeds burnscars one geometry at a time and rolls back a failure |
| ties at cell centres (approx) | its own intercept arithmetic | its own | can flip a cell whose centre is within ~1e-10 of an edge |

## 3. Inputs

| name | what | grid |
|---|---|---|
| `rects` | 1,997 axis-aligned rectangles, a quarter with corners on lattice lines | 1000 × 700, non-dyadic cell size; the exact answer is known for every cell |
| `coast-1e5`, `coast-1e6` | one jittered ring of 10⁵ / 10⁶ vertices | 4096², 16384² |
| `small-1e5` | 10⁵ star polygons, 8–48 vertices, radius 1.5–25 cells | 8192² |
| `rock-{1024,512,256,128,64,32}m` | 25,954 SCAR ADD rock outcrop multipolygons, v7.3 medium resolution (~1.33 M vertices), not validated or repaired | REMA v2 32 m mosaic grid, EPSG:3031, 170318 × 182568 (3.1 × 10¹⁰ cells), and coarser grids that nest in it |

The rock outcrop case follows the
[ice-free Antarctica post](https://www.hypertidy.org/posts/2026-08-19_antarctica-icefree-rasterizing/).
The source zip is pinned by SHA-256 in `data/SHA256SUMS`, since its URL is a
branch rather than a release.

## 4. Correctness first

`verify` compares every cell for every id, across all four tables, without
building a dense raster. It also checks:

- each engine against the exact answer for `rects`;
- each polygon's covered area against its planar area;
- feature-parallel output against serial output, record for record.

Results on the dev VM:

- **rects:** both engines are bit-equal to the exact answer on all 3,070,060
  cells, and to each other. Both drop one cell whose true fraction (4.5e-7)
  is below their tolerance.
- **rock outcrops, 1024 m to 256 m, both modes:** every cell agrees. Up to 9
  cells per run differ by float rounding only (≤ 1e-6), from summing the
  records of multipolygon parts.
- **rock outcrops at 32 m, coverage:** 73,248,217 cells. 73,248,202 are
  bit-equal, 14 differ by ≤ 1e-6, and one differs. The one is in geometry
  25589, which self-intersects inside that cell. The exact share after GEOS
  `MakeValid` is 0.979; cb gives 1.0 and burnscars 0.021. Invalid input is
  outside both crates' contracts, so this is reported, not scored.
- **rock outcrops at 32 m, approx:** one cell appears in burnscars only, in
  geometry 25938. Its edge passes about 2 × 10⁻¹⁰ m from the cell centre,
  and GEOS puts the centre outside (exact fraction 0.49999999999).
- Feature-parallel output equals serial output for both engines on every
  input.
- burnscars failed on none of the 25,954 rock outcrop geometries.

## 5. Performance

See `results/summary.md` for every configuration with its spread, and
`results/figures/`. What is measured:

- **A. Serial whole-grid burn**, every input, both modes.
- **B. Thread scaling.** Feature-parallel burn onto the whole grid (geometries
  split into groups of about equal vertex count, ids kept global, tables
  concatenated), for 1 up to the node's core count.
- **C. Tiles, two ways, measured separately:**
  - *burn once, then slice*: a whole-grid burn (feature-parallel when
    threads > 1) followed by a one-pass slice into tiles. Burn and slice are
    timed separately, for both crates.
  - *fixed-tile re-burn*: each tile burns the geometries whose bounding box
    meets it onto `grid.window(tile)`, in parallel over tiles. This is
    burnscars' raster-parallel contract.

  Both run at several tile sizes, at 1 thread and at all threads.
- **D. Thread scaling** of the tiled strategies at one tile size.
- **E.** `crop()` once per tile, the pattern `slice_tiles` replaces, once
  for scale.
- **Memory:** the peak RSS of each configuration's process, input included.

Every configuration runs in its own process: one warm-up run, then 5 timed
repeats (2 in `--quick`). The summary reports the median with the minimum and
maximum.

*Findings go here once the Pawsey run is in. Expect them to depend on the
input: one large ring and many small polygons pull in opposite directions,
so this section will show the variation rather than a single speedup.*

## 6. Reproducibility

`./run_all.sh` regenerates every number and figure. The engines are pinned
by commit and the toolchain by `rust-toolchain.toml`, and the rock outcrop
data is pinned by checksum. `Dockerfile` gives the same environment without
any local setup, and `pawsey/setonix.slurm` runs the full plan on an
exclusive node. Results record the machine and versions in
`results/env.json`.

## 7. Limits and caveats

- **Inputs:**
  - The rock outcrops are the only real data so far.
  - There are no line or point inputs yet, so the point-on-edge difference is
    not exercised.
  - The REMA 10 m and 2 m grids are not included yet.
- **Tiles:**
  - Regular tile grids aligned to the burn grid only. A target on another
    lattice, or a reprojected one, needs a real re-burn or a resample, and is
    out of scope.
  - The re-burn bucketing uses bounding boxes. Geometries that touch a tile's
    box but no cell in it are burned for nothing. That is part of the
    strategy's cost, but a finer prefilter would lower it.
  - Dense materialization of tiles and writing them to Zarr/COG are not
    timed. Both are the same work for every strategy, but at REMA scale they
    can cost as much as the burn.
- **Measurement:**
  - Peak RSS is per process and includes the input bytes; it is not the
    memory of the burn alone.
  - On a shared VM, timings have more spread than on an exclusive node. The
    dev-VM numbers are for checking the pipeline, not for quoting.
  - Feature-parallel grouping is by vertex count, which is a proxy for cost;
    a skewed input (one huge ring among many small ones) will not balance
    well.
- **Workflows:** burn-once needs the whole sparse output in one place.
  Distributed workers that cannot share it need a shuffle of
  O(perimeter) records, which is not measured here. Re-burn per tile needs
  no global step.

## 8. Relationship between the two crates

Both crates derive from the controlledburn C++ core and exactextract;
burnscars was written for earth-mover's chunked engine. The two crates are two takes on one problem and have borrowed tests
and benchmarks from each other. Both are linked above. Cases that favour
either crate are welcome: the harness takes any WKB and grid (see README).
