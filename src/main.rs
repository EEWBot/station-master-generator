use anyhow::Result;
use clap::Parser;

use jma_station_master::cli::{Cli, run};

fn main() -> Result<()> {
    run(Cli::parse())
}
