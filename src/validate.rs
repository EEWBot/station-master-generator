//! Structural checks on a master.
//!
//! These are errors, never warnings. A master that violates any of them is not a
//! weaker master, it is one whose indices can no longer be trusted, and every
//! consumer downstream has already been told those indices are permanent.

use std::collections::HashMap;
use std::fmt;

use crate::code::CodeShape;
use crate::model::{Master, SCHEMA_VERSION, Scope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// All violations found, so one run surfaces every problem rather than the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(pub Vec<ValidationError>);

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} validation error(s):", self.0.len())?;
        for error in &self.0 {
            writeln!(f, "  {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

pub fn validate(master: &Master, previous: Option<&Master>) -> Result<(), ValidationErrors> {
    let mut errors = Vec::new();

    if master.schema_version != SCHEMA_VERSION {
        errors.push(error(format!(
            "schema_version is {} but this tool writes {SCHEMA_VERSION}",
            master.schema_version
        )));
    }

    check_codes_unique(master, &mut errors);
    check_code_shapes(master, &mut errors);
    check_index_mapping_preserved(master, previous, &mut errors);
    check_releases(master, &mut errors);
    check_station_histories(master, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

fn error(message: String) -> ValidationError {
    ValidationError { message }
}

fn check_codes_unique(master: &Master, errors: &mut Vec<ValidationError>) {
    let mut seen: HashMap<&str, usize> = HashMap::with_capacity(master.stations.len());
    for (index, station) in master.stations.iter().enumerate() {
        if let Some(first) = seen.insert(station.code.as_str(), index) {
            errors.push(error(format!(
                "station code {:?} appears at index {first} and index {index}",
                station.code
            )));
        }
    }
}

/// Codes must have the shape their code system gives them.
///
/// Uniqueness alone is not enough. A station code that lost a leading zero on the
/// way in is unique, well formed as a string, and completely wrong: it names a
/// station that does not exist and holds an index that can never be reclaimed.
/// The same damage to a city code is quieter but still costly, because
/// `append::build_revision` carries a location forward only while the city code
/// matches, so a mangled one silently drops the coordinate it should inherit.
///
/// A master that fails this check cannot be repaired by this tool — there is no
/// repair mode, by design — but refusing to extend it is the point: every index
/// it hands out afterwards rests on identities it can no longer vouch for.
fn check_code_shapes(master: &Master, errors: &mut Vec<ValidationError>) {
    for (index, station) in master.stations.iter().enumerate() {
        if !CodeShape::Point.accepts(&station.code) {
            errors.push(error(format!(
                "index {index}: {}",
                CodeShape::Point.describe_violation(&station.code)
            )));
        }

        for revision in &station.metadata {
            let mut check = |shape: CodeShape, code: &str| {
                if !shape.accepts(code) {
                    errors.push(error(format!(
                        "station {:?} (index {index}) has, in the revision effective {}, {}",
                        station.code,
                        revision.effective_from.to_rfc3339(),
                        shape.describe_violation(code)
                    )));
                }
            };

            if let Some(region) = &revision.region {
                check(CodeShape::Region, &region.code);
            }
            if let Some(city) = &revision.city {
                check(CodeShape::City, &city.code);
            }
        }
    }
}

/// The heart of the contract: an existing index must still mean the same station.
///
/// Comparing the whole prefix in one pass covers deletion, reordering, and
/// reassignment at once, and leaves appending as the only permitted change.
fn check_index_mapping_preserved(
    master: &Master,
    previous: Option<&Master>,
    errors: &mut Vec<ValidationError>,
) {
    let Some(previous) = previous else {
        return;
    };

    if master.source_kind != previous.source_kind {
        errors.push(error(format!(
            "source_kind changed from {} to {}",
            previous.source_kind, master.source_kind
        )));
    }

    if master.stations.len() < previous.stations.len() {
        errors.push(error(format!(
            "station table shrank from {} to {} entries; stations are never removed",
            previous.stations.len(),
            master.stations.len()
        )));
    }

    for (index, before) in previous.stations.iter().enumerate() {
        match master.stations.get(index) {
            Some(after) if after.code == before.code => {}
            Some(after) => errors.push(error(format!(
                "index {index} changed from {:?} to {:?}; assigned indices are permanent",
                before.code, after.code
            ))),
            None => errors.push(error(format!(
                "index {index} ({:?}) is missing; stations are never removed",
                before.code
            ))),
        }
    }
}

fn check_releases(master: &Master, errors: &mut Vec<ValidationError>) {
    let mut seen_ids: HashMap<&str, usize> = HashMap::new();
    for (position, release) in master.releases.iter().enumerate() {
        if let Some(first) = seen_ids.insert(release.id.as_str(), position) {
            errors.push(error(format!(
                "release id {:?} appears at positions {first} and {position}",
                release.id
            )));
        }
        if release.index_count > master.stations.len() {
            errors.push(error(format!(
                "release {:?} has index_count {} but the station table holds {} entries",
                release.id,
                release.index_count,
                master.stations.len()
            )));
        }
    }

    for window in master.releases.windows(2) {
        let (before, after) = (&window[0], &window[1]);
        if after.effective_from <= before.effective_from {
            errors.push(error(format!(
                "release {:?} ({}) does not come after release {:?} ({})",
                after.id,
                after.effective_from.to_rfc3339(),
                before.id,
                before.effective_from.to_rfc3339()
            )));
        }
        if after.index_count < before.index_count {
            errors.push(error(format!(
                "release {:?} has index_count {}, below release {:?}'s {}; the index space \
                 never shrinks",
                after.id, after.index_count, before.id, before.index_count
            )));
        }
    }
}

fn check_station_histories(master: &Master, errors: &mut Vec<ValidationError>) {
    for (index, station) in master.stations.iter().enumerate() {
        let at = |what: &str| format!("station {:?} (index {index}) {what}", station.code);

        if station.metadata.is_empty() {
            errors.push(error(at("has no metadata revision")));
        }
        for window in station.metadata.windows(2) {
            if window[1].effective_from <= window[0].effective_from {
                errors.push(error(at(&format!(
                    "has metadata revisions out of order or duplicated at {}",
                    window[1].effective_from.to_rfc3339()
                ))));
            }
        }

        for revision in &station.metadata {
            let Some(location) = revision.location else {
                continue;
            };
            if !(-90.0..=90.0).contains(&location.latitude) {
                errors.push(error(at(&format!(
                    "has latitude {} outside -90..=90",
                    location.latitude
                ))));
            }
            if !(-180.0..=180.0).contains(&location.longitude) {
                errors.push(error(at(&format!(
                    "has longitude {} outside -180..=180",
                    location.longitude
                ))));
            }
            if !(location.resolution_deg.is_finite() && location.resolution_deg > 0.0) {
                errors.push(error(at(&format!(
                    "has resolution_deg {}, which must be finite and positive",
                    location.resolution_deg
                ))));
            }
        }

        for window in station.lifecycle.windows(2) {
            if window[1].effective_from <= window[0].effective_from {
                errors.push(error(at(&format!(
                    "has lifecycle events out of order or duplicated at {}",
                    window[1].effective_from.to_rfc3339()
                ))));
            }
        }

        check_scope_events(station, &at, errors);
    }
}

fn check_scope_events(
    station: &crate::model::Station,
    at: &dyn Fn(&str) -> String,
    errors: &mut Vec<ValidationError>,
) {
    for window in station.scope_events.windows(2) {
        let (before, after) = (&window[0], &window[1]);
        if (after.effective_from, after.scope) < (before.effective_from, before.scope) {
            errors.push(error(at(&format!(
                "has scope events out of order at {}",
                after.effective_from.to_rfc3339()
            ))));
        }
    }

    // Each scope carries an independent timeline, so duplication and redundancy
    // have to be judged per scope rather than across the flat list.
    for scope in [
        Scope::PointSeismicIntensity,
        Scope::LongPeriodSeismicIntensity,
    ] {
        let events: Vec<_> = station
            .scope_events
            .iter()
            .filter(|e| e.scope == scope)
            .collect();
        for window in events.windows(2) {
            if window[1].effective_from == window[0].effective_from {
                errors.push(error(at(&format!(
                    "has two `{scope}` scope events at {}",
                    window[1].effective_from.to_rfc3339()
                ))));
            }
            if window[1].enabled == window[0].enabled {
                errors.push(error(at(&format!(
                    "records `{scope}` as {} twice in a row at {}; an event must mark a change",
                    window[1].enabled,
                    window[1].effective_from.to_rfc3339()
                ))));
            }
        }
    }
}
