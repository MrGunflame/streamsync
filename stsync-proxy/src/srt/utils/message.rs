//!
//! See https://datatracker.ietf.org/doc/html/rfc1982
use std::fmt::{self, Display, Formatter};
use std::ops::{Add, AddAssign};

use crate::utils::serial;

const BITS: usize = 26;

/// A 26-bit message number.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageNumber(u32);

impl MessageNumber {
    #[inline]
    pub const fn new(num: u32) -> Self {
        assert!(num <= (1 << BITS) - 1, "MessageNumber::new overflow");

        unsafe { Self::new_unchecked(num) }
    }

    pub const unsafe fn new_unchecked(num: u32) -> Self {
        debug_assert!(
            num <= (1 << BITS) - 1,
            "MessageNumber::new_unchecked overflow"
        );

        Self(num)
    }

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

impl Display for MessageNumber {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}
