use std::sync::atomic::{AtomicUsize, Ordering};

/// An increasing counter.
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct Counter(AtomicUsize);

impl Counter {
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
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}
