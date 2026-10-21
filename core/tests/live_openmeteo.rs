//! Live network smoke test for the real Open-Meteo source. Ignored by default so
//! the offline suite stays deterministic and CI needs no network.
//!
//! Run it manually:
//!   cargo test -p airqlality-core --test live_openmeteo -- --ignored --nocapture

use airqlality_core::{AirQualitySource, OpenMeteoSource};

#[test]
#[ignore = "hits the real Open-Meteo API; run with --ignored"]
fn live_openmeteo_fetch_is_well_formed() {
    let source = OpenMeteoSource::cairo().expect("build source");
    let series = source.fetch().expect("live Open-Meteo fetch");
    eprintln!(
        "live: {} points, now_index {}, location {}, url {}",
        series.len(),
        series.now_index,
        series.location,
        source.url()
    );
    assert!(series.len() >= 24, "expected at least a day of hours");
    assert!(series.now_index < series.len());
    assert!(series.points.iter().all(|p| p.pm2_5 >= 0.0));
    // Points are hourly and ascending.
    assert!(
        series
            .points
            .windows(2)
            .all(|w| (w[1].time - w[0].time).num_seconds() == 3600)
    );
}
