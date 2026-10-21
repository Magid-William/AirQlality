"""EXP16 de-risk probe: sample GRIP4 road density at known cities.

GRIP4 density rasters are ESRI ASCII grids (plain text): ncols/nrows/xllcorner/
yllcorner/cellsize/NODATA_value header, then rows north-to-south. Global grid is
4320x2160 at 5 arcmin (0.083333 deg), WGS84. Data: ODbL (Meijer et al 2018).
"""
import json
import numpy as np

ASC = "experiments/EXP16/tmp/total/grip4_total_dens_m_km2.asc"


def load_ascii_grid(path):
    with open(path) as f:
        header = {}
        for _ in range(6):
            line = f.readline().split()
            header[line[0]] = float(line[1])
        data = f.read()
    arr = np.fromstring(data, dtype=np.float32, sep=" ")
    nrows, ncols = int(header["nrows"]), int(header["ncols"])
    assert arr.size == nrows * ncols, (arr.size, nrows * ncols)
    arr = arr.reshape(nrows, ncols)
    return header, arr


def sample(header, arr, lat, lon):
    cs = header["cellsize"]
    r = int((90.0 - lat) / cs)
    c = int((lon + 180.0) / cs)
    r = min(max(r, 0), arr.shape[0] - 1)
    c = min(max(c, 0), arr.shape[1] - 1)
    v = float(arr[r, c])
    return v, r, c


h, a = load_ascii_grid(ASC)
print("grid", a.shape, "nodata", h["NODATA_value"])
land = a[a != h["NODATA_value"]]
print("cells with data:", land.size, "max", float(land.max()), "median", float(np.median(land)))

pts = {
    "Cairo, EG": (30.0444, 31.2357),
    "Alexandria, EG": (31.2001, 29.9187),
    "Aswan, EG": (24.0908, 32.8998),
    "Tokyo, JP": (35.6895, 139.6917),
    "London, UK": (51.5074, -0.1278),
    "Paris, FR": (48.8566, 2.3522),
    "Oslo, NO": (59.9139, 10.7522),
}
for name, (lat, lon) in pts.items():
    v, r, c = sample(h, a, lat, lon)
    print(f"{name:16s} lat={lat:8.4f} lon={lon:9.4f} -> {v:9.1f} m/km2  (r={r} c={c})")

# Compare Cairo against the values of the cities.json baked set near it.
cities = json.load(open("core/assets/cities.json", encoding="utf-8"))
print("cities:", len(cities))
