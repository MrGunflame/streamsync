use crate::srt::{ControlPacketType, PacketType};

use super::{Ack, Keepalive, LightAck};

#[derive(Clone, Debug)]
pub struct KeepaliveBuilder(Keepalive);

impl KeepaliveBuilder {
    pub fn new() -> Self {
        let mut packet = Keepalive::default();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::Keepalive);

        Self(packet)
    }

    pub fn build(self) -> Keepalive {
        self.0
    }
}

impl Default for KeepaliveBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct AckBuilder(Ack);

impl AckBuilder {
    pub fn new() -> Self {
        let mut packet = Ack::default();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::Ack);

        Self(packet)
    }

    pub const fn last_acknowledged_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.last_acknowledged_packet_sequence_number = n;
        self
    }

    pub const fn rtt(mut self, n: u32) -> Self {
        self.0.rtt = n;
        self
    }

    pub const fn rtt_variance(mut self, n: u32) -> Self {
        self.0.rtt_variance = n;
        self
    }

    pub const fn avaliable_buffer_size(mut self, n: u32) -> Self {
        self.0.avaliable_buffer_size = n;
        self
    }

    pub const fn packets_receiving_rate(mut self, n: u32) -> Self {
        self.0.packets_receiving_rate = n;
        self
    }

    pub const fn estimated_link_capacity(mut self, n: u32) -> Self {
        self.0.estimated_link_capacity = n;
        self
    }

    pub const fn receiving_rate(mut self, n: u32) -> Self {
        self.0.receiving_rate = n;
        self
    }

    pub fn build(self) -> Ack {
        self.0
    }
}

impl Default for AckBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A builder for a [`LightAck`] packet.
#[derive(Clone, Debug)]
pub struct LightAckBuilder(LightAck);

impl LightAckBuilder {
    pub fn new() -> Self {
        // Set correct header (Control + Ack).
        let mut packet = LightAck::default();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::Ack);

        Self(packet)
    }

    pub const fn last_acknowledged_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.last_acknowledged_packet_sequence_number = n;
        self
    }

    pub fn build(self) -> LightAck {
        self.0
    }
}

impl Default for LightAckBuilder {
    fn default() -> Self {
        Self::new()
    }
}
