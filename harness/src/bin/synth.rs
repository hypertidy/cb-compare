//! Writes the synthetic inputs: `synth <outdir>`.
//!   rects        2000 axis-aligned rectangles, a quarter with corners on
//!                lattice lines, on a non-dyadic grid; the exact answer is
//!                known cell by cell (`rect_oracle`)
//!   coast-1e5    one jittered "coastline" ring of 10^5 vertices, 4096^2
//!   coast-1e6    the same with 10^6 vertices, 16384^2
//!   small-1e5    10^5 small star polygons (8-48 vertices, radius
//!                1.5-25 cells), 8192^2
use cbcmp::*;

struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
}

fn closed(mut v: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    v.push(v[0]);
    v
}

fn star(rng: &mut Rng, cx: f64, cy: f64, n: usize, r: f64) -> Vec<(f64, f64)> {
    closed(
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                let rr = r * rng.range(0.55, 1.0);
                (cx + rr * a.cos(), cy + rr * a.sin())
            })
            .collect(),
    )
}

fn coast(n: usize) -> Vec<(f64, f64)> {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    closed(
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                let r = 400.0 + 50.0 * (13.0 * a).sin() + rng.range(-8.0, 8.0);
                (500.0 + r * a.cos(), 500.0 + r * a.sin())
            })
            .collect(),
    )
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: synth <outdir>");
    std::fs::create_dir_all(&dir).unwrap();
    let put = |name: &str, g: GridDef, w: Vec<Vec<u8>>, note: &str| {
        write_wkbs(&format!("{dir}/{name}.wkbs"), &w);
        g.write(&format!("{dir}/{name}.grid.json"), note);
        eprintln!("{name}: {} geometries, {} x {}", w.len(), g.ncol, g.nrow);
    };

    // rects: 777.7 x 555.5 units in 1000 x 700 cells (dx, dy not dyadic)
    let g = GridDef { xmin: 0.0, ymin: 0.0, xmax: 777.7, ymax: 555.5, ncol: 1000, nrow: 700 };
    let mut rng = Rng(42);
    let (dx, dy) = (g.dx(), g.dy());
    let mut rects = Vec::new();
    for k in 0..2000 {
        let w = rng.range(0.2, 60.0);
        let h = rng.range(0.2, 60.0);
        let mut x0 = rng.range(0.0, g.xmax - w);
        let mut y0 = rng.range(0.0, g.ymax - h);
        let (mut x1, mut y1) = (x0 + w, y0 + h);
        if k % 4 == 0 {
            // snap the corners to lattice lines
            x0 = g.xmin + (x0 / dx).round() * dx;
            x1 = g.xmin + (x1 / dx).round() * dx;
            y0 = g.ymax - ((g.ymax - y0) / dy).round() * dy;
            y1 = g.ymax - ((g.ymax - y1) / dy).round() * dy;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
        }
        rects.push(wkb_polygon(&[vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]]));
    }
    put("rects", g, rects, "axis-aligned rectangles; exact answer from rect_oracle");

    let g = GridDef { xmin: 0.0, ymin: 0.0, xmax: 1000.0, ymax: 1000.0, ncol: 4096, nrow: 4096 };
    put("coast-1e5", g, vec![wkb_polygon(&[coast(100_000)])], "one 10^5-vertex ring");
    let g = GridDef { ncol: 16384, nrow: 16384, ..g };
    put("coast-1e6", g, vec![wkb_polygon(&[coast(1_000_000)])], "one 10^6-vertex ring");

    let g = GridDef { xmin: 0.0, ymin: 0.0, xmax: 8192.0, ymax: 8192.0, ncol: 8192, nrow: 8192 };
    let mut rng = Rng(7);
    let small: Vec<Vec<u8>> = (0..100_000)
        .map(|_| {
            let r = rng.range(1.5, 25.0);
            let (cx, cy) = (rng.range(r, 8192.0 - r), rng.range(r, 8192.0 - r));
            let n = 8 + (rng.f() * 41.0) as usize;
            wkb_polygon(&[star(&mut rng, cx, cy, n, r)])
        })
        .collect();
    put("small-1e5", g, small, "10^5 star polygons, 8-48 vertices, radius 1.5-25 cells");
}
