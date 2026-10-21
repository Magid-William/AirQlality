//! Inferred outdoor noise: a city-level estimate with no measurement.
//!
//! There is **no free, live, measured noise feed** for Cairo (EXP16 research),
//! so this module *infers* an hourly LAeq (dB(A)) from open geodata:
//!
//! ```text
//! L(h) = base_level(road_density, population) + profile[h]
//! ```
//!
//! - `road_density` is the GRIP4 global road density (m of road per km^2) at the
//!   city, baked into `assets/noise_density.json` (see `make_noise_density.py`).
//! - `population` is GeoNames population, already baked in `assets/cities.json`.
//! - `profile[h]` is a documented, generic urban *road*-traffic diurnal shape
//!   (peak 0 dB at the rush hours, floor at ~04:00). It is what changes hour to
//!   hour; the base level only shifts the curve up or down.
//!
//! This is an **estimate, not a measurement** - the app labels it as such. It is
//! calibrated so a Cairo-sized city lands in the band reported by the published
//! Greater-Cairo noise literature (roadside LAeq ~65-85 dB(A) by day, dropping
//! to ~55-65 dB(A) at night).

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::Timelike;

use crate::geo::City;
use crate::model::Series;

/// Diurnal road-traffic level in dB relative to the daily peak (0 dB).
///
/// A generic urban pattern: a night floor at 04:00, a morning peak at 08:00 and
/// a broader evening peak at 17:00-18:00. It reproduces the day/night swing the
/// Greater-Cairo studies report (a ~10-15 dB drop overnight).
pub const PROFILE_DB: [f32; 24] = [
    -4.0, -6.0, -9.0, -11.0, -12.0, -10.0, -6.0, -3.0, // 00..07
    0.0, -1.0, -2.0, -2.0, -2.0, -2.0, -2.0, -2.0, // 08..15
    -1.0, 0.0, 0.0, -1.0, -2.0, -3.0, -4.0, -4.0, // 16..23
];

/// Fixed vertical range for the noise curve in the UI (dB(A)).
pub const MIN_DB: f32 = 45.0;
pub const MAX_DB: f32 = 90.0;

/// `L(h) = A + B*log10(density+1) + C*log10(population+1)` (dB(A), peak).
///
/// Constants calibrated so Greater Cairo (density ~12.7k m/km^2, pop ~9.5M)
/// peaks near 75 dB(A), consistent with the ~63-80 dB(A) roadside LAeq reported
/// for Cairo. Clamped to a physically plausible band.
const A: f32 = 28.0;
const B: f32 = 8.0;
const C: f32 = 2.0;
const BASE_MIN: f32 = 45.0;
const BASE_MAX: f32 = 88.0;

static DENSITY_JSON: &str = include_str!("../assets/noise_density.json");
static DENSITY: OnceLock<HashMap<u32, f32>> = OnceLock::new();

/// The baked road density (m/km^2) for a city id, if known.
pub fn density_for(id: u32) -> Option<f32> {
    DENSITY
        .get_or_init(|| {
            let raw: HashMap<String, f32> =
                serde_json::from_str(DENSITY_JSON).expect("baked noise_density.json is valid");
            raw.into_iter()
                .filter_map(|(k, v)| k.parse::<u32>().ok().map(|k| (k, v)))
                .collect()
        })
        .get(&id)
        .copied()
}

/// The city-level peak LAeq (dB(A)) from road density and population.
pub fn base_level(density_m_per_km2: f32, population: u64) -> f32 {
    let d = (density_m_per_km2.max(0.0) + 1.0).log10();
    let p = (population as f32 + 1.0).log10();
    (A + B * d + C * p).clamp(BASE_MIN, BASE_MAX)
}

/// The inferred hourly LAeq (dB(A)), indexed by local hour 0..24.
pub fn estimate_for_city(city: &City) -> Option<[f32; 24]> {
    let density = density_for(city.id)?;
    let base = base_level(density, city.pop);
    let mut out = [0.0f32; 24];
    for (h, slot) in out.iter_mut().enumerate() {
        *slot = (base + PROFILE_DB[h]).clamp(MIN_DB, MAX_DB);
    }
    Some(out)
}

/// The noise estimate aligned to a series: each point takes the value for its
/// local hour-of-day. The estimate is the same on every calendar day, so the
/// resulting curve repeats daily while the timestamps are absolute.
pub fn align_to_series(series: &Series, estimate: &[f32; 24]) -> Vec<f32> {
    series
        .points
        .iter()
        .map(|p| estimate[p.time.hour() as usize])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::city_by_id;
    use crate::location::CAIRO_ID;

    #[test]
    fn profile_has_a_rush_peak_and_a_night_floor() {
        assert_eq!(PROFILE_DB.len(), 24);
        let max = PROFILE_DB.iter().copied().fold(f32::MIN, f32::max);
        let min = PROFILE_DB.iter().copied().fold(f32::MAX, f32::min);
        assert_eq!(max, 0.0, "peak is the reference");
        assert!(min <= -10.0, "a clear night floor, got {min}");
        // The floor is in the small hours.
        let floor_hour = PROFILE_DB.iter().position(|&v| v == min).unwrap();
        assert!((2..=6).contains(&floor_hour), "floor at hour {floor_hour}");
    }

    #[test]
    fn base_level_rises_with_density_and_population() {
        assert!(base_level(50_000.0, 1_000_000) > base_level(5_000.0, 1_000_000));
        assert!(base_level(5_000.0, 10_000_000) > base_level(5_000.0, 100_000));
        // Clamped into the plausible band.
        assert_eq!(base_level(0.0, 0), BASE_MIN);
        assert_eq!(base_level(1e9, 1_000_000_000), BASE_MAX);
    }

    #[test]
    fn cairo_estimate_lands_in_the_literature_band() {
        let cairo = city_by_id(CAIRO_ID).expect("Cairo baked");
        let est = estimate_for_city(cairo).expect("Cairo has GRIP density");
        let peak = est.iter().copied().fold(f32::MIN, f32::max);
        let floor = est.iter().copied().fold(f32::MAX, f32::min);
        assert!((70.0..=82.0).contains(&peak), "Cairo peak {peak}");
        assert!((52.0..=68.0).contains(&floor), "Cairo night floor {floor}");
        assert!(peak - floor >= 10.0, "a real day/night swing");
    }

    #[test]
    fn unknown_city_has_no_estimate() {
        // A plausible-but-absent GeoNames id.
        assert!(density_for(999_999_999).is_none());
    }

    #[test]
    fn align_maps_each_point_to_its_local_hour_across_midnight() {
        use crate::model::AirQualityPoint;
        use chrono::{FixedOffset, TimeZone};
        let tz = FixedOffset::east_opt(3 * 3600).unwrap();
        let start = tz.with_ymd_and_hms(2026, 10, 4, 22, 0, 0).single().unwrap();
        let points = (0..4)
            .map(|i| AirQualityPoint {
                time: start + chrono::Duration::hours(i),
                pm2_5: 10.0,
                pm10: 0.0,
                us_aqi: 0.0,
                wind_speed: 0.0,
            })
            .collect();
        let series = Series::new("test", points, 0);
        let mut est = [0.0f32; 24];
        for (h, slot) in est.iter_mut().enumerate() {
            *slot = h as f32;
        }
        assert_eq!(align_to_series(&series, &est), vec![22.0, 23.0, 0.0, 1.0]);
    }
}
