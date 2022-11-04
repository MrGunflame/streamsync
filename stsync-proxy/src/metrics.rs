use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicUsize, Ordering};

/// An increasing counter.
///
/// Note that all operations on `Counter` correspond to [`Relaxed`] atomic operations. The value
/// must not be relied upon for exact correctness.
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct Counter(AtomicUsize);

impl Counter {
    /// Creates a new `Counter` initialized to `0`.
    #[inline]
    pub const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    /// Adds `n` to the `Counter`.
    ///
    /// Note that this corresponds to a relaxed atomic operation.
    #[inline]
    pub fn add(&self, n: usize) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    /// Increments the `Counter` by `1`.
    ///
    /// Note that this corresponds to a relaxed atomic operation.
    #[inline]
    pub fn inc(&self) {
        self.add(1);
    }

    /// Returns the current value of the `Counter`.
    ///
    /// Note that this corresponds to a relaxed atomic load.
    #[inline]
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl Display for Counter {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}

/// A counter that can go up and down arbitrarily.
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct Gauge(AtomicUsize);

impl Gauge {
    /// Creates a new `Gauge` initialized to `0`.
    #[inline]
    pub const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    #[inline]
    pub fn add(&self, n: usize) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc(&self) {
        self.add(1);
    }

    #[inline]
    pub fn sub(&self, n: usize) {
        self.0.fetch_sub(n, Ordering::Relaxed);
    }

    #[inline]
    pub fn dec(&self) {
        self.sub(1);
    }

    #[inline]
    pub fn set(&self, n: usize) {
        self.0.store(n, Ordering::Relaxed);
    }

    /// Returns the current value of the `Gauge`.
    #[inline]
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl Display for Gauge {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}
