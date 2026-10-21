//! The testable heart of the app: the air model, the pluggable data-source
//! boundary, and the transparent open-the-window rule.
//!
//! `model`, `decision` and `fuse` are pure. The only I/O in the crate is in the
//! source clients (`openmeteo`, `openaq`, `openweathermap`), all of which parse
//! in a pure function that is unit-tested against a committed fixture with no
//! network.

pub mod dayshape;
pub mod decision;
pub mod fuse;
pub mod geo;
mod http;
pub mod location;
pub mod model;
pub mod noise;
pub mod openaq;
pub mod openmeteo;
pub mod openweathermap;
pub mod source;

pub use dayshape::{
    DAY_END_HOUR, DAY_START_HOUR, DayShape, HORIZON_HOURS, band_census, is_daytime, night_amount,
    night_curve,
};
pub use decision::{Decision, Thresholds, Verdict, classify, evaluate, evaluate_hourly};
pub use fuse::{Fusion, Kind, SourceReading, SourceSeries, Spread, Stack, stack};
pub use geo::{City, SEARCH_LIMIT, city_by_id, cities, country_name, search};
pub use location::{CAIRO_ID, Location};
pub use model::{AirQualityPoint, Series, now_index_for};
pub use noise::{MIN_DB, MAX_DB, PROFILE_DB, align_to_series, base_level, density_for, estimate_for_city};
pub use openaq::{OpenAqSource, Station, nearest_pm25_sensor, parse_openaq_hours};
pub use openmeteo::{OpenMeteoSource, parse_openmeteo};
pub use openweathermap::{OpenWeatherMapSource, parse_openweathermap};
pub use source::{AirQualitySource, MockSource, SourceError, mock_stack};
