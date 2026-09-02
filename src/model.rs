//! Canonical JSON schema.
//!
//! The array offset of an entry in [`Master::stations`] *is* its canonical index.
//! There is deliberately no `index` field: storing it would allow the stored value
//! and the array offset to disagree.

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

/// Which input format a master was created from.
///
/// A master is pinned to one source kind for its whole life: mixing sources would
/// require reconciling differing coordinate precision and identity fields across
/// providers, which this tool deliberately does not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    StationJson,
    JmaPublic,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StationJson => "station_json",
            Self::JmaPublic => "jma_public",
        }
    }
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A capability a station can hold, tracked independently of its identity.
///
/// Being listed in the PointSeismicIntensity code table is evidence for
/// [`Scope::PointSeismicIntensity`]; it says nothing about whether the station
/// exists, which is what [`LifecycleEvent`] records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    PointSeismicIntensity,
    LongPeriodSeismicIntensity,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PointSeismicIntensity => "point_seismic_intensity",
            Self::LongPeriodSeismicIntensity => "long_period_seismic_intensity",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Jma,
    Nied,
    LocalGovernment,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub code: String,
    pub name: String,
    pub kana: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    /// Size of the quantization cell the coordinate was reported in.
    ///
    /// `35.12` from a two-decimal source means "somewhere in [35.115, 35.125)",
    /// not "exactly 35.12". See [`crate::location`].
    pub resolution_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataRevision {
    pub effective_from: DateTime<FixedOffset>,
    pub name: String,
    pub kana: Option<String>,
    pub region: Option<Area>,
    pub city: Option<Area>,
    /// `null` means "we do not know where this station is", which is always
    /// preferable to inheriting a coordinate that may belong to a different site.
    pub location: Option<Location>,
    pub provider: Provider,
    pub provider_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub effective_from: DateTime<FixedOffset>,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeEvent {
    pub effective_from: DateTime<FixedOffset>,
    pub scope: Scope,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub code: String,
    pub lifecycle: Vec<LifecycleEvent>,
    pub scope_events: Vec<ScopeEvent>,
    pub metadata: Vec<MetadataRevision>,
}

impl Station {
    pub fn new(code: String) -> Self {
        Self {
            code,
            lifecycle: Vec::new(),
            scope_events: Vec::new(),
            metadata: Vec::new(),
        }
    }

    /// The metadata revision in force at `at`, i.e. the latest one whose
    /// `effective_from <= at`.
    pub fn metadata_at(&self, at: DateTime<FixedOffset>) -> Option<&MetadataRevision> {
        self.metadata.iter().rev().find(|r| r.effective_from <= at)
    }

    pub fn is_active_at(&self, at: DateTime<FixedOffset>) -> Option<bool> {
        self.lifecycle
            .iter()
            .rev()
            .find(|e| e.effective_from <= at)
            .map(|e| e.active)
    }

    pub fn scope_enabled_at(&self, scope: Scope, at: DateTime<FixedOffset>) -> Option<bool> {
        self.scope_events
            .iter()
            .rev()
            .find(|e| e.scope == scope && e.effective_from <= at)
            .map(|e| e.enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub id: String,
    pub effective_from: DateTime<FixedOffset>,
    /// Length of the canonical station table as of this release.
    ///
    /// Not a count of active or in-scope stations: it is the number of index slots
    /// an encoder must be prepared to address. It never decreases.
    pub index_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Master {
    pub schema_version: u32,
    pub source_kind: SourceKind,
    pub generated_at: DateTime<FixedOffset>,
    pub releases: Vec<Release>,
    pub stations: Vec<Station>,
}

impl Master {
    pub fn empty(source_kind: SourceKind, generated_at: DateTime<FixedOffset>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            source_kind,
            generated_at,
            releases: Vec::new(),
            stations: Vec::new(),
        }
    }
}
