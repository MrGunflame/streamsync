//!
//! See https://datatracker.ietf.org/doc/html/rfc1982
use std::cmp::Ordering;
use std::fmt::{self, Binary, Display, Formatter, LowerHex, Octal, UpperHex};
use std::ops::{Add, AddAssign};

use crate::utils::serial;

const BITS: usize = 26;

/// A 26-bit message number.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MessageNumber(u32);

impl MessageNumber {
    /// Creates a new `MessageNumber` with the given initial value.
    ///
    /// # Panics
    ///
    /// Panics if the given value exceeds the maximum value of `(1 << 26) -1`.
    #[inline]
    pub const fn new(num: u32) -> Self {
        assert!(num <= (1 << BITS) - 1, "MessageNumber::new overflow");

        unsafe { Self::new_unchecked(num) }
    }

    /// Creates a new `MessageNumber` with the given initial value without checking that it fits
    /// into the serial range.
    ///
    /// # Safety
    ///
    /// Calling this function with a value greater than `(1 << 26) - 1` is undefined behavoir.
    pub const unsafe fn new_unchecked(num: u32) -> Self {
        debug_assert!(
            num <= (1 << BITS) - 1,
            "MessageNumber::new_unchecked overflow"
        );

        Self(num)
    }

    /// Returns the current value as a `u32`.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Add<u32> for MessageNumber {
    type Output = Self;

    #[inline]
    fn add(self, rhs: u32) -> Self::Output {
        Self(serial::add::<BITS>(self.0, rhs))
    }
}

impl PartialOrd for MessageNumber {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MessageNumber {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        serial::cmp::<BITS>(self.0, other.0)
    }
}

impl AddAssign<u32> for MessageNumber {
    #[inline]
    fn add_assign(&mut self, rhs: u32) {
        self.0 = serial::add::<BITS>(self.0, rhs);
    }
}

impl From<MessageNumber> for u32 {
    #[inline]
    fn from(src: MessageNumber) -> Self {
        src.0
    }
}

//
// ---- impl core::fmt -----
//

impl Binary for MessageNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Binary::fmt(&self.0, f)
    }
}

impl Display for MessageNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl LowerHex for MessageNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0, f)
    }
}

impl UpperHex for MessageNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        UpperHex::fmt(&self.0, f)
    }
}

impl Octal for MessageNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Octal::fmt(&self.0, f)
    }
}
