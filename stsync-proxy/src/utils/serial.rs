//! Serial Number Arithmetic
//!
//! Also see https://datatracker.ietf.org/doc/html/rfc1982

use std::cmp::Ordering;

/// Adds `rhs` to `lhs` using serial number arithmetic.
#[inline]
pub fn add<const N: usize>(mut lhs: u32, mut rhs: u32) -> u32 {
    let max = (1 << N) - 1;

    lhs &= max;
    rhs &= max;

    (lhs + rhs) & max
}

/// Subtracts `rhs` from `lhs` using serial number artihmetic.
#[inline]
pub fn sub<const N: usize>(mut lhs: u32, mut rhs: u32) -> u32 {
    let max = (1 << N) - 1;

    lhs &= max;
    rhs &= max;

    lhs.wrapping_sub(rhs) & max
}

/// Compares `lhs` to `rhs` using serial number comparisons.
#[inline]
pub fn cmp<const N: usize>(lhs: u32, rhs: u32) -> Ordering {
    if lhs == rhs {
        return Ordering::Equal;
    }

    // See RFC1982
    if (lhs < rhs && rhs.wrapping_sub(lhs) < 1 << (N - 1))
        || (lhs > rhs && lhs.wrapping_sub(rhs) > 1 << (N - 1))
    {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{add, cmp, sub};

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

    #[test]
    fn test_cmp() {
        assert_eq!(cmp::<4>(0, 0), Ordering::Equal);
        assert_eq!(cmp::<4>(0, 1), Ordering::Less);
        assert_eq!(cmp::<4>(1, 0), Ordering::Greater);

        assert_eq!(cmp::<8>(1, 0), Ordering::Greater);
        assert_eq!(cmp::<8>(44, 0), Ordering::Greater);
        assert_eq!(cmp::<8>(100, 0), Ordering::Greater);
        assert_eq!(cmp::<8>(100, 44), Ordering::Greater);
        assert_eq!(cmp::<8>(200, 100), Ordering::Greater);
        assert_eq!(cmp::<8>(255, 200), Ordering::Greater);
        assert_eq!(cmp::<8>(100, 255), Ordering::Greater);
        assert_eq!(cmp::<8>(0, 200), Ordering::Greater);
        assert_eq!(cmp::<8>(44, 200), Ordering::Greater);
    }
}
