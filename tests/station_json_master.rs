//! End-to-end behaviour of a master built from station.json.

mod common;

use chrono::{DateTime, FixedOffset};
use common::{CodeTable, apply, at, index_of, station, station_snapshot};
use jma_station_master::append::append;
use jma_station_master::model::{Provider, SourceKind};

const R2: &str = "2026-03-12T12:00:00+09:00";
const R3: &str = "2026-07-23T12:00:00+09:00";

/// The pinned stamp, for the error paths that call `append` directly.
fn stamp() -> DateTime<FixedOffset> {
    at(common::GENERATED_AT)
}

#[test]
fn init_records_the_release_and_every_station() {
    let snapshot = station_snapshot("station_min.json");
    let (master, report) = apply(None, &snapshot);

    assert_eq!(master.schema_version, 1);
    assert_eq!(master.source_kind, SourceKind::StationJson);
    assert_eq!(master.stations.len(), 5);
    assert_eq!(master.releases.len(), 1);
    assert_eq!(master.releases[0].id, "20260312");
    assert_eq!(master.releases[0].effective_from, at(R2));
    assert_eq!(master.releases[0].index_count, 5);

    let yamakawa = station(&master, "0999100");
    assert_eq!(yamakawa.metadata.len(), 1);
    assert_eq!(yamakawa.metadata[0].name, "甲野市山川");
    assert_eq!(yamakawa.metadata[0].provider, Provider::Jma);
    assert_eq!(
        yamakawa.metadata[0].provider_detail.as_deref(),
        Some("気象庁")
    );
    assert_eq!(
        yamakawa.lifecycle,
        [jma_station_master::model::LifecycleEvent {
            effective_from: at(R2),
            active: true,
        }]
    );
    // station.json cannot attest to the PointSeismicIntensity code table.
    assert!(yamakawa.scope_events.is_empty());

    // Indices are handed out in code order, not in the order the feed listed them.
    assert_eq!(
        common::codes(&master),
        ["0999010", "0999100", "0999101", "0999120", "0999320"]
    );

    assert_eq!(report.stations_appended, 5);
    assert_eq!(report.stations_total, 5);
    assert_eq!(report.unresolved_active, 0);
    assert_eq!(report.unresolved_in_scope, None);
}

#[test]
fn new_codes_are_appended_and_existing_indices_never_move() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let before = index_of(&first);

    let (second, report) = apply(Some(&first), &station_snapshot("station_min_v2.json"));
    let after = index_of(&second);

    for (code, index) in &before {
        assert_eq!(after.get(code), Some(index), "index of {code} moved");
    }
    assert_eq!(second.stations.len(), 7);
    assert_eq!(report.stations_appended, 2);
    assert_eq!(report.stations_existing, 5);
}

#[test]
fn appended_codes_are_ordered_by_code_not_by_feed_order() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let (second, _) = apply(Some(&first), &station_snapshot("station_min_v2.json"));

    // The feed lists 0999900 before 0999199; the index mapping must not inherit
    // that ordering, or a re-sorted export would hand out different indices.
    assert_eq!(second.stations[5].code, "0999199");
    assert_eq!(second.stations[6].code, "0999900");
}

#[test]
fn a_changed_name_appends_a_revision_and_an_unchanged_one_does_not() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let (second, report) = apply(Some(&first), &station_snapshot("station_min_v2.json"));

    let renamed = station(&second, "0999101");
    assert_eq!(renamed.metadata.len(), 2);
    assert_eq!(renamed.metadata[0].name, "甲野市服部");
    assert_eq!(renamed.metadata[0].effective_from, at(R2));
    assert_eq!(renamed.metadata[1].name, "甲野市服部西");
    assert_eq!(renamed.metadata[1].effective_from, at(R3));

    let untouched = station(&second, "0999100");
    assert_eq!(
        untouched.metadata.len(),
        1,
        "nothing changed, nothing added"
    );

    // 0999100, 0999120 and 0999010 are unchanged; 0999101 was renamed, 0999320
    // moved, and the two new codes each get a first revision.
    assert_eq!(report.metadata_unchanged, 3);
    assert_eq!(report.metadata_revised, 4);
}

#[test]
fn a_real_relocation_appends_a_revision() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let (second, _) = apply(Some(&first), &station_snapshot("station_min_v2.json"));

    let moved = station(&second, "0999320");
    assert_eq!(moved.metadata.len(), 2);
    assert!((moved.metadata[0].location.unwrap().latitude - 35.22).abs() < 1e-12);
    assert!((moved.metadata[1].location.unwrap().latitude - 35.29).abs() < 1e-12);
}

#[test]
fn retirement_is_a_lifecycle_event_not_a_deletion() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let (second, report) = apply(Some(&first), &station_snapshot("station_min_v2.json"));

    let retired = station(&second, "0999120");
    assert_eq!(retired.lifecycle.len(), 2);
    assert!(retired.lifecycle[0].active);
    assert!(!retired.lifecycle[1].active);
    assert_eq!(retired.lifecycle[1].effective_from, at(R3));
    // Still present, still holding its index.
    assert_eq!(second.stations[3].code, "0999120");

    assert_eq!(report.lifecycle_deactivated, 1);
    assert_eq!(report.lifecycle_activated, 2, "only the two new codes");
}

#[test]
fn an_unchanged_lifecycle_state_adds_no_event() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let (second, _) = apply(Some(&first), &station_snapshot("station_min_v2.json"));

    // Active at both releases, so there is nothing to record at the second.
    assert_eq!(station(&second, "0999100").lifecycle.len(), 1);
}

#[test]
fn building_twice_from_the_same_inputs_is_byte_identical() {
    // With the stamp pinned there is no other source of variation, so the same
    // inputs must always produce the same master.
    let (once, _) = apply(None, &station_snapshot("station_min.json"));
    let (twice, _) = apply(None, &station_snapshot("station_min.json"));

    assert_eq!(once, twice);
    assert_eq!(
        serde_json::to_string(&once).unwrap(),
        serde_json::to_string(&twice).unwrap()
    );
}

/// station.json is only held to the *shape* of its codes, so a code repeated
/// within one file is stopped at the entrance to `append` instead.
///
/// This is the init path on purpose: a duplicate here has no earlier master to be
/// checked against, and an exactly repeated entry also slips past the final
/// validation, so this is the only place it is ever caught.
#[test]
fn a_duplicate_station_code_is_rejected() {
    let snapshot = station_snapshot("station_duplicate_code.json");

    let error = append(None, &snapshot, stamp()).unwrap_err().to_string();
    assert!(error.contains("duplicate station code"), "{error}");
    assert!(error.contains("0999100"), "{error}");
    assert!(error.contains("entries 1 and 2"), "{error}");
}

#[test]
fn resupplying_a_recorded_release_is_an_error() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));

    let error = append(Some(&first), &station_snapshot("station_min.json"), stamp())
        .unwrap_err()
        .to_string();
    assert!(error.contains("is already recorded"), "{error}");
    assert!(error.contains("never revisited"), "{error}");
}

#[test]
fn a_release_id_cannot_be_reused_for_a_different_release() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));

    // Same id, different effective time. The id has to keep identifying one fixed
    // set of parameters, so this is refused just as a plain repeat is.
    let mut relabelled = station_snapshot("station_min_v2.json");
    relabelled.release_id = "20260312".to_owned();

    let error = append(Some(&first), &relabelled, stamp())
        .unwrap_err()
        .to_string();
    assert!(error.contains("is already recorded"), "{error}");
}

#[test]
fn releases_must_arrive_oldest_first() {
    let (later, _) = apply(None, &station_snapshot("station_min.json"));
    let earlier = station_snapshot("station_min_earlier.json");

    let error = append(Some(&later), &earlier, stamp())
        .unwrap_err()
        .to_string();
    assert!(error.contains("out-of-order release"), "{error}");
    assert!(error.contains("re-date every state"), "{error}");
}

#[test]
fn index_count_tracks_the_table_and_never_shrinks() {
    let (first, _) = apply(None, &station_snapshot("station_min.json"));
    let (second, _) = apply(Some(&first), &station_snapshot("station_min_v2.json"));

    assert_eq!(second.releases[0].index_count, 5);
    assert_eq!(second.releases[1].index_count, 7);
    assert_eq!(second.releases[1].index_count, second.stations.len());
    for window in second.releases.windows(2) {
        assert!(window[1].index_count >= window[0].index_count);
    }
}

#[test]
fn a_master_cannot_change_source_kind() {
    let (from_station_json, _) = apply(None, &station_snapshot("station_min.json"));
    let public = common::public_snapshot("stations_min.json", CodeTable::Min, "20260723", R3);

    let error = append(Some(&from_station_json), &public, stamp())
        .unwrap_err()
        .to_string();
    assert!(error.contains("source kind mismatch"), "{error}");
}
