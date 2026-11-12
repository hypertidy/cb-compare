//! Shared pieces of the controlledburn-rs / burnscars comparison:
//! input files, the two engines behind one interface, parallel and tiled
//! strategies, and a cell-by-cell comparison that works on records
//! (never a dense raster), so it scales to the REMA grids.

use std::ops::Range;

use burnscars as bs;
use controlledburn as cb;
use geo_traits::{
    CoordTrait, GeometryTrait, GeometryType, LineStringTrait, MultiPolygonTrait, PolygonTrait,
};
use rayon::prelude::*;

// ------------------------------------------------------------------ inputs

/// The `.wkbs` format (as in controlledburn-rs `book/data`): u32 LE count,
/// then for each geometry a u32 LE byte length and the WKB bytes.
pub fn read_wkbs(path: &str) -> Vec<Vec<u8>> {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as usize;
    let n = u32_at(0);
    let mut out = Vec::with_capacity(n);
    let mut i = 4;
    for _ in 0..n {
        let len = u32_at(i);
        out.push(b[i + 4..i + 4 + len].to_vec());
        i += 4 + len;
    }
    assert_eq!(i, b.len(), "{path}: trailing bytes");
    out
}

pub fn write_wkbs(path: &str, geoms: &[Vec<u8>]) {
    let mut b = (geoms.len() as u32).to_le_bytes().to_vec();
    for g in geoms {
        b.extend((g.len() as u32).to_le_bytes());
        b.extend(g);
    }
    std::fs::write(path, b).unwrap();
}

/// A grid as stored in `inputs/*.grid.json`.
#[derive(Clone, Copy, Debug)]
pub struct GridDef {
    pub xmin: f64,
    pub ymin: f64,
    pub xmax: f64,
    pub ymax: f64,
    pub ncol: u32,
    pub nrow: u32,
}

impl GridDef {
    pub fn read(path: &str) -> Self {
        let s = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let f = |k: &str| v[k].as_f64().unwrap_or_else(|| panic!("{path}: no {k}"));
        GridDef { xmin: f("xmin"), ymin: f("ymin"), xmax: f("xmax"), ymax: f("ymax"), ncol: f("ncol") as u32, nrow: f("nrow") as u32 }
    }
    pub fn write(&self, path: &str, note: &str) {
        let v = serde_json::json!({"xmin": self.xmin, "ymin": self.ymin, "xmax": self.xmax, "ymax": self.ymax,
                                   "ncol": self.ncol, "nrow": self.nrow, "note": note});
        std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
    pub fn dx(&self) -> f64 {
        (self.xmax - self.xmin) / self.ncol as f64
    }
    pub fn dy(&self) -> f64 {
        (self.ymax - self.ymin) / self.nrow as f64
    }
    pub fn cb(&self) -> cb::GridSpec {
        cb::GridSpec::new(self.xmin, self.ymin, self.xmax, self.ymax, self.ncol, self.nrow)
    }
    pub fn bs(&self) -> bs::Grid {
        bs::Grid::from_extent(self.xmin, self.ymin, self.xmax, self.ymax, self.ncol, self.nrow).unwrap()
    }
}

// ------------------------------------------------------------------ WKB writing (synthetic inputs)

fn put_ring(b: &mut Vec<u8>, ring: &[(f64, f64)]) {
    b.extend((ring.len() as u32).to_le_bytes());
    for &(x, y) in ring {
        b.extend(x.to_le_bytes());
        b.extend(y.to_le_bytes());
    }
}

/// Polygon WKB (little endian); rings must be closed.
pub fn wkb_polygon(rings: &[Vec<(f64, f64)>]) -> Vec<u8> {
    let mut b = vec![1u8];
    b.extend(3u32.to_le_bytes());
    b.extend((rings.len() as u32).to_le_bytes());
    for r in rings {
        put_ring(&mut b, r);
    }
    b
}

// ------------------------------------------------------------------ geometry helpers

pub fn parse(w: &[u8]) -> wkb::reader::Wkb<'_> {
    wkb::reader::read_wkb(w).expect("wkb parse")
}

fn ring_area<L: LineStringTrait<T = f64>>(r: &L) -> f64 {
    let c: Vec<(f64, f64)> = r.coords().map(|c| (c.x(), c.y())).collect();
    let mut a = 0.0;
    for i in 0..c.len().saturating_sub(1) {
        a += c[i].0 * c[i + 1].1 - c[i + 1].0 * c[i].1;
    }
    a / 2.0
}

fn poly_area<P: PolygonTrait<T = f64>>(p: &P) -> f64 {
    let shell = p.exterior().map(|r| ring_area(&r).abs()).unwrap_or(0.0);
    shell - p.interiors().map(|r| ring_area(&r).abs()).sum::<f64>()
}

/// Planar area of a (multi)polygon WKB; 0 for other types.
pub fn wkb_area(w: &[u8]) -> f64 {
    match parse(w).as_type() {
        GeometryType::Polygon(p) => poly_area(p),
        GeometryType::MultiPolygon(m) => m.polygons().map(|p| poly_area(&p)).sum(),
        _ => 0.0,
    }
}

pub fn wkb_bbox(w: &[u8]) -> Option<[f64; 4]> {
    bs::bbox(&parse(w))
}

pub fn n_vertices(w: &[u8]) -> usize {
    // 16 bytes per 2D coordinate dominates; good enough for load balancing
    w.len() / 16
}

// ------------------------------------------------------------------ engines

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Coverage,
    Approx,
}

impl Mode {
    pub fn parse(s: &str) -> Self {
        match s {
            "coverage" => Mode::Coverage,
            "approx" => Mode::Approx,
            _ => panic!("mode must be coverage or approx, not {s}"),
        }
    }
    fn cb(self) -> cb::BurnOptions {
        match self {
            Mode::Coverage => cb::BurnOptions::coverage(),
            Mode::Approx => cb::BurnOptions::approx(),
        }
    }
    fn bs(self) -> bs::Mode {
        match self {
            Mode::Coverage => bs::Mode::Coverage,
            Mode::Approx => bs::Mode::Approx,
        }
    }
}

/// controlledburn-rs from WKB bytes (parse included), ids = position.
pub fn cb_burn(wkbs: &[Vec<u8>], grid: &cb::GridSpec, mode: Mode) -> cb::BurnResult {
    cb::burn_wkb(wkbs.iter().map(|v| v.as_slice()), grid, mode.cb()).unwrap()
}

/// burnscars from WKB bytes (parse included), ids = `id0 + position`.
/// burnscars stops a burn at the first failing geometry, so geometries are
/// fed one at a time; a failing one is rolled back and reported.
pub fn bs_burn(wkbs: &[Vec<u8>], id0: u32, grid: &bs::Grid, mode: Mode) -> (bs::BurnResult, Vec<(u32, String)>) {
    let mut burner = bs::Burner::new(mode.bs());
    let mut out = bs::BurnResult::default();
    let mut failed = Vec::new();
    for (i, w) in wkbs.iter().enumerate() {
        let id = id0 + i as u32;
        let mark = (out.runs.row.len(), out.edges.row.len(), out.lines.row.len(), out.points.row.len(), out.geom_ptr.len());
        if let Err(e) = burner.burn_into(grid, [(bs::GeomId(id), parse(w))], &mut out) {
            failed.push((id, e.to_string()));
            truncate_bs(&mut out, mark);
        }
    }
    (out, failed)
}

fn truncate_bs(o: &mut bs::BurnResult, m: (usize, usize, usize, usize, usize)) {
    let r = &mut o.runs;
    r.row.truncate(m.0);
    r.col_start.truncate(m.0);
    r.col_end.truncate(m.0);
    r.id.truncate(m.0);
    let e = &mut o.edges;
    e.row.truncate(m.1);
    e.col.truncate(m.1);
    e.fraction.truncate(m.1);
    e.id.truncate(m.1);
    let l = &mut o.lines;
    l.row.truncate(m.2);
    l.col.truncate(m.2);
    l.length.truncate(m.2);
    l.id.truncate(m.2);
    let p = &mut o.points;
    p.row.truncate(m.3);
    p.col.truncate(m.3);
    p.id.truncate(m.3);
    o.geom_ptr.truncate(m.4);
}

pub fn bs_records(r: &bs::BurnResult) -> usize {
    r.runs.row.len() + r.edges.row.len() + r.lines.row.len() + r.points.row.len()
}

pub fn cb_records(r: &cb::BurnResult) -> usize {
    r.runs.len() + r.edges.len() + r.lines.len() + r.points.len()
}

/// burnscars tables (global indices) into the controlledburn-rs AoS form.
pub fn bs_to_cb(r: &bs::BurnResult) -> cb::BurnResult {
    let mut o = cb::BurnResult::default();
    let (ru, e, l, p) = (&r.runs, &r.edges, &r.lines, &r.points);
    for i in 0..ru.row.len() {
        o.runs.push(cb::GridRun { row: ru.row[i] as i32, col_start: ru.col_start[i] as i32, col_end: ru.col_end[i] as i32, id: ru.id[i] as i32 });
    }
    for i in 0..e.row.len() {
        o.edges.push(cb::GridEdge { row: e.row[i] as i32, col: e.col[i] as i32, fraction: e.fraction[i], id: e.id[i] as i32 });
    }
    for i in 0..l.row.len() {
        o.lines.push(cb::GridLine { row: l.row[i] as i32, col: l.col[i] as i32, length: l.length[i], id: l.id[i] as i32 });
    }
    for i in 0..p.row.len() {
        o.points.push(cb::GridPoint { row: p.row[i] as i32, col: p.col[i] as i32, id: p.id[i] as i32 });
    }
    o
}

// ------------------------------------------------------------------ parallel strategies

/// Contiguous index ranges with roughly equal vertex counts.
pub fn groups(wkbs: &[Vec<u8>], n: usize) -> Vec<Range<usize>> {
    let total: usize = wkbs.iter().map(|w| n_vertices(w)).sum();
    let target = total.div_ceil(n.max(1)).max(1);
    let (mut out, mut start, mut acc) = (Vec::new(), 0, 0);
    for (i, w) in wkbs.iter().enumerate() {
        acc += n_vertices(w);
        if acc >= target {
            out.push(start..i + 1);
            start = i + 1;
            acc = 0;
        }
    }
    if start < wkbs.len() {
        out.push(start..wkbs.len());
    }
    out
}

/// controlledburn-rs on `threads` threads: `burn_wkb_par`, which parses
/// each blob on the thread that burns it (parse inside the timing, as for
/// every strategy).
pub fn cb_par(wkbs: &[Vec<u8>], grid: &cb::GridSpec, mode: Mode, threads: usize) -> cb::BurnResult {
    cb::burn_wkb_par(wkbs, grid, mode.cb(), threads).unwrap()
}

/// The harness's own feature-parallel controlledburn-rs, as used before
/// `burn_par` existed: burn each group of geometries onto the full grid
/// in the rayon pool, shift ids by the group's start, concatenate.
pub fn cb_par_groups(wkbs: &[Vec<u8>], grid: &cb::GridSpec, mode: Mode, ngroups: usize) -> cb::BurnResult {
    let parts: Vec<cb::BurnResult> = groups(wkbs, ngroups)
        .into_par_iter()
        .map(|r| {
            let off = r.start as i32;
            let mut b = cb_burn(&wkbs[r], grid, mode);
            b.runs.iter_mut().for_each(|x| x.id += off);
            b.edges.iter_mut().for_each(|x| x.id += off);
            b.lines.iter_mut().for_each(|x| x.id += off);
            b.points.iter_mut().for_each(|x| x.id += off);
            b
        })
        .collect();
    let mut out = cb::BurnResult::default();
    for p in parts {
        out.runs.extend(p.runs);
        out.edges.extend(p.edges);
        out.lines.extend(p.lines);
        out.points.extend(p.points);
        out.notes.extend(p.notes);
    }
    out
}

/// Minor and major page faults of this process so far, from
/// `/proc/self/stat` (Linux; 0 elsewhere). A difference across a phase is
/// its first-touch cost, which depends on the kernel's huge-page policy.
pub fn faults() -> (u64, u64) {
    let Ok(s) = std::fs::read_to_string("/proc/self/stat") else { return (0, 0) };
    // Fields after the command name (which may hold spaces), from `state`:
    // minflt is the 8th, majflt the 10th.
    let f: Vec<&str> = s.rsplit_once(')').map(|x| x.1).unwrap_or("").split_whitespace().collect();
    let n = |i: usize| f.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
    (n(7), n(9))
}

/// Feature-parallel burnscars (ids are global already), concatenated.
pub fn bs_par(wkbs: &[Vec<u8>], grid: &bs::Grid, mode: Mode, ngroups: usize) -> (bs::BurnResult, usize) {
    let parts: Vec<(bs::BurnResult, Vec<(u32, String)>)> =
        groups(wkbs, ngroups).into_par_iter().map(|r| bs_burn(&wkbs[r.clone()], r.start as u32, grid, mode)).collect();
    let mut out = bs::BurnResult::default();
    let mut nfail = 0;
    for (p, f) in parts {
        nfail += f.len();
        out.runs.row.extend(p.runs.row);
        out.runs.col_start.extend(p.runs.col_start);
        out.runs.col_end.extend(p.runs.col_end);
        out.runs.id.extend(p.runs.id);
        out.edges.row.extend(p.edges.row);
        out.edges.col.extend(p.edges.col);
        out.edges.fraction.extend(p.edges.fraction);
        out.edges.id.extend(p.edges.id);
        out.lines.row.extend(p.lines.row);
        out.lines.col.extend(p.lines.col);
        out.lines.length.extend(p.lines.length);
        out.lines.id.extend(p.lines.id);
        out.points.row.extend(p.points.row);
        out.points.col.extend(p.points.col);
        out.points.id.extend(p.points.id);
    }
    (out, nfail)
}

// ------------------------------------------------------------------ tiles

/// One-pass slice of burnscars tables into tiles (the same routing as
/// controlledburn::slice_tiles), returned as AoS tiles in row-major order,
/// empty ones included.
pub fn bs_slice(r: &bs::BurnResult, g: &GridDef, tc: u32, tr: u32) -> Vec<cb::BurnResult> {
    let ntc = g.ncol.div_ceil(tc);
    let ntr = g.nrow.div_ceil(tr);
    let mut out = vec![cb::BurnResult::default(); (ntc * ntr) as usize];
    let at = |row: u32, col: u32| ((row / tr) * ntc + col / tc) as usize;
    let ru = &r.runs;
    for i in 0..ru.row.len() {
        let (row, ce, id) = (ru.row[i], ru.col_end[i], ru.id[i] as i32);
        let mut c = ru.col_start[i];
        while c < ce {
            let c0 = (c / tc) * tc;
            let e = (c0 + tc).min(ce);
            out[at(row, c)].runs.push(cb::GridRun { row: (row % tr) as i32, col_start: (c - c0) as i32, col_end: (e - c0) as i32, id });
            c = e;
        }
    }
    let e = &r.edges;
    for i in 0..e.row.len() {
        out[at(e.row[i], e.col[i])].edges.push(cb::GridEdge {
            row: (e.row[i] % tr) as i32, col: (e.col[i] % tc) as i32, fraction: e.fraction[i], id: e.id[i] as i32,
        });
    }
    let l = &r.lines;
    for i in 0..l.row.len() {
        out[at(l.row[i], l.col[i])].lines.push(cb::GridLine {
            row: (l.row[i] % tr) as i32, col: (l.col[i] % tc) as i32, length: l.length[i], id: l.id[i] as i32,
        });
    }
    let p = &r.points;
    for i in 0..p.row.len() {
        out[at(p.row[i], p.col[i])].points.push(cb::GridPoint { row: (p.row[i] % tr) as i32, col: (p.col[i] % tc) as i32, id: p.id[i] as i32 });
    }
    out
}

/// For each tile of the tile grid, the geometries whose bbox meets it.
pub fn bucket(wkbs: &[Vec<u8>], g: &GridDef, tc: u32, tr: u32) -> Vec<Vec<u32>> {
    let ntc = g.ncol.div_ceil(tc);
    let ntr = g.nrow.div_ceil(tr);
    let mut out = vec![Vec::new(); (ntc * ntr) as usize];
    let (dx, dy) = (g.dx(), g.dy());
    for (i, w) in wkbs.iter().enumerate() {
        let Some([x0, y0, x1, y1]) = wkb_bbox(w) else { continue };
        // one cell of slack each way: tile membership by bbox is a prefilter only
        let c0 = (((x0 - g.xmin) / dx).floor() as i64 - 1).clamp(0, g.ncol as i64 - 1) as u32;
        let c1 = (((x1 - g.xmin) / dx).floor() as i64 + 1).clamp(0, g.ncol as i64 - 1) as u32;
        let r0 = (((g.ymax - y1) / dy).floor() as i64 - 1).clamp(0, g.nrow as i64 - 1) as u32;
        let r1 = (((g.ymax - y0) / dy).floor() as i64 + 1).clamp(0, g.nrow as i64 - 1) as u32;
        if x1 < g.xmin || x0 > g.xmax || y1 < g.ymin || y0 > g.ymax {
            continue;
        }
        for ti in r0 / tr..=r1 / tr {
            for tj in c0 / tc..=c1 / tc {
                out[(ti * ntc + tj) as usize].push(i as u32);
            }
        }
    }
    out
}

/// burnscars per-tile re-burn: every tile burns the geometries that meet
/// it onto `grid.window(tile)`, in parallel over tiles (the current pool).
/// Returns (records, geometry-tile pairs, failures).
pub fn bs_reburn(wkbs: &[Vec<u8>], g: &GridDef, mode: Mode, tc: u32, tr: u32, buckets: &[Vec<u32>]) -> (usize, usize, usize) {
    let grid = g.bs();
    let ntc = g.ncol.div_ceil(tc);
    let res: Vec<(usize, usize, usize)> = buckets
        .par_iter()
        .enumerate()
        .filter(|(_, b)| !b.is_empty())
        .map_init(
            || bs::Burner::new(mode.bs()),
            |burner, (k, b)| {
                let (ti, tj) = (k as u32 / ntc, k as u32 % ntc);
                let rows = bs::Span::new(bs::Row(ti * tr), bs::Row(((ti + 1) * tr).min(g.nrow))).unwrap();
                let cols = bs::Span::new(bs::Col(tj * tc), bs::Col(((tj + 1) * tc).min(g.ncol))).unwrap();
                let win = grid.window(rows, cols).unwrap();
                let mut out = bs::BurnResult::default();
                let mut fail = 0;
                for &i in b {
                    if burner.burn_into(&win, [(bs::GeomId(i), parse(&wkbs[i as usize]))], &mut out).is_err() {
                        fail += 1;
                    }
                }
                (bs_records(&out), b.len(), fail)
            },
        )
        .collect();
    res.iter().fold((0, 0, 0), |a, x| (a.0 + x.0, a.1 + x.1, a.2 + x.2))
}

// ------------------------------------------------------------------ comparison

/// One canonical segment: table (0 polygon, 1 line, 2 point), id, row,
/// half-open columns and a value per cell. Polygon runs have value 1.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
struct Seg {
    t: u8,
    id: i32,
    row: i32,
    c0: i32,
    c1: i32,
    v: f64,
}

/// Records into sorted, non-overlapping segments, summing the records that
/// land on the same (table, id, cell): controlledburn-rs keeps one record
/// per polygon part, burnscars one per geometry.
fn canon(r: &cb::BurnResult) -> Vec<Seg> {
    let mut s: Vec<Seg> = Vec::with_capacity(cb_records(r));
    s.extend(r.runs.iter().map(|x| Seg { t: 0, id: x.id, row: x.row, c0: x.col_start, c1: x.col_end, v: 1.0 }));
    s.extend(r.edges.iter().map(|x| Seg { t: 0, id: x.id, row: x.row, c0: x.col, c1: x.col + 1, v: x.fraction as f64 }));
    s.extend(r.lines.iter().map(|x| Seg { t: 1, id: x.id, row: x.row, c0: x.col, c1: x.col + 1, v: x.length as f64 }));
    s.extend(r.points.iter().map(|x| Seg { t: 2, id: x.id, row: x.row, c0: x.col, c1: x.col + 1, v: 1.0 }));
    s.sort_by(|a, b| (a.t, a.id, a.row, a.c0).cmp(&(b.t, b.id, b.row, b.c0)));
    // split overlaps into a sum: walk cell boundaries within each (t,id,row)
    let mut out: Vec<Seg> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let key = (s[i].t, s[i].id, s[i].row);
        let mut j = i;
        while j < s.len() && (s[j].t, s[j].id, s[j].row) == key {
            j += 1;
        }
        let grp = &s[i..j];
        if grp.windows(2).all(|w| w[0].c1 <= w[1].c0) {
            out.extend_from_slice(grp);
        } else {
            let mut cuts: Vec<i32> = grp.iter().flat_map(|g| [g.c0, g.c1]).collect();
            cuts.sort_unstable();
            cuts.dedup();
            for w in cuts.windows(2) {
                let v: f64 = grp.iter().filter(|g| g.c0 <= w[0] && g.c1 >= w[1]).map(|g| g.v).sum();
                if grp.iter().any(|g| g.c0 <= w[0] && g.c1 >= w[1]) {
                    out.push(Seg { t: key.0, id: key.1, row: key.2, c0: w[0], c1: w[1], v });
                }
            }
        }
        i = j;
    }
    out
}

/// Cell-level agreement between two results.
#[derive(Debug, Default, Clone)]
pub struct Diff {
    pub cells_a: u64,
    pub cells_b: u64,
    pub equal: u64,
    pub within_1e6: u64,
    pub differ: u64,
    pub only_a: u64,
    pub only_b: u64,
    pub max_abs: f64,
    pub examples: Vec<String>,
}

impl Diff {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"cells_a": self.cells_a, "cells_b": self.cells_b, "equal": self.equal,
            "within_1e-6": self.within_1e6, "differ": self.differ, "only_a": self.only_a,
            "only_b": self.only_b, "max_abs": self.max_abs, "examples": self.examples})
    }
}

/// Compare two results cell by cell, per table and id. `table` restricts
/// the comparison to one table (0 polygon, 1 line, 2 point).
pub fn compare(a: &cb::BurnResult, b: &cb::BurnResult, table: Option<u8>) -> Diff {
    let keep = |s: &Seg| table.is_none_or(|t| s.t == t);
    let sa: Vec<Seg> = canon(a).into_iter().filter(keep).collect();
    let sb: Vec<Seg> = canon(b).into_iter().filter(keep).collect();
    let mut d = Diff::default();
    let key = |s: &Seg| (s.t, s.id, s.row);
    let (mut i, mut j) = (0, 0);
    let (mut ca, mut cb_) = (sa.first().map(|s| s.c0), sb.first().map(|s| s.c0));
    let note = |d: &mut Diff, msg: String| {
        if d.examples.len() < 12 {
            d.examples.push(msg)
        }
    };
    while i < sa.len() || j < sb.len() {
        // current cell cursor within each segment
        let ka = sa.get(i).map(|s| (key(s), ca.unwrap()));
        let kb = sb.get(j).map(|s| (key(s), cb_.unwrap()));
        match (ka, kb) {
            (Some(x), Some(y)) if x == y => {
                let (s, t) = (&sa[i], &sb[j]);
                // advance by the overlap length in one step
                let end = s.c1.min(t.c1);
                let n = (end - x.1) as u64;
                d.cells_a += n;
                d.cells_b += n;
                let diff = (s.v - t.v).abs();
                if s.v.to_bits() == t.v.to_bits() {
                    d.equal += n;
                } else if diff <= 1e-6 {
                    d.within_1e6 += n;
                } else {
                    d.differ += n;
                    if diff > d.max_abs {
                        d.max_abs = diff;
                    }
                    note(&mut d, format!("t{} id {} row {} col {}: a {} b {}", s.t, s.id, s.row, x.1, s.v, t.v));
                }
                ca = Some(end);
                cb_ = Some(end);
            }
            (Some(x), y) if y.is_none() || x < y.unwrap() => {
                let s = &sa[i];
                // only in a, up to the next cell of b on the same key or the segment end
                let lim = match y { Some(y) if y.0 == x.0 => y.1.min(s.c1), _ => s.c1 };
                let n = (lim - x.1) as u64;
                d.cells_a += n;
                d.only_a += n;
                note(&mut d, format!("only a: t{} id {} row {} cols {}..{} v {}", s.t, s.id, s.row, x.1, lim, s.v));
                ca = Some(lim);
            }
            (x, Some(y)) => {
                let t = &sb[j];
                let lim = match x { Some(x) if x.0 == y.0 => x.1.min(t.c1), _ => t.c1 };
                let n = (lim - y.1) as u64;
                d.cells_b += n;
                d.only_b += n;
                note(&mut d, format!("only b: t{} id {} row {} cols {}..{} v {}", t.t, t.id, t.row, y.1, lim, t.v));
                cb_ = Some(lim);
            }
            _ => unreachable!(),
        }
        if i < sa.len() && ca == Some(sa[i].c1) {
            i += 1;
            ca = sa.get(i).map(|s| s.c0);
        }
        if j < sb.len() && cb_ == Some(sb[j].c1) {
            j += 1;
            cb_ = sb.get(j).map(|s| s.c0);
        }
    }
    d
}

/// Covered area per id: (run cells + edge fractions) × cell area.
pub fn area_by_id(r: &cb::BurnResult, n: usize, g: &GridDef) -> Vec<f64> {
    let ca = g.dx() * g.dy();
    let mut a = vec![0.0; n];
    for x in &r.runs {
        a[x.id as usize] += (x.col_end - x.col_start) as f64 * ca;
    }
    for x in &r.edges {
        a[x.id as usize] += x.fraction as f64 * ca;
    }
    a
}

/// The exact answer for axis-aligned rectangles: per cell, the product of
/// the two 1-D overlaps over the cell area. The cell edges are the grid's
/// lattice `xmin + k*dx`, `ymax - k*dy` (the outermost edges are the
/// extent itself).
pub fn rect_oracle(rects: &[[f64; 4]], g: &GridDef) -> cb::BurnResult {
    let (dx, dy) = (g.dx(), g.dy());
    let xe = |k: u32| if k == g.ncol { g.xmax } else { g.xmin + k as f64 * dx };
    let ye = |k: u32| if k == g.nrow { g.ymin } else { g.ymax - k as f64 * dy };
    let mut o = cb::BurnResult::default();
    for (id, &[x0, y0, x1, y1]) in rects.iter().enumerate() {
        let c0 = ((x0 - g.xmin) / dx).floor().max(0.0) as u32;
        let c1 = (((x1 - g.xmin) / dx).ceil() as u32).min(g.ncol);
        let r0 = ((g.ymax - y1) / dy).floor().max(0.0) as u32;
        let r1 = (((g.ymax - y0) / dy).ceil() as u32).min(g.nrow);
        for r in r0..r1 {
            let oy = (y1.min(ye(r)) - y0.max(ye(r + 1))).max(0.0) / (ye(r) - ye(r + 1));
            for c in c0..c1 {
                let ox = (x1.min(xe(c + 1)) - x0.max(xe(c))).max(0.0) / (xe(c + 1) - xe(c));
                let f = ox * oy;
                if f > 0.0 {
                    o.edges.push(cb::GridEdge { row: r as i32, col: c as i32, fraction: f as f32, id: id as i32 });
                }
            }
        }
    }
    o
}

/// Edges with fraction 1 become one-cell runs, so an oracle that writes
/// every cell as an edge compares like for like (values are equal either way).
pub fn edges_to_runs(r: &cb::BurnResult) -> cb::BurnResult {
    let mut o = cb::BurnResult { runs: r.runs.clone(), lines: r.lines.clone(), points: r.points.clone(), ..Default::default() };
    for e in &r.edges {
        if e.fraction >= 1.0 {
            o.runs.push(cb::GridRun { row: e.row, col_start: e.col, col_end: e.col + 1, id: e.id });
        } else {
            o.edges.push(*e);
        }
    }
    o
}
