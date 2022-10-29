use crate::{
    proto::{Bits, Zeroable, U32},
    srt::{ControlPacketType, PacketType},
};

use super::{
    Ack, AckAck, DropRequest, Keepalive, LightAck, Nak, SequenceNumbers, Shutdown, SmallAck,
};

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

#[derive(Clone, Debug)]
pub struct SmallAckBuilder(SmallAck);

impl SmallAckBuilder {
    pub fn new() -> Self {
        let mut packet = SmallAck::default();
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

    pub const fn build(self) -> SmallAck {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct AckAckBuilder(AckAck);

impl AckAckBuilder {
    pub fn new() -> Self {
        let mut packet = AckAck::zeroed();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::AckAck);

        Self(packet)
    }

    pub const fn acknowledgement_number(mut self, n: u32) -> Self {
        self.0.header.seg1 = Bits(U32(n));
        self
    }

    pub const fn build(self) -> AckAck {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct NakBuilder(Nak);

impl NakBuilder {
    pub fn new() -> Self {
        let mut packet = Nak::default();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::Nak);

        Self(packet)
    }

    pub fn lost_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.set_lost_packet_sequence_number(n);
        self
    }

    pub fn lost_packet_sequence_numbers<T>(mut self, seq: T) -> Self
    where
        T: Into<SequenceNumbers>,
    {
        self.0.set_lost_packet_sequence_numbers(seq);
        self
    }

    pub fn build(self) -> Nak {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct ShutdownBuilder(Shutdown);

impl ShutdownBuilder {
    pub fn new() -> Self {
        let mut packet = Shutdown::default();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::Shutdown);

        Self(packet)
    }

    pub const fn build(self) -> Shutdown {
        self.0
    }
}

impl Default for ShutdownBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct DropRequestBuilder(DropRequest);

impl DropRequestBuilder {
    pub fn new() -> Self {
        let mut packet = DropRequest::default();
        packet.header.set_packet_type(PacketType::Control);
        packet
            .header
            .as_control()
            .unwrap()
            .set_control_type(ControlPacketType::DropReq);

        Self(packet)
    }

    pub fn message_number(mut self, n: u32) -> Self {
        self.0.set_message_number(n);
        self
    }

    pub const fn first_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.first_packet_sequence_number = n;
        self
    }

    pub const fn last_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.last_packet_sequence_number = n;
        self
    }

    pub const fn build(self) -> DropRequest {
        self.0
    }
}
