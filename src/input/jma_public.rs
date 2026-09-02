//! Adapter for the published `stations.json` combined with the code table.
//!
//! The code table supplies identity (station code, region, city, reading); the
//! public feed supplies coordinates and the operating body. They are joined on the
//! station name and nothing else: an exact match or no match at all. A near match
//! would silently attach one station's coordinates to another, which is far worse
//! than an unresolved location.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

use super::code_table_xls::{self, CodeTableRow};
use super::{
    RawCoordinate, Snapshot, SnapshotStation, StationMetadata, Warning, parse_coordinate_pair,
};
use crate::model::{Area, Location, Provider, Scope, SourceKind};

#[derive(Debug, Deserialize)]
struct RawStation {
    name: String,
    lat: Option<RawCoordinate>,
    lon: Option<RawCoordinate>,
    affi: Option<String>,
}

/// A published entry after its coordinates have been validated.
#[derive(Debug, Clone)]
struct PublicEntry {
    location: Option<Location>,
    provider: Provider,
}

pub fn load(
    stations_json: &Path,
    code_table_xls: &Path,
    release_id: String,
    effective_from: DateTime<FixedOffset>,
) -> Result<Snapshot> {
    let text = std::fs::read_to_string(stations_json)
        .with_context(|| format!("reading stations.json at {}", stations_json.display()))?;
    let code_table = code_table_xls::load(code_table_xls)?;
    build(&text, &code_table, release_id, effective_from)
}

pub fn build(
    stations_json: &str,
    code_table: &[CodeTableRow],
    release_id: String,
    effective_from: DateTime<FixedOffset>,
) -> Result<Snapshot> {
    let raw: Vec<RawStation> =
        serde_json::from_str(stations_json).context("parsing stations.json")?;

    let mut published: HashMap<String, PublicEntry> = HashMap::with_capacity(raw.len());
    for entry in raw {
        let location = parse_coordinate_pair(
            entry.lat.as_ref(),
            entry.lon.as_ref(),
            &format!("published station {:?}", entry.name),
        )?;
        let provider = parse_affiliation(entry.affi.as_deref());
        if published
            .insert(entry.name.clone(), PublicEntry { location, provider })
            .is_some()
        {
            bail!(
                "stations.json: duplicate station name {:?}; names are the join key",
                entry.name
            );
        }
    }

    let mut warnings = Vec::new();
    let mut stations = Vec::with_capacity(code_table.len());
    let mut joined = 0usize;

    for row in code_table {
        let matched = published.get(&row.point_name);
        if matched.is_some() {
            joined += 1;
        } else {
            warnings.push(Warning::new(
                "W001",
                Some(row.point_code.clone()),
                format!("coordinate join failed for {:?}", row.point_name),
            ));
        }

        stations.push(SnapshotStation {
            code: row.point_code.clone(),
            // Presence in the code table means the station exists. Absence from the
            // public feed is a join failure, never evidence of retirement.
            active: Some(true),
            scopes: vec![Scope::PointSeismicIntensity],
            metadata: StationMetadata {
                name: row.point_name.clone(),
                kana: row.point_kana.clone(),
                region: Some(Area {
                    code: row.region_code.clone(),
                    name: row.region_name.clone(),
                    kana: row.region_kana.clone(),
                }),
                city: Some(Area {
                    code: row.city_code.clone(),
                    name: row.city_name.clone(),
                    kana: row.city_kana.clone(),
                }),
                location: matched.and_then(|entry| entry.location),
                // The feed identifies the operator only by a numeric code, so there
                // is no original wording to preserve.
                provider: matched.map_or(Provider::Unknown, |entry| entry.provider),
                provider_detail: None,
            },
        });
    }

    // Published entries with no code table row cannot be given an index, because a
    // canonical index is a mapping from a station code and they have none.
    let unmatched_published = published.len() - joined;
    if unmatched_published > 0 {
        warnings.push(Warning::new(
            "W002",
            None,
            format!(
                "{unmatched_published} published station(s) have no code table entry \
                 and were dropped (no station code, so no index can be assigned)"
            ),
        ));
    }

    Ok(Snapshot {
        source_kind: SourceKind::JmaPublic,
        release_id,
        effective_from,
        stations,
        // Sheet 24 enumerates the PointSeismicIntensity scope exhaustively, so a
        // station's absence from it is evidence that it left the scope.
        complete_scopes: vec![Scope::PointSeismicIntensity],
        warnings,
    })
}

fn parse_affiliation(affi: Option<&str>) -> Provider {
    match affi {
        Some("0") => Provider::Jma,
        Some("1") => Provider::LocalGovernment,
        Some("2") => Provider::Nied,
        _ => Provider::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{build, parse_affiliation};
    use crate::input::code_table_xls::CodeTableRow;
    use crate::model::{Provider, Scope};
    use chrono::DateTime;

    fn row(point_code: &str, point_name: &str) -> CodeTableRow {
        CodeTableRow {
            region_code: "900".to_owned(),
            region_name: "甲野地方北部".to_owned(),
            region_kana: Some("コウノチホウホクブ".to_owned()),
            city_code: "0999100".to_owned(),
            city_name: "甲野市".to_owned(),
            city_kana: Some("コウノシ".to_owned()),
            point_code: point_code.to_owned(),
            point_name: point_name.to_owned(),
            point_kana: Some("コウノシヤマカワ".to_owned()),
        }
    }

    fn snapshot(stations_json: &str, table: &[CodeTableRow]) -> crate::input::Snapshot {
        build(
            stations_json,
            table,
            "20260723".to_owned(),
            DateTime::parse_from_rfc3339("2026-07-23T12:00:00+09:00").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn joins_on_exact_name_and_declares_the_scope_complete() {
        let table = [row("0999100", "甲野市山川")];
        let json = r#"[{"lat":"35.12","lon":"135.68","name":"甲野市山川","affi":"0"}]"#;
        let snapshot = snapshot(json, &table);

        assert_eq!(snapshot.complete_scopes, vec![Scope::PointSeismicIntensity]);
        assert_eq!(
            snapshot.stations[0].scopes,
            vec![Scope::PointSeismicIntensity]
        );
        assert_eq!(snapshot.stations[0].active, Some(true));

        let location = snapshot.stations[0].metadata.location.unwrap();
        assert!((location.latitude - 35.12).abs() < 1e-12);
        assert!((location.resolution_deg - 0.01).abs() < 1e-12);
        assert_eq!(snapshot.stations[0].metadata.provider, Provider::Jma);
        assert!(snapshot.warnings.is_empty());
    }

    #[test]
    fn a_failed_join_keeps_the_station_without_a_location() {
        let table = [row("0999100", "甲野市山川")];
        let json = r#"[{"lat":"35.12","lon":"135.68","name":"甲野市山川西","affi":"0"}]"#;
        let snapshot = snapshot(json, &table);

        assert_eq!(snapshot.stations.len(), 1);
        assert!(snapshot.stations[0].metadata.location.is_none());
        // Without a match there is no evidence about the operator either.
        assert_eq!(snapshot.stations[0].metadata.provider, Provider::Unknown);

        let codes: Vec<&str> = snapshot.warnings.iter().map(|w| w.code).collect();
        assert_eq!(codes, ["W001", "W002"]);
    }

    #[test]
    fn published_entries_without_a_code_are_dropped_with_a_warning() {
        let table = [row("0999100", "甲野市山川")];
        let json = r#"[
            {"lat":"35.12","lon":"135.68","name":"甲野市山川","affi":"0"},
            {"lat":"35.28","lon":"135.42","name":"無名観測点","affi":"1"}
        ]"#;
        let snapshot = snapshot(json, &table);

        assert_eq!(snapshot.stations.len(), 1);
        let warning = snapshot.warnings.iter().find(|w| w.code == "W002").unwrap();
        assert!(warning.message.contains('1'), "{}", warning.message);
    }

    #[test]
    fn affiliation_maps_to_provider() {
        assert_eq!(parse_affiliation(Some("0")), Provider::Jma);
        assert_eq!(parse_affiliation(Some("1")), Provider::LocalGovernment);
        assert_eq!(parse_affiliation(Some("2")), Provider::Nied);
        assert_eq!(parse_affiliation(Some("9")), Provider::Unknown);
        assert_eq!(parse_affiliation(None), Provider::Unknown);
    }

    #[test]
    fn duplicate_published_names_are_rejected() {
        let table = [row("0999100", "甲野市山川")];
        let json = r#"[
            {"lat":"35.12","lon":"135.68","name":"甲野市山川","affi":"0"},
            {"lat":"35.18","lon":"135.33","name":"甲野市山川","affi":"1"}
        ]"#;
        let err = build(
            json,
            &table,
            "20260723".to_owned(),
            DateTime::parse_from_rfc3339("2026-07-23T12:00:00+09:00").unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("duplicate station name"), "{err}");
    }

    #[test]
    fn malformed_published_coordinates_are_rejected() {
        let table = [row("0999100", "甲野市山川")];
        let json = r#"[{"lat":"35.xx","lon":"135.68","name":"甲野市山川","affi":"0"}]"#;
        let err = build(
            json,
            &table,
            "20260723".to_owned(),
            DateTime::parse_from_rfc3339("2026-07-23T12:00:00+09:00").unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed lat"), "{err}");
    }
}
