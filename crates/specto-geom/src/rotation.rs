use std::ops::Add;

use serde::{Deserialize, Serialize};

/// Clockwise page rotation for display, in quarter turns.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Rotation {
    /// Interprets a `/Rotate` value. The specification requires a multiple of
    /// 90. Anything else is treated as 0, as other viewers do. Negative values are allowed.
    pub fn from_degrees(degrees: i64) -> Self {
        if degrees % 90 != 0 {
            return Self::None;
        }
        Self::from_quarter_turns(degrees / 90)
    }

    pub fn from_quarter_turns(turns: i64) -> Self {
        match turns.rem_euclid(4) {
            0 => Self::None,
            1 => Self::Cw90,
            2 => Self::Cw180,
            _ => Self::Cw270,
        }
    }

    pub fn quarter_turns(self) -> i64 {
        self as i64
    }

    pub fn degrees(self) -> i64 {
        self.quarter_turns() * 90
    }

    /// Whether width and height trade places.
    pub fn swaps_axes(self) -> bool {
        matches!(self, Self::Cw90 | Self::Cw270)
    }
}

impl Add for Rotation {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::from_quarter_turns(self.quarter_turns() + rhs.quarter_turns())
    }
}

#[cfg(test)]
mod tests {
    use super::Rotation;

    #[test]
    fn from_degrees() {
        assert_eq!(Rotation::from_degrees(0), Rotation::None);
        assert_eq!(Rotation::from_degrees(90), Rotation::Cw90);
        assert_eq!(Rotation::from_degrees(-90), Rotation::Cw270);
        assert_eq!(Rotation::from_degrees(450), Rotation::Cw90);
        assert_eq!(Rotation::from_degrees(45), Rotation::None);
    }
}
