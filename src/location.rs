//! Comparing coordinates that were reported at different precisions.
//!
//! A coordinate is not a point but a quantization cell: `35.12` from a
//! two-decimal source means the true latitude lies in `[35.115, 35.125)`. Treating
//! it as a point makes every precision change look like a relocation, which would
//! spray bogus metadata revisions across the history.

use crate::model::Location;

/// Slack for the half-open cell test.
///
/// `35.12 - 0.01 / 2` is not exactly `35.115` in binary floating point, so the
/// bounds need a tolerance far smaller than any real coordinate difference.
const EPS: f64 = 1e-9;

fn axis_contains(center: f64, resolution: f64, value: f64) -> bool {
    let half = resolution / 2.0;
    value >= center - half - EPS && value < center + half - EPS
}

/// Whether `fine` falls inside the quantization cell `coarse` denotes.
fn cell_contains(coarse: &Location, fine: &Location) -> bool {
    axis_contains(coarse.latitude, coarse.resolution_deg, fine.latitude)
        && axis_contains(coarse.longitude, coarse.resolution_deg, fine.longitude)
}

/// Whether two coordinates can denote the same physical site.
///
/// The coarser reading is treated as the cell and the finer one as the point, so
/// `35.1234 / 0.0001` and `35.12 / 0.01` are the same place, while a genuine move
/// out of the cell is not.
pub fn same_point(a: &Location, b: &Location) -> bool {
    let (coarse, fine) = if a.resolution_deg >= b.resolution_deg {
        (a, b)
    } else {
        (b, a)
    };
    cell_contains(coarse, fine)
}

/// Whether two possibly-absent readings describe the same place.
///
/// A finer reading of a site already on record is not a change: the station has
/// not moved, only the publisher's precision has. Since a revision is written only
/// when something about the station actually changed, the coordinate first
/// recorded for a site stays until the site really moves.
pub fn same_place(a: Option<&Location>, b: Option<&Location>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_point(a, b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{same_place, same_point};
    use crate::model::Location;

    fn loc(latitude: f64, longitude: f64, resolution_deg: f64) -> Location {
        Location {
            latitude,
            longitude,
            resolution_deg,
        }
    }

    #[test]
    fn coarse_cell_contains_fine_coordinate() {
        let coarse = loc(35.12, 135.68, 0.01);
        let fine = loc(35.1234, 135.6789, 0.0001);
        assert!(same_point(&coarse, &fine));
        assert!(same_point(&fine, &coarse));
    }

    #[test]
    fn cell_is_half_open() {
        let coarse = loc(35.12, 135.68, 0.01);
        // Lower bound is inside the cell.
        assert!(same_point(&coarse, &loc(35.115, 135.68, 0.0001)));
        // Upper bound belongs to the next cell.
        assert!(!same_point(&coarse, &loc(35.125, 135.68, 0.0001)));
    }

    #[test]
    fn longitude_is_checked_too() {
        let coarse = loc(35.12, 135.68, 0.01);
        assert!(!same_point(&coarse, &loc(35.1234, 135.6889, 0.0001)));
    }

    #[test]
    fn a_real_move_leaves_the_cell() {
        let before = loc(35.1234, 135.6789, 0.0001);
        let after = loc(35.2300, 135.4100, 0.0001);
        assert!(!same_point(&before, &after));
    }

    #[test]
    fn identical_fine_coordinates_are_the_same_point() {
        let a = loc(35.1234, 135.6789, 0.0001);
        assert!(same_point(&a, &a));
    }

    #[test]
    fn adjacent_fine_coordinates_are_different_points() {
        let a = loc(35.1234, 135.6789, 0.0001);
        let b = loc(35.1235, 135.6789, 0.0001);
        assert!(!same_point(&a, &b));
    }

    #[test]
    fn a_finer_reading_of_the_same_site_is_the_same_place() {
        // Only the publisher's precision changed, so there is nothing to record.
        let fine = loc(35.1234, 135.6789, 0.0001);
        let coarse = loc(35.12, 135.68, 0.01);
        assert!(same_place(Some(&fine), Some(&coarse)));
        assert!(same_place(Some(&coarse), Some(&fine)));
    }

    #[test]
    fn a_missing_reading_is_not_the_same_as_a_known_one() {
        let fine = loc(35.1234, 135.6789, 0.0001);
        assert!(same_place(None, None));
        assert!(!same_place(Some(&fine), None));
        assert!(!same_place(None, Some(&fine)));
    }

    #[test]
    fn a_move_is_not_the_same_place() {
        let before = loc(35.1234, 135.6789, 0.0001);
        let after = loc(35.2300, 135.4100, 0.0001);
        assert!(!same_place(Some(&before), Some(&after)));
    }
}
