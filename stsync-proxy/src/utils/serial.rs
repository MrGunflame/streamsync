//! Serial Number Arithmetic
//!
//! Also see https://datatracker.ietf.org/doc/html/rfc1982

use std::cmp::Ordering;

#[inline]
pub fn add<const N: usize>(mut lhs: u32, mut rhs: u32) -> u32 {
    let max = (1 << N) - 1;

    lhs &= max;
    rhs &= max;

    (lhs + rhs) & max
}

#[inline]
pub fn sub<const N: usize>(mut lhs: u32, mut rhs: u32) -> u32 {
    let max = (1 << N) - 1;

    lhs &= max;
    rhs &= max;

    lhs.wrapping_sub(rhs) & max
}

pub fn cmp<const N: usize>(mut lhs: u32, mut rhs: u32) -> Ordering {
    if lhs == rhs {
        return Ordering::Equal;
    }

    const WRAP_THRESHOLD: u32 = 1024;

    // TODO: Compare depending on the maximum sequence number.
    if lhs.checked_add(WRAP_THRESHOLD).is_some() || rhs.checked_add(WRAP_THRESHOLD).is_some() {
        lhs = lhs.wrapping_add(WRAP_THRESHOLD);
        rhs = rhs.wrapping_add(WRAP_THRESHOLD);
    }

    lhs.cmp(&rhs)
}

#[cfg(test)]
mod tests {
    use super::{add, sub};

    #[test]
    fn test_add() {
        assert_eq!(add::<4>(1, 2), 3);
        assert_eq!(add::<4>(0b1110, 0b1), 0b1111);
        assert_eq!(add::<4>(0b1111, 0b1), 0b0000);
        assert_eq!(add::<4>(0b1111, 0b0001_0001), 0b0000);
    }

    #[test]
    fn test_sub() {
        assert_eq!(sub::<4>(0b1111, 0b1110), 0b1);
        assert_eq!(sub::<4>(0b1111, 0b1111), 0b0);
        assert_eq!(sub::<4>(0b1111, 0b0001_0000), 0b1111);
        assert_eq!(sub::<4>(0b1111, 0b0001_1010), 0b0101);
    }
}
