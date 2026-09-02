//! Build a canonical JMA seismic intensity station master.
//!
//! The offset of a station in [`model::Master::stations`] is its canonical index,
//! and that mapping is permanent: consumers encode station readings positionally,
//! so an index that moves silently corrupts every message that uses it. Metadata
//! is stored as a history of revisions rather than current values, so a past event
//! can be redrawn with the station names and coordinates that were in force when
//! it happened.
//!
//! Input formats are normalized by [`input`] into a [`input::Snapshot`] before
//! [`append`] sees them, so the append rules stay independent of any one feed.

pub mod append;
pub mod cli;
pub mod input;
pub mod kana;
pub mod location;
pub mod model;
pub mod report;
pub mod validate;
