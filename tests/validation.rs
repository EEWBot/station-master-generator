//! The structural guarantees a published master must hold.

mod common;

use common::{apply, at, station_snapshot};
use jma_station_master::model::{Location, Master, ScopeEvent};
use jma_station_master::validate::validate;

fn master() -> Master {
    apply(None, &station_snapshot("station_min.json")).0
}

#[test]
fn a_freshly_built_master_validates() {
    assert!(validate(&master(), None).is_ok());
}

fn message(errors: &jma_station_master::validate::ValidationErrors) -> String {
    errors.to_string()
}

#[test]
fn removing_a_station_is_rejected() {
    let before = master();
    let mut after = before.clone();
    after.stations.remove(1);

    let errors = validate(&after, Some(&before)).unwrap_err();
    let text = message(&errors);
    assert!(text.contains("station table shrank"), "{text}");
    assert!(text.contains("assigned indices are permanent"), "{text}");
}

#[test]
fn reordering_stations_is_rejected() {
    let before = master();
    let mut after = before.clone();
    after.stations.swap(0, 1);

    let text = message(&validate(&after, Some(&before)).unwrap_err());
    assert!(text.contains("assigned indices are permanent"), "{text}");
}

#[test]
fn inserting_a_station_before_the_end_is_rejected() {
    let before = master();
    let mut after = before.clone();
    let newcomer = after.stations[0].clone();
    after.stations.insert(0, newcomer);

    // The table only grew, but every existing index shifted.
    assert!(after.stations.len() > before.stations.len());
    let text = message(&validate(&after, Some(&before)).unwrap_err());
    assert!(text.contains("assigned indices are permanent"), "{text}");
}

#[test]
fn duplicate_codes_are_rejected() {
    let mut master = master();
    let duplicate = master.stations[0].clone();
    master.stations.push(duplicate);

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("appears at index"), "{text}");
}

/// The damage this check exists for: a code that lost its leading zero is
/// unique, well formed as a string, and names a station that does not exist —
/// while holding an index that can never be handed back.
#[test]
fn a_malformed_station_code_is_rejected() {
    let mut master = master();
    master.stations[0].code = "999100".to_owned();

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("not 7 ASCII digits"), "{text}");
    assert!(text.contains("leading zero"), "{text}");
}

#[test]
fn a_malformed_city_code_in_a_revision_is_rejected() {
    let mut master = master();
    let city = master.stations[0].metadata[0].city.as_mut().unwrap();
    city.code = "999100".to_owned();

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("city code"), "{text}");
    // The revision has to be named, or there is no way to find it in a history.
    assert!(text.contains("revision effective"), "{text}");
}

#[test]
fn appending_a_station_is_accepted() {
    let before = master();
    let (after, _) = apply(Some(&before), &station_snapshot("station_min_v2.json"));
    assert!(validate(&after, Some(&before)).is_ok());
}

#[test]
fn releases_must_be_strictly_increasing() {
    let mut master = master();
    let mut duplicate = master.releases[0].clone();
    duplicate.id = "20260313".to_owned();
    master.releases.push(duplicate);

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("does not come after release"), "{text}");
}

#[test]
fn the_index_space_may_not_shrink() {
    let mut master = master();
    let mut later = master.releases[0].clone();
    later.id = "20260723".to_owned();
    later.effective_from = at("2026-07-23T12:00:00+09:00");
    later.index_count = 2;
    master.releases.push(later);

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("the index space never shrinks"), "{text}");
}

#[test]
fn index_count_may_not_exceed_the_table() {
    let mut master = master();
    master.releases[0].index_count = 99;

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("but the station table holds"), "{text}");
}

#[test]
fn duplicated_revision_timestamps_are_rejected() {
    let mut master = master();
    let duplicate = master.stations[0].metadata[0].clone();
    master.stations[0].metadata.push(duplicate);

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("out of order or duplicated"), "{text}");
}

#[test]
fn duplicated_lifecycle_timestamps_are_rejected() {
    let mut master = master();
    let duplicate = master.stations[0].lifecycle[0];
    master.stations[0].lifecycle.push(duplicate);

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("out of order or duplicated"), "{text}");
}

#[test]
fn a_scope_event_that_marks_no_change_is_rejected() {
    let mut master = master();
    let scope = jma_station_master::model::Scope::PointSeismicIntensity;
    master.stations[0].scope_events = vec![
        ScopeEvent {
            effective_from: at("2026-03-12T12:00:00+09:00"),
            scope,
            enabled: true,
        },
        ScopeEvent {
            effective_from: at("2026-07-23T12:00:00+09:00"),
            scope,
            enabled: true,
        },
    ];

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("twice in a row"), "{text}");
}

#[test]
fn out_of_range_coordinates_are_rejected() {
    let mut master = master();
    master.stations[0].metadata[0].location = Some(Location {
        latitude: 91.0,
        longitude: 181.0,
        resolution_deg: 0.0,
    });

    let text = message(&validate(&master, None).unwrap_err());
    assert!(text.contains("outside -90..=90"), "{text}");
    assert!(text.contains("outside -180..=180"), "{text}");
    assert!(text.contains("finite and positive"), "{text}");
}

#[test]
fn every_violation_is_reported_at_once() {
    let mut master = master();
    master.schema_version = 99;
    master.releases[0].index_count = 99;

    let errors = validate(&master, None).unwrap_err();
    assert!(errors.0.len() >= 2, "{}", message(&errors));
}
