use std::cmp::Ordering;
use std::fmt::{self, Display, Formatter};
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// The maximum range that should be checked for wrapping.
const WRAP_THRESHOLD: u32 = 1024;

/// A `u32` wrapping sequence number.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sequence(u32);

impl Sequence {
    /// Creates a new `Sequence` with the given initial `seq`.
    #[inline]
    pub const fn new(seq: u32) -> Self {
        Self(seq)
    }

    /// Returns the current value of the `Sequence`.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Sets the current value of the `Sequence`.
    #[inline]
    pub fn set(&mut self, val: u32) {
        self.0 = val;
    }
}

impl Add<u32> for Sequence {
    type Output = Self;

    #[inline]
    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0.wrapping_add(rhs))
    }
}

impl AddAssign<u32> for Sequence {
    #[inline]
    fn add_assign(&mut self, rhs: u32) {
        self.0 = self.0.wrapping_add(rhs);
    }
}

impl Sub for Sequence {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl SubAssign for Sequence {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_sub(rhs.0);
    }
}

impl PartialEq<u32> for Sequence {
    #[inline]
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl PartialOrd for Sequence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let mut lhs = self.0;
        let mut rhs = other.0;

        // If either number has wrapped around, we attempt to wrap around both numbers.
        if lhs.checked_add(WRAP_THRESHOLD).is_some() || rhs.checked_add(WRAP_THRESHOLD).is_some() {
            lhs = lhs.wrapping_add(WRAP_THRESHOLD);
            rhs = rhs.wrapping_add(WRAP_THRESHOLD);
        }

        lhs.partial_cmp(&rhs)
    }
}

impl Ord for Sequence {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut lhs = self.0;
        let mut rhs = other.0;

        // If either number has wrapped around, we attempt to wrap around both numbers.
        if lhs.checked_add(WRAP_THRESHOLD).is_some() || rhs.checked_add(WRAP_THRESHOLD).is_some() {
            lhs = lhs.wrapping_add(WRAP_THRESHOLD);
            rhs = rhs.wrapping_add(WRAP_THRESHOLD);
        }

        lhs.cmp(&rhs)
    }
}

impl PartialOrd<u32> for Sequence {
    #[inline]
    fn partial_cmp(&self, other: &u32) -> Option<Ordering> {
        self.partial_cmp(&Self(*other))
    }
}

impl From<Sequence> for u32 {
    #[inline]
    fn from(src: Sequence) -> Self {
        src.0
    }
}

impl From<u32> for Sequence {
    #[inline]
    fn from(src: u32) -> Self {
        Self(src)
    }
}

impl Display for Sequence {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::Sequence;

    #[test]
    fn test_sequence() {
        let mut seq = Sequence::new(0);
        assert_eq!(seq.0, 0);

        seq += 1;
        assert_eq!(seq.0, 1);

        seq += u32::MAX - 1;
        assert_eq!(seq.0, u32::MAX);

        seq += 255;
        assert_eq!(seq.0, 254);
    }

    #[test]
    fn test_sequence_partial_cmp() {
        let mut seq = Sequence::new(0);

        assert!(seq == 0);
        assert!(seq < 1);

        seq += 2000;

        assert!(seq > 1999);
        assert!(seq == 2000);
        assert!(seq < 2001);

        let mut seq = Sequence::new(u32::MAX);

        assert!(seq == u32::MAX);
        assert!(seq > u32::MAX - 1);
        assert!(seq < 0);

        seq += 1;

        assert!(seq == 0);
        assert!(seq > u32::MAX);
        assert!(seq < 1);
    }
}
