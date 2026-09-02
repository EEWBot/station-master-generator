//! The shape of the codes that identify a station.
//!
//! A code is not just a label: the offset a code receives in
//! [`crate::model::Master::stations`] is permanent, so a code that arrives one
//! character short is not a typo to tolerate, it is a brand new station that will
//! burn an index forever. Every boundary that admits a code therefore checks its
//! shape, and the wording of the complaint lives here so all of them say the same
//! thing.

/// Which code column a value came from, and therefore what shape it must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeShape {
    /// AreaForecastLocalE, three digits. The published workbook stores these as
    /// numbers, which is harmless: there is no leading zero to lose.
    Region,
    /// AreaInformationCity, seven digits, zero padded.
    City,
    /// PointSeismicIntensity, seven digits, zero padded. This is the station
    /// identity that indices are assigned against.
    Point,
}

impl CodeShape {
    pub fn width(self) -> usize {
        match self {
            Self::Region => 3,
            Self::City | Self::Point => 7,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Region => "region code",
            Self::City => "city code",
            Self::Point => "station code",
        }
    }

    /// Exactly [`Self::width`] ASCII digits.
    ///
    /// Byte length is character length here, because a value that passes is
    /// ASCII by construction and one that is not fails on the digit test anyway.
    pub fn accepts(self, value: &str) -> bool {
        value.len() == self.width() && value.bytes().all(|b| b.is_ascii_digit())
    }

    /// How a value that [`Self::accepts`] rejected should be reported.
    pub fn describe_violation(self, value: &str) -> String {
        let mut message = format!(
            "{} {value:?} is not {} ASCII digits",
            self.label(),
            self.width()
        );
        // The overwhelmingly likely cause, and the one that matters: a code one
        // digit short is a different code, not a malformed one.
        if matches!(self, Self::City | Self::Point)
            && value.len() < self.width()
            && value.bytes().all(|b| b.is_ascii_digit())
        {
            message.push_str("; a leading zero was probably lost");
        }
        message
    }
}

#[cfg(test)]
mod tests {
    use super::CodeShape;

    #[test]
    fn well_formed_codes_are_accepted() {
        assert!(CodeShape::Point.accepts("0999100"));
        assert!(CodeShape::City.accepts("0999100"));
        assert!(CodeShape::Region.accepts("900"));
        assert!(CodeShape::Point.accepts("0000000"));
    }

    #[test]
    fn a_seven_digit_code_is_not_a_region_code() {
        assert!(!CodeShape::Region.accepts("0999100"));
        assert!(!CodeShape::Point.accepts("900"));
    }

    #[test]
    fn wrong_width_or_non_digits_are_rejected() {
        // The case this whole module exists for: a lost leading zero.
        assert!(!CodeShape::Point.accepts("999100"));
        assert!(!CodeShape::Point.accepts("09991000"));
        assert!(!CodeShape::Point.accepts("099910x"));
        assert!(!CodeShape::Point.accepts(""));
        assert!(!CodeShape::Region.accepts("90"));
        // Full-width digits are not ASCII digits, and are three times as long in
        // bytes, so they fail on both counts.
        assert!(!CodeShape::Point.accepts("０９９９１００"));
    }

    #[test]
    fn a_short_numeric_code_is_reported_as_a_lost_zero() {
        let message = CodeShape::Point.describe_violation("999100");
        assert!(message.contains("station code"), "{message}");
        assert!(message.contains("7 ASCII digits"), "{message}");
        assert!(message.contains("leading zero"), "{message}");

        // Nothing to blame on a leading zero when the value is not a number, or
        // when it is too long rather than too short.
        assert!(
            !CodeShape::Point
                .describe_violation("099910x")
                .contains("leading zero")
        );
        assert!(
            !CodeShape::Point
                .describe_violation("09991000")
                .contains("leading zero")
        );
        assert!(
            !CodeShape::Region
                .describe_violation("90")
                .contains("leading zero")
        );
    }
}
