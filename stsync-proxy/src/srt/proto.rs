pub mod builder;

use std::convert::Infallible;
use std::io::{Read, Write};

use crate::proto::{Bits, Encode, U32};

use self::builder::{
    AckAckBuilder, AckBuilder, KeepaliveBuilder, LightAckBuilder, NakBuilder, ShutdownBuilder,
};

use super::{ControlPacketType, Error, Header, InvalidControlType, IsPacket, Packet};

#[derive(Clone, Debug, Default)]
pub struct Keepalive {
    pub header: Header,
}

impl Keepalive {
    pub fn builder() -> KeepaliveBuilder {
        KeepaliveBuilder::new()
    }
}

impl Encode for Keepalive {
    type Error = Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: std::io::Write,
    {
        self.header.encode(writer)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Ack {
    pub header: Header,
    pub last_acknowledged_packet_sequence_number: u32,
    pub rtt: u32,
    pub rtt_variance: u32,
    pub avaliable_buffer_size: u32,
    pub packets_receiving_rate: u32,
    pub estimated_link_capacity: u32,
    pub receiving_rate: u32,
}

impl Ack {
    pub fn builder() -> AckBuilder {
        AckBuilder::new()
    }

    pub fn acknowledgement_number(&self) -> u32 {
        self.header.seg1.0 .0
    }

    pub fn set_acknowledgement_number(&mut self, n: u32) {
        self.header.seg1 = Bits(U32(n));
    }
}

impl Encode for Ack {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.header.encode(&mut writer)?;
        self.last_acknowledged_packet_sequence_number
            .encode(&mut writer)?;
        self.rtt.encode(&mut writer)?;
        self.rtt_variance.encode(&mut writer)?;
        self.avaliable_buffer_size.encode(&mut writer)?;
        self.packets_receiving_rate.encode(&mut writer)?;
        self.estimated_link_capacity.encode(&mut writer)?;
        self.receiving_rate.encode(&mut writer)?;

        Ok(())
    }
}

/// A Light ACK control packet includes only the Last Acknowledged
/// Packet Sequence Number field.  The Type-specific Information field
/// should be set to 0.
#[derive(Clone, Debug, Default)]
pub struct LightAck {
    pub header: Header,
    pub last_acknowledged_packet_sequence_number: u32,
}

impl LightAck {
    pub fn builder() -> LightAckBuilder {
        LightAckBuilder::new()
    }
}

impl Encode for LightAck {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: std::io::Write,
    {
        self.header.encode(&mut writer)?;
        self.last_acknowledged_packet_sequence_number
            .encode(&mut writer)?;
        Ok(())
    }
}

pub struct SmallAck {}

///     0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+- SRT Header +-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |1|        Control Type         |           Reserved            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     Acknowledgement Number                    |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                           Timestamp                           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                     Destination Socket ID                     |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Clone, Debug, Default)]
pub struct AckAck {
    pub header: Header,
}

impl AckAck {
    pub fn builder() -> AckAckBuilder {
        AckAckBuilder::new()
    }

    /// This field contains the Acknowledgement
    /// Number of the full ACK packet the reception of which is being
    /// acknowledged by this ACKACK packet.
    pub fn acknowledgement_number(&self) -> u32 {
        self.header.seg1.0 .0
    }
}

impl IsPacket for AckAck {
    type Error = Error;

    fn upcast(self) -> Packet {
        Packet {
            header: self.header,
            body: Vec::new(),
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let header = packet.header.as_control()?;
        if header.control_type() != ControlPacketType::AckAck {
            return Err(Error::InvalidControlType(header.control_type().to_u16()));
        }

        Ok(Self {
            header: packet.header,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Nak {
    pub header: Header,
    pub lost_packet_sequence_number: Bits<U32>,
}

impl Nak {
    pub fn builder() -> NakBuilder {
        NakBuilder::new()
    }

    pub fn lost_packet_sequence_number(&self) -> u32 {
        self.lost_packet_sequence_number.bits(1..32).0
    }

    pub fn set_lost_packet_sequence_number(&mut self, n: u32) {
        self.lost_packet_sequence_number.set_bits(0, 0);
        self.lost_packet_sequence_number.set_bits(1..32, n);
    }
}

impl Encode for Nak {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.header.encode(&mut writer)?;
        self.lost_packet_sequence_number.encode(&mut writer)?;

        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Shutdown {
    header: Header,
}

impl Shutdown {
    pub fn builder() -> ShutdownBuilder {
        ShutdownBuilder::new()
    }
}

impl IsPacket for Shutdown {
    type Error = Error;

    fn upcast(self) -> Packet {
        Packet {
            header: self.header,
            body: Vec::new(),
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let header = packet.header.as_control()?;
        if header.control_type() != ControlPacketType::Shutdown {
            return Err(Error::InvalidControlType(header.control_type().to_u16()));
        }

        Ok(Self {
            header: packet.header,
        })
    }
}
