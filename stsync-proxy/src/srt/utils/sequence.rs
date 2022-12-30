//!
//! See https://datatracker.ietf.org/doc/html/rfc1982
use std::cmp::Ordering;
use std::fmt::{self, Binary, Display, Formatter, LowerHex, Octal, UpperHex};
use std::ops::{Add, AddAssign, Sub, SubAssign};

use crate::utils::serial;

const BITS: usize = 31;

/// A `u32` wrapping sequence number.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sequence(u32);

impl Sequence {
    /// Creates a new `Sequence` with the given initial `seq`.
    ///
    /// # Panics
    ///
    /// Panics if the given value exceeds the maximum serial value of `(1 << 31) - 1`.
    #[inline]
    pub const fn new(seq: u32) -> Self {
        assert!(seq <= (1 << BITS) - 1, "Sequence::new overflow");

        unsafe { Self::new_unchecked(seq) }
    }

    /// Creates a new `Sequence` with the given initial value without checking that it fits into
    /// the serial range.
    ///
    /// # Safety
    ///
    /// Calling this function with a value greater than `(1 << 31) -1` is undefined behavoir.
    #[inline]
    pub const unsafe fn new_unchecked(seq: u32) -> Self {
        debug_assert!(seq <= (1 << BITS) - 1, "Sequence::new_unchecked overflow");

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
        Self(serial::add::<BITS>(self.0, rhs))
    }
}

impl AddAssign<u32> for Sequence {
    #[inline]
    fn add_assign(&mut self, rhs: u32) {
        self.0 = serial::add::<BITS>(self.0, rhs);
    }
}

impl Sub for Sequence {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(serial::sub::<BITS>(self.0, rhs.0))
    }
}

impl SubAssign for Sequence {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = serial::sub::<BITS>(self.0, rhs.0);
    }
}

impl PartialEq<u32> for Sequence {
    #[inline]
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl PartialOrd for Sequence {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Sequence {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        serial::cmp::<BITS>(self.0, other.0)
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

//
// ----- impl core::fmt -----
//

impl Binary for Sequence {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Binary::fmt(&self.0, f)
    }
}

impl Display for Sequence {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl LowerHex for Sequence {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0, f)
    }
}

impl UpperHex for Sequence {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        UpperHex::fmt(&self.0, f)
    }
}

impl Octal for Sequence {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Octal::fmt(&self.0, f)
    }
}
