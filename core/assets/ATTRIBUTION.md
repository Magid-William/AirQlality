# Baked data attribution

`cities.json` and `countries.json` in this directory are derived from the
**GeoNames** geographical database (https://www.geonames.org/), specifically the
`cities15000` dump and `countryInfo.txt`, retrieved via
http://download.geonames.org/export/dump/ and converted by
`experiments/EXP08/make_cities.ps1`.

GeoNames data is licensed under the **Creative Commons Attribution 4.0
License** (CC BY 4.0): https://creativecommons.org/licenses/by/4.0/

The app shows this attribution in the location-search panel ("city data ©
GeoNames (CC BY 4.0)").

Air-quality data (a separate source) is © Open-Meteo, CC BY 4.0, attributed in
the header.

## Stacked sources (EXP19)

The widget can fuse up to three independent air-quality sources and shows the
per-source readings plus their spread. Attribution for each:

- **Open-Meteo Air Quality** — https://open-meteo.com/ — data provided by the
  **Copernicus Atmosphere Monitoring Service (CAMS)**, CC BY 4.0. All users must
  attribute CAMS ENSEMBLE as the data provider as well as Open-Meteo.
- **OpenAQ** — https://openaq.org/ — ground-station measurements aggregated from
  national networks and embassy monitors. OpenAQ data is licensed CC BY 4.0; the
  individual station/provider attribution returned by the API is the
  authoritative one.
- **OpenWeatherMap Air Pollution** — https://openweathermap.org/api/air-pollution
  — a second model forecast, itself derived from CAMS. Used as a redundant
  second model, not as independent evidence.

The header names every source actually in the stack. The app never presents the
fused value without saying how many sources produced it.


`noise_density.json` is derived from the **Global Roads Inventory Project
(GRIP4)** road-density rasters (https://www.globio.info/download-grip-dataset),
sampled at each city by `experiments/EXP16/make_noise_density.py`. GRIP4 is
available under the **Open Data Commons Open Database License (ODbL)**; cite
Meijer et al. (2018), Environ. Res. Lett. 13 064006
(https://doi.org/10.1088/1748-9326/aabd42). The app's noise curve is an estimate
modelled from this data, not a measurement, and is labelled as such.
