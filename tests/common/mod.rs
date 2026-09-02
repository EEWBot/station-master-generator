//! Shared helpers for the integration tests.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, FixedOffset};
use jma_station_master::append::{AppendReport, append};
use jma_station_master::input::{Snapshot, jma_public, station_json};
use jma_station_master::model::Master;

/// A fixed stamp so every produced master is byte-for-byte comparable.
pub const GENERATED_AT: &str = "2026-08-28T03:00:00+09:00";

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

pub fn at(text: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(text).expect("test timestamps are well formed")
}

pub fn station_snapshot(name: &str) -> Snapshot {
    station_json::load(&fixture(name), &station_json::Overrides::default()).expect("fixture parses")
}

pub fn public_snapshot(
    stations_json: &str,
    code_table: &str,
    release_id: &str,
    effective_from: &str,
) -> Snapshot {
    jma_public::load(
        &fixture(stations_json),
        &fixture(code_table),
        release_id.to_owned(),
        at(effective_from),
    )
    .expect("fixture parses")
}

/// Append and assert the result is structurally sound, which every test wants.
pub fn apply(previous: Option<&Master>, snapshot: &Snapshot) -> (Master, AppendReport) {
    let (master, report) =
        append(previous, snapshot, at(GENERATED_AT)).expect("the release appends");
    jma_station_master::validate::validate(&master, previous).expect("result validates");
    (master, report)
}

pub fn index_of(master: &Master) -> HashMap<String, usize> {
    master
        .stations
        .iter()
        .enumerate()
        .map(|(index, station)| (station.code.clone(), index))
        .collect()
}

pub fn codes(master: &Master) -> Vec<&str> {
    master.stations.iter().map(|s| s.code.as_str()).collect()
}

pub fn station<'a>(master: &'a Master, code: &str) -> &'a jma_station_master::model::Station {
    master
        .stations
        .iter()
        .find(|s| s.code == code)
        .unwrap_or_else(|| panic!("station {code} is present"))
}
