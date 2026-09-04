//! Appending one release to a master.
//!
//! This is the whole write path, and it only ever grows things:
//!
//! * a station's position in [`Master::stations`] is a permanent contract with the
//!   protobuf encoder, so the existing prefix is never reordered, removed from, or
//!   reassigned — new codes go on the end;
//! * a release must be newer than every release already recorded, and nothing
//!   already written is ever revisited.
//!
//! The second rule is what makes the first affordable. A revision is written only
//! when something actually changed, so a state that persists across releases is
//! represented by its *absence*. Inserting or rewriting a release inside that
//! representation would silently re-date every state that follows it, so a release
//! that is not strictly newer is rejected rather than merged.
//!
//! Both rules assume the snapshot is self-consistent, so that much is checked
//! first: whatever adapter produced it, a snapshot may name each station code
//! only once.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Result, bail};
use chrono::{DateTime, FixedOffset};

use crate::input::{Snapshot, StationMetadata, Warning};
use crate::model::{
    LifecycleEvent, Master, MetadataRevision, Provider, Release, Scope, ScopeEvent, SourceKind,
    Station,
};

/// What one release did to a master.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendReport {
    pub release_id: String,
    pub effective_from: DateTime<FixedOffset>,
    pub source_kind: SourceKind,
    pub stations_existing: usize,
    pub stations_appended: usize,
    pub stations_total: usize,
    pub metadata_unchanged: usize,
    pub metadata_revised: usize,
    /// Active stations with no known location at this release.
    pub unresolved_active: usize,
    /// Active *and* in PointSeismicIntensity scope; `None` when the master tracks
    /// no scopes at all, where the figure would be meaningless.
    pub unresolved_in_scope: Option<usize>,
    pub lifecycle_activated: usize,
    pub lifecycle_deactivated: usize,
    pub scopes: BTreeMap<Scope, ScopeSummary>,
    pub warnings: Vec<Warning>,
}

/// Movement in and out of one scope during a release.
///
/// Nothing stops a truncated code table from retiring thousands of stations at
/// once, so this is the audit trail: the departed codes are recorded in full for
/// the operator to check before promoting the result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeSummary {
    pub enabled: usize,
    pub disabled: usize,
    /// Codes that left the scope, in code order.
    pub departed: Vec<String>,
}

/// Fold a snapshot into `previous`, or build a fresh master when there is none.
pub fn append(
    previous: Option<&Master>,
    snapshot: &Snapshot,
    generated_at: DateTime<FixedOffset>,
) -> Result<(Master, AppendReport)> {
    check_snapshot(snapshot)?;
    if let Some(previous) = previous {
        check_can_append(previous, snapshot)?;
    }

    let t = snapshot.effective_from;
    let mut master = match previous {
        Some(previous) => previous.clone(),
        None => Master::empty(snapshot.source_kind, generated_at),
    };
    master.generated_at = generated_at;

    let mut outcome = Outcome::default();
    assign_indices(&mut master, snapshot, &mut outcome);
    let index_of = index_of(&master);

    for incoming in &snapshot.stations {
        let station = &mut master.stations[index_of[incoming.code.as_str()]];
        apply_metadata(station, &incoming.metadata, t, &mut outcome);
        apply_lifecycle(station, incoming.active, t, &mut outcome);
        enter_scopes(station, &incoming.scopes, t, &mut outcome);
    }

    leave_scopes(&mut master, snapshot, t, &mut outcome);

    // Every index that exists is addressable, whether or not this release's
    // stations use it, so the table length is what an encoder must allocate.
    master.releases.push(Release {
        id: snapshot.release_id.clone(),
        effective_from: t,
        index_count: master.stations.len(),
    });

    let report = outcome.into_report(&master, snapshot, t);
    Ok((master, report))
}

/// Hold a snapshot to what one release can honestly say, before any of it is
/// believed.
///
/// A snapshot speaks about each station once. [`assign_indices`] collects unseen
/// codes into a set, so a code listed twice quietly becomes one index while
/// `stations_existing` is still counted against the item total — the report then
/// claims a station already existed that never did. Identical duplicates end
/// there, silently; conflicting ones push two revisions at the same instant and
/// are caught only by `validate`, one layer too late. Both are a broken input, so
/// the input is refused rather than reconciled.
fn check_snapshot(snapshot: &Snapshot) -> Result<()> {
    let mut seen: HashMap<&str, usize> = HashMap::with_capacity(snapshot.stations.len());
    for (index, station) in snapshot.stations.iter().enumerate() {
        if let Some(first) = seen.insert(station.code.as_str(), index) {
            bail!(
                "release {}: duplicate station code {:?} (entries {} and {})\n  \
                 a snapshot describes each station once; two entries for one code \
                 cannot both be recorded at the same instant",
                snapshot.release_id,
                station.code,
                first + 1,
                index + 1
            );
        }
    }
    Ok(())
}

fn check_can_append(previous: &Master, snapshot: &Snapshot) -> Result<()> {
    if previous.source_kind != snapshot.source_kind {
        bail!(
            "source kind mismatch\n  \
             the master was created from `{}` but this run supplies `{}`\n  \
             a master may only be updated from the source kind it was created with, \
             because the two feeds differ in coordinate precision and in what they \
             can attest about a station",
            previous.source_kind,
            snapshot.source_kind
        );
    }

    if let Some(existing) = previous
        .releases
        .iter()
        .find(|r| r.id == snapshot.release_id)
    {
        bail!(
            "release {} is already recorded (effective {})\n  \
             a release is written once and never revisited; if this snapshot is a \
             correction, discard the master you were about to produce and rebuild it \
             from the corrected inputs",
            existing.id,
            existing.effective_from.to_rfc3339()
        );
    }

    // Releases are strictly increasing and only ever appended, so the last one is
    // the newest.
    if let Some(latest) = previous.releases.last()
        && snapshot.effective_from <= latest.effective_from
    {
        bail!(
            "out-of-order release\n  \
             release {} is effective {}, which is not after the latest recorded release \
             ({}, effective {})\n  \
             releases must be applied oldest first; an unchanged state is stored as the \
             absence of a revision, so inserting an older release would re-date every \
             state that follows it",
            snapshot.release_id,
            snapshot.effective_from.to_rfc3339(),
            latest.id,
            latest.effective_from.to_rfc3339()
        );
    }

    Ok(())
}

/// Give every unseen code an index, at the end of the table.
///
/// New codes are appended in code order rather than in the order the adapter
/// happened to emit them, so the index a station receives does not depend on how a
/// spreadsheet was sorted.
fn assign_indices(master: &mut Master, snapshot: &Snapshot, outcome: &mut Outcome) {
    let known: BTreeSet<&str> = master.stations.iter().map(|s| s.code.as_str()).collect();
    let unknown: BTreeSet<String> = snapshot
        .stations
        .iter()
        .filter(|s| !known.contains(s.code.as_str()))
        .map(|s| s.code.clone())
        .collect();

    outcome.stations_existing = snapshot.stations.len() - unknown.len();
    outcome.stations_appended = unknown.len();

    for code in unknown {
        master.stations.push(Station::new(code));
    }
}

/// The index mapping, read from the order of the table itself.
fn index_of(master: &Master) -> HashMap<String, usize> {
    master
        .stations
        .iter()
        .enumerate()
        .map(|(index, station)| (station.code.clone(), index))
        .collect()
}

#[derive(Debug, Default)]
struct Outcome {
    stations_existing: usize,
    stations_appended: usize,
    metadata_unchanged: usize,
    metadata_revised: usize,
    lifecycle_activated: usize,
    lifecycle_deactivated: usize,
    scopes: BTreeMap<Scope, ScopeSummary>,
}

impl Outcome {
    fn into_report(
        self,
        master: &Master,
        snapshot: &Snapshot,
        t: DateTime<FixedOffset>,
    ) -> AppendReport {
        let tracks_scopes = master.stations.iter().any(|s| !s.scope_events.is_empty());
        let mut unresolved_active = 0usize;
        let mut unresolved_in_scope = 0usize;

        for station in &master.stations {
            // A station with no lifecycle evidence at all is not yet claimed to
            // exist, so it is not counted as a gap in our knowledge.
            if station.is_active_at(t) != Some(true) {
                continue;
            }
            if station.metadata_at(t).is_some_and(|r| r.location.is_none()) {
                unresolved_active += 1;
                if station.scope_enabled_at(Scope::PointSeismicIntensity, t) == Some(true) {
                    unresolved_in_scope += 1;
                }
            }
        }

        // Report a scope this snapshot enumerates even when nothing moved, so a
        // quiet release still shows the line an anomalous one would disturb.
        let mut scopes = self.scopes;
        for scope in &snapshot.complete_scopes {
            scopes.entry(*scope).or_default();
        }
        // Departures are collected in index order; code order is what a reader
        // scanning the audit trail expects.
        for summary in scopes.values_mut() {
            summary.departed.sort();
        }

        AppendReport {
            release_id: snapshot.release_id.clone(),
            effective_from: t,
            source_kind: snapshot.source_kind,
            stations_existing: self.stations_existing,
            stations_appended: self.stations_appended,
            stations_total: master.stations.len(),
            metadata_unchanged: self.metadata_unchanged,
            metadata_revised: self.metadata_revised,
            unresolved_active,
            unresolved_in_scope: tracks_scopes.then_some(unresolved_in_scope),
            lifecycle_activated: self.lifecycle_activated,
            lifecycle_deactivated: self.lifecycle_deactivated,
            scopes,
            warnings: snapshot.warnings.clone(),
        }
    }
}

/// Record this release's metadata, but only if it says something new.
fn apply_metadata(
    station: &mut Station,
    incoming: &StationMetadata,
    t: DateTime<FixedOffset>,
    outcome: &mut Outcome,
) {
    let base = station.metadata.last().cloned();
    let candidate = build_revision(base.as_ref(), incoming, t);

    if base.is_some_and(|base| same_metadata(&base, &candidate)) {
        outcome.metadata_unchanged += 1;
    } else {
        outcome.metadata_revised += 1;
        station.metadata.push(candidate);
    }
}

fn build_revision(
    base: Option<&MetadataRevision>,
    incoming: &StationMetadata,
    t: DateTime<FixedOffset>,
) -> MetadataRevision {
    // Whether this is recognisably the same station as before. Once the name or the
    // municipality moves, anything we knew but were not told again may describe the
    // old site, so it must not be carried across.
    let still_the_same_station = base.is_some_and(|base| {
        base.name == incoming.name
            && base.city.as_ref().map(|c| &c.code) == incoming.city.as_ref().map(|c| &c.code)
    });

    // A snapshot that carries no coordinate has failed to resolve one, not moved
    // the station to nowhere. Inheriting the old reading is safe only while the
    // station is still recognisably itself.
    let location = match incoming.location {
        Some(fresh) => Some(fresh),
        None => base
            .and_then(|base| base.location)
            .filter(|_| still_the_same_station),
    };

    // A provider of `unknown` with no accompanying wording is the absence of
    // evidence, not a claim that the operator changed, and is carried forward on the
    // same terms as a missing coordinate. An `unknown` that does carry wording came
    // from an operator name we simply do not recognize, which is a real observation.
    let incoming_knows_provider =
        incoming.provider != Provider::Unknown || incoming.provider_detail.is_some();
    let (provider, provider_detail) = match base {
        Some(base) if still_the_same_station && !incoming_knows_provider => {
            (base.provider, base.provider_detail.clone())
        }
        _ => (incoming.provider, incoming.provider_detail.clone()),
    };

    MetadataRevision {
        effective_from: t,
        name: incoming.name.clone(),
        kana: incoming.kana.clone(),
        region: incoming.region.clone(),
        city: incoming.city.clone(),
        location,
        provider,
        provider_detail,
    }
}

/// Whether a revision would tell a reader anything the previous one did not.
fn same_metadata(a: &MetadataRevision, b: &MetadataRevision) -> bool {
    a.name == b.name
        && a.kana == b.kana
        && a.region == b.region
        && a.city == b.city
        && a.provider == b.provider
        && a.provider_detail == b.provider_detail
        && a.location == b.location
}

fn apply_lifecycle(
    station: &mut Station,
    incoming: Option<bool>,
    t: DateTime<FixedOffset>,
    outcome: &mut Outcome,
) {
    let Some(active) = incoming else {
        return;
    };
    if station.lifecycle.last().map(|e| e.active) == Some(active) {
        return;
    }
    station.lifecycle.push(LifecycleEvent {
        effective_from: t,
        active,
    });
    if active {
        outcome.lifecycle_activated += 1;
    } else {
        outcome.lifecycle_deactivated += 1;
    }
}

fn enter_scopes(
    station: &mut Station,
    scopes: &[Scope],
    t: DateTime<FixedOffset>,
    outcome: &mut Outcome,
) {
    // Emitting in scope order keeps `scope_events` sorted by `(effective_from,
    // scope)` without a separate pass, since `t` is always the newest instant.
    for scope in sorted(scopes) {
        if station.scope_enabled_at(scope, t) == Some(true) {
            continue;
        }
        station.scope_events.push(ScopeEvent {
            effective_from: t,
            scope,
            enabled: true,
        });
        outcome.scopes.entry(scope).or_default().enabled += 1;
    }
}

/// Retire scope membership for stations the snapshot no longer lists.
///
/// Only a snapshot that enumerates a scope exhaustively may do this, and it
/// touches scope alone: a station dropping out of the code table has stopped being
/// reportable, which is not the same as having been dismantled.
fn leave_scopes(
    master: &mut Master,
    snapshot: &Snapshot,
    t: DateTime<FixedOffset>,
    outcome: &mut Outcome,
) {
    if snapshot.complete_scopes.is_empty() {
        return;
    }
    let present: BTreeSet<&str> = snapshot.stations.iter().map(|s| s.code.as_str()).collect();
    let scopes = sorted(&snapshot.complete_scopes);

    for station in &mut master.stations {
        if present.contains(station.code.as_str()) {
            continue;
        }
        for scope in scopes.iter().copied() {
            if station.scope_enabled_at(scope, t) != Some(true) {
                continue;
            }
            station.scope_events.push(ScopeEvent {
                effective_from: t,
                scope,
                enabled: false,
            });
            let summary = outcome.scopes.entry(scope).or_default();
            summary.disabled += 1;
            summary.departed.push(station.code.clone());
        }
    }
}

fn sorted(scopes: &[Scope]) -> Vec<Scope> {
    let mut scopes = scopes.to_vec();
    scopes.sort_unstable();
    scopes.dedup();
    scopes
}
