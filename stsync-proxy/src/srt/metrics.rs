use crate::metrics::{Counter, Gauge};

#[derive(Debug, Default)]
pub struct ServerMetrics {
    pub connections_total: Counter,
    pub connections_publish_current: Gauge,
    pub connections_request_current: Gauge,
    pub connections_handshake_current: Gauge,
}

impl ServerMetrics {
    pub const fn new() -> Self {
        Self {
            connections_total: Counter::new(),
            connections_publish_current: Gauge::new(),
            connections_request_current: Gauge::new(),
            connections_handshake_current: Gauge::new(),
        }
    }
}

#[derive(Debug, Default)]
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
    pub rtt: Gauge,
    pub rtt_variance: Gauge,
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
            rtt: Gauge::new(),
            rtt_variance: Gauge::new(),
        }
    }
}
