//! End-to-end behaviour of a master built from the published feed plus the code
//! table, where identity and scope come from different places.

mod common;

use common::{apply, at, public_snapshot, station};
use jma_station_master::append::append;
use jma_station_master::model::{Provider, Scope, SourceKind};

const R1: &str = "2026-03-12T12:00:00+09:00";
const R2: &str = "2026-07-23T12:00:00+09:00";

fn initial() -> jma_station_master::model::Master {
    let snapshot = public_snapshot("stations_min.json", "code_table_min.xls", "20260312", R1);
    apply(None, &snapshot).0
}

#[test]
fn the_code_table_supplies_identity_and_the_feed_supplies_coordinates() {
    let snapshot = public_snapshot("stations_min.json", "code_table_min.xls", "20260312", R1);
    let (master, report) = apply(None, &snapshot);

    assert_eq!(master.source_kind, SourceKind::JmaPublic);
    assert_eq!(master.stations.len(), 5);

    let yamakawa = station(&master, "0999100");
    let revision = &yamakawa.metadata[0];
    assert_eq!(revision.name, "甲野市山川");
    // Sheet 24 spells readings in hiragana; the master stores katakana throughout.
    assert_eq!(revision.kana.as_deref(), Some("コウノシヤマカワ"));
    assert_eq!(revision.region.as_ref().unwrap().code, "900");
    assert_eq!(revision.city.as_ref().unwrap().code, "0999100");
    assert_eq!(revision.provider, Provider::Jma);
    // The feed names the operator only by a numeric code.
    assert_eq!(revision.provider_detail, None);

    let location = revision.location.unwrap();
    assert!((location.latitude - 35.12).abs() < 1e-12);
    assert!((location.resolution_deg - 0.01).abs() < 1e-12);

    assert_eq!(
        yamakawa.scope_events.len(),
        1,
        "listing in the code table enables the scope"
    );
    assert!(yamakawa.scope_events[0].enabled);
    assert_eq!(yamakawa.scope_events[0].scope, Scope::PointSeismicIntensity);

    let summary = &report.scopes[&Scope::PointSeismicIntensity];
    assert_eq!(summary.enabled, 5);
    assert_eq!(summary.disabled, 0);
}

#[test]
fn a_station_the_feed_does_not_name_keeps_its_index_without_a_location() {
    let master = initial();

    // 丙川区北二条 has a code table entry but no published coordinates.
    let unresolved = station(&master, "0999010");
    assert_eq!(unresolved.metadata.len(), 1);
    assert!(unresolved.metadata[0].location.is_none());
    assert_eq!(unresolved.metadata[0].provider, Provider::Unknown);
    assert!(unresolved.is_active_at(at(R1)) == Some(true));
}

#[test]
fn the_report_separates_unresolved_from_out_of_scope() {
    let snapshot = public_snapshot("stations_min.json", "code_table_min.xls", "20260312", R1);
    let (_, report) = apply(None, &snapshot);

    assert_eq!(report.unresolved_active, 1);
    assert_eq!(report.unresolved_in_scope, Some(1));

    let codes: Vec<&str> = report.warnings.iter().map(|w| w.code).collect();
    // One failed join, plus the published entry that has no code table row.
    assert_eq!(codes, ["W001", "W002"]);
}

#[test]
fn a_failed_join_alone_does_not_erase_a_known_location() {
    let first = initial();
    let before = station(&first, "0999120").metadata[0].location.unwrap();

    // The later feed no longer lists 甲野市月見, but the code table still names it
    // exactly as before, so it is recognisably the same station.
    let snapshot = public_snapshot("stations_min_v2.json", "code_table_min.xls", "20260723", R2);
    let (second, _) = apply(Some(&first), &snapshot);

    let tsukimi = station(&second, "0999120");
    assert_eq!(
        tsukimi.metadata.len(),
        1,
        "nothing changed, nothing recorded"
    );
    assert_eq!(tsukimi.metadata[0].location.unwrap(), before);
}

#[test]
fn a_renamed_station_with_no_join_becomes_unresolved() {
    let first = initial();

    // Here the code table renames 甲野市月見 and the feed cannot be joined, so the
    // old coordinate might belong to the old site.
    let snapshot = public_snapshot(
        "stations_min_v2.json",
        "code_table_min_renamed.xls",
        "20260723",
        R2,
    );
    let (second, _) = apply(Some(&first), &snapshot);

    let renamed = station(&second, "0999120");
    assert_eq!(renamed.metadata.len(), 2);
    assert_eq!(renamed.metadata[1].name, "甲野市月見東");
    assert!(
        renamed.metadata[1].location.is_none(),
        "an inherited coordinate would be a guess"
    );
}

#[test]
fn leaving_the_code_table_retires_the_scope_and_nothing_else() {
    let first = initial();
    let snapshot = public_snapshot(
        "stations_min_v2.json",
        "code_table_min_shrunk.xls",
        "20260723",
        R2,
    );
    // A departure is an ordinary part of an update; no flag stands in the way.
    let (second, report) = apply(Some(&first), &snapshot);

    let departed = station(&second, "0999120");
    assert_eq!(departed.scope_events.len(), 2);
    assert!(departed.scope_events[0].enabled);
    assert!(!departed.scope_events[1].enabled);
    assert_eq!(departed.scope_events[1].effective_from, at(R2));

    // Dropping out of the code table is not the same as being dismantled.
    assert_eq!(departed.lifecycle.len(), 1);
    assert!(departed.lifecycle[0].active);
    assert_eq!(departed.is_active_at(at(R2)), Some(true));

    // Its index is untouched and still addressable.
    assert_eq!(second.stations.len(), 5);
    assert_eq!(second.releases[1].index_count, 5);

    // Still unresolved and in scope: 丙川区北二条. 甲野市月見 has a location.
    assert_eq!(report.unresolved_in_scope, Some(1));
}

#[test]
fn departures_are_named_in_the_report() {
    // Nothing refuses a truncated code table, so the report is the whole audit
    // trail: it has to say which stations left, not just how many.
    let first = initial();
    let snapshot = public_snapshot(
        "stations_min_v2.json",
        "code_table_min_shrunk.xls",
        "20260723",
        R2,
    );
    let (_, report) = apply(Some(&first), &snapshot);

    let summary = &report.scopes[&Scope::PointSeismicIntensity];
    assert_eq!(summary.enabled, 0);
    assert_eq!(summary.disabled, 1);
    assert_eq!(summary.departed, ["0999120"]);
}

#[test]
fn a_quiet_release_still_reports_the_scope() {
    // A release where nothing enters or leaves must still produce the scope line,
    // so an anomalous run stands out against a familiar shape.
    let first = initial();
    let snapshot = public_snapshot("stations_min_v2.json", "code_table_min.xls", "20260723", R2);
    let (_, report) = apply(Some(&first), &snapshot);

    let summary = &report.scopes[&Scope::PointSeismicIntensity];
    assert_eq!(summary.enabled, 0);
    assert_eq!(summary.disabled, 0);
    assert!(summary.departed.is_empty());
}

#[test]
fn resupplying_a_recorded_release_is_an_error() {
    let first = initial();
    let snapshot = public_snapshot("stations_min.json", "code_table_min.xls", "20260312", R1);

    let error = append(Some(&first), &snapshot, at(common::GENERATED_AT))
        .unwrap_err()
        .to_string();
    assert!(error.contains("is already recorded"), "{error}");
}
