//! Rendering an append outcome for a human and for a machine.
//!
//! Nothing in this tool refuses a snapshot on the grounds that it looks drastic,
//! so this report is the whole detection mechanism. It always states how scope
//! membership moved, even when it did not, and it names every station that left.

use anyhow::{Context, Result};
use serde::Serialize;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use crate::append::AppendReport;

/// Column widths for the summary, chosen so the longest label still leaves a gap
/// and every figure lines up when the report is scanned.
const LABEL_WIDTH: usize = 24;
const VALUE_WIDTH: usize = 6;

/// How many departed codes the text summary names before summarizing. The JSON
/// report always carries the full list.
const MAX_LISTED_DEPARTURES: usize = 10;

pub fn to_text(report: &AppendReport) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "Release: {}", report.release_id);
    let _ = writeln!(out, "Effective: {}", report.effective_from.to_rfc3339());
    let _ = writeln!(out, "Source: {}", report.source_kind);

    let _ = writeln!(out, "\nStations:");
    row(&mut out, "existing", report.stations_existing);
    row(&mut out, "appended", report.stations_appended);
    row(&mut out, "total", report.stations_total);

    let _ = writeln!(out, "\nMetadata:");
    row(&mut out, "unchanged", report.metadata_unchanged);
    row(&mut out, "revised", report.metadata_revised);
    row(&mut out, "unresolved (active)", report.unresolved_active);
    if let Some(in_scope) = report.unresolved_in_scope {
        row(&mut out, "unresolved (in scope)", in_scope);
    }

    let _ = writeln!(out, "\nLifecycle:");
    row(&mut out, "activated", report.lifecycle_activated);
    row(&mut out, "deactivated", report.lifecycle_deactivated);

    for (scope, summary) in &report.scopes {
        let _ = writeln!(out, "\nScope ({scope}):");
        // Signed, so a release that empties the scope is obvious at a glance.
        signed_row(&mut out, "enabled", summary.enabled as i64);
        signed_row(&mut out, "disabled", -(summary.disabled as i64));
        for code in summary.departed.iter().take(MAX_LISTED_DEPARTURES) {
            let _ = writeln!(out, "    {code}");
        }
        if summary.departed.len() > MAX_LISTED_DEPARTURES {
            let _ = writeln!(
                out,
                "    ... and {} more",
                summary.departed.len() - MAX_LISTED_DEPARTURES
            );
        }
    }

    if !report.warnings.is_empty() {
        let _ = writeln!(out, "\nWarnings:");
        for warning in &report.warnings {
            let _ = writeln!(out, "  {warning}");
        }
    }

    out
}

fn row(out: &mut String, label: &str, value: usize) {
    let _ = writeln!(
        out,
        "  {:<LABEL_WIDTH$}{:>VALUE_WIDTH$}",
        format!("{label}:"),
        value
    );
}

fn signed_row(out: &mut String, label: &str, value: i64) {
    // "no movement" reads better as a bare zero than as a signed one.
    let rendered = if value == 0 {
        "0".to_owned()
    } else {
        format!("{value:+}")
    };
    let _ = writeln!(
        out,
        "  {:<LABEL_WIDTH$}{:>VALUE_WIDTH$}",
        format!("{label}:"),
        rendered
    );
}

#[derive(Debug, Serialize)]
struct JsonWarning<'a> {
    code: &'a str,
    subject: Option<&'a str>,
    message: &'a str,
}

#[derive(Debug, Serialize)]
struct JsonScope<'a> {
    scope: &'static str,
    enabled: usize,
    disabled: usize,
    /// Every code that left the scope, so the change can be audited in full.
    departed: &'a [String],
}

#[derive(Debug, Serialize)]
struct JsonReport<'a> {
    release_id: &'a str,
    effective_from: String,
    source_kind: &'static str,
    stations_existing: usize,
    stations_appended: usize,
    stations_total: usize,
    metadata_unchanged: usize,
    metadata_revised: usize,
    unresolved_active: usize,
    unresolved_in_scope: Option<usize>,
    lifecycle_activated: usize,
    lifecycle_deactivated: usize,
    scopes: Vec<JsonScope<'a>>,
    warnings: Vec<JsonWarning<'a>>,
}

pub fn to_json(report: &AppendReport) -> String {
    let json = JsonReport {
        release_id: &report.release_id,
        effective_from: report.effective_from.to_rfc3339(),
        source_kind: report.source_kind.as_str(),
        stations_existing: report.stations_existing,
        stations_appended: report.stations_appended,
        stations_total: report.stations_total,
        metadata_unchanged: report.metadata_unchanged,
        metadata_revised: report.metadata_revised,
        unresolved_active: report.unresolved_active,
        unresolved_in_scope: report.unresolved_in_scope,
        lifecycle_activated: report.lifecycle_activated,
        lifecycle_deactivated: report.lifecycle_deactivated,
        scopes: report
            .scopes
            .iter()
            .map(|(scope, summary)| JsonScope {
                scope: scope.as_str(),
                enabled: summary.enabled,
                disabled: summary.disabled,
                departed: &summary.departed,
            })
            .collect(),
        warnings: report
            .warnings
            .iter()
            .map(|w| JsonWarning {
                code: w.code,
                subject: w.subject.as_deref(),
                message: &w.message,
            })
            .collect(),
    };

    let mut text = serde_json::to_string_pretty(&json).expect("report is always serializable");
    text.push('\n');
    text
}

/// Write the report, refusing a path that is already taken.
///
/// The caller checks the path before doing any work, so this is the second layer:
/// it closes the window between that check and this write, and it holds for any
/// other caller that has not checked at all. A report aimed at a master would
/// destroy it, so the refusal belongs at the write itself as well.
pub fn write_json(report: &AppendReport, path: &Path) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating report at {}", path.display()))?;

    file.write_all(to_json(report).as_bytes())
        .with_context(|| format!("writing report to {}", path.display()))
}
