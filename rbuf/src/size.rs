use std::ops::{Add, AddAssign, Mul, MulAssign};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Size(usize);

impl Size {
    /// A `Size` with zero bytes.
    const ZERO: Self = Self(0);

    /// A single byte.
    const B: Self = Size(1);
    /// A KiB.
    const KIB: Self = Size(1024);
    /// A MiB.
    const MIB: Self = Size(1024 * 1024);
    /// A GiB.
    const GIB: Self = Size(1024 * 1024 * 1024);

    pub const fn bytes(n: usize) -> Size {
        Self(n)
    }

    pub const fn kib(n: usize) -> Size {
        Self(Self::KIB.0 * n)
    }

    pub const fn mib(n: usize) -> Self {
        Self(Self::MIB.0 * n)
    }

    pub const fn gib(n: usize) -> Self {
        Self(Self::GIB.0 * n)
    }

    pub fn get(&self) -> usize {
        self.0
    }

    pub fn to_u64(&self) -> u64 {
        self.get() as u64
    }

    pub fn to_u32(&self) -> u32 {
        self.get() as u32
    }
}

impl Add for Size {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Size {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Mul for Size {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl MulAssign for Size {
    fn mul_assign(&mut self, rhs: Self) {
        self.0 *= rhs.0;
    }
}
