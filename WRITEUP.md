# Sparse rasterization in Rust: controlledburn-rs and burnscars, compared

*The numbers come from `results/summary.md`, a full run on one exclusive
Pawsey Setonix node (2 × AMD EPYC 7763, 128 cores, 256 logical CPUs,
250 GB; Slurm job 52637376). `results/env.json` records the machine and
versions.*

## TL;DR

**Both give the same answer.** On valid input the two crates agree cell for
cell, both match the exact answer on the rectangles, and parallel output
equals serial output. They differ only on invalid polygons (one cell in
73 M), near-ties at cell centres, and points exactly on cell edges. Neither
crate defines behaviour for those (§2, §4).

**Burn once, then slice** (controlledburn-rs)

- **Pro:** the cost hardly depends on how many tiles you cut. Slicing is
  integer arithmetic over the records, and tiles are exact because nothing
  is recomputed. Cutting rock-32m into 7,560 tiles with `slice_tiles` takes
  0.019 s, against 5.5 s for one `crop()` per tile.
- **Pro:** any tile size, or several tilings, come from the same burn.
- **Pro:** for one large polygon over many tiles, it wins clearly on few
  threads. On coast-1e6 (one 10⁶-vertex ring) at 1 thread, re-burning
  256-cell tiles took 461 s and burn-once 13 s (35×).
- **Con:** the parallelism is per geometry, so one huge geometry gets no
  speedup: coast-1e5 and coast-1e6 run at 1.0× on 128 threads.
- **Con:** the slice is serial. At 128 threads it is most of the time
  (rock-32m coverage: 0.08 s burn, 0.20 s slice). Making the slice parallel
  is the clearest thing to improve.

**Re-burn each fixed tile** (burnscars' raster-parallel design)

- **Pro:** there's no global step. Tiles are independent, which suits
  distributed or lazy work. For many small geometries the memory per
  process stays small: 44 MB on rock-32m coverage at 1 thread (0.2–0.4 GB
  at 128), against 0.3–0.9 GB to burn the whole grid.
- **Pro:** it parallelizes even a single geometry. At 128 threads it was the
  fastest strategy on the coastlines, rects, small-1e5, and the rock outcrops
  at 128–32 m in coverage mode. On rock-32m coverage it took 0.04 s, against
  0.2–0.3 s to burn and slice. On coast-1e6 with 4096-cell tiles it took
  3.9 s, against 13 s. Burn-once stayed ahead on the coarse rock grids
  (1024 m and 512 m) and on most of the rock runs in approx mode.
- **Con:** the cost grows with vertices × tiles touched. On coast-1e6 at
  128 threads it went from 3.9 s (4096-cell tiles) to 32.5 s (256-cell
  tiles), while burn-once stayed at 13 s. Sharing prepared edges across
  tiles is still a TODO in burnscars.
- **Con:** for one huge geometry, memory grows with the threads: 33–75 GB
  peak on coast-1e6 at 128 threads, against 13 GB for burn-once.
- **Con:** exact tiles need deliberate lattice rules. burnscars has them;
  burn-once doesn't need them.

**The engines themselves (serial, whole grid):**

- controlledburn-rs is faster in approx mode: 1.3–1.6× on the rock outcrops,
  1.5–1.8× on rects and small-1e5, and 2.9–3.6× on the coastline rings.
- burnscars is faster in coverage mode on many polygons: 1.3× (rock 1024 m),
  growing to 2.2× (rock 32 m), and 2.0–2.3× on rects and small-1e5. On the
  single coastline rings the two are level, with cb ahead by 2–12%.
  The likely cause is per-polygon allocation in controlledburn-rs, but that
  is unconfirmed.
- Feature-parallel, controlledburn-rs scales further. Its best speedup is
  19–23× on most many-polygon inputs (28× on small-1e5), against 9–17× for
  burnscars. In coverage mode this about cancels burnscars'
  serial lead (rock-32m at 128 threads: 0.088 s vs 0.082 s). In approx mode
  cb ends about 2× ahead.

**In one line:** burn once and slice suits large or complex geometry,
arbitrary or repeated tilings, and small thread counts. Re-burning per tile
suits a whole node, many small geometries, tight memory per worker and
workers with no shared state. A hybrid also works: burn in parallel by
geometry, then route each worker's records to tiles (a parallel slice).

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
| controlledburn-rs | `56ba259` (hypertidy/controlledburn-rs#25, `slice_tiles`) | the Setonix run in `results/`; the harness now pins `e1696fc` (#30, `slice_tiles_par`), not yet run on Setonix |
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

Results on Setonix (identical to the earlier dev-VM run; verification is
deterministic):

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

### Findings

The results depend on the input, so they are grouped by the shape of the
input rather than given as one speedup. Times are medians on the Setonix
node.

**One very large geometry** (coast-1e5 and coast-1e6, a single ring of
10⁵ or 10⁶ vertices):

- Feature-parallel burning gives nothing: both engines run at 1.0× from 2 to
  128 threads. cb's approx burn of coast-1e6 gets slightly slower (0.9×),
  and its peak RSS in coverage mode rises from 8.7 GB to 13 GB.
- Serial whole-grid: cb coverage 12.9 s, bs 14.5 s; cb approx 1.06 s, bs
  3.76 s (3.5×). Serial peak RSS is 8.7 GB for cb and 12.5 GB for bs in
  coverage, and 1.3 GB and 2.7 GB in approx.
- Re-burning per tile is the only strategy here that uses the cores, and
  its cost follows the number of tiles the ring touches. For coast-1e6
  coverage:

  | tile | tiles touched | re-burn, 1 thread | re-burn, 128 threads | burn + slice | re-burn RSS, 128 threads |
  |---|---|---|---|---|---|
  | 4096 | 16 | 33.8 s | 3.9 s | 13.2 s | 44 GB |
  | 1024 | 178 | 117 s | 8.9 s | 13.1 s | 75 GB |
  | 256 | 2,317 | 461 s | 32.5 s | 13.1 s | 33 GB |

  With large tiles and a whole node, re-burning wins about 3×. With small
  tiles burn-once wins, and at 1 thread it wins at every tile size (2.5–35×).

**Many small and medium geometries** (rock outcrops at 1024 m to 32 m,
small-1e5, rects):

- Serial coverage favours bs, by 1.3× at 1024 m rising to 2.2× at 32 m
  (rock-32m: cb 1.95 s, bs 0.88 s). Serial approx favours cb, by 1.3–1.6×
  (rock-32m: cb 0.186 s, bs 0.305 s).
- Feature-parallel, cb scales further. On rock-32m at 128 threads, coverage
  takes cb 0.088 s (22×) and bs 0.082 s (11×), and approx takes cb 0.016 s
  (12×) and bs 0.034 s (9×). Gains past 64 threads are small, and the
  smallest runs (rects approx, rock 1024 m approx) get slower.
- Tiles at 128 threads. On the fine grids re-burning wins in coverage mode:
  rock-32m takes 0.035–0.043 s against 0.25–0.30 s for burn + slice, and
  small-1e5 takes 0.065–0.080 s against 0.36–0.46 s at 256–1024-cell tiles. That
  is because the serial slice is most of the burn-once time: 0.15–0.20 s of
  rock-32m coverage. Burn-once is ahead on the coarse grids (1024 m, 512 m),
  and in approx mode on the rock outcrops at 64 m and coarser; at 32 m
  approx the two are level.
- Tiles at 1 thread: in coverage mode, re-burning and bs_slice (both bs's
  engine) are within about 10% of each other, and cb_slice takes about 2×
  as long. In approx mode cb_slice is fastest, 1.6–2.5× ahead of
  re-burning on rock-32m.
- Memory: re-burning stays at 41–45 MB per process on the rock outcrops at
  most settings; it rises to 0.1–0.4 GB at 128 threads on the fine grids.
  Burn-once holds the whole output, 0.3–0.9 GB for rock-32m coverage.

**Slicing:** `slice_tiles` cut rock-32m approx into 7,560 tiles in 0.019 s.
A `crop()` per tile took 5.49 s on the same burn (280×).

**What to improve:**

- in controlledburn-rs, a parallel slice (done since this run, #30: `cb_slice`
  now slices on its thread count), and the serial
  coverage engine on many polygons;
- in burnscars, sharing prepared edges across tiles, which would cut the
  re-burn cost on large geometries, and the memory of re-burning one huge
  geometry on many threads.

## 6. Reproducibility

`./run_all.sh` regenerates every number and figure. The engines are pinned
by commit and the toolchain by `rust-toolchain.toml`, and the rock outcrop
data is pinned by checksum. The image `ghcr.io/hypertidy/cb-compare` (built
by CI from `Dockerfile`) holds the compiled harness and the prepared
inputs, and runs read-only with only a results directory bound in.
`pawsey/setonix.slurm` runs the full plan from it on an exclusive node. Results record the machine and versions in
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
  - The quoted numbers come from one exclusive Setonix node. Other machines,
    especially shared VMs, will differ and show more spread.
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
