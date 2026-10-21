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
    let mut keys = load_file_keys();
    if let Some(value) = env_key("AIRQLALITY_OPENAQ_KEY") {
        keys.openaq = Some(value);
    }
    if let Some(value) = env_key("AIRQLALITY_OWM_KEY") {
        keys.owm = Some(value);
    }
    keys
}

/// Read only the file's keys, without the environment override. The key prompts
/// (EXP24) show this state, because saving a file key cannot change a key that
/// an environment variable is already overriding.
pub fn load_file_keys() -> Keys {
    keys_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| parse_keys(&text))
        .unwrap_or_default()
}

/// Write the keys to `%APPDATA%\AirQlality\config.json`, preserving every other
/// field in the file and removing the fields of keys that are now `None`.
///
/// Written to a temporary file and renamed into place: a crash mid-write can
/// leave the old config, never a truncated one. Every failure is ignored and
/// never surfaced (the file is a convenience, not a contract), like
/// [`save`](self::save) for the location.
pub fn save_keys(keys: &Keys) {
    let Some(path) = keys_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let text = merge_keys(&existing, keys);
    // Same directory, so the rename stays on one volume and is atomic enough
    // for a bytes-sized config.
    let tmp = path.with_file_name("config.json.tmp");
    if std::fs::write(&tmp, text).is_err() {
        return;
    }
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Pure: fold our two fields into whatever JSON object is already in the file.
/// Unknown fields survive, a missing/blank key removes its field, and anything
/// that is not a JSON object is replaced rather than merged.
fn merge_keys(existing_json: &str, keys: &Keys) -> String {
    let mut object = match serde_json::from_str::<serde_json::Value>(existing_json) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    for (name, value) in [("openaq_key", &keys.openaq), ("owm_key", &keys.owm)] {
        match value.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(key) => {
                object.insert(name.to_string(), serde_json::Value::String(key.to_string()));
            }
            None => {
                object.remove(name);
            }
        }
    }
    serde_json::Value::Object(object).to_string()
}

/// Where the notification choices live (EXP28). Deliberately a separate file
/// from the keys: `config.json` is rewritten wholesale by the key prompts, and
/// the tray menu writes this one from its own thread.
fn notify_path() -> Option<PathBuf> {
    Some(config_dir()?.join("notify.txt"))
}

/// The verdicts that may notify, from `%APPDATA%\AirQlality\notify.txt`.
///
/// `AIRQLALITY_NOTIFY_ON` wins over the file so a test (or a quick try) can pick
/// the set without touching the saved choice. A missing or unreadable file is
/// the pre-EXP28 default: closing only.
pub fn load_notify() -> crate::notify::Targets {
    if let Ok(value) = std::env::var("AIRQLALITY_NOTIFY_ON") {
        return crate::notify::Targets::parse(&value);
    }
    notify_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| crate::notify::Targets::parse(&text))
        .unwrap_or_default()
}

/// Best-effort save of the notification choices (EXP28). Errors are ignored,
/// like every other file in this directory.
pub fn save_notify(targets: &crate::notify::Targets) {
    let Some(path) = notify_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, targets.to_text());
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

    fn both() -> Keys {
        Keys {
            openaq: Some("0123456789abcdef".into()),
            owm: Some("fedcba9876543210".into()),
        }
    }

    #[test]
    fn saving_merges_into_the_existing_file_and_keeps_unknown_fields() {
        let merged = merge_keys(r#"{"owm_key": "old", "future": 7}"#, &both());
        let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(value["openaq_key"], "0123456789abcdef");
        assert_eq!(value["owm_key"], "fedcba9876543210");
        assert_eq!(value["future"], 7, "fields this app does not know survive");
    }

    #[test]
    fn a_missing_key_removes_its_field_rather_than_writing_a_blank() {
        let keys = Keys {
            openaq: None,
            owm: Some("fedcba9876543210".into()),
        };
        let merged = merge_keys(r#"{"openaq_key": "old", "owm_key": "old"}"#, &keys);
        let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert!(value.get("openaq_key").is_none());
        assert_eq!(value["owm_key"], "fedcba9876543210");
    }

    #[test]
    fn a_blank_or_broken_file_is_replaced_cleanly() {
        for existing in ["", "{ not json", "[]", r#"{"openaq_key": 5}"#] {
            let merged = merge_keys(existing, &both());
            assert_eq!(parse_keys(&merged), both(), "merging {existing:?}");
        }
    }

    #[test]
    fn what_the_prompt_saves_is_what_the_sources_read() {
        let saved = merge_keys("", &both());
        let file = parse_keys(&saved);
        assert_eq!(file.openaq.as_deref(), Some("0123456789abcdef"));
        assert_eq!(file.owm.as_deref(), Some("fedcba9876543210"));
    }
}
