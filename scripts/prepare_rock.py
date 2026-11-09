#!/usr/bin/env python3
"""Fetch and pin the real Antarctic inputs, then write them in the harness format.

- Rock outcrop polygons: SCAR ADD medium-resolution rock outcrop v7.3
  (Burton-Johnson et al.), the copy in AustralianAntarcticDivision/rema.proc,
  as in https://www.hypertidy.org/posts/2026-08-19_antarctica-icefree-rasterizing/
  The zip is downloaded once and checked against data/SHA256SUMS.
- Grid: the REMA v2 32 m DEM mosaic (metadata only, from the VRT in
  mdsumner/rema-ovr), EPSG:3031. Coarser grids share its top-left corner and
  are multiples of 32 m, so they nest in the 32 m lattice.

usage: prepare_rock.py <data-dir> <inputs-dir>
"""
import hashlib, json, os, struct, sys, urllib.request
from osgeo import gdal, ogr
gdal.UseExceptions()

ROCK_URL = ("https://github.com/AustralianAntarcticDivision/rema.proc/raw/refs/heads/master/"
            "01_rock_classification/Medium_resolution_vector_polygons_of_Antarctic_rock_outcrop_-_VERSION_7.3.zip")
ROCK_ZIP = "rock_outcrop_medium_v7.3.zip"
ROCK_GPKG = ("Medium resolution vector polygons of Antarctic rock outcrop - VERSION 7.3/"
             "add_rock_outcrop_medium_res_polygon_v7.3.gpkg")
REMA_VRT = "/vsicurl/https://raw.githubusercontent.com/mdsumner/rema-ovr/main/rema-vrt/32m_dem_tiles.vrt"
COARSER = [64, 128, 256, 512, 1024]

data, inputs = sys.argv[1], sys.argv[2]
os.makedirs(inputs, exist_ok=True)

def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()

zp = os.path.join(data, ROCK_ZIP)
if not os.path.exists(zp):
    print("downloading", ROCK_URL, file=sys.stderr)
    urllib.request.urlretrieve(ROCK_URL, zp)
sums = dict(l.split()[::-1] for l in open(os.path.join(data, "SHA256SUMS")) if l.strip())
got = sha256(zp)
if ROCK_ZIP in sums:
    assert sums[ROCK_ZIP] == got, f"{ROCK_ZIP}: sha256 {got} != pinned {sums[ROCK_ZIP]}"
else:
    print(f"NOTE: no pin for {ROCK_ZIP}; add this line to data/SHA256SUMS:\n{got}  {ROCK_ZIP}", file=sys.stderr)

# polygons -> .wkbs (u32 count, then u32 length + ISO WKB, 2D)
ds = ogr.Open(f"/vsizip/{zp}/{ROCK_GPKG}")
lyr = ds.GetLayer(0)
blobs = []
for f in lyr:
    g = f.GetGeometryRef()
    g.FlattenTo2D()
    blobs.append(bytes(g.ExportToIsoWkb()))
with open(os.path.join(inputs, "rock.wkbs"), "wb") as o:
    o.write(struct.pack("<I", len(blobs)))
    for b in blobs:
        o.write(struct.pack("<I", len(b)))
        o.write(b)
nv = sum((len(b) - 9) // 16 for b in blobs)
print(f"rock: {len(blobs)} geometries, ~{nv} vertices", file=sys.stderr)

# grids: the 32 m REMA grid is cached in data/ so later runs need no network
gp = os.path.join(data, "rema32.grid.json")
if not os.path.exists(gp):
    r = gdal.Open(REMA_VRT)
    gt, nc, nr = r.GetGeoTransform(), r.RasterXSize, r.RasterYSize
    assert gt[2] == 0 and gt[4] == 0 and gt[1] == 32 and gt[5] == -32, gt
    json.dump({"xmin": gt[0], "ymax": gt[3], "xmax": gt[0] + nc * gt[1], "ymin": gt[3] + nr * gt[5],
               "ncol": nc, "nrow": nr, "crs": "EPSG:3031", "source": REMA_VRT}, open(gp, "w"), indent=1)
g32 = json.load(open(gp))
def put(name, res):
    k = res // 32
    nc, nr = -(-g32["ncol"] // k), -(-g32["nrow"] // k)
    g = {"xmin": g32["xmin"], "ymax": g32["ymax"], "xmax": g32["xmin"] + nc * res, "ymin": g32["ymax"] - nr * res,
         "ncol": nc, "nrow": nr, "note": f"REMA v2 {res} m lattice (32 m top-left corner), EPSG:3031"}
    json.dump(g, open(os.path.join(inputs, f"{name}.grid.json"), "w"), indent=1)
    lnk = os.path.join(inputs, f"{name}.wkbs")
    if not os.path.lexists(lnk):
        os.symlink("rock.wkbs", lnk)
for res in [32] + COARSER:
    put(f"rock-{res}m", res)
