//! The runtime location the widget fetches and shows.
//!
//! OWNED, because it now comes from the baked city set (see [`crate::geo`])
//! rather than a fixed `'static` list. The `id` is the GeoNames id and is what
//! gets persisted.

use crate::geo::{City, city_by_id, country_name};

/// Cairo's GeoNames id; also the fallback when nothing valid is stored.
pub const CAIRO_ID: u32 = 360630;

/// A concrete location to fetch and display.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    /// Stable id (GeoNames id as a string). Persisted to disk.
    pub id: String,
    /// Display label, e.g. "Cairo, Egypt".
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// IANA timezone name.
    pub tz: String,
}

impl Location {
    /// A location derived from a baked city: `"<name>, <country>"`.
    pub fn from_city(city: &City) -> Self {
        Self {
            id: city.id.to_string(),
            label: format!("{}, {}", city.name, country_name(&city.country)),
            lat: city.lat,
            lon: city.lon,
            tz: city.tz.clone(),
        }
    }

    /// A location from its persisted id, if it is a known city.
    pub fn from_id(id: &str) -> Option<Self> {
        id.trim().parse::<u32>().ok().and_then(city_by_id).map(Self::from_city)
    }

    /// The default location: Cairo, Egypt (from the baked set, with a hardcoded
    /// fallback should it ever be missing).
    pub fn cairo() -> Self {
        if let Some(city) = city_by_id(CAIRO_ID) {
            return Self::from_city(city);
        }
        Self {
            id: CAIRO_ID.to_string(),
            label: "Cairo, Egypt".to_string(),
            lat: 30.0444,
            lon: 31.2357,
            tz: "Africa/Cairo".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cairo_comes_from_the_baked_set() {
        let cairo = Location::cairo();
        assert_eq!(cairo.id, "360630");
        assert_eq!(cairo.label, "Cairo, Egypt");
        assert_eq!(cairo.tz, "Africa/Cairo");
    }

    #[test]
    fn from_id_round_trips_known_and_rejects_unknown() {
        let cairo = Location::cairo();
        assert_eq!(Location::from_id(&cairo.id), Some(cairo));
        assert_eq!(Location::from_id("360630"), Location::from_id("360630 "));
        // EXP07's old slug ids and anything unknown are rejected.
        assert_eq!(Location::from_id("cairo-eg"), None);
        assert_eq!(Location::from_id(""), None);
    }

    #[test]
    fn from_city_labels_with_the_country_name() {
        let tokyo = city_by_id(1850147).expect("Tokyo in the baked set");
        let loc = Location::from_city(tokyo);
        assert_eq!(loc.label, "Tokyo, Japan");
        assert_eq!(loc.tz, "Asia/Tokyo");
    }
}
