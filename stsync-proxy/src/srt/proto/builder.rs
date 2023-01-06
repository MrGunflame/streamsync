use bytes::Bytes;

use crate::srt::{DataPacket, PacketPosition};

use super::{
    Ack, AckAck, DropRequest, Keepalive, LightAck, Nak, SequenceNumbers, Shutdown, SmallAck,
};

/// A builder for a [`Keepalive`] packet.
#[derive(Clone, Debug, Default)]
pub struct KeepaliveBuilder(Keepalive);

impl KeepaliveBuilder {
    /// Creates a new `KeepaliveBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(Keepalive::default())
    }

    /// Consumes this `KeepaliveBuilder`, returning the constructed [`Keepalive`] packet.
    #[inline]
    pub const fn build(self) -> Keepalive {
        self.0
    }
}

/// A builder for a [`Ack`] packet.
#[derive(Clone, Debug, Default)]
pub struct AckBuilder(Ack);

impl AckBuilder {
    /// Creates a new `AckBuilder`.
    pub fn new() -> Self {
        Self(Ack::default())
    }

    pub fn acknowledgement_number(mut self, n: u32) -> Self {
        self.0.set_acknowledgement_number(n);
        self
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

    /// Consumes this `AckBuilder`, returning the constructed [`Ack`] packet.
    #[inline]
    pub const fn build(self) -> Ack {
        self.0
    }
}

/// A builder for a [`LightAck`] packet.
#[derive(Clone, Debug, Default)]
pub struct LightAckBuilder(LightAck);

impl LightAckBuilder {
    /// Creates a new `LightAckBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(LightAck::default())
    }

    pub const fn last_acknowledged_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.last_acknowledged_packet_sequence_number = n;
        self
    }

    /// Consumes this `LightAckBuilder`, returning the constructed [`LightAck`] packet.
    #[inline]
    pub const fn build(self) -> LightAck {
        self.0
    }
}

/// A builder for a [`SmallAck`] packet.
#[derive(Clone, Debug, Default)]
pub struct SmallAckBuilder(SmallAck);

impl SmallAckBuilder {
    /// Creates a new `SmallAckBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(SmallAck::default())
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

    /// Consumes this `SmallAckBuilder`, returning the constructed [`SmallAck`] packet.
    #[inline]
    pub const fn build(self) -> SmallAck {
        self.0
    }
}

/// A builder for a [`AckAck`] packet.
#[derive(Clone, Debug, Default)]
pub struct AckAckBuilder(AckAck);

impl AckAckBuilder {
    /// Creates a new `AckAckBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(AckAck::default())
    }

    pub fn acknowledgement_number(mut self, n: u32) -> Self {
        self.0.set_acknowledgement_number(n);
        self
    }

    /// Consumes this `AckAckBuilder`, returning the constructed [`AckAck`] packet.
    #[inline]
    pub const fn build(self) -> AckAck {
        self.0
    }
}

/// A builder for a [`Nak`] packet.
#[derive(Clone, Debug, Default)]
pub struct NakBuilder(Nak);

impl NakBuilder {
    /// Creates a new `NakBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(Nak::default())
    }

    pub fn lost_packet_sequence_number(mut self, n: u32) -> Self {
        self.0.set_lost_packet_sequence_number(n);
        self
    }

    #[inline]
    pub fn lost_packet_sequence_numbers<T>(mut self, seq: T) -> Self
    where
        T: Into<SequenceNumbers>,
    {
        self.0.set_lost_packet_sequence_numbers(seq);
        self
    }

    /// Consumes this `NakBuilder`, returning the constructed [`Nak`] packet.
    #[inline]
    pub const fn build(self) -> Nak {
        self.0
    }
}

/// A builder for a [`Shutdown`] packet.
#[derive(Clone, Debug, Default)]
pub struct ShutdownBuilder(Shutdown);

impl ShutdownBuilder {
    /// Creates a new `ShutdownBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(Shutdown::default())
    }

    /// Consumes this `ShutdownBuilder`, returning the constructed [`Shutdown`] packet.
    #[inline]
    pub const fn build(self) -> Shutdown {
        self.0
    }
}

/// A builder for a [`DropRequest`] packet.
#[derive(Clone, Debug, Default)]
pub struct DropRequestBuilder(DropRequest);

impl DropRequestBuilder {
    /// Creates a new `DropRequestBuilder`.
    #[inline]
    pub fn new() -> Self {
        Self(DropRequest::default())
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

    /// Consumes this `DropRequestBuilder`, returning the constructed [`DropRequest`] packet.
    #[inline]
    pub const fn build(self) -> DropRequest {
        self.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct DataPacketBuilder(DataPacket);

impl DataPacketBuilder {
    #[inline]
    pub fn new() -> Self {
        Self(DataPacket::default())
    }

    /// Sets the sequence number of the [`DataPacket`].
    ///
    /// **The default value is `0`.**
    #[inline]
    pub fn sequence_number<T>(mut self, val: T) -> Self
    where
        T: Into<u32>,
    {
        self.0.header().set_packet_sequence_number(val.into());
        self
    }

    /// Sets the message number of the [`DataPacket`].
    ///
    /// **The default value is `0`.**
    #[inline]
    pub fn message_number<T>(mut self, val: T) -> Self
    where
        T: Into<u32>,
    {
        self.0.header().set_message_number(val.into());
        self
    }

    /// Sets the order flag of the [`DataPacket`].
    ///
    /// **The default value is `false`.**
    #[inline]
    pub fn ordered(mut self, val: bool) -> Self {
        self.0.header().set_ordered(val);
        self
    }

    /// Sets the packet position of the [`DataPacket`].
    ///
    /// **The default value is [`PacketPosition::Solo`].**
    #[inline]
    pub fn packet_position(mut self, val: PacketPosition) -> Self {
        self.0.header().set_packet_position(val);
        self
    }

    /// Sets the retransmitted flag of the [`DataPacket`].
    ///
    /// **The default value is `false`.**
    #[inline]
    pub fn retransmitted(mut self, val: bool) -> Self {
        self.0.header().set_retransmitted(val);
        self
    }

    #[inline]
    pub fn body<T>(mut self, val: T) -> Self
    where
        T: Into<Bytes>,
    {
        self.0.data = val.into();
        self
    }

    #[inline]
    pub fn build(self) -> DataPacket {
        self.0
    }
}
