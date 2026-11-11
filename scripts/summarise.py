#!/usr/bin/env python3
"""Collect results/{verify,timing} into results/summary.md and results/figures/*.png.

Every timing is reported as median [min-max] over the repeats, never a mean
alone, and the figures show the spread (whiskers = min..max).
usage: summarise.py <results-dir>
"""
import glob, json, os, statistics, sys
from collections import defaultdict
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

R = sys.argv[1]
FIG = os.path.join(R, "figures"); os.makedirs(FIG, exist_ok=True)
env = json.load(open(os.path.join(R, "env.json")))

# fixed series colours (categorical slots 1-3 of the reference palette, light mode)
COL = {"cb": "#2a78d6", "bs": "#eb6834", "reburn": "#1baf7a"}
INK, INK2, GRID = "#0b0b0b", "#52514e", "#e4e3df"
LABEL = {"cb_burn": "controlledburn-rs", "bs_burn": "burnscars",
         "cb_par": "controlledburn-rs", "bs_par": "burnscars",
         "cb_slice": "cb: burn + slice", "bs_slice": "bs: burn + slice", "bs_reburn": "bs: re-burn per tile"}
SCOL = {"cb_burn": COL["cb"], "cb_par": COL["cb"], "cb_slice": COL["cb"],
        "bs_burn": COL["bs"], "bs_par": COL["bs"], "bs_slice": COL["bs"], "bs_reburn": COL["reburn"]}
plt.rcParams.update({"font.size": 9, "axes.edgecolor": INK2, "axes.labelcolor": INK2, "xtick.color": INK2,
                     "ytick.color": INK2, "axes.spines.top": False, "axes.spines.right": False,
                     "axes.grid": True, "grid.color": GRID, "grid.linewidth": 0.6, "figure.dpi": 150})

# ---- load
runs = defaultdict(list); rss = {}
for f in glob.glob(os.path.join(R, "timing", "*.jsonl")):
    key = os.path.basename(f)[:-6]
    for l in open(f):
        runs[key].append(json.loads(l))
    rp = f[:-6] + ".rss"
    if os.path.exists(rp):
        rss[key] = int(open(rp).read().split()[-1]) / 1024  # MB
from matplotlib.ticker import FuncFormatter, LogLocator, NullFormatter
def logfmt(ax, axis="both"):
    """plain-number labels on log axes, major ticks only"""
    for a in ([ax.xaxis, ax.yaxis] if axis == "both" else [getattr(ax, axis + "axis")]):
        if a.get_scale() == "log":
            a.set_major_locator(LogLocator(base=10, subs=(1, 2, 5)))
            a.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}"))
            a.set_minor_formatter(NullFormatter())
def stat(rows, field="total_s"):
    v = sorted(r[field] for r in rows if field in r)
    return (statistics.median(v), v[0], v[-1]) if v else None
def cfg(i, m, s, t, tile): return f"{i}.{m}.{s}.t{t}.tile{tile}"
inputs = sorted({r[0]["input"] for r in runs.values()}, key=lambda s: (not s.startswith("rock"), s))
order = ["rects", "coast-1e5", "coast-1e6", "small-1e5", "rock-1024m", "rock-512m", "rock-256m", "rock-128m", "rock-64m", "rock-32m"]
inputs = [i for i in order if i in inputs] + [i for i in inputs if i not in order]
threads = sorted({r[0]["threads"] for r in runs.values()})
fmt = lambda s: "–" if s is None else (f"{s[0]:.3g} [{s[1]:.3g}–{s[2]:.3g}]")

md = [f"# Results\n",
      f"Machine: {env['cpu']} ({env['nproc']} logical CPUs, {env['mem_gb']} GB), {env['os']}, kernel {env['kernel']}. "
      f"Label: {env['hostname_class']}{' Slurm job ' + env['slurm_job'] if env['slurm_job'] else ''}.  ",
      f"controlledburn-rs `{env['controlledburn_rs'][:10]}`, burnscars `{env['burnscars'][:10]}`, {env['rustc']}.\n",
      "Times are seconds, median [min–max] over the repeats after one warm-up. RSS is the peak resident set of the whole process (input included), MB.\n"]

# ---- correctness
md.append("## Correctness (before timing)\n")
md.append("Cells compared per geometry id, with records on the same cell summed (cb keeps one record per polygon part, bs one per geometry). "
          "`equal` is bit-equal; `≤1e-6` differs only by float rounding.\n")
md.append("| input | mode | cells | equal | ≤1e-6 | differ | only cb | only bs | bs failed | par = serial (cb/bs) | area rel. err cb / bs | exact oracle cb / bs |")
md.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
for i in inputs:
    for m in ["coverage", "approx"]:
        f = os.path.join(R, "verify", f"{i}.{m}.json")
        if not os.path.exists(f): continue
        d = json.load(open(f)); p = d["polygons"]
        ac = d.get("area_check"); ar = f"{ac['max_rel_err_cb']:.1e} / {ac['max_rel_err_bs']:.1e}" if ac else "–"
        orc = (f"{d['oracle_cb']['differ']+d['oracle_cb']['only_a']} / {d['oracle_bs']['differ']+d['oracle_bs']['only_a']} cells wrong, "
               f"{d['oracle_cb']['only_b']} below tol.") if "oracle_cb" in d else "–"
        md.append(f"| {i} | {m} | {p['cells_a']:,} | {p['equal']:,} | {p['within_1e-6']:,} | {p['differ']} | {p['only_a']} | {p['only_b']} | "
                  f"{d['bs_failed']} | {d['par_equals_serial_cb']}/{d['par_equals_serial_bs']} | {ar} | {orc} |")
md.append("")
for i in inputs:
    for m in ["coverage", "approx"]:
        f = os.path.join(R, "verify", f"{i}.{m}.json")
        if os.path.exists(f):
            ex = json.load(open(f))["polygons"]["examples"]
            if ex: md.append(f"- {i} {m}: " + "; ".join(ex[:4]))
md.append("")

# ---- A: single thread
md.append("## A. Single thread, whole grid\n")
md.append("| input | mode | records cb / bs | cb_burn | bs_burn | RSS cb / bs |")
md.append("|---|---|---|---|---|---|")
for i in inputs:
    for m in ["coverage", "approx"]:
        a, b = runs.get(cfg(i, m, "cb_burn", 1, 0)), runs.get(cfg(i, m, "bs_burn", 1, 0))
        if not a or not b: continue
        md.append(f"| {i} | {m} | {a[0]['records']:,} / {b[0]['records']:,} | {fmt(stat(a))} | {fmt(stat(b))} | "
                  f"{rss.get(cfg(i,m,'cb_burn',1,0),0):.0f} / {rss.get(cfg(i,m,'bs_burn',1,0),0):.0f} |")
md.append("\n![single thread](figures/single_thread.png)\n")

fig, axs = plt.subplots(1, 2, figsize=(8, 0.35 * len(inputs) + 1.2), sharey=True)
for ax, m in zip(axs, ["coverage", "approx"]):
    for k, s in enumerate(["cb_burn", "bs_burn"]):
        for y, i in enumerate(inputs):
            st = stat(runs.get(cfg(i, m, s, 1, 0), []))
            if not st: continue
            yy = y + (k - 0.5) * 0.28
            ax.plot([st[1], st[2]], [yy, yy], color=SCOL[s], lw=2, solid_capstyle="round")
            ax.plot(st[0], yy, "o", ms=6, color=SCOL[s], mec="white", mew=1, label=LABEL[s] if y == 0 else None)
    ax.set_xscale("log"); ax.set_title(f"{m} mode", color=INK, loc="left"); ax.set_xlabel("seconds (log), dot = median, bar = min–max")
    ax.grid(axis="y", visible=False); logfmt(ax, "x")
axs[0].set_yticks(range(len(inputs)), inputs); axs[0].invert_yaxis()
fig.legend(*axs[0].get_legend_handles_labels(), frameon=False, loc="upper right", ncol=2)
fig.suptitle("Serial whole-grid burn from WKB", x=0.01, ha="left", color=INK)
fig.tight_layout(); fig.savefig(os.path.join(FIG, "single_thread.png")); plt.close(fig)

# ---- B: scaling
md.append("## B. Thread scaling, feature-parallel whole-grid burn\n")
md.append("Speedup is the serial median over the parallel median, each engine against its own serial burn.\n")
md.append("| input | mode | threads | cb_par | speedup | bs_par | speedup | RSS cb / bs |")
md.append("|---|---|---|---|---|---|---|---|")
for i in inputs:
    for m in ["coverage", "approx"]:
        sc, sb = stat(runs.get(cfg(i, m, "cb_burn", 1, 0), [])), stat(runs.get(cfg(i, m, "bs_burn", 1, 0), []))
        for t in threads:
            if t == 1: continue
            a, b = stat(runs.get(cfg(i, m, "cb_par", t, 0), [])), stat(runs.get(cfg(i, m, "bs_par", t, 0), []))
            if not a or not b: continue
            md.append(f"| {i} | {m} | {t} | {fmt(a)} | {sc[0]/a[0]:.1f}× | {fmt(b)} | {sb[0]/b[0]:.1f}× | "
                      f"{rss.get(cfg(i,m,'cb_par',t,0),0):.0f} / {rss.get(cfg(i,m,'bs_par',t,0),0):.0f} |")
md.append("\n![scaling](figures/scaling.png)\n")
n = len(inputs); nc = min(4, n); nr = -(-n // nc)
fig, axs = plt.subplots(nr, nc, figsize=(2.3 * nc, 2.1 * nr + 0.6), squeeze=False, sharex=True)
for ax, i in zip(axs.flat, inputs):
    for s, base in [("cb_par", "cb_burn"), ("bs_par", "bs_burn")]:
        b0 = stat(runs.get(cfg(i, "coverage", base, 1, 0), []))
        pts = [(1, 1.0)] + [(t, b0[0] / stat(runs[cfg(i, "coverage", s, t, 0)])[0]) for t in threads
                            if t > 1 and cfg(i, "coverage", s, t, 0) in runs] if b0 else []
        if pts:
            ax.plot(*zip(*pts), "-o", lw=2, ms=4, color=SCOL[s], label=LABEL[s])
    ax.plot(threads, threads, "--", lw=1, color=INK2, alpha=0.5)
    ax.set_xscale("log", base=2); ax.set_yscale("log", base=2); ax.set_title(i, color=INK, loc="left", fontsize=9)
for ax in axs.flat[n:]: ax.set_visible(False)
fig.legend(*axs[0][0].get_legend_handles_labels(), frameon=False, loc="upper right", ncol=2, fontsize=8)
fig.supxlabel("threads (log₂); dashed = linear", color=INK2, fontsize=9); fig.supylabel("speedup over own serial burn", color=INK2, fontsize=9)
fig.suptitle("Feature-parallel burn, coverage mode", x=0.01, ha="left", color=INK)
fig.tight_layout(); fig.savefig(os.path.join(FIG, "scaling.png")); plt.close(fig)

# ---- C/D: tiles
md.append("## C. Tiles: burn once and slice, against re-burn per fixed tile\n")
md.append("`cb_slice` / `bs_slice`: whole-grid burn (feature-parallel when threads > 1), then a one-pass slice; burn and slice times shown. cb slices on the same threads (`slice_tiles_par`), bs's slice is serial. "
          "`bs_reburn`: each tile burns the geometries whose bbox meets it onto `grid.window(tile)`, parallel over tiles. "
          "pairs = geometry–tile pairs burned.\n")
md.append("| input | mode | tile | threads | tiles (non-empty) | cb_slice total (burn + slice) | bs_slice total (burn + slice) | bs_reburn total (pairs) | RSS cb / bs / reburn |")
md.append("|---|---|---|---|---|---|---|---|---|")
tiles_of = defaultdict(set)
for k, r in runs.items():
    if r[0]["strategy"] in ("cb_slice", "bs_slice", "bs_reburn"):
        tiles_of[r[0]["input"]].add(r[0]["tile"])
for i in inputs:
    for m in ["coverage", "approx"]:
        for tile in sorted(tiles_of[i]):
            for t in threads:
                c, b, rb = (runs.get(cfg(i, m, s, t, tile)) for s in ["cb_slice", "bs_slice", "bs_reburn"])
                if not (c and b and rb): continue
                med = lambda rows, f: statistics.median(x[f] for x in rows)
                md.append(f"| {i} | {m} | {tile} | {t} | {c[0]['tiles']:,} ({c[0]['tiles_nonempty']:,}) | "
                          f"{fmt(stat(c))} ({med(c,'burn_s'):.3g} + {med(c,'slice_s'):.3g}) | "
                          f"{fmt(stat(b))} ({med(b,'burn_s'):.3g} + {med(b,'slice_s'):.3g}) | "
                          f"{fmt(stat(rb))} ({rb[0]['geom_tile_pairs']:,}) | "
                          f"{rss.get(cfg(i,m,'cb_slice',t,tile),0):.0f} / {rss.get(cfg(i,m,'bs_slice',t,tile),0):.0f} / {rss.get(cfg(i,m,'bs_reburn',t,tile),0):.0f} |")
md.append("\n![tiles](figures/tiles.png)\n")
tmax = max(threads)
tin = [i for i in inputs if tiles_of[i]]
if tin:
    n = len(tin); nc = min(4, n); nr = -(-n // nc)
    fig, axs = plt.subplots(nr, nc, figsize=(2.3 * nc, 2.1 * nr + 0.6), squeeze=False)
    for ax, i in zip(axs.flat, tin):
        for s in ["cb_slice", "bs_slice", "bs_reburn"]:
            pts = [(tile, stat(runs[cfg(i, "coverage", s, tmax, tile)])) for tile in sorted(tiles_of[i]) if cfg(i, "coverage", s, tmax, tile) in runs]
            if not pts: continue
            x = [p[0] for p in pts]
            ax.plot(x, [p[1][0] for p in pts], "-o", lw=2, ms=4, color=SCOL[s], label=LABEL[s])
            ax.fill_between(x, [p[1][1] for p in pts], [p[1][2] for p in pts], color=SCOL[s], alpha=0.15, lw=0)
        ax.set_xscale("log", base=2); ax.set_yscale("log"); ax.set_title(i, color=INK, loc="left", fontsize=9); logfmt(ax, "y")
    for ax in axs.flat[n:]: ax.set_visible(False)
    fig.legend(*axs[0][0].get_legend_handles_labels(), frameon=False, loc="upper right", ncol=3, fontsize=8)
    fig.supxlabel("tile edge, cells (log₂)", color=INK2, fontsize=9); fig.supylabel(f"seconds, {tmax} threads (band: min–max)", color=INK2, fontsize=9)
    fig.suptitle("Tiled output, coverage mode", x=0.01, ha="left", color=INK)
    fig.tight_layout(); fig.savefig(os.path.join(FIG, "tiles.png")); plt.close(fig)

# ---- E: crop loop
cl = [r for k, r in runs.items() if r[0]["strategy"] == "cb_crop_loop"]
if cl:
    md.append("## E. `crop()` per tile, for scale\n")
    for r in cl:
        x = r[0]; s = runs.get(cfg(x["input"], x["mode"], "cb_slice", 1, x["tile"]))
        md.append(f"- {x['input']} {x['mode']}, tile {x['tile']} ({x['tiles']:,} tiles): crop loop {x['slice_s']:.3g} s after a {x['burn_s']:.3g} s burn"
                  + (f"; `slice_tiles` on the same burn: {statistics.median(y['slice_s'] for y in s):.3g} s." if s else "."))
open(os.path.join(R, "summary.md"), "w").write("\n".join(md) + "\n")
print(os.path.join(R, "summary.md"))
