pub mod builder;

use std::convert::Infallible;
use std::io::{self, Read, Write};

use crate::proto::{Bits, Decode, Encode, U32};

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

    fn encode_body<W>(&self, mut writer: W) -> Result<(), Error>
    where
        W: Write,
    {
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

    fn decode_body<R>(mut reader: R, header: Header) -> Result<Self, Error>
    where
        R: Read,
    {
        let last_acknowledged_packet_sequence_number = u32::decode(&mut reader)?;
        let rtt = u32::decode(&mut reader)?;
        let rtt_variance = u32::decode(&mut reader)?;
        let avaliable_buffer_size = u32::decode(&mut reader)?;
        let packets_receiving_rate = u32::decode(&mut reader)?;
        let estimated_link_capacity = u32::decode(&mut reader)?;
        let receiving_rate = u32::decode(&mut reader)?;

        Ok(Self {
            header,
            last_acknowledged_packet_sequence_number,
            rtt,
            rtt_variance,
            avaliable_buffer_size,
            packets_receiving_rate,
            estimated_link_capacity,
            receiving_rate,
        })
    }
}

impl IsPacket for Ack {
    type Error = Error;

    fn upcast(self) -> Packet {
        let mut body = Vec::new();
        self.encode_body(&mut body).unwrap();

        Packet {
            header: self.header,
            body,
        }
    }

    fn downcast(packet: Packet) -> Result<Self, Self::Error> {
        Ok(Self::decode_body(&packet.body[..], packet.header)?)
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
    /// A single or a list of lost sequence numbers.
    pub lost_packet_sequence_numbers: SequenceNumbers,
}

impl Nak {
    pub fn builder() -> NakBuilder {
        NakBuilder::new()
    }

    pub fn lost_packet_sequence_number(&self) -> &SequenceNumbers {
        &self.lost_packet_sequence_numbers
    }

    pub fn set_lost_packet_sequence_number(&mut self, n: u32) {
        self.lost_packet_sequence_numbers = SequenceNumbers::new_single(n);
    }
}

impl IsPacket for Nak {
    type Error = Error;

    fn upcast(self) -> Packet {
        let body = self.lost_packet_sequence_numbers.encode_to_vec().unwrap();

        Packet {
            header: self.header,
            body,
        }
    }

    fn downcast(packet: Packet) -> Result<Self, Self::Error> {
        let lost_packet_sequence_numbers = SequenceNumbers::decode(&packet.body[..])?;

        Ok(Self {
            header: packet.header,
            lost_packet_sequence_numbers,
        })
    }
}

impl Encode for Nak {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.header.encode(&mut writer)?;
        self.lost_packet_sequence_numbers.encode(&mut writer)?;

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

/// A list of one or more encoded sequence numbers.
///
/// See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#appendix-A
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SequenceNumbers {
    Single(Bits<U32>),
    Multiple(Vec<Bits<U32>>),
}

impl SequenceNumbers {
    pub fn new_single<T>(seq: T) -> Self
    where
        T: Into<U32>,
    {
        let mut bits = Bits(seq.into());
        bits.set_bits(0, 0);

        Self::Single(bits)
    }

    pub fn new_multiple<T, I>(iter: I) -> Self
    where
        T: Into<U32>,
        I: Iterator<Item = T>,
    {
        let mut buf: Vec<Bits<U32>> = iter.map(|item| Bits(item.into())).collect();

        for bits in buf.iter_mut() {
            bits.set_bits(0, 1);
        }

        if let Some(bits) = buf.last_mut() {
            bits.set_bits(0, 0);
        }

        Self::Multiple(buf)
    }

    pub fn set_single<T>(&mut self, seq: T)
    where
        T: Into<U32>,
    {
        let mut bits = Bits(seq.into());
        bits.set_bits(0, 0);

        *self = Self::Single(bits);
    }

    fn is_final(bits: Bits<U32>) -> bool {
        bits.bits(0) == 0
    }
}

impl Encode for SequenceNumbers {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        match self {
            Self::Single(buf) => buf.encode(writer)?,
            Self::Multiple(buf) => {
                for b in buf {
                    b.encode(&mut writer)?;
                }
            }
        }

        Ok(())
    }
}

impl Decode for SequenceNumbers {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let seq = Bits::decode(&mut reader)?;

        if Self::is_final(seq) {
            return Ok(Self::Single(seq));
        }

        let mut buf = vec![seq];
        loop {
            let seq = Bits::decode(&mut reader)?;
            buf.push(seq);

            if Self::is_final(seq) {
                return Ok(Self::Multiple(buf));
            }
        }
    }
}

impl<A> FromIterator<A> for SequenceNumbers
where
    A: Into<U32>,
{
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut vec: Vec<U32> = iter.into_iter().map(|item| item.into()).collect();

        if vec.len() == 1 {
            Self::new_single(vec[0])
        } else {
            Self::new_multiple(vec.into_iter())
        }
    }
}

impl Default for SequenceNumbers {
    fn default() -> Self {
        Self::Single(Bits(U32(0)))
    }
}
