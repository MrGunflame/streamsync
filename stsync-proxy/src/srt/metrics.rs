use crate::metrics::Counter;

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
