//! Adapter for the high-quality `station.json` master.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

use super::{RawCoordinate, Snapshot, SnapshotStation, StationMetadata, parse_coordinate_pair};
use crate::code::CodeShape;
use crate::model::{Area, Provider, SourceKind};

#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(rename = "changeTime")]
    change_time: Option<String>,
    version: Option<String>,
    items: Vec<RawItem>,
}

#[derive(Debug, Deserialize)]
struct RawArea {
    code: String,
    name: String,
    kana: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawItem {
    region: Option<RawArea>,
    city: Option<RawArea>,
    code: String,
    name: String,
    kana: Option<String>,
    status: String,
    owner: Option<String>,
    latitude: Option<RawCoordinate>,
    longitude: Option<RawCoordinate>,
}

/// Overrides for values the file normally supplies itself.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub release_id: Option<String>,
    pub effective_from: Option<DateTime<FixedOffset>>,
}

pub fn load(path: &Path, overrides: &Overrides) -> Result<Snapshot> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading station.json at {}", path.display()))?;
    parse(&text, overrides)
}

pub fn parse(text: &str, overrides: &Overrides) -> Result<Snapshot> {
    let raw: RawFile = serde_json::from_str(text).context("parsing station.json")?;

    let release_id = match (&overrides.release_id, &raw.version) {
        (Some(id), _) => id.clone(),
        (None, Some(version)) => version.clone(),
        (None, None) => bail!("station.json has no `version`; pass --release-id"),
    };

    let effective_from = match (&overrides.effective_from, &raw.change_time) {
        (Some(dt), _) => *dt,
        (None, Some(change_time)) => DateTime::parse_from_rfc3339(change_time)
            .with_context(|| format!("parsing changeTime {change_time:?}"))?,
        (None, None) => bail!("station.json has no `changeTime`; pass --effective-from"),
    };

    let mut stations = Vec::with_capacity(raw.items.len());
    for item in raw.items {
        check_code(&item.code, CodeShape::Point, &item.code)?;
        if let Some(region) = &item.region {
            check_code(&region.code, CodeShape::Region, &item.code)?;
        }
        if let Some(city) = &item.city {
            check_code(&city.code, CodeShape::City, &item.code)?;
        }

        let active = parse_status(&item.status)
            .ok_or_else(|| anyhow!("station {}: unknown status {:?}", item.code, item.status))?;
        let location = parse_coordinate_pair(
            item.latitude.as_ref(),
            item.longitude.as_ref(),
            &format!("station {}", item.code),
        )?;
        let (provider, provider_detail) = parse_owner(item.owner.as_deref());

        stations.push(SnapshotStation {
            code: item.code,
            active: Some(active),
            // station.json lists retired stations too but is not the code table, so
            // it cannot say which stations a PointSeismicIntensity report may name.
            scopes: Vec::new(),
            metadata: StationMetadata {
                name: item.name,
                kana: item.kana,
                region: item.region.map(convert_area),
                city: item.city.map(convert_area),
                location,
                provider,
                provider_detail,
            },
        });
    }

    Ok(Snapshot {
        source_kind: SourceKind::StationJson,
        release_id,
        effective_from,
        stations,
        complete_scopes: Vec::new(),
        warnings: Vec::new(),
    })
}

/// Hold one code to its fixed shape.
///
/// There is no numeric hazard on this side: every code field is typed `String`,
/// so serde rejects a bare JSON number outright rather than rounding a leading
/// zero away. What is left to catch is a code that was already damaged before it
/// reached the file, which would otherwise be appended as a station in its own
/// right and given a permanent index.
fn check_code(value: &str, shape: CodeShape, station: &str) -> Result<()> {
    if !shape.accepts(value) {
        bail!("station {station}: {}", shape.describe_violation(value));
    }
    Ok(())
}

fn convert_area(area: RawArea) -> Area {
    Area {
        code: area.code,
        name: area.name,
        kana: area.kana,
    }
}

fn parse_status(status: &str) -> Option<bool> {
    match status {
        "現" | "新規" | "変更" => Some(true),
        "廃止" => Some(false),
        _ => None,
    }
}

fn parse_owner(owner: Option<&str>) -> (Provider, Option<String>) {
    let Some(owner) = owner else {
        return (Provider::Unknown, None);
    };
    let provider = match owner {
        "気象庁" => Provider::Jma,
        "防災科研" => Provider::Nied,
        "都道府県" | "市町村" => Provider::LocalGovernment,
        "その他" => Provider::Other,
        _ => Provider::Unknown,
    };
    (provider, Some(owner.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{Overrides, parse, parse_owner, parse_status};
    use crate::model::Provider;

    const MINIMAL: &str = r#"{
        "changeTime": "2026-07-23T12:00:00+09:00",
        "version": "20260723",
        "items": [
            {
                "region": {"code": "900", "name": "甲野地方北部", "kana": "コウノチホウホクブ"},
                "city": {"code": "0999100", "name": "甲野市", "kana": "コウノシ"},
                "code": "0999100", "name": "甲野市山川", "kana": "コウノシヤマカワ",
                "status": "現", "owner": "気象庁",
                "latitude": "35.1234", "longitude": "135.6789"
            }
        ]
    }"#;

    #[test]
    fn takes_release_and_effective_from_from_the_file() {
        let snapshot = parse(MINIMAL, &Overrides::default()).unwrap();
        assert_eq!(snapshot.release_id, "20260723");
        assert_eq!(
            snapshot.effective_from.to_rfc3339(),
            "2026-07-23T12:00:00+09:00"
        );
        assert!(snapshot.complete_scopes.is_empty());
    }

    #[test]
    fn cli_overrides_win() {
        let overrides = Overrides {
            release_id: Some("override".to_owned()),
            effective_from: Some(
                chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00+09:00").unwrap(),
            ),
        };
        let snapshot = parse(MINIMAL, &overrides).unwrap();
        assert_eq!(snapshot.release_id, "override");
        assert_eq!(
            snapshot.effective_from.to_rfc3339(),
            "2020-01-01T00:00:00+09:00"
        );
    }

    #[test]
    fn reads_the_coordinate() {
        let snapshot = parse(MINIMAL, &Overrides::default()).unwrap();
        let location = snapshot.stations[0].metadata.location.unwrap();
        assert!((location.latitude - 35.1234).abs() < 1e-12);
        assert!((location.longitude - 135.6789).abs() < 1e-12);
    }

    #[test]
    fn status_maps_to_lifecycle_state() {
        assert_eq!(parse_status("現"), Some(true));
        assert_eq!(parse_status("新規"), Some(true));
        assert_eq!(parse_status("変更"), Some(true));
        assert_eq!(parse_status("廃止"), Some(false));
        assert_eq!(parse_status("謎"), None);
    }

    #[test]
    fn unknown_status_is_rejected() {
        let text = MINIMAL.replace(r#""status": "現""#, r#""status": "謎""#);
        let err = parse(&text, &Overrides::default()).unwrap_err().to_string();
        assert!(err.contains("unknown status"), "{err}");
    }

    #[test]
    fn owner_maps_to_provider_and_keeps_the_original_wording() {
        assert_eq!(parse_owner(Some("気象庁")).0, Provider::Jma);
        assert_eq!(parse_owner(Some("防災科研")).0, Provider::Nied);
        assert_eq!(parse_owner(Some("都道府県")).0, Provider::LocalGovernment);
        assert_eq!(parse_owner(Some("市町村")).0, Provider::LocalGovernment);
        assert_eq!(parse_owner(Some("その他")).0, Provider::Other);
        assert_eq!(parse_owner(Some("謎の組織")).0, Provider::Unknown);
        assert_eq!(parse_owner(None).0, Provider::Unknown);
        assert_eq!(parse_owner(Some("都道府県")).1.as_deref(), Some("都道府県"));
    }

    /// A code short of its leading zero is not a malformed record to skip past:
    /// it is a station code that has never existed, and appending it would spend
    /// a permanent index on it.
    #[test]
    fn a_malformed_station_code_is_rejected() {
        let text = MINIMAL.replace(
            r#""code": "0999100", "name""#,
            r#""code": "999100", "name""#,
        );
        let err = parse(&text, &Overrides::default()).unwrap_err().to_string();
        assert!(err.contains("station code"), "{err}");
        assert!(err.contains("not 7 ASCII digits"), "{err}");
        assert!(err.contains("leading zero"), "{err}");
    }

    #[test]
    fn a_malformed_city_code_is_rejected() {
        let text = MINIMAL.replace(
            r#""city": {"code": "0999100""#,
            r#""city": {"code": "999100""#,
        );
        let err = parse(&text, &Overrides::default()).unwrap_err().to_string();
        assert!(err.contains("city code"), "{err}");
        assert!(err.contains("not 7 ASCII digits"), "{err}");
    }

    #[test]
    fn a_malformed_region_code_is_rejected() {
        let text = MINIMAL.replace(
            r#""region": {"code": "900""#,
            r#""region": {"code": "9000""#,
        );
        let err = parse(&text, &Overrides::default()).unwrap_err().to_string();
        assert!(err.contains("region code"), "{err}");
        assert!(err.contains("not 3 ASCII digits"), "{err}");
    }

    #[test]
    fn blank_coordinates_are_unresolved() {
        let text = MINIMAL.replace(r#""latitude": "35.1234""#, r#""latitude": """#);
        let snapshot = parse(&text, &Overrides::default()).unwrap();
        assert!(snapshot.stations[0].metadata.location.is_none());
    }

    #[test]
    fn malformed_coordinates_are_rejected() {
        let text = MINIMAL.replace(r#""latitude": "35.1234""#, r#""latitude": "35.xxxx""#);
        let err = parse(&text, &Overrides::default()).unwrap_err().to_string();
        assert!(err.contains("malformed latitude"), "{err}");
    }

    #[test]
    fn a_blank_latitude_does_not_excuse_a_malformed_longitude() {
        let text = MINIMAL
            .replace(r#""latitude": "35.1234""#, r#""latitude": """#)
            .replace(r#""longitude": "135.6789""#, r#""longitude": "135.xxxx""#);
        let err = parse(&text, &Overrides::default()).unwrap_err().to_string();
        assert!(err.contains("malformed longitude"), "{err}");
    }
}
