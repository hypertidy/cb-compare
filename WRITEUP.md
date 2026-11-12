# Sparse rasterization in Rust: controlledburn-rs and burnscars, compared

*`results/` holds the latest run: controlledburn-rs 0.3.0 on one exclusive
Pawsey Setonix node (2 × AMD EPYC 7763, 128 cores, 256 logical CPUs,
250 GB; Slurm job 52923373, 2026-11-12), with `results/env.json` recording
the machine and versions. The TL;DR and §§2–5 still quote the run before it
(job 52790194, 2026-11-11, controlledburn-rs `e1696fc`; its summary is
[`results/summary.md` at 0263102](https://github.com/hypertidy/cb-compare/blob/0263102/results/summary.md)),
because in the latest run the parallel burn-once timings measure mostly
harness overhead (§0). An earlier run (job 52637376, `56ba259`) is compared
where the numbers moved.*

## 0. Latest run: controlledburn-rs 0.3.0 (job 52923373)

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

**Next:** a WKB-parsing `burn_par` in controlledburn-rs, `cb_par` on it,
and a rerun, after which the TL;DR moves to the new numbers.

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
  0.035 s, against 5.5 s for one `crop()` per tile.
- **Pro:** any tile size, or several tilings, come from the same burn.
- **Pro:** for one large polygon over many tiles, it wins clearly on few
  threads. On coast-1e6 (one 10⁶-vertex ring) at 1 thread, re-burning
  256-cell tiles took 463 s and burn-once 12.7 s (36×).
- **Con:** the parallelism is per geometry, so one huge geometry gets no
  speedup: coast-1e5 and coast-1e6 run at 1.0× on 128 threads.
- **Con:** on many small geometries with a whole node it is still slower
  than re-burning. The slice is now parallel (`slice_tiles_par`), which
  halved burn + slice on rock-32m coverage, from 0.27 s to 0.13 s, but
  re-burning takes 0.036 s. What is left is the parallel burn (0.08 s).
- **Con:** the parallel slice starts its threads even when the output is
  small. At 128 threads that costs about 15 ms, so up to about 2 M records
  burn + slice is fastest at 16–32 threads, not 128.

**Re-burn each fixed tile** (burnscars' raster-parallel design)

- **Pro:** there's no global step. Tiles are independent, which suits
  distributed or lazy work. For many small geometries the memory per
  process stays small: 44 MB on rock-32m coverage at 1 thread (0.2–0.4 GB
  at 128), against 0.3–0.9 GB to burn the whole grid.
- **Pro:** it parallelizes even a single geometry. At 128 threads it was
  the fastest strategy in coverage mode on rects, small-1e5 (256–1024-cell
  tiles), the rock outcrops at 128–32 m, and the coastlines with
  1024–4096-cell tiles. On rock-32m coverage it took 0.036 s, against
  0.13 s to burn and slice. On coast-1e6 with 4096-cell tiles it took
  4.1 s, against 12.8 s. Burn-once was ahead on the coarse rock grids
  (1024 m and 512 m, where bs's burn with a serial slice was fastest), on
  the coastlines in approx mode, and on small-1e5 with 4096-cell tiles.
- In approx mode on the finer grids the answer depends on the thread
  count. At 128 threads re-burning was faster on rock 64–32 m at every tile
  size, and on rock 256–128 m and small-1e5 at 256–1024-cell tiles, by 3%
  to 1.6× (rock-32m, 2048-cell tiles: 0.030 s against 0.048 s), because
  cb's slice pays its thread start-up. At 512-cell
  tiles, the one size run at every thread count, burn-once at 16–32 threads
  beat re-burning on every rock grid, by 1.2× (32 m) to 4× (1024 m).
- **Con:** the cost grows with vertices × tiles touched. On coast-1e6 at
  128 threads it went from 4.1 s (4096-cell tiles) to 32.0 s (256-cell
  tiles), while burn-once stayed at 12.7–13.0 s. Sharing prepared edges across
  tiles is still a TODO in burnscars.
- **Con:** for one huge geometry, memory grows with the threads: 35–73 GB
  peak on coast-1e6 at 128 threads, against 14 GB for burn-once.
- **Con:** exact tiles need deliberate lattice rules. burnscars has them;
  burn-once doesn't need them.

**The engines themselves (serial, whole grid):**

- controlledburn-rs is faster in approx mode: 1.3–1.7× on the rock outcrops,
  1.6–1.8× on rects and small-1e5, and 2.9–3.6× on the coastline rings.
- In coverage mode the two are now close. controlledburn-rs reuses its
  buffers across polygons (controlledburn-rs #26), which halved its serial
  coverage time on many polygons (rock-32m: 1.95 s to 1.00 s). cb is ahead
  on rects (1.09×) and rock 1024–512 m (1.1–1.2×), the two are level at
  256–128 m, and burnscars is ahead by 1.07–1.14× on small-1e5 and rock
  64–32 m, down from 2.0–2.3× in the earlier run. On the coastline rings cb
  is ahead by 5–15%.
- Feature-parallel, controlledburn-rs scales further: its best speedup is
  12–20× on small-1e5 and the rock outcrops (8–10× on rects), against
  10–17× for burnscars (4–5× on rects). Its speedups are lower than in the
  earlier run (19–28×) because its serial time fell; its 128-thread times
  improved a little (rock-32m coverage: 0.088 s to 0.078 s, against 0.087 s
  for bs). In approx mode cb is 2.0–3.1× ahead at 128 threads.

**In one line:** burn once and slice suits large or complex geometry,
arbitrary or repeated tilings, and small thread counts. Re-burning per tile
suits a whole node, many small geometries, tight memory per worker and
workers with no shared state. A hybrid also works: burn in parallel by
geometry, then route the records to tiles in parallel, as
controlledburn-rs's `slice_tiles_par` now does.

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
| controlledburn-rs | `e1696fc` (hypertidy/controlledburn-rs#30, `slice_tiles_par`) | the earlier run used `56ba259` (#25, `slice_tiles`) |
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
node. "Earlier run" means job 52637376 with controlledburn-rs `56ba259`;
burnscars is the same commit in both, and its times moved by under 1% on
average between the runs, which bounds the run-to-run drift of the node.

**One very large geometry** (coast-1e5 and coast-1e6, a single ring of
10⁵ or 10⁶ vertices):

- Feature-parallel burning gives nothing: both engines run at 1.0× from 2 to
  128 threads. cb's approx burn of coast-1e6 gets slightly slower (0.9×),
  and its peak RSS in coverage mode rises from 9.3 GB to 13.5 GB.
- Serial whole-grid: cb coverage 12.6 s, bs 14.6 s; cb approx 1.05 s, bs
  3.79 s (3.6×). Serial peak RSS is 9.3 GB for cb and 12.8 GB for bs in
  coverage, and 1.3 GB and 2.8 GB in approx.
- Re-burning per tile is the only strategy here that uses the cores, and
  its cost follows the number of tiles the ring touches. For coast-1e6
  coverage:

  | tile | tiles touched | re-burn, 1 thread | re-burn, 128 threads | burn + slice, 128 threads | re-burn RSS, 128 threads |
  |---|---|---|---|---|---|
  | 4096 | 16 | 33.8 s | 4.1 s | 12.8 s | 45 GB |
  | 1024 | 178 | 117 s | 8.9 s | 13.0 s | 73 GB |
  | 256 | 2,317 | 463 s | 32.0 s | 12.9 s | 35 GB |

  With large tiles and a whole node, re-burning wins about 3×. With small
  tiles burn-once wins, and at 1 thread it wins at every tile size
  (2.6–37×).

**Many small and medium geometries** (rock outcrops at 1024 m to 32 m,
small-1e5, rects):

- Serial coverage is now close. cb is ahead on rects (1.09×) and rock
  1024–512 m (1.1–1.2×), the two are level at 256–128 m, and bs is ahead
  by 1.07–1.14× on small-1e5 and rock 64–32 m (rock-32m: cb 1.00 s, bs
  0.88 s). In the earlier run bs led by 1.3–2.3× (rock-32m: cb 1.95 s); the
  change is controlledburn-rs reusing its buffers across polygons
  (controlledburn-rs #26). Serial approx favours cb, by 1.3–1.7× (rock-32m:
  cb 0.182 s, bs 0.305 s).
- Feature-parallel, cb scales further. On rock-32m at 128 threads, coverage
  takes cb 0.078 s (12.8×) and bs 0.087 s (10.1×), and approx takes cb
  0.015 s (12.3×) and bs 0.036 s (8.5×; bs is fastest at 32 threads,
  0.025 s). Gains past 64 threads are small, and the smallest runs get
  slower (rects approx: 9.5× at 32 threads, 2.3× at 128).
- Tiles at 128 threads, coverage. Re-burning wins on the fine grids:
  rock-32m takes 0.036–0.043 s against 0.13 s for burn + slice (0.08 s
  burn, 0.04 s slice), and small-1e5 takes 0.065–0.079 s against
  0.16–0.19 s at 256–1024-cell tiles. In the earlier run, with a serial
  slice, burn + slice took 0.25–0.30 s and 0.36–0.46 s, so the gap
  narrowed from about 7× to about 3.5× on rock-32m and from about 6× to
  about 2.5× on small-1e5. Burn-once is ahead on the coarse grids
  (1024 m, 512 m), and on small-1e5 with 4096-cell tiles (0.23 s against
  0.47 s).
- Tiles at 128 threads, approx. Re-burning is ahead on rock 64–32 m at
  every tile size, and on rock 256–128 m and small-1e5 at 256–1024-cell
  tiles, by 3% to 1.6×. On rock-32m with 2048-cell tiles it takes 0.030 s
  against 0.048 s, of which cb's slice is 0.026 s. Burn-once is ahead with
  2048–4096-cell tiles on those inputs and on the coarse grids.
- Thread scaling at 512-cell tiles (every thread count was run). Burn +
  slice is fastest at 16–32 threads up to about 2 M records and at 64–128
  threads above 4 M: the parallel slice spends about 15 ms
  starting 128 threads (rock-1024m approx: a 0.5 ms serial slice takes
  16 ms at 128 threads). At its best thread count burn-once beats
  re-burning in approx mode on every rock grid, from 1.2× (32 m: 0.035 s
  at 32 threads, against 0.043 s) to 4× (1024 m), and in coverage mode on
  rock 1024–512 m. In coverage mode re-burning wins on rock 256–32 m
  (rock-32m: 0.036 s against 0.127 s at 64 threads).
- Tiles at 1 thread: in coverage mode the three strategies are within
  about 25% of each other at each tile size (rock-32m: cb_slice
  1.15–1.18 s, bs_slice 0.99–1.02 s, re-burning 0.93–1.30 s). In the
  earlier run cb_slice took about 2× as long as bs_slice. In approx mode cb_slice is fastest, 1.5–2.4× ahead of
  re-burning on rock-32m.
- The serial slice took longer in this run in approx mode on the same
  records: rock-32m with 512-cell tiles 21 → 28 ms, small-1e5 with
  256-cell tiles 31 → 67 ms. Coverage slices were the same or faster. Run
  in one process on the dev VM, the old and new `slice_tiles` take the same
  time on the same burn. Burning first, as the harness does, makes the new
  slice 16% slower on small-1e5 approx and no slower on rock-32m. So it
  looks like an interaction between the new burn's memory reuse and the
  slice's many small allocations, not slower slice code. This is open.
- Memory: re-burning stays at 41–46 MB per process on the rock outcrops at
  most settings; it rises to 0.1–0.45 GB at 128 threads on the fine grids.
  Burn-once holds the whole output: 0.32–0.41 GB for rock-32m coverage at
  1 thread, and 1.3 GB at 128 threads, up from 0.87–0.93 GB in the earlier
  run (the parallel burn alone went from 0.73 to 0.91 GB).

**Slicing:** `slice_tiles` cut rock-32m approx into 7,560 tiles in 0.035 s.
A `crop()` per tile took 5.53 s on the same burn (160×).

**What to improve:**

- in controlledburn-rs: start fewer slice threads when the output is small
  (about one thread per 100–250 k records would match the best thread
  counts measured here); find what slowed the serial slice after a burn in
  approx mode, and what raised peak memory at 128 threads; and the
  parallel burn of many polygons, which is now most of the burn-once time
  at 128 threads;
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
