pub mod builder;

use std::{
    io::{Cursor, Read, Write},
    ops::{Range, RangeInclusive},
};

use bytes::Buf;
use streamsync_macros::Packet;

use crate::proto::{Bits, Decode, Encode, Zeroable, U32};

use self::builder::{
    AckAckBuilder, AckBuilder, DropRequestBuilder, KeepaliveBuilder, LightAckBuilder, NakBuilder,
    ShutdownBuilder,
};

use super::{ControlPacketType, Error, Header, IsPacket, Packet};

#[derive(Clone, Debug, Default)]
pub struct Keepalive {
    pub header: Header,
    _unused: u32,
}

impl Keepalive {
    pub fn builder() -> KeepaliveBuilder {
        KeepaliveBuilder::new()
    }
}

impl IsPacket for Keepalive {
    type Error = Error;

    fn upcast(self) -> Packet {
        Packet {
            header: self.header,
            body: vec![0, 0, 0, 0].into(),
        }
    }

    fn downcast(packet: Packet) -> Result<Self, Self::Error> {
        Ok(Self {
            header: packet.header,
            _unused: 0,
        })
    }
}

impl Encode for Keepalive {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: std::io::Write,
    {
        self.header.encode(&mut writer)?;
        self._unused.encode(writer)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Packet)]
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
}

impl IsPacket for Ack {
    type Error = Error;

    fn upcast(self) -> Packet {
        let mut body = Vec::new();
        self.encode_body(&mut body).unwrap();

        Packet {
            header: self.header,
            body: body.into(),
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

#[derive(Clone, Debug, Default)]
pub struct SmallAck {
    pub header: Header,
    pub last_acknowledged_packet_sequence_number: u32,
    pub rtt: u32,
    pub rtt_variance: u32,
    pub avaliable_buffer_size: u32,
}

impl SmallAck {}

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
#[derive(Clone, Debug)]
pub struct AckAck {
    pub header: Header,
    _unused: u32,
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
            body: vec![0, 0, 0, 0].into(),
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let header = packet.header.as_control()?;
        if header.control_type() != ControlPacketType::AckAck {
            return Err(Error::InvalidControlType(header.control_type().to_u16()));
        }

        Ok(Self {
            header: packet.header,
            _unused: 0,
        })
    }
}

unsafe impl Zeroable for AckAck {}

#[derive(Clone, Debug, Default, Packet)]
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
        self.lost_packet_sequence_numbers = SequenceNumbers::Single(n);
    }

    pub fn set_lost_packet_sequence_numbers<T>(&mut self, seq: T)
    where
        T: Into<SequenceNumbers>,
    {
        self.lost_packet_sequence_numbers = seq.into();
    }
}

impl IsPacket for Nak {
    type Error = Error;

    fn upcast(self) -> Packet {
        let body = self.lost_packet_sequence_numbers.encode_to_vec().unwrap();

        Packet {
            header: self.header,
            body: body.into(),
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let lost_packet_sequence_numbers = SequenceNumbers::decode(&mut packet.body)?;

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
    _unused: u32,
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
            body: vec![0, 0, 0, 0].into(),
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let header = packet.header.as_control()?;
        if header.control_type() != ControlPacketType::Shutdown {
            return Err(Error::InvalidControlType(header.control_type().to_u16()));
        }

        Ok(Self {
            header: packet.header,
            _unused: 0,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct DropRequest {
    pub header: Header,
    pub first_packet_sequence_number: u32,
    pub last_packet_sequence_number: u32,
}

impl DropRequest {
    pub fn builder() -> DropRequestBuilder {
        DropRequestBuilder::new()
    }

    pub fn message_number(&self) -> u32 {
        self.header.seg1.0 .0
    }

    pub fn set_message_number(&mut self, n: u32) {
        self.header.seg1 = Bits(U32(n));
    }
}

impl IsPacket for DropRequest {
    type Error = Error;

    fn upcast(self) -> Packet {
        let mut body = Vec::new();
        body.extend(self.first_packet_sequence_number.to_be_bytes());
        body.extend(self.last_packet_sequence_number.to_be_bytes());

        Packet {
            header: self.header,
            body: body.into(),
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let header = packet.header.as_control()?;
        if header.control_type() != ControlPacketType::DropReq {
            return Err(Error::InvalidControlType(header.control_type().to_u16()));
        }

        let first_packet_sequence_number = u32::decode(&mut packet.body)?;
        let last_packet_sequence_number = u32::decode(&mut packet.body)?;

        Ok(Self {
            header: packet.header,
            first_packet_sequence_number,
            last_packet_sequence_number,
        })
    }
}

/// A list of a single sequence number, or a range of sequence numbers.
///
/// See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#appendix-A
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SequenceNumbers {
    Single(u32),
    Range(RangeInclusive<u32>),
}

impl SequenceNumbers {
    pub fn contains(&self, num: u32) -> bool {
        match self {
            Self::Single(n) => *n == num,
            Self::Range(range) => range.contains(&num),
        }
    }

    pub fn first(&self) -> u32 {
        match self {
            Self::Single(num) => *num,
            Self::Range(range) => *range.start(),
        }
    }

    pub fn last(&self) -> u32 {
        match self {
            Self::Single(num) => *num,
            Self::Range(range) => *range.end(),
        }
    }

    /// Returns the number of sequence numbers encapsulated.
    pub fn len(&self) -> u32 {
        match self {
            Self::Single(_) => 1,
            Self::Range(range) => range.end() - range.start(),
        }
    }

    pub fn iter(&self) -> SequenceNumbersIter {
        match self {
            Self::Single(num) => SequenceNumbersIter {
                start: *num as usize,
                end: *num as usize,
            },
            Self::Range(range) => SequenceNumbersIter {
                start: (*range.start()) as usize,
                end: (*range.end()) as usize,
            },
        }
    }
}

impl Encode for SequenceNumbers {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        match self {
            Self::Single(num) => {
                let mut bits = Bits(U32(*num));
                bits.set_bits(0, 0);

                bits.encode(writer)?;
            }
            Self::Range(range) => {
                let mut start = Bits(U32(*range.start()));
                start.set_bits(1, 0);

                let mut end = Bits(U32(*range.end()));
                end.set_bits(0, 0);

                start.encode(&mut writer)?;
                end.encode(&mut writer)?;
            }
        }

        Ok(())
    }
}

impl Decode for SequenceNumbers {
    type Error = Error;

    fn decode<B>(bytes: &mut B) -> Result<Self, Self::Error>
    where
        B: Buf,
    {
        let seq = Bits::<U32>::decode(bytes)?;

        // Is a range
        if seq.bits(0) == 1 {
            let mut start = seq;
            start.set_bits(0, 0);

            let end = Bits::<U32>::decode(bytes)?;
            // TODO: Check that the ending sequence has the MSB bit set to 0.

            Ok(Self::Range(start.0 .0..=end.0 .0))
        } else {
            Ok(Self::Single(seq.0 .0))
        }
    }
}

impl IntoIterator for SequenceNumbers {
    type Item = u32;
    type IntoIter = SequenceNumbersIter;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl IntoIterator for &SequenceNumbers {
    type Item = u32;
    type IntoIter = SequenceNumbersIter;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Default for SequenceNumbers {
    fn default() -> Self {
        Self::Single(0)
    }
}

impl From<u32> for SequenceNumbers {
    fn from(src: u32) -> Self {
        Self::Single(src)
    }
}

impl From<RangeInclusive<u32>> for SequenceNumbers {
    fn from(src: RangeInclusive<u32>) -> Self {
        Self::Range(src)
    }
}

impl From<Range<u32>> for SequenceNumbers {
    fn from(src: Range<u32>) -> Self {
        if src.len() == 1 {
            Self::from(src.start)
        } else {
            Self::from(src.start..src.end - 1)
        }
    }
}

impl PartialEq<u32> for SequenceNumbers {
    fn eq(&self, other: &u32) -> bool {
        match self {
            Self::Single(num) => *num == *other,
            Self::Range(_) => false,
        }
    }
}

impl PartialEq<RangeInclusive<u32>> for SequenceNumbers {
    fn eq(&self, other: &RangeInclusive<u32>) -> bool {
        match self {
            Self::Single(_) => false,
            Self::Range(range) => range.start() == other.start() && range.end() == other.end(),
        }
    }
}

/// An [`Iterator`] over sequence numbers. Returned by [`iter`].
///
/// [`iter`]: SequenceNumbers::iter
#[derive(Clone, Debug)]
pub struct SequenceNumbersIter {
    // Note that we use `usize` here to avoid overflows when using a range
    // with the upper bound of `u32::MAX`.
    start: usize,
    end: usize,
}

impl Iterator for SequenceNumbersIter {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start > self.end {
            None
        } else {
            let num = self.start;
            self.start += 1;
            Some(num as u32)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
}

impl ExactSizeIterator for SequenceNumbersIter {
    fn len(&self) -> usize {
        self.end - self.start + 1
    }
}

#[cfg(test)]
mod tests {
    use crate::proto::Decode;

    use super::SequenceNumbers;

    #[test]
    fn test_sequence_numbers() {
        let mut buf: &[u8] = &[0x86, 0x2D, 0x67, 0xFA, 0x06, 0x2D, 0x68, 0x13];
        let seqnum = SequenceNumbers::decode(&mut buf).unwrap();

        assert_eq!(seqnum, 103639034..=103639059);
    }

    #[test]
    fn test_sequence_numbers_iter() {
        let mut iter = SequenceNumbers::Single(69).iter();
        assert_eq!(iter.len(), 1);
        assert_eq!(iter.next(), Some(69));
        assert_eq!(iter.next(), None);

        let mut iter = SequenceNumbers::Range(1..=5).iter();
        assert_eq!(iter.len(), 5);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(3));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(5));
        assert_eq!(iter.next(), None);
    }
}
