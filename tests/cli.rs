//! The command line surface, driven as a user would drive it.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{GENERATED_AT, fixture};

const BIN: &str = env!("CARGO_BIN_EXE_jma-station-master");

/// A directory of its own per test, so runs cannot collide.
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jma-station-master-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory is creatable");
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().expect("binary runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn init_from_station_json(output: &Path) -> Output {
    run(&[
        "init",
        "--station-json",
        fixture("station_min.json").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ])
}

#[test]
fn init_writes_a_master_and_prints_a_summary() {
    let dir = workspace("init");
    let master = dir.join("master.json");
    let report = dir.join("report.json");

    let output = run(&[
        "init",
        "--stations-json",
        fixture("stations_min.json").to_str().unwrap(),
        "--code-table-xls",
        fixture("code_table_min.xls").to_str().unwrap(),
        "--release-id",
        "20260312",
        "--effective-from",
        "2026-03-12T12:00:00+09:00",
        "--output",
        master.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let summary = stderr(&output);
    assert!(summary.contains("Release: 20260312"), "{summary}");
    assert!(summary.contains("Source: jma_public"), "{summary}");
    assert!(summary.contains("W001"), "{summary}");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&master).unwrap()).unwrap();
    assert_eq!(written["schema_version"], 1);
    assert_eq!(written["source_kind"], "jma_public");
    assert_eq!(written["stations"].as_array().unwrap().len(), 5);
    // The index is the array offset; storing it as a field could let the two drift.
    assert!(written["stations"][0].get("index").is_none());

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(report["stations_total"], 5);
    assert_eq!(report["unresolved_active"], 1);
    assert_eq!(report["unresolved_in_scope"], 1);
    assert_eq!(report["scopes"][0]["scope"], "point_seismic_intensity");
    assert_eq!(report["scopes"][0]["enabled"], 5);
    assert_eq!(report["scopes"][0]["disabled"], 0);
    // The audit trail is present even when it is empty.
    assert_eq!(report["scopes"][0]["departed"], serde_json::json!([]));
}

#[test]
fn departures_are_reported_in_full() {
    let dir = workspace("departures");
    let first = dir.join("first.json");
    let report_path = dir.join("report.json");

    let stations = fixture("stations_min.json");
    let full_table = fixture("code_table_min.xls");
    let shrunk_table = fixture("code_table_min_shrunk.xls");
    let second = dir.join("second.json");

    let init = run(&[
        "init",
        "--stations-json",
        stations.to_str().unwrap(),
        "--code-table-xls",
        full_table.to_str().unwrap(),
        "--release-id",
        "20260312",
        "--effective-from",
        "2026-03-12T12:00:00+09:00",
        "--output",
        first.to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);
    assert!(init.status.success(), "{}", stderr(&init));

    // The next code table has lost a station.
    let output = run(&[
        "update",
        "--previous",
        first.to_str().unwrap(),
        "--stations-json",
        stations.to_str().unwrap(),
        "--code-table-xls",
        shrunk_table.to_str().unwrap(),
        "--release-id",
        "20260723",
        "--effective-from",
        "2026-07-23T12:00:00+09:00",
        "--output",
        second.to_str().unwrap(),
        "--report",
        report_path.to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    // The signed movement is what an operator scans for.
    let summary = stderr(&output);
    assert!(summary.contains("disabled:"), "{summary}");
    assert!(summary.contains("-1"), "{summary}");
    assert!(summary.contains("0999120"), "{summary}");

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).unwrap()).unwrap();
    assert_eq!(report["scopes"][0]["disabled"], 1);
    assert_eq!(
        report["scopes"][0]["departed"],
        serde_json::json!(["0999120"])
    );
}

#[test]
fn an_existing_output_is_never_overwritten() {
    let dir = workspace("no-overwrite");
    let master = dir.join("master.json");

    assert!(init_from_station_json(&master).status.success());
    let before = std::fs::read_to_string(&master).unwrap();

    // There is no flag that would let this through: a master is produced at a new
    // path and promoted by hand once its report has been checked.
    let second = init_from_station_json(&master);
    assert!(!second.status.success());
    assert!(
        stderr(&second).contains("already exists"),
        "{}",
        stderr(&second)
    );
    assert_eq!(std::fs::read_to_string(&master).unwrap(), before);
}

#[test]
fn an_existing_report_is_never_overwritten() {
    let dir = workspace("no-report-overwrite");
    let master = dir.join("master.json");
    let next = dir.join("next.json");

    assert!(init_from_station_json(&master).status.success());
    let before = std::fs::read_to_string(&master).unwrap();

    // A report path aimed at a master would destroy it as thoroughly as an output
    // path would, so it is refused on the same terms.
    let output = run(&[
        "update",
        "--previous",
        master.to_str().unwrap(),
        "--station-json",
        fixture("station_min_v2.json").to_str().unwrap(),
        "--output",
        next.to_str().unwrap(),
        "--report",
        master.to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("already exists"),
        "{}",
        stderr(&output)
    );
    assert_eq!(std::fs::read_to_string(&master).unwrap(), before);
    // The refusal comes before any work, so no master is produced either.
    assert!(!next.exists(), "nothing is written on failure");
}

#[test]
fn a_report_may_not_share_the_output_path() {
    let dir = workspace("report-is-output");
    let master = dir.join("master.json");

    // Neither path exists yet, so only comparing the two catches this. The report
    // is written after the master and would land on top of it.
    let output = run(&[
        "init",
        "--station-json",
        fixture("station_min.json").to_str().unwrap(),
        "--output",
        master.to_str().unwrap(),
        "--report",
        master.to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("same path"), "{}", stderr(&output));
    assert!(!master.exists(), "nothing is written on failure");
}

#[test]
fn resupplying_a_recorded_release_is_refused() {
    let dir = workspace("repeat-release");
    let first = dir.join("first.json");
    assert!(init_from_station_json(&first).status.success());

    let output = run(&[
        "update",
        "--previous",
        first.to_str().unwrap(),
        "--station-json",
        fixture("station_min.json").to_str().unwrap(),
        "--output",
        dir.join("second.json").to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("is already recorded"),
        "{}",
        stderr(&output)
    );
    assert!(!dir.join("second.json").exists());
}

#[test]
fn update_extends_a_master_and_is_reproducible() {
    let dir = workspace("update");
    let first = dir.join("first.json");
    let second = dir.join("second.json");
    let third = dir.join("third.json");

    assert!(init_from_station_json(&first).status.success());

    let update = |output: &Path| {
        run(&[
            "update",
            "--previous",
            first.to_str().unwrap(),
            "--station-json",
            fixture("station_min_v2.json").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--generated-at",
            GENERATED_AT,
        ])
    };

    let output = update(&second);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("appended:"), "{}", stderr(&output));

    // Same inputs, same pinned stamp, byte-for-byte identical result.
    assert!(update(&third).status.success());
    assert_eq!(
        std::fs::read_to_string(&second).unwrap(),
        std::fs::read_to_string(&third).unwrap()
    );
}

#[test]
fn a_master_may_not_be_updated_from_a_different_source() {
    let dir = workspace("pinning");
    let master = dir.join("master.json");
    assert!(init_from_station_json(&master).status.success());

    let output = run(&[
        "update",
        "--previous",
        master.to_str().unwrap(),
        "--stations-json",
        fixture("stations_min.json").to_str().unwrap(),
        "--code-table-xls",
        fixture("code_table_min.xls").to_str().unwrap(),
        "--release-id",
        "20260723",
        "--effective-from",
        "2026-07-23T12:00:00+09:00",
        "--output",
        dir.join("out.json").to_str().unwrap(),
        "--generated-at",
        GENERATED_AT,
    ]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("source kind mismatch"),
        "{}",
        stderr(&output)
    );
    assert!(
        !dir.join("out.json").exists(),
        "nothing is written on failure"
    );
}

#[test]
fn the_published_feed_requires_an_explicit_release() {
    let dir = workspace("release-required");

    let output = run(&[
        "init",
        "--stations-json",
        fixture("stations_min.json").to_str().unwrap(),
        "--code-table-xls",
        fixture("code_table_min.xls").to_str().unwrap(),
        "--output",
        dir.join("out.json").to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("--release-id is required"), "{text}");
}

#[test]
fn the_published_feed_requires_the_code_table() {
    let dir = workspace("code-table-required");

    let output = run(&[
        "init",
        "--stations-json",
        fixture("stations_min.json").to_str().unwrap(),
        "--release-id",
        "20260312",
        "--effective-from",
        "2026-03-12T12:00:00+09:00",
        "--output",
        dir.join("out.json").to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("--code-table-xls"),
        "{}",
        stderr(&output)
    );
}
