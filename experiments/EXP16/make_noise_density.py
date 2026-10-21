"""EXP16 data generator.

Samples the GRIP4 global road-density raster at every baked GeoNames city and
writes the committed asset the app bakes in:

    core/assets/noise_density.json   - { "<geonames id>": <road m per km2>, ... }

Input:  experiments/EXP16/tmp/total/grip4_total_dens_m_km2.asc
        (GRIP4 total road density, ESRI ASCII grid, 5 arcmin, WGS84)
        core/assets/cities.json (EXP08 GeoNames set; provides lat/lon/id)
Output: core/assets/noise_density.json

GRIP4 is ODbL (Meijer et al 2018, doi:10.1088/1748-9326/aabd42). Re-running is
deterministic for a given raster.

Usage:  py -3.12 experiments/EXP16/make_noise_density.py
"""
import json
import os
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASC = os.path.join(HERE, "tmp", "total", "grip4_total_dens_m_km2.asc")
CITIES = os.path.join(ROOT, "core", "assets", "cities.json")
OUT = os.path.join(ROOT, "core", "assets", "noise_density.json")


def load_ascii_grid(path):
    with open(path) as f:
        header = {}
        for _ in range(6):
            parts = f.readline().split()
            header[parts[0]] = float(parts[1])
        arr = np.fromstring(f.read(), dtype=np.float32, sep=" ")
    nrows, ncols = int(header["nrows"]), int(header["ncols"])
    assert arr.size == nrows * ncols, (arr.size, nrows * ncols)
    return header, arr.reshape(nrows, ncols)


def density_around(header, arr, lat, lon):
    """Road density (m/km2) for the city.

    Primary: the 8 km cell the city falls in. Fallback: the mean of the
    surrounding 3x3 cells (NODATA excluded) when the city's own cell is water or
    otherwise NODATA. A single cell is used so a dense city on a desert or
    coastal edge (e.g. Cairo) is not diluted by its low-density surroundings.
    """
    cs = header["cellsize"]
    nodata = header["NODATA_value"]
    r = min(max(int((90.0 - lat) / cs), 0), arr.shape[0] - 1)
    c = min(max(int((lon + 180.0) / cs), 0), arr.shape[1] - 1)
    if arr[r, c] != nodata:
        return float(arr[r, c])
    win = arr[max(r - 1, 0):r + 2, max(c - 1, 0):c + 2]
    valid = win[win != nodata]
    return float(valid.mean()) if valid.size else None


def main():
    header, arr = load_ascii_grid(ASC)
    cities = json.load(open(CITIES, encoding="utf-8"))
    out = {}
    missing = 0
    for city in cities:
        d = density_around(header, arr, city["lat"], city["lon"])
        if d is None:
            missing += 1
            continue
        out[str(city["id"])] = int(round(d))

    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"), sort_keys=True)

    vals = np.array(list(out.values()), dtype=np.float64)
    print(f"cities={len(cities)} written={len(out)} missing={missing}")
    print(f"density m/km2: min={vals.min():.0f} p25={np.percentile(vals,25):.0f} "
          f"median={np.median(vals):.0f} p75={np.percentile(vals,75):.0f} "
          f"max={vals.max():.0f}")
    for cid, label in [(360630, "Cairo"), (1850147, "Tokyo"), (2643743, "London"),
                       (2988507, "Paris"), (361058, "Alexandria"), (359792, "Aswan")]:
        print(f"  {label:12s} {out.get(str(cid))}")
    print(f"wrote {OUT} ({os.path.getsize(OUT)/1024:.0f} KB)")


if __name__ == "__main__":
    main()
