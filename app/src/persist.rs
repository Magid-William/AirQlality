//! Tiny, std-only persistence for the selected location and the optional source
//! API keys.
//!
//! The chosen location's stable id (a GeoNames id) is stored in
//! `%APPDATA%\AirQlality\location.txt`; the keys for the stacked sources
//! (EXP19) in `%APPDATA%\AirQlality\config.json`. Every failure mode (no
//! `APPDATA`, missing/unreadable file, unknown id, bad JSON) is non-fatal: the
//! location falls back to the default (Cairo) and a missing key simply means
//! that source is not part of the stack. Writes are best-effort and never
//! surfaced.

use std::path::PathBuf;

use airqlality_core::Location;

/// Resolve a stored id to a location, or the default. Pure, so the fallback
/// rules are unit-testable without touching the filesystem.
fn resolve(stored: Option<String>) -> Location {
    stored
        .as_deref()
        .and_then(Location::from_id)
        .unwrap_or_else(Location::cairo)
}

fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("AirQlality"))
}

fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("location.txt"))
}

fn keys_path() -> Option<PathBuf> {
    Some(config_dir()?.join("config.json"))
}

/// The persisted location, or the default when nothing valid is stored.
pub fn load() -> Location {
    let stored = config_path().and_then(|p| std::fs::read_to_string(p).ok());
    resolve(stored)
}

/// Best-effort save of the selected location. Errors are ignored.
pub fn save(location: &Location) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, &location.id);
}

/// API keys for the optional stacked sources (EXP19). A missing key is not an
/// error: that source is simply not fetched, and the app runs as it did before.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    /// OpenAQ v3 key (ground-station observations).
    pub openaq: Option<String>,
    /// OpenWeatherMap key (second model forecast).
    pub owm: Option<String>,
}

/// Read the keys from `%APPDATA%\AirQlality\config.json`, letting environment
/// variables win so a key can be tried without writing a file.
pub fn load_keys() -> Keys {
    let mut keys = keys_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| parse_keys(&text))
        .unwrap_or_default();
    if let Some(value) = env_key("AIRQLALITY_OPENAQ_KEY") {
        keys.openaq = Some(value);
    }
    if let Some(value) = env_key("AIRQLALITY_OWM_KEY") {
        keys.owm = Some(value);
    }
    keys
}

fn env_key(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Pure: the config is `{"openaq_key": "…", "owm_key": "…"}`. Anything else
/// (bad JSON, wrong types, blanks) yields no key rather than an error.
fn parse_keys(json: &str) -> Keys {
    let value: serde_json::Value =
        serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
    let get = |name: &str| {
        value
            .get(name)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Keys {
        openaq: get("openaq_key"),
        owm: get("owm_key"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airqlality_core::{Location, city_by_id};

    #[test]
    fn missing_or_unknown_ids_fall_back_to_cairo() {
        assert_eq!(resolve(None), Location::cairo());
        assert_eq!(resolve(Some("does-not-exist".into())), Location::cairo());
        // EXP07's old slug ids are no longer valid.
        assert_eq!(resolve(Some("cairo-eg".into())), Location::cairo());
    }

    #[test]
    fn a_known_id_round_trips() {
        // Tokyo's GeoNames id; a trailing newline is tolerated.
        let stored = format!("{}\n", 1850147);
        let loc = resolve(Some(stored));
        assert_eq!(loc.id, "1850147");
        assert_eq!(loc.label, "Tokyo, Japan");
        assert_eq!(city_by_id(1850147).unwrap().tz, loc.tz);
    }

    #[test]
    fn config_keys_are_read_and_blanked_values_ignored() {
        let keys = parse_keys(r#"{"openaq_key": "  abc  ", "owm_key": ""}"#);
        assert_eq!(keys.openaq.as_deref(), Some("abc"));
        assert_eq!(keys.owm, None);
    }

    #[test]
    fn bad_json_yields_no_keys() {
        assert_eq!(parse_keys("{ not json"), Keys::default());
        assert_eq!(parse_keys(""), Keys::default());
        assert_eq!(parse_keys(r#"{"openaq_key": 5}"#), Keys::default());
    }
}
