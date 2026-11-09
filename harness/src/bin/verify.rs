//! Correctness before timing: `verify <name> <inputs-dir> <mode>`.
//! Prints one JSON object:
//!   - burnscars failures (it stops on an error per geometry; we roll back
//!     that geometry and carry on)
//!   - cb vs bs, cell by cell per id (records summed per cell), per table
//!   - covered area per id against the planar area, for polygons that lie
//!     inside the grid
//!   - for `rects`, each engine against the exact cell-by-cell answer
//!   - feature-parallel output against serial, for both engines
use cbcmp::*;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (name, dir, mode) = (&a[1], &a[2], Mode::parse(&a[3]));
    let wkbs = read_wkbs(&format!("{dir}/{name}.wkbs"));
    let g = GridDef::read(&format!("{dir}/{name}.grid.json"));
    let t = Instant::now();
    let rc = cb_burn(&wkbs, &g.cb(), mode);
    let (rbs, failed) = bs_burn(&wkbs, 0, &g.bs(), mode);
    let rb = bs_to_cb(&rbs);
    let burn_s = t.elapsed().as_secs_f64();

    let mut out = serde_json::json!({
        "input": name, "mode": a[3], "geometries": wkbs.len(), "grid": [g.ncol, g.nrow],
        "records_cb": cb_records(&rc), "records_bs": cb_records(&rb),
        "bs_failed": failed.len(), "bs_failed_examples": failed.iter().take(5).map(|f| format!("{}: {}", f.0, f.1)).collect::<Vec<_>>(),
        "cb_notes": rc.notes.len(),
        "polygons": compare(&rc, &rb, Some(0)).json(),
        "lines": compare(&rc, &rb, Some(1)).json(),
        "points": compare(&rc, &rb, Some(2)).json(),
    });

    if mode == Mode::Coverage {
        // area per id, polygons inside the grid only, skipping bs failures
        let fail: std::collections::HashSet<u32> = failed.iter().map(|f| f.0).collect();
        let (ac, ab) = (area_by_id(&rc, wkbs.len(), &g), area_by_id(&rb, wkbs.len(), &g));
        let (mut n, mut ec, mut eb) = (0, 0.0f64, 0.0f64);
        for (i, w) in wkbs.iter().enumerate() {
            let Some([x0, y0, x1, y1]) = wkb_bbox(w) else { continue };
            let area = wkb_area(w);
            if area <= 0.0 || x0 < g.xmin || x1 > g.xmax || y0 < g.ymin || y1 > g.ymax || fail.contains(&(i as u32)) {
                continue;
            }
            n += 1;
            ec = ec.max((ac[i] - area).abs() / area);
            eb = eb.max((ab[i] - area).abs() / area);
        }
        out["area_check"] = serde_json::json!({"polygons": n, "max_rel_err_cb": ec, "max_rel_err_bs": eb});
    }

    if name == "rects" && mode == Mode::Coverage {
        let rects: Vec<[f64; 4]> = wkbs.iter().map(|w| wkb_bbox(w).unwrap()).collect();
        let truth = edges_to_runs(&rect_oracle(&rects, &g));
        out["oracle_cb"] = compare(&rc, &truth, Some(0)).json();
        out["oracle_bs"] = compare(&rb, &truth, Some(0)).json();
    }

    // feature-parallel equals serial, record for record after a stable sort
    let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
    let (pc, (pb, _)) = pool.install(|| (cb_par(&wkbs, &g.cb(), mode, 32), bs_par(&wkbs, &g.bs(), mode, 32)));
    out["par_equals_serial_cb"] = serde_json::json!(pc.runs == rc.runs && pc.edges == rc.edges && pc.lines == rc.lines && pc.points == rc.points);
    let pb = bs_to_cb(&pb);
    out["par_equals_serial_bs"] = serde_json::json!(pb.runs == rb.runs && pb.edges == rb.edges && pb.lines == rb.lines && pb.points == rb.points);
    out["seconds"] = serde_json::json!(burn_s);
    println!("{}", serde_json::to_string(&out).unwrap());
}
