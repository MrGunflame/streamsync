pub mod builder;
pub mod header;

use std::{
    io::Write,
    ops::{Range, RangeInclusive},
};

use bytes::Buf;
use streamsync_macros::Packet;

use crate::proto::{Bits, Decode, Encode, Zeroable, U32};

use self::{
    builder::{
        AckAckBuilder, AckBuilder, DropRequestBuilder, KeepaliveBuilder, LightAckBuilder,
        NakBuilder, ShutdownBuilder,
    },
    header::{AckHeader, DropRequestHeader, HandshakeHeader, KeepaliveHeader, ShutdownHeader},
};

use super::{EncryptionField, Error, ExtensionField, Extensions, HandshakeType, Header};

#[derive(Clone, Debug, Default, Packet)]
pub struct Handshake {
    pub header: HandshakeHeader,
    /// A base protocol version number.  Currently used
    /// values are 4 and 5.  Values greater than 5 are reserved for future
    /// use.
    pub version: u32,
    /// Block cipher family and key size.  The
    /// values of this field are described in Table 2.  The default value
    /// is AES-128.
    pub encryption_field: EncryptionField,
    /// This field is message specific extension
    /// related to Handshake Type field.  The value MUST be set to 0
    /// except for the following cases.  (1) If the handshake control
    /// packet is the INDUCTION message, this field is sent back by the
    /// Listener. (2) In the case of a CONCLUSION message, this field
    /// value should contain a combination of Extension Type values.  For
    /// more details, see Section 4.3.1.
    pub extension_field: ExtensionField,
    /// The sequence number of the very first data packet to be sent.
    pub initial_packet_sequence_number: u32,
    /// This value is typically set
    /// to 1500, which is the default Maximum Transmission Unit (MTU) size
    /// for Ethernet, but can be less.
    pub maximum_transmission_unit_size: u32,
    /// The value of this field is the
    /// maximum number of data packets allowed to be "in flight" (i.e. the
    /// number of sent packets for which an ACK control packet has not yet
    /// been received).
    pub maximum_flow_window_size: u32,
    /// This field indicates the handshake packet
    /// type.  The possible values are described in Table 4.  For more
    /// details refer to Section 4.3.
    pub handshake_type: HandshakeType,
    /// This field holds the ID of the source SRT
    /// socket from which a handshake packet is issued.
    pub srt_socket_id: u32,
    /// Randomized value for processing a handshake.
    /// The value of this field is specified by the handshake message
    /// type.  See Section 4.3.
    pub syn_cookie: u32,
    /// IPv4 or IPv6 address of the packet's
    /// sender.  The value consists of four 32-bit fields.  In the case of
    /// IPv4 addresses, fields 2, 3 and 4 are filled with zeroes.
    pub peer_ip_address: u128,
    pub extensions: Extensions,
}

/// The `Keep-Alive` packet.
#[derive(Clone, Debug, Default, Packet)]
pub struct Keepalive {
    pub header: KeepaliveHeader,
    _unused: u32,
}

impl Keepalive {
    pub fn builder() -> KeepaliveBuilder {
        KeepaliveBuilder::new()
    }
}

#[derive(Clone, Debug, Default, Packet)]
pub struct Ack {
    pub header: AckHeader,
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

/// A Light ACK control packet includes only the Last Acknowledged
/// Packet Sequence Number field.  The Type-specific Information field
/// should be set to 0.
#[derive(Clone, Debug, Default, Packet)]
pub struct LightAck {
    pub header: AckHeader,
    pub last_acknowledged_packet_sequence_number: u32,
}

impl LightAck {
    pub fn builder() -> LightAckBuilder {
        LightAckBuilder::new()
    }
}

#[derive(Clone, Debug, Default, Packet)]
pub struct SmallAck {
    pub header: AckHeader,
    pub last_acknowledged_packet_sequence_number: u32,
    pub rtt: u32,
    pub rtt_variance: u32,
    pub avaliable_buffer_size: u32,
}

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
#[derive(Clone, Debug, Packet)]
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

#[derive(Clone, Debug, Default, Packet)]
pub struct Shutdown {
    header: ShutdownHeader,
    _unused: u32,
}

impl Shutdown {
    pub fn builder() -> ShutdownBuilder {
        ShutdownBuilder::new()
    }
}

#[derive(Clone, Debug, Default, Packet)]
pub struct DropRequest {
    pub header: DropRequestHeader,
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
