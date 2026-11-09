//! `timing <name> <inputs-dir> <mode> <strategy> <threads> <tile> <repeats>`
//!
//! One configuration per process, so the caller can read peak RSS per
//! configuration (`/usr/bin/time -f %M`). Prints one JSON line per repeat,
//! after one untimed warm-up run. Input loading is outside the timing;
//! WKB parsing is inside it for both engines.
//!
//! Strategies (threads = rayon pool size; tile = tile edge in cells, 0 = no tiles):
//!   cb_burn, bs_burn        serial whole-grid burn (threads ignored)
//!   cb_par, bs_par          feature-parallel whole-grid burn, concatenated
//!   cb_slice, bs_slice      whole-grid burn (feature-parallel when threads > 1),
//!                           then a one-pass slice into tiles; burn and slice
//!                           are timed separately
//!   bs_reburn               fixed tiles: each tile burns the geometries whose
//!                           bbox meets it onto grid.window(tile), in parallel
//!                           over tiles (bucketing timed separately)
//!   cb_crop_loop            whole-grid burn, then crop() once per tile (the
//!                           pattern slice_tiles replaces; for scale only)
use cbcmp::*;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 8 {
        eprintln!("usage: timing <name> <inputs-dir> <mode> <strategy> <threads> <tile> <repeats>");
        std::process::exit(2);
    }
    let (name, dir, mode, strat) = (&a[1], &a[2], Mode::parse(&a[3]), a[4].as_str());
    let threads: usize = a[5].parse().unwrap();
    let tile: u32 = a[6].parse().unwrap();
    let repeats: usize = a[7].parse().unwrap();
    let wkbs = read_wkbs(&format!("{dir}/{name}.wkbs"));
    let g = GridDef::read(&format!("{dir}/{name}.grid.json"));
    let (gc, gb) = (g.cb(), g.bs());
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
    // feature groups: a few per thread, for load balance
    let ngroups = (threads * 4).max(1);
    let ntiles = if tile > 0 { (g.ncol.div_ceil(tile) as u64) * (g.nrow.div_ceil(tile) as u64) } else { 0 };

    let run = || -> serde_json::Value {
        let t0 = Instant::now();
        let secs = |t: Instant| t.elapsed().as_secs_f64();
        pool.install(|| match strat {
            "cb_burn" => {
                let r = cb_burn(&wkbs, &gc, mode);
                serde_json::json!({"burn_s": secs(t0), "records": cb_records(&r)})
            }
            "bs_burn" => {
                let (r, f) = bs_burn(&wkbs, 0, &gb, mode);
                serde_json::json!({"burn_s": secs(t0), "records": bs_records(&r), "failed": f.len()})
            }
            "cb_par" => {
                let r = cb_par(&wkbs, &gc, mode, ngroups);
                serde_json::json!({"burn_s": secs(t0), "records": cb_records(&r)})
            }
            "bs_par" => {
                let (r, f) = bs_par(&wkbs, &gb, mode, ngroups);
                serde_json::json!({"burn_s": secs(t0), "records": bs_records(&r), "failed": f})
            }
            "cb_slice" => {
                let r = if threads > 1 { cb_par(&wkbs, &gc, mode, ngroups) } else { cb_burn(&wkbs, &gc, mode) };
                let burn_s = secs(t0);
                let t1 = Instant::now();
                let tiles = controlledburn::slice_tiles(&r, &gc, tile, tile);
                let n: usize = tiles.iter().map(|t| cb_records(&t.result)).sum();
                serde_json::json!({"burn_s": burn_s, "slice_s": secs(t1), "records": n, "tiles_nonempty": tiles.len()})
            }
            "bs_slice" => {
                let r = if threads > 1 { bs_par(&wkbs, &gb, mode, ngroups).0 } else { bs_burn(&wkbs, 0, &gb, mode).0 };
                let burn_s = secs(t0);
                let t1 = Instant::now();
                let tiles = bs_slice(&r, &g, tile, tile);
                let n: usize = tiles.iter().map(cb_records).sum();
                let ne = tiles.iter().filter(|t| cb_records(t) > 0).count();
                serde_json::json!({"burn_s": burn_s, "slice_s": secs(t1), "records": n, "tiles_nonempty": ne})
            }
            "bs_reburn" => {
                let b = bucket(&wkbs, &g, tile, tile);
                let bucket_s = secs(t0);
                let t1 = Instant::now();
                let (n, pairs, fail) = bs_reburn(&wkbs, &g, mode, tile, tile, &b);
                serde_json::json!({"bucket_s": bucket_s, "burn_s": secs(t1), "records": n, "geom_tile_pairs": pairs, "failed": fail,
                                   "tiles_nonempty": b.iter().filter(|x| !x.is_empty()).count()})
            }
            "cb_crop_loop" => {
                let r = cb_burn(&wkbs, &gc, mode);
                let burn_s = secs(t0);
                let t1 = Instant::now();
                let (mut n, mut ne) = (0, 0);
                let (dx, dy) = (g.dx(), g.dy());
                for r0 in (0..g.nrow).step_by(tile as usize) {
                    for c0 in (0..g.ncol).step_by(tile as usize) {
                        let (r1, c1) = ((r0 + tile).min(g.nrow), (c0 + tile).min(g.ncol));
                        let ext = [g.xmin + c0 as f64 * dx, g.ymax - r1 as f64 * dy, g.xmin + c1 as f64 * dx, g.ymax - r0 as f64 * dy];
                        if let Some((t, _)) = controlledburn::crop(&r, &gc, ext) {
                            let k = cb_records(&t);
                            n += k;
                            ne += usize::from(k > 0);
                        }
                    }
                }
                serde_json::json!({"burn_s": burn_s, "slice_s": secs(t1), "records": n, "tiles_nonempty": ne})
            }
            s => panic!("unknown strategy {s}"),
        })
    };

    let _ = run(); // warm-up, untimed
    for rep in 0..repeats {
        let t = Instant::now();
        let mut v = run();
        v["total_s"] = serde_json::json!(t.elapsed().as_secs_f64());
        v["input"] = serde_json::json!(name);
        v["mode"] = serde_json::json!(a[3]);
        v["strategy"] = serde_json::json!(strat);
        v["threads"] = serde_json::json!(threads);
        v["tile"] = serde_json::json!(tile);
        v["tiles"] = serde_json::json!(ntiles);
        v["rep"] = serde_json::json!(rep);
        v["geometries"] = serde_json::json!(wkbs.len());
        v["grid"] = serde_json::json!([g.ncol, g.nrow]);
        println!("{}", serde_json::to_string(&v).unwrap());
    }
}
