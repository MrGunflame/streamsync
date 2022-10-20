use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use futures::Stream;
use tokio::net::UdpSocket;

use super::Packet;

pub struct Connection {}

#[derive(Clone, Debug)]
pub struct AckQueue {
    inner: VecDeque<(u32, Instant)>,
}

impl AckQueue {
    pub fn new() -> Self {
        Self {
            inner: VecDeque::new(),
        }
    }

    pub fn push_back(&mut self, seq: u32, ts: Instant) {
        self.inner.push_back((seq, ts));
    }

    pub fn pop_front(&mut self) -> Option<(u32, Instant)> {
        self.inner.pop_front()
    }

    pub fn last(&self) -> Option<(u32, Instant)> {
        if self.inner.is_empty() {
            None
        } else {
            self.inner.get(self.inner.len() - 1).copied()
        }
    }
}
