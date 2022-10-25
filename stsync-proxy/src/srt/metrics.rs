use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct ConnectionMetrics {
    pub packets_dropped: Counter,
    pub packets_sent: Counter,
    pub bytes_sent: Counter,
    pub packets_recv: Counter,
    pub bytes_recv: Counter,
}

impl ConnectionMetrics {
    pub const fn new() -> Self {
        Self {
            packets_dropped: Counter::new(),
            packets_sent: Counter::new(),
            bytes_sent: Counter::new(),
            packets_recv: Counter::new(),
            bytes_recv: Counter::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Counter(AtomicUsize);

impl Counter {
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

    pub fn load(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl Display for Counter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.load().fmt(f)
    }
}
