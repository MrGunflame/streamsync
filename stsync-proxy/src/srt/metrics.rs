use std::fmt::{self, Display, Formatter};
use std::ops::AddAssign;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct ConnectionMetrics {
    pub ctrl_packets_sent: Counter,
    pub ctrl_packets_recv: Counter,
    pub ctrl_packets_lost: Counter,
    pub ctrl_bytes_sent: Counter,
    pub ctrl_bytes_recv: Counter,
    pub ctrl_bytes_lost: Counter,
    pub data_packets_sent: Counter,
    pub data_packets_recv: Counter,
    pub data_packets_lost: Counter,
    pub data_bytes_sent: Counter,
    pub data_bytes_recv: Counter,
    pub data_bytes_lost: Counter,
}

impl ConnectionMetrics {
    pub const fn new() -> Self {
        Self {
            ctrl_packets_sent: Counter::new(),
            ctrl_packets_recv: Counter::new(),
            ctrl_packets_lost: Counter::new(),
            ctrl_bytes_sent: Counter::new(),
            ctrl_bytes_recv: Counter::new(),
            ctrl_bytes_lost: Counter::new(),
            data_packets_sent: Counter::new(),
            data_packets_recv: Counter::new(),
            data_packets_lost: Counter::new(),
            data_bytes_sent: Counter::new(),
            data_bytes_recv: Counter::new(),
            data_bytes_lost: Counter::new(),
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
