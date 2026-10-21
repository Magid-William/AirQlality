//! Baked-in GeoNames city data and a pure search over it.
//!
//! The data is GeoNames `cities15000` (CC BY 4.0), converted to
//! `assets/cities.json` + `assets/countries.json` by
//! `experiments/EXP08/make_cities.ps1`. It is compiled into the binary
//! (`include_str!`), so search is offline and instant — no geocoding request.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// One baked city: enough to point Open-Meteo at it and label it.
#[derive(Debug, Clone, Deserialize)]
pub struct City {
    /// GeoNames id; stable, used as the persisted location id.
    pub id: u32,
    pub name: String,
    /// ASCII transliteration, used for diacritic-insensitive matching.
    pub ascii: String,
    /// ISO-3166-1 alpha2 country code.
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    pub tz: String,
    pub pop: u64,
}

static CITIES_JSON: &str = include_str!("../assets/cities.json");
static COUNTRIES_JSON: &str = include_str!("../assets/countries.json");

static CITIES: OnceLock<Vec<City>> = OnceLock::new();
static COUNTRIES: OnceLock<HashMap<String, String>> = OnceLock::new();

/// All baked cities, parsed once.
pub fn cities() -> &'static [City] {
    CITIES.get_or_init(|| {
        serde_json::from_str(CITIES_JSON).expect("baked assets/cities.json is valid JSON")
    })
}

/// ISO alpha2 -> country name, parsed once.
pub fn countries() -> &'static HashMap<String, String> {
    COUNTRIES.get_or_init(|| {
        serde_json::from_str(COUNTRIES_JSON).expect("baked assets/countries.json is valid JSON")
    })
}

/// The country name for an ISO alpha2 code, or the code itself if unknown.
pub fn country_name(code: &str) -> &str {
    countries().get(code).map(String::as_str).unwrap_or(code)
}

/// The city with `id`, if present.
pub fn city_by_id(id: u32) -> Option<&'static City> {
    cities().iter().find(|c| c.id == id)
}

/// Maximum number of search results the dropdown shows.
pub const SEARCH_LIMIT: usize = 50;

/// Case- and diacritic-insensitive city search: prefix match on the name or its
/// ASCII form, ranked by population (descending), capped at `limit`.
///
/// Queries shorter than two characters return nothing, matching Open-Meteo's
/// geocoding rule.
pub fn search(query: &str, limit: usize) -> Vec<&'static City> {
    let q = fold(query.trim());
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let mut hits: Vec<&City> = cities()
        .iter()
        .filter(|c| {
            fold(&c.ascii).starts_with(&q) || fold(&c.name).starts_with(&q)
        })
        .collect();
    hits.sort_by(|a, b| b.pop.cmp(&a.pop));
    hits.truncate(limit);
    hits
}

/// Lowercase and strip common Latin diacritics, so `sao` matches `São`.
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(fold_char)
        .collect()
}

fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ß' => 's',
        'ł' => 'l',
        'ż' | 'ź' | 'ž' => 'z',
        'ś' | 'š' => 's',
        'ş' => 's',
        'ğ' => 'g',
        'ı' => 'i',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_loads_and_cairo_is_present() {
        assert!(cities().len() > 10_000, "expected the full baked set");
        let cairo = city_by_id(crate::location::CAIRO_ID).expect("Cairo in the baked set");
        assert_eq!(cairo.name, "Cairo");
        assert_eq!(cairo.country, "EG");
        assert_eq!(cairo.tz, "Africa/Cairo");
        assert_eq!(country_name("EG"), "Egypt");
    }

    #[test]
    fn search_ranks_and_is_case_insensitive() {
        let hits = search("cair", SEARCH_LIMIT);
        assert_eq!(hits[0].id, crate::location::CAIRO_ID, "Cairo ranks first");
        let lower = search("CAIRO", SEARCH_LIMIT);
        assert_eq!(lower[0].id, crate::location::CAIRO_ID);
    }

    #[test]
    fn search_folds_diacritics() {
        // "Sao" should match "São Paulo" via its ASCII name / folded name.
        let hits = search("sao paulo", SEARCH_LIMIT);
        assert!(
            hits.iter().any(|c| c.name.contains("Paulo")),
            "expected São Paulo, got {:?}",
            hits.iter().map(|c| &c.name).take(5).collect::<Vec<_>>()
        );
    }

    #[test]
    fn short_queries_and_misses_return_nothing() {
        assert!(search("", SEARCH_LIMIT).is_empty());
        assert!(search("a", SEARCH_LIMIT).is_empty());
        assert!(search("zzzzzzzz", SEARCH_LIMIT).is_empty());
    }

    #[test]
    fn results_are_capped_and_population_sorted() {
        let hits = search("san", 10);
        assert!(hits.len() <= 10);
        assert!(hits.windows(2).all(|w| w[0].pop >= w[1].pop));
    }
}
