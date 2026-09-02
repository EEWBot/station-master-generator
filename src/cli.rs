use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local};
use clap::{Args, Parser, Subcommand};

use crate::append::append;
use crate::input::{Snapshot, jma_public, station_json};
use crate::model::Master;
use crate::{report, validate};

#[derive(Debug, Parser)]
#[command(
    name = "jma-station-master",
    about = "Build and extend a canonical JMA seismic intensity station master",
    long_about = "Converts JMA seismic intensity station information into a canonical JSON \
                  master. The offset of a station in the `stations` array is its permanent \
                  index; metadata is kept as a history of revisions so past events can be \
                  redrawn with the values that were in force at the time."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a new master from a snapshot.
    Init {
        #[command(flatten)]
        source: SourceArgs,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Extend an existing master with a newer snapshot.
    Update {
        /// Master to build on. Its station order is the authority for every index.
        #[arg(long, value_name = "PATH")]
        previous: PathBuf,

        #[command(flatten)]
        source: SourceArgs,
        #[command(flatten)]
        output: OutputArgs,
    },
}

#[derive(Debug, Args)]
#[group(required = true, multiple = true)]
pub struct SourceArgs {
    /// station.json master. Supplies its own release id and effective time.
    #[arg(long, value_name = "PATH", group = "source")]
    pub station_json: Option<PathBuf>,

    /// Published stations.json. Requires --code-table-xls.
    #[arg(
        long,
        value_name = "PATH",
        group = "source",
        requires = "code_table_xls"
    )]
    pub stations_json: Option<PathBuf>,

    /// 地震火山関連コード表.xls, read from sheet "24".
    #[arg(long, value_name = "PATH", requires = "stations_json")]
    pub code_table_xls: Option<PathBuf>,

    /// Release identifier. Required for the published feed; overrides
    /// station.json's own `version` when given.
    #[arg(long, value_name = "ID")]
    pub release_id: Option<String>,

    /// When these parameters took effect, as RFC 3339. Required for the published
    /// feed; overrides station.json's own `changeTime` when given.
    ///
    /// This is the time the values became authoritative, not the time the file was
    /// downloaded.
    #[arg(long, value_name = "RFC3339")]
    pub effective_from: Option<String>,
}

#[derive(Debug, Args)]
pub struct OutputArgs {
    /// Where to write the master.
    #[arg(long, value_name = "PATH")]
    pub output: PathBuf,

    /// Also write a machine-readable report here.
    #[arg(long, value_name = "PATH")]
    pub report: Option<PathBuf>,

    /// Freeze the `generated_at` stamp, so runs are byte-for-byte reproducible.
    #[arg(long, value_name = "RFC3339")]
    pub generated_at: Option<String>,
}

impl SourceArgs {
    fn effective_from(&self) -> Result<Option<DateTime<FixedOffset>>> {
        self.effective_from
            .as_deref()
            .map(|text| {
                DateTime::parse_from_rfc3339(text)
                    .with_context(|| format!("parsing --effective-from {text:?}"))
            })
            .transpose()
    }

    pub fn load(&self) -> Result<Snapshot> {
        let effective_from = self.effective_from()?;

        if let Some(path) = &self.station_json {
            return station_json::load(
                path,
                &station_json::Overrides {
                    release_id: self.release_id.clone(),
                    effective_from,
                },
            );
        }

        let (Some(stations_json), Some(code_table_xls)) =
            (&self.stations_json, &self.code_table_xls)
        else {
            bail!("either --station-json or --stations-json with --code-table-xls is required");
        };

        // The published feed carries no release stamp of its own, so the operator
        // has to supply one rather than have the tool invent a plausible date.
        let Some(release_id) = self.release_id.clone() else {
            bail!(
                "--release-id is required with --stations-json; the published feed does not carry one"
            );
        };
        let Some(effective_from) = effective_from else {
            bail!(
                "--effective-from is required with --stations-json; the published feed does not \
                 carry one, and the download time is not when the parameters took effect"
            );
        };

        jma_public::load(stations_json, code_table_xls, release_id, effective_from)
    }
}

impl OutputArgs {
    fn generated_at(&self) -> Result<DateTime<FixedOffset>> {
        match &self.generated_at {
            Some(text) => DateTime::parse_from_rfc3339(text)
                .with_context(|| format!("parsing --generated-at {text:?}")),
            None => Ok(Local::now().fixed_offset()),
        }
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let (source, output, previous_path) = match &cli.command {
        Command::Init { source, output } => (source, output, None),
        Command::Update {
            previous,
            source,
            output,
        } => (source, output, Some(previous.as_path())),
    };

    // The tool never writes over a file it was not asked to create. The intended
    // flow is to produce a new master, read the report, and promote it deliberately;
    // canonical indices are not something to overwrite on the strength of a command
    // line flag. The report is held to the same rule, because a report path aimed at
    // a master would destroy one just as thoroughly.
    refuse_existing(
        &output.output,
        "write the master to a new path and promote it once you have checked the report",
    )?;
    if let Some(report) = &output.report {
        refuse_existing(
            report,
            "give the report a path of its own; written over a master it would destroy it",
        )?;

        // The output does not exist yet, so the check above cannot catch the two
        // naming the same file. The report is written last, so it would land on the
        // master that had just been produced.
        if absolute(report)? == absolute(&output.output)? {
            bail!(
                "--report and --output are the same path ({})\n  \
                 the report would be written over the master",
                output.output.display()
            );
        }
    }

    let previous = previous_path.map(read_master).transpose()?;
    let snapshot = source.load()?;

    let (master, summary) = append(previous.as_ref(), &snapshot, output.generated_at()?)?;
    validate::validate(&master, previous.as_ref())?;

    write_master(&master, &output.output)?;
    if let Some(path) = &output.report {
        report::write_json(&summary, path)?;
    }

    eprint!("{}", report::to_text(&summary));
    Ok(())
}

/// Refuse a path that is already taken, with advice for the file that was bound
/// for it.
fn refuse_existing(path: &Path, advice: &str) -> Result<()> {
    if path.exists() {
        bail!("{} already exists\n  {advice}", path.display());
    }
    Ok(())
}

/// Absolute form of `path`, for comparing two paths that need not both exist.
///
/// Symlinks are not resolved, so this is one layer of the defence rather than the
/// whole of it; the existence checks and the report's own `create_new` cover what
/// it cannot see.
fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).with_context(|| format!("resolving {}", path.display()))
}

fn read_master(path: &Path) -> Result<Master> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading master at {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing master at {}", path.display()))
}

/// Write through a temporary file so a failure part-way cannot truncate a master.
///
/// The temporary lives beside the target, keeping the final move within one
/// filesystem. `rename` replaces an existing file rather than the target being
/// unlinked first, so there is no moment where the previous master is gone and
/// the new one is not yet in place.
fn write_master(master: &Master, path: &Path) -> Result<()> {
    let mut text = serde_json::to_string_pretty(master).context("serializing master")?;
    text.push('\n');

    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, &text)
        .with_context(|| format!("writing {}", temporary.display()))?;

    std::fs::rename(&temporary, path)
        .inspect_err(|_| {
            // Do not leave a stray temporary behind when the move fails.
            let _ = std::fs::remove_file(&temporary);
        })
        .with_context(|| format!("moving {} into place", temporary.display()))?;
    Ok(())
}
