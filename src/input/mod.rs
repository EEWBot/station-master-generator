//! Input adapters.
//!
//! Every input format is normalized into a [`Snapshot`] before it reaches
//! `merge`, so the merge rules never learn what a spreadsheet or a JSON feed
//! looks like.

pub mod code_table_xls;
pub mod jma_public;
pub mod station_json;

use chrono::{DateTime, FixedOffset};

use crate::model::{Area, Location, Provider, Scope, SourceKind};

/// Something worth telling the operator about that is not fatal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub code: &'static str,
    pub subject: Option<String>,
    pub message: String,
}

impl Warning {
    pub fn new(code: &'static str, subject: Option<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            subject,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.subject {
            Some(subject) => write!(f, "{}: {}: {}", self.code, subject, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

/// The identity-independent facts a snapshot reports about one station.
#[derive(Debug, Clone, PartialEq)]
pub struct StationMetadata {
    pub name: String,
    pub kana: Option<String>,
    pub region: Option<Area>,
    pub city: Option<Area>,
    pub location: Option<Location>,
    pub provider: Provider,
    pub provider_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotStation {
    pub code: String,
    /// `None` when the source carries no evidence either way. Absence from a feed
    /// is never evidence of retirement.
    pub active: Option<bool>,
    /// Scopes this snapshot positively vouches for.
    pub scopes: Vec<Scope>,
    pub metadata: StationMetadata,
}

/// One release as seen through one input format.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub source_kind: SourceKind,
    pub release_id: String,
    pub effective_from: DateTime<FixedOffset>,
    pub stations: Vec<SnapshotStation>,
    /// Scopes this snapshot enumerates exhaustively.
    ///
    /// Only for these may `merge` read "absent from the snapshot" as "left the
    /// scope". A partial feed must not declare anything here.
    pub complete_scopes: Vec<Scope>,
    pub warnings: Vec<Warning>,
}

/// A coordinate as the feed wrote it.
///
/// The published feed quotes most coordinates but emits a minority of them as bare
/// JSON numbers, so both spellings have to be accepted. The written form is kept
/// rather than only the parsed value, because the number of decimals is itself
/// information: it says how precisely the publisher pinned the location down.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub(crate) enum RawCoordinate {
    Text(String),
    Number(serde_json::Number),
}

impl RawCoordinate {
    pub(crate) fn as_written(&self) -> String {
        match self {
            Self::Text(text) => text.trim().to_owned(),
            Self::Number(number) => number.to_string(),
        }
    }
}

/// Parse a coordinate pair, recovering the precision from how it was written.
///
/// The pair is only as well determined as its coarser axis, so the two decimal
/// counts are combined by taking the larger cell. That also keeps a bare number
/// such as `141.70499999999998` from claiming a precision its companion latitude
/// `"42.42"` plainly does not have.
///
/// A missing coordinate is unresolved; a present but unparsable one is a corrupt
/// feed and must not be quietly turned into `null`, or a broken export could erase
/// the coordinate set one release at a time.
pub(crate) fn parse_coordinate_pair(
    latitude: Option<&RawCoordinate>,
    longitude: Option<&RawCoordinate>,
    subject: &str,
) -> anyhow::Result<Option<Location>> {
    let lat_text = latitude.map(RawCoordinate::as_written).unwrap_or_default();
    let lon_text = longitude.map(RawCoordinate::as_written).unwrap_or_default();
    if lat_text.is_empty() || lon_text.is_empty() {
        return Ok(None);
    }

    let latitude: f64 = lat_text
        .parse()
        .map_err(|_| anyhow::anyhow!("{subject}: malformed latitude {lat_text:?}"))?;
    let longitude: f64 = lon_text
        .parse()
        .map_err(|_| anyhow::anyhow!("{subject}: malformed longitude {lon_text:?}"))?;

    Ok(Some(Location {
        latitude,
        longitude,
        resolution_deg: resolution_from_decimals(&lat_text)
            .max(resolution_from_decimals(&lon_text)),
    }))
}

/// Count decimal places in a coordinate as written, to recover the precision the
/// publisher actually committed to.
///
/// `"43.17"` is a claim about two decimals; parsing it to `43.17_f64` first would
/// lose exactly the fact we need.
pub(crate) fn resolution_from_decimals(text: &str) -> f64 {
    let decimals = match text.split_once('.') {
        Some((_, frac)) => frac.chars().filter(char::is_ascii_digit).count(),
        None => 0,
    };
    10f64.powi(-(decimals as i32))
}

#[cfg(test)]
mod tests {
    use super::{RawCoordinate, parse_coordinate_pair, resolution_from_decimals};

    fn text(value: &str) -> RawCoordinate {
        RawCoordinate::Text(value.to_owned())
    }

    fn number(value: f64) -> RawCoordinate {
        RawCoordinate::Number(serde_json::Number::from_f64(value).unwrap())
    }

    #[test]
    fn resolution_follows_written_precision() {
        assert!((resolution_from_decimals("43.1714") - 0.0001).abs() < 1e-12);
        assert!((resolution_from_decimals("43.17") - 0.01).abs() < 1e-12);
        assert!((resolution_from_decimals("43") - 1.0).abs() < 1e-12);
        assert!((resolution_from_decimals("-141.3156") - 0.0001).abs() < 1e-12);
    }

    #[test]
    fn a_pair_is_only_as_precise_as_its_coarser_axis() {
        let location = parse_coordinate_pair(Some(&text("43.1714")), Some(&text("141.32")), "s")
            .unwrap()
            .unwrap();
        assert!((location.resolution_deg - 0.01).abs() < 1e-12);
    }

    #[test]
    fn bare_numbers_are_accepted_without_inflating_precision() {
        // The published feed quotes most coordinates but not all; a stray number
        // must not make the pair look far more precise than the latitude allows.
        let location = parse_coordinate_pair(
            Some(&text("44.36")),
            Some(&number(141.704_999_999_999_98)),
            "羽幌町南町",
        )
        .unwrap()
        .unwrap();
        assert!((location.longitude - 141.705).abs() < 1e-9);
        assert!((location.resolution_deg - 0.01).abs() < 1e-12);
    }

    #[test]
    fn a_missing_coordinate_is_unresolved() {
        assert!(
            parse_coordinate_pair(Some(&text("")), Some(&text("141.32")), "s")
                .unwrap()
                .is_none()
        );
        assert!(parse_coordinate_pair(None, None, "s").unwrap().is_none());
    }

    #[test]
    fn a_malformed_coordinate_is_an_error() {
        let error = parse_coordinate_pair(Some(&text("43.xxxx")), Some(&text("141.32")), "s")
            .unwrap_err()
            .to_string();
        assert!(error.contains("malformed latitude"), "{error}");
    }
}
