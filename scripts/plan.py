#!/usr/bin/env python3
"""Write the timing plan, one configuration per line (TSV):
   input mode strategy threads tile repeats

usage: plan.py [--quick] [--threads 1,2,4,8,16,32] [--repeats 5] > plan.tsv

The plan is the experiment design, kept in one place:
  A. single thread, whole grid: cb_burn, bs_burn, every input and mode
  B. thread scaling, whole grid (feature-parallel): cb_par, bs_par
  C. tiles: burn-then-slice (cb_slice, bs_slice) against the fixed-tile
     re-burn (bs_reburn), at several tile sizes, at 1 thread and at max
  D. thread scaling of the tiled strategies at one tile size
  E. the crop()-per-tile loop, once, for scale
"""
import argparse
p = argparse.ArgumentParser()
p.add_argument("--quick", action="store_true", help="small inputs, few repeats (a smoke test)")
p.add_argument("--threads", default=None, help="default 1,2,4,8,16,32 (quick: 1,4,16)")
p.add_argument("--repeats", type=int, default=5)
p.add_argument("--inputs", default=None, help="comma list; default depends on --quick")
a = p.parse_args()
T = [int(t) for t in (a.threads or ("1,4,16" if a.quick else "1,2,4,8,16,32")).split(",")]
R = 2 if a.quick else a.repeats
tmax = max(T)
if a.inputs:
    inputs = a.inputs.split(",")
elif a.quick:
    inputs = ["rects", "coast-1e5", "rock-1024m", "rock-256m"]
else:
    inputs = ["rects", "coast-1e5", "coast-1e6", "small-1e5",
              "rock-1024m", "rock-512m", "rock-256m", "rock-128m", "rock-64m", "rock-32m"]
tiles = {"rects": [64, 256], "coast-1e5": [256, 1024], "coast-1e6": [256, 1024, 4096],
         "small-1e5": [256, 1024, 4096]}
rock_tiles = [256, 512, 2048]
lines = []
def add(*x): lines.append("\t".join(str(v) for v in x))
for i in inputs:
    ts = tiles.get(i, rock_tiles)
    for m in ["coverage", "approx"]:
        for s in ["cb_burn", "bs_burn"]:
            add(i, m, s, 1, 0, R)                                 # A
        for t in T:
            if t > 1:
                for s in ["cb_par", "bs_par"]:
                    add(i, m, s, t, 0, R)                         # B
        for tile in ts:
            for t in sorted({1, tmax}):
                for s in ["cb_slice", "bs_slice", "bs_reburn"]:
                    add(i, m, s, t, tile, R)                      # C
        for t in T:
            if t not in (1, tmax):
                for s in ["cb_slice", "bs_slice", "bs_reburn"]:
                    add(i, m, s, t, ts[len(ts) // 2], R)          # D
# E: the crop loop, one input that shows it, one repeat
e = "rock-256m" if a.quick else "rock-32m"
if e in inputs:
    add(e, "approx", "cb_crop_loop", 1, 2048 if not a.quick else 256, 1)
print("\n".join(lines))
