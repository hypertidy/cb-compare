# Sparse rasterization in Rust: controlledburn-rs and burnscars, compared

*`results/` holds the latest run: controlledburn-rs 0.3.2 on one exclusive
Pawsey Setonix node (2 × AMD EPYC 7763, 128 cores, 256 logical CPUs,
250 GB; Slurm job 52949684, 2026-11-12), with `results/env.json` recording
the machine and versions. The TL;DR and §§2–5 quote this run. §0 says what
changed since the run before it (job 52923373, controlledburn-rs 0.3.0,
kept as the appendix); older runs (jobs 52790194 and 52637376) are compared
where the numbers moved.*

## 0. Latest run: controlledburn-rs 0.3.2 (job 52949684)

What changed since job 52923373: the harness's `cb_par` is now the crate's
`burn_wkb_par`, which parses each geometry's WKB on the thread that burns
it (controlledburn-rs #47), instead of parsing everything first and calling
`burn_par`. That was the fix the 0.3.0 run called for (appendix).
`cb_par_groups`, the harness's older split into groups burned in a rayon
pool, is kept at 128 threads for comparison. Nothing on the serial path
changed, and burnscars, the toolchain and the node are the same.

- **Correctness is unchanged.** The verification table is identical to
  the 0.3.0 run's: all 20 input/mode pairs verify, parallel output equals
  serial output for both crates, and the same two single-cell differences
  on rock-32m remain (§4).
- **Serial times are the same** as in the 0.3.0 run: cb within 5% on
  every input and mode (16 of 20 within 2%), bs within 1% except rects
  approx, a 10 ms burn that was 10% faster.
- **`cb_par` is 1.2–3.2× faster than in the 0.3.0 run** at 32 and 128
  threads on small-1e5, the rock outcrops and rects approx, and no slower
  anywhere. At 128 threads:

  | input | mode | 0.3.0 run | this run | `cb_par_groups` | bs_par |
  |---|---|---|---|---|---|
  | rock-1024m | approx | 0.0288 s | 0.0123 s | 0.0025 s | 0.0057 s |
  | rock-128m | coverage | 0.0404 s | 0.0208 s | 0.0191 s | 0.0306 s |
  | rock-32m | coverage | 0.0669 s | 0.0481 s | 0.0658 s | 0.0921 s |
  | rock-32m | approx | 0.0344 s | 0.0188 s | 0.0163 s | 0.0246 s |
  | small-1e5 | approx | 0.0965 s | 0.0303 s | 0.0260 s | 0.0465 s |
  | rects | approx | 0.0041 s | 0.0018 s | 0.0028 s | 0.0055 s |

  The coastlines (one ring each) are unchanged, as they should be.
- **At its best thread count, cb's feature-parallel burn now beats bs's on
  14 of the 20 input/mode pairs** (8 in the 0.3.0 run): every coastline,
  rects, small-1e5, and rock 128–32 m. On rock 1024–256 m bs is ahead by
  1.25–1.74× (rock-1024m approx: 5.4 ms at 16 threads, against 3.1 ms at
  32).
- **Burn once and slice gained most.** Its burn is `cb_par`, so it now
  beats re-burning per tile in 31 of the 56 input/mode/tile combinations,
  each strategy at its best thread count, against 13 in the 0.3.0 run. It
  now wins on every rock grid from 1024 m to 256 m (except 256-cell tiles in
  coverage mode at 512–256 m), in approx mode on rock-128m, rock-64m with
  256–512-cell tiles and rock-32m with 512-cell tiles, and on small-1e5
  with 4096-cell tiles. rock-512m approx, 512-cell tiles:
  7.6 ms at 32 threads, against 18.7 ms (0.3.0 run: 19.9 ms against
  18.6 ms).
- **What is left is small burns on many threads.** On the coarse rock grids
  `cb_par` is fastest at 16–32 threads and gets slower above that
  (rock-1024m approx: 5.4 ms at 16 threads, 8.0 ms at 64, 12.3 ms at 128),
  while `cb_par_groups` and bs_par, which run in a rayon pool that already
  exists, take 2.5 and 5.7 ms at 128. The crate caps threads at one per
  8,192 units of estimated work (coordinates plus rows spanned), and the
  outcrops' 1.33 M vertices allow about 160 threads, so the cap does not
  bite on the 31 ms serial burn of rock-1024m. Starting 128 threads per
  call, and their first allocations, are the likely cost; this run doesn't
  measure that directly. A tighter cap, or reusing threads, is the next
  thing to try in the crate.
- The page-fault and huge-page findings of the 0.3.0 run hold unchanged
  (the same node setting, THP `always`).

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
  0.030 s, against 5.5 s for one `crop()` per tile.
- **Pro:** any tile size, or several tilings, come from the same burn.
- **Pro:** for one large polygon over many tiles, it wins clearly on few
  threads. On coast-1e6 (one 10⁶-vertex ring) at 1 thread, re-burning
  256-cell tiles took 466 s and burn-once 12.5 s (37×).
- **Pro:** on many small and medium geometries it is now the faster way on
  coarse and medium grids, and in approx mode on most grids. With each
  strategy at its best thread count it beat re-burning in 31 of 56
  input/mode/tile combinations: all of rock 1024–512 m but one, most of
  rock-256m, rock-128m in approx mode, and small-1e5 with 4096-cell tiles.
- **Con:** the parallelism is per geometry, so one huge geometry gets no
  speedup: coast-1e5 and coast-1e6 run at 1.0× on 128 threads.
- **Con:** on fine grids in coverage mode with a whole node, it is still
  slower than re-burning: rock-32m burn + slice takes 0.093 s at 128
  threads (0.057 s burn, 0.034 s slice), against 0.034–0.043 s.
- **Con:** small burns are fastest at 16–32 threads; above that,
  `burn_wkb_par` gets slower (§0).

**Re-burn each fixed tile** (burnscars' raster-parallel design)

- **Pro:** there's no global step. Tiles are independent, which suits
  distributed or lazy work. For many small geometries the memory per
  process stays small: 44 MB on rock-32m coverage at 1 thread (0.23–0.42 GB
  at 128), against 0.3–0.9 GB to burn the whole grid.
- **Pro:** it parallelizes even a single geometry. On coast-1e6 with
  4096-cell tiles at 128 threads it took 3.9 s, against 12.4 s to burn
  once.
- **Pro:** at 128 threads in coverage mode it was fastest on rects, on rock
  128–32 m and small-1e5 at 256–1024-cell tiles, on rock 512–256 m at
  256-cell tiles, and on the coastlines with
  1024–4096-cell tiles (rock-32m: 0.036 s with 2048-cell tiles, against
  0.093 s).
- **Con:** the cost grows with vertices × tiles touched. On coast-1e6 at
  128 threads it went from 3.9 s (4096-cell tiles) to 32.8 s (256-cell
  tiles), while burn-once stayed at 12.3–12.4 s. Sharing prepared edges
  across tiles is still a TODO in burnscars.
- **Con:** for one huge geometry, memory grows with the threads: 34–66 GB
  peak on coast-1e6 at 128 threads, against 7.8 GB for burn-once.
- **Con:** exact tiles need deliberate lattice rules. burnscars has them;
  burn-once doesn't need them.

**The engines themselves (serial, whole grid):**

- controlledburn-rs is faster in approx mode: 1.3–1.6× on the rock
  outcrops, 1.5–1.6× on rects and small-1e5, and 2.7–3.4× on the coastline
  rings.
- In coverage mode the two are close. cb is ahead on the coastline rings
  (1.2×) and rock 1024–128 m (1.06–1.25×), and the two are within 3% on
  rects, small-1e5 and rock 64–32 m.
- Feature-parallel at 128 threads, cb is ahead in coverage mode on rects,
  small-1e5 and rock 128–32 m (rock-32m: 0.048 s against 0.092 s, 19.5×
  against 9.9× over each crate's own serial time), and in approx mode on
  rects, small-1e5 and rock-32m (0.019 s against 0.025 s). On rock
  1024–256 m, and in approx mode on rock 128–64 m, bs's burn is faster at
  128 threads (§0).

**In one line:** burn once and slice suits large or complex geometry,
arbitrary or repeated tilings, small thread counts, and, now, most of the
many-geometry cases on a whole node too. Re-burning per tile suits fine grids
in coverage mode on a whole node, one huge geometry with large tiles, tight
memory per worker, and workers with no shared state. A hybrid also works:
burn in parallel by geometry, then route the records to tiles in parallel,
as controlledburn-rs's `slice_tiles_par` does.

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
| controlledburn-rs | `0.3.2` (crates.io) | `cb_par` is `burn_wkb_par`; earlier runs used 0.3.0, `e1696fc` and `56ba259` |
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
| records for a multipolygon | one per geometry per cell (parts merged, since 0.3.0; one per part before) | one per geometry per cell | compare sums per cell and id |
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

Results on Setonix (identical in the last two runs; verification is
deterministic):

- **rects:** both engines are bit-equal to the exact answer on all 3,070,060
  cells, and to each other. Both drop one cell whose true fraction (4.5e-7)
  is below their tolerance.
- **rock outcrops, 1024 m to 64 m, both modes:** every cell is bit-equal.
  Before controlledburn-rs 0.3.0 merged the parts of a multipolygon, up to
  9 cells per run differed by float rounding (≤ 1e-6), from summing the
  parts' records.
- **rock outcrops at 32 m, coverage:** 73,248,217 cells. 73,248,216 are
  bit-equal and one differs. The one is in geometry
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
- **B. Thread scaling.** Feature-parallel burn onto the whole grid, for 1 up
  to the node's core count. For cb this is `burn_wkb_par` (chunks of about
  equal estimated work, each geometry parsed on the thread that burns it);
  for bs, geometries split into groups of about equal vertex count, burned
  in a rayon pool with ids kept global and the tables concatenated
  (`cb_par_groups` does the same with cb, at 128 threads).
- **C. Tiles, two ways, measured separately:**
  - *burn once, then slice*: a whole-grid burn (feature-parallel when
    threads > 1) followed by a one-pass slice into tiles. Burn and slice are
    timed separately, for both crates. cb slices on the same threads
    (`slice_tiles_par`, serial at 1 thread); bs's slice is serial.
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
node. burnscars is the same commit in this run and the one before it, and
its times moved by 1% or less on all but one configuration, which bounds the
run-to-run drift of the node.

**One very large geometry** (coast-1e5 and coast-1e6, a single ring of
10⁵ or 10⁶ vertices):

- Feature-parallel burning gives nothing: both engines run at 1.0× from 2 to
  128 threads.
- Serial whole-grid: cb coverage 12.4 s, bs 14.7 s; cb approx 1.10 s, bs
  3.76 s (3.4×). Serial peak RSS is 6.9 GB for cb and 12.5 GB for bs in
  coverage, and 1.3 GB and 2.7 GB in approx.
- Re-burning per tile is the only strategy here that uses the cores, and
  its cost follows the number of tiles the ring touches. For coast-1e6
  coverage:

  | tile | tiles touched | re-burn, 1 thread | re-burn, 128 threads | burn + slice, 128 threads | re-burn RSS, 128 threads |
  |---|---|---|---|---|---|
  | 4096 | 16 | 34.1 s | 3.9 s | 12.4 s | 44 GB |
  | 1024 | 178 | 116 s | 9.0 s | 12.3 s | 66 GB |
  | 256 | 2,317 | 466 s | 32.8 s | 12.4 s | 34 GB |

  With large tiles and a whole node, re-burning wins about 3×. With small
  tiles burn-once wins, and at 1 thread it wins at every tile size
  (2.8–37×). In approx mode burn-once wins at every tile size and thread
  count (coast-1e6, 128 threads: 1.1 s against 1.7–7.6 s).

**Many small and medium geometries** (rock outcrops at 1024 m to 32 m,
small-1e5, rects):

- Serial coverage is close: cb is ahead on rock 1024–128 m (1.06–1.25×),
  and the two are within 3% on rects, small-1e5 and rock 64–32 m (rock-32m:
  cb 0.94 s, bs 0.92 s). Serial approx favours cb, by 1.3–1.6× (rock-32m:
  cb 0.196 s, bs 0.313 s).
- Feature-parallel, cb scales further on the larger outputs. On rock-32m at
  128 threads, coverage takes cb 0.048 s (19.5×) and bs 0.092 s (9.9×), and
  approx takes cb 0.019 s (10.4×; 0.014 s at 64 threads) and bs 0.025 s
  (12.7×). On small burns cb is fastest at 16–32 threads and slows above
  that; bs keeps its time better, so on rock 1024–256 m bs's best is
  1.25–1.74× faster than cb's (§0).
- Tiles at 128 threads, coverage. Re-burning wins on the fine grids:
  rock-32m takes 0.034–0.043 s against 0.093–0.095 s for burn + slice
  (0.056–0.058 s burn, 0.033–0.035 s slice), and small-1e5 takes
  0.065–0.081 s against 0.14–0.17 s at 256–1024-cell tiles. In the 0.3.0
  run burn + slice on rock-32m took 0.11 s, and in the run before 0.13 s.
  Each at its best thread count, burn-once is ahead on rock 1024–256 m with
  512–2048-cell tiles; at 128 threads it is ahead on small-1e5 with
  4096-cell tiles (0.22 s against 0.47 s).
- Tiles at 128 threads, approx. The two are within about 20% of each other
  on rock 128–32 m and small-1e5 at 256–2048-cell tiles, either way round
  (rock-32m, 2048-cell tiles: burn + slice 0.034 s, of which 0.010 s is the
  slice, against 0.030 s re-burning; rock-64m, 256-cell tiles: 0.031 s
  against 0.034 s). Burn-once is ahead on the coarse grids, by up to 2×,
  and on small-1e5 with 4096-cell tiles (0.080 s against 0.176 s).
- Thread scaling at 512-cell tiles (every thread count was run). Burn +
  slice is fastest at 32 threads in approx mode on every rock grid, and in
  coverage mode at 32 threads on rock 1024–256 m and at 64–128 threads on
  rock 128–32 m. At its best thread count it beats re-burning at its best in
  approx mode on every rock grid, from 1.3× (32 m: 0.030 s at 32 threads,
  against 0.040 s) to 3.4× (1024 m: 6.1 ms against 20.9 ms), and in
  coverage mode on rock 1024–512 m (2.9× and 1.5×); at 256 m the two are
  level. In coverage mode re-burning wins on rock 128–32 m (rock-32m:
  0.034 s against 0.093 s, both at 128 threads). On rock 1024–256 m in
  approx mode and rock 1024–512 m in coverage mode, bs's own burn followed
  by its serial slice is faster still (rock-1024m approx: 3.5 ms at 32
  threads), because its parallel burn is faster there.
- Tiles at 1 thread: in coverage mode the three strategies are within
  about 30% of each other at each tile size (rock-32m: cb_slice
  1.10–1.14 s, bs_slice 0.99–1.01 s, re-burning 0.93–1.30 s). In approx
  mode cb_slice is fastest, 1.5–2.3× ahead of re-burning on rock-32m.
- The serial slice: with huge pages on this node it takes few page faults
  (small-1e5 approx, 256-cell tiles: 41.7 ms and 1,149 minor faults, the
  same as in the 0.3.0 run; see the appendix for the dev-VM comparison).
- Memory: re-burning stays at 41–46 MB per process on the rock outcrops at
  most settings; it rises to 0.1–0.42 GB at 128 threads on the fine grids.
  Burn-once holds the whole output: 0.31–0.40 GB for rock-32m coverage at
  1 thread, and 0.86–0.93 GB at 128 threads.

**Slicing:** `slice_tiles` cut rock-32m approx into 7,560 tiles in 0.030 s.
A `crop()` per tile took 5.49 s on the same burn (186×).

**What to improve:**

- in controlledburn-rs: start fewer threads in `burn_wkb_par` when the burn
  is small (or reuse them), since it is fastest at 16–32 threads up to a few
  hundred thousand records; and the parallel slice on fine grids, which at
  0.033–0.035 s on rock-32m coverage alone takes about as long as
  re-burning every tile;
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
  - Feature-parallel splitting is by estimated cost (vertex count for bs;
    coordinates plus rows spanned for cb), a proxy; a skewed input (one huge
    ring among many small ones) will not balance well.
- **Workflows:** burn-once needs the whole sparse output in one place.
  Distributed workers that cannot share it need a shuffle of
  O(perimeter) records, which is not measured here. Re-burn per tile needs
  no global step.

## 8. Relationship between the two crates

Both crates derive from the controlledburn C++ core and exactextract;
burnscars was written for earth-mover's chunked engine. The two crates are two takes on one problem and have borrowed tests
and benchmarks from each other. Both are linked above. Cases that favour
either crate are welcome: the harness takes any WKB and grid (see README).

## Appendix: the controlledburn-rs 0.3.0 run (job 52923373)

What changed since job 52790194: the crate is the 0.3.0 release (multi-part
records merged, huge rings stored compactly, `burn_par`, a thread cap on the
parallel slice); the harness's `cb_par` is now the crate's `burn_par` after
parsing the WKB in parallel, with the old groups-and-concatenate burn kept as
`cb_par_groups` at 128 threads; and every timing records page faults, with
the node's huge-page and allocator settings in `env.json`. burnscars and the
toolchain are unchanged.

- **Correctness is unchanged.** All 20 input/mode pairs verify as before:
  every cell equal except the same two single-cell differences on rock-32m
  (§4), and parallel output equals serial output for both crates. Record
  counts are identical.
- **Serial coverage is faster, and uses less memory on one huge ring.**
  coast-1e5 0.373 → 0.323 s (−13%), rock-32m 1.00 → 0.957 s, coast-1e6
  12.6 → 12.4 s with peak RSS 9.0 → 6.9 GB (a ring's traversals now take
  16 bytes each, controlledburn-rs #37).
- **Serial approx is 4–13% slower** (coast-1e5 0.0349 → 0.0383 s, small-1e5
  0.305 → 0.346 s, rock-32m 0.182 → 0.196 s), while burnscars moved by
  4% or less. On the dev VM rock-32m approx is 3–8% slower in repeated
  alternating runs. Bisected over the merges, it arrives with
  controlledburn-rs #38, which adds `burn_par` but changes no code on the
  serial path, so it is code generation (inlining and layout), not work.
- **Huge pages explain the slice timings, now measured.** This node had
  transparent huge pages `always`. The serial slice of small-1e5 (approx,
  256-cell tiles) took 1,149 minor faults and 41.8 ms; on the dev VM with
  4 KB pages it takes 14,900 faults and 82 ms, and the two earlier Setonix
  runs took 31 and 67 ms. That is the first-touch cost described in
  controlledburn-rs `design/slice-after-burn.md`, which those runs could
  only infer.
- **The slice thread cap works.** The parallel slice of a small output no
  longer starts 128 threads: rock-1024m approx, 512-cell tiles, 128
  threads, 16.3 ms → 1.2 ms (controlledburn-rs #31).
- **`cb_par` (`burn_par`) was 2–11× slower than `cb_par_groups` at 128
  threads** on the rock outcrops and small-1e5, and level on the coastlines:

  | input | mode | cb_par | cb_par_groups |
  |---|---|---|---|
  | rock-1024m | coverage | 0.0285 s | 0.0042 s |
  | rock-1024m | approx | 0.0288 s | 0.0027 s |
  | rock-32m | coverage | 0.0669 s | 0.0647 s |
  | rock-32m | approx | 0.0344 s | 0.0162 s |
  | small-1e5 | coverage | 0.137 s | 0.104 s |
  | small-1e5 | approx | 0.0965 s | 0.0252 s |
  | coast-1e6 | coverage | 12.6 s | 12.6 s |

  The cost is nearly constant across the rock grids (about 25–35 ms from
  1024 m to 32 m, the same 25,954 polygons), so it is per geometry, not per
  cell. Split apart on the dev VM at 32 threads:

  | input | mode | parse | `burn_par` | free the parsed geometries | groups |
  |---|---|---|---|---|---|
  | rock-1024m | approx | 1.9 ms | 7.4 ms | 7.3 ms | 3.1 ms |
  | rock-32m | approx | 2.0 ms | 19.7 ms | 10.4 ms | 37.3 ms |
  | rock-32m | coverage | 1.6 ms | 130 ms | 15.0 ms | 176 ms |
  | small-1e5 | approx | 5.7 ms | 27.0 ms | 27.7 ms | 63.5 ms |
  | small-1e5 | coverage | 3.5 ms | 227 ms | 22.0 ms | 257 ms |

  `burn_par` itself beats the groups except on the smallest burns, where
  starting threads still costs a few milliseconds. What `cb_par` mostly
  measured is building and freeing an owned geometry for each of tens of
  thousands of polygons; the groups never do, because `burn_wkb` parses
  each geometry as it burns it. Any caller who starts from WKB pays that
  cost with `burn_par`, so the fix belongs in the crate: a parallel burn
  that parses WKB inside each worker's chunk. Until then the parallel
  burn-once numbers in this run (B, and C/D above 1 thread) overstate the
  burn by that overhead: small-1e5 approx's burn before slicing went from
  0.025 s to 0.114 s at 128 threads, and its peak RSS from 186 to 379 MB.
  Where the burn is large the gain shows anyway: rock-32m coverage burn +
  slice at 128 threads (2048-cell tiles) went from 0.13 to 0.11 s, and its
  peak RSS from 1.29 to 0.94 GB.

That WKB-parsing burn became `burn_wkb_par` in controlledburn-rs 0.3.2,
measured in §0.
