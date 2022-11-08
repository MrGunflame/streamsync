//! Secure Reliable Transport (SRT) implementation.
//!
//! https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01
mod buffer;
pub mod config;
mod conn;
mod handshake;
mod metrics;
pub mod proto;
pub mod server;
mod sink;
pub mod state;
mod stream;
mod utils;

use std::{
    collections::HashMap,
    convert::Infallible,
    fmt::Debug,
    io::{self, ErrorKind, Read, Write},
    ops::{BitAnd, BitOr},
    str::FromStr,
};

use crate::proto::{Bits, Decode, Encode, Zeroable, U32};

/// The SRT version supported by this library.
pub const VERSION: u32 = 0x00010501;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("invalid packet type: {0}")]
    InvalidPacketType(u8),
    #[error("invalid handshake type: {0}")]
    InvalidHandshakeType(u32),
    #[error("invalid control type: {0}")]
    InvalidControlType(u16),
    #[error("invalid extension type {0}")]
    InvalidExtensionType(u16),
    #[error("{0}")]
    FromUtf8Error(std::str::Utf8Error),
    #[error("unsupported extension {0:?}")]
    UnsupportedExtension(ExtensionType),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid control type: {0}")]
pub struct InvalidControlType(u16);

/// SRT header followed directly by UDP header.
#[derive(Copy, Clone, Debug, Default)]
pub struct Header {
    /// First bit indicates packet type: 0 = Data, 1 = Control.
    /// The rest is packet type dependent.
    seg0: Bits<U32>,
    /// Packet type dependant.
    seg1: Bits<U32>,
    timestamp: u32,
    destination_socket_id: u32,
}

impl Header {
    pub const SIZE: usize = 128;

    pub fn packet_type(&self) -> PacketType {
        // First BE bit.
        match self.seg0.bits(0).0 {
            0 => PacketType::Data,
            1 => PacketType::Control,
            _ => unreachable!(),
        }
    }

    pub fn set_packet_type(&mut self, type_: PacketType) {
        self.seg0.set_bits(0, type_.to_u32());
    }

    pub fn as_control(&mut self) -> Result<ControlHeader, Error> {
        match self.packet_type() {
            PacketType::Control => Ok(ControlHeader { header: self }),
            PacketType::Data => Err(Error::InvalidPacketType(0)),
        }
    }

    pub fn as_control_unchecked(&mut self) -> ControlHeader<'_> {
        ControlHeader { header: self }
    }

    pub fn as_data_unchecked(&mut self) -> DataHeader<'_> {
        DataHeader { header: self }
    }
}

impl Encode for Header {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.seg0.encode(&mut writer)?;
        self.seg1.encode(&mut writer)?;
        self.timestamp.encode(&mut writer)?;
        self.destination_socket_id.encode(&mut writer)?;

        Ok(())
    }

    fn size_hint(&self) -> usize {
        Encode::size_hint(&self.seg0)
            + Encode::size_hint(&self.seg1)
            + Encode::size_hint(&self.timestamp)
            + Encode::size_hint(&self.destination_socket_id)
    }
}

impl Decode for Header {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let seg0 = Decode::decode(&mut reader)?;
        let seg1 = Decode::decode(&mut reader)?;
        let timestamp = Decode::decode(&mut reader)?;
        let destination_socket_id = Decode::decode(&mut reader)?;

        Ok(Self {
            seg0,
            seg1,
            timestamp,
            destination_socket_id,
        })
    }
}

unsafe impl Zeroable for Header {}

#[derive(Clone, Debug, Default)]
pub struct DataPacket {
    header: Header,
    data: Vec<u8>,
}

impl DataPacket {
    pub fn header(&mut self) -> DataHeader<'_> {
        self.header.as_data_unchecked()
    }

    pub fn packet_sequence_number(&self) -> u32 {
        self.header.seg0.bits(1..32).0
    }

    pub fn packet_position_flag(&self) -> PacketPosition {
        match self.header.seg1.bits(0..2).0 {
            0b10 => PacketPosition::First,
            0b00 => PacketPosition::Middle,
            0b11 => PacketPosition::Full,
            _ => unreachable!(),
        }
    }

    pub fn order_flag(&self) -> OrderFlag {
        match self.header.seg1.bits(2).0 {
            1 => OrderFlag::InOrder,
            0 => OrderFlag::NotInOrder,
            _ => unreachable!(),
        }
    }

    pub fn encryption_flag(&self) -> EncryptionFlag {
        match self.header.seg1.bits(3..5).0 {
            0b00 => EncryptionFlag::None,
            0b01 => EncryptionFlag::Even,
            0b11 => EncryptionFlag::Odd,
            _ => unreachable!(),
        }
    }

    /// 1 if packet was retransmitted.
    pub fn retransmission_flag(&self) -> u8 {
        self.header.seg1.bits(5).0 as u8
    }

    pub fn message_number(&self) -> u32 {
        // 26 bits
        self.header.seg1.bits(6..32).0
    }
}

impl IsPacket for DataPacket {
    type Error = Error;

    fn upcast(self) -> Packet {
        Packet {
            header: self.header,
            body: self.data,
        }
    }

    fn downcast(packet: Packet) -> Result<Self, Self::Error> {
        Ok(Self {
            header: packet.header,
            data: packet.body,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ShutdownPacket {
    header: Header,
}

impl Encode for ShutdownPacket {
    type Error = Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.header.encode(writer)
    }
}

impl Decode for ShutdownPacket {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let header = Header::decode(reader)?;

        Ok(Self { header })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PacketType {
    Data,
    Control,
}

impl PacketType {
    pub fn to_u32(self) -> u32 {
        match self {
            Self::Data => 0x0,
            Self::Control => 0x01,
        }
    }
}

pub enum OrderFlag {
    InOrder,
    NotInOrder,
}

pub enum EncryptionFlag {
    None,
    Even,
    Odd,
}

pub struct ControlPacket {
    header: Header,
}

impl ControlPacket {
    pub fn control_type(&self) -> ControlPacketType {
        match self.header.seg0.bits(1..16).0 as u16 {
            0x0000 => ControlPacketType::Handshake,
            0x0001 => ControlPacketType::Keepalive,
            0x0002 => ControlPacketType::Ack,
            0x0003 => ControlPacketType::Nak,
            0x0004 => ControlPacketType::CongestionWarning,
            0x0005 => ControlPacketType::Shutdown,
            0x0006 => ControlPacketType::AckAck,
            0x0007 => ControlPacketType::DropReq,
            0x0008 => ControlPacketType::PeerError,
            0x7FFF => ControlPacketType::UserDefined,
            n => panic!("invalid control packet frame: {}", n),
        }
    }

    pub fn set_control_type(&mut self, type_: ControlPacketType) {
        self.header.seg0.set_bits(1..16, type_.to_u16() as u32)
    }

    pub fn subtype(&self) -> ControlSubType {
        unimplemented!()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ControlPacketType {
    Handshake = 0x00,
    Keepalive = 0x01,
    Ack = 0x02,
    Nak = 0x03,
    CongestionWarning = 0x04,
    Shutdown = 0x05,
    AckAck = 0x06,
    DropReq = 0x07,
    PeerError = 0x08,
    UserDefined = 0x7FFF,
}

impl ControlPacketType {
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0x0000 => Some(Self::Handshake),
            0x0001 => Some(Self::Keepalive),
            0x0002 => Some(Self::Ack),
            0x0003 => Some(Self::Nak),
            0x0004 => Some(Self::CongestionWarning),
            0x0005 => Some(Self::Shutdown),
            0x0006 => Some(Self::AckAck),
            0x0007 => Some(Self::DropReq),
            0x0008 => Some(Self::PeerError),
            0x7FFF => Some(Self::UserDefined),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            Self::Handshake => 0x00,
            Self::Keepalive => 0x01,
            Self::Ack => 0x02,
            Self::Nak => 0x03,
            Self::CongestionWarning => 0x04,
            Self::Shutdown => 0x05,
            Self::AckAck => 0x06,
            Self::DropReq => 0x07,
            Self::PeerError => 0x08,
            Self::UserDefined => 0x7FFF,
        }
    }
}

impl TryFrom<u16> for ControlPacketType {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x0000 => Ok(Self::Handshake),
            0x0001 => Ok(Self::Keepalive),
            0x0002 => Ok(Self::Ack),
            0x0003 => Ok(Self::Nak),
            0x0004 => Ok(Self::CongestionWarning),
            0x0005 => Ok(Self::Shutdown),
            0x0006 => Ok(Self::AckAck),
            0x0007 => Ok(Self::DropReq),
            0x0008 => Ok(Self::PeerError),
            0x7FFF => Ok(Self::UserDefined),
            _ => Err(()),
        }
    }
}

pub enum ControlSubType {}

#[derive(Clone, Debug, Default)]
pub struct HandshakePacket {
    header: Header,
    /// A base protocol version number.  Currently used
    /// values are 4 and 5.  Values greater than 5 are reserved for future
    /// use.
    version: u32,
    /// Block cipher family and key size.  The
    /// values of this field are described in Table 2.  The default value
    /// is AES-128.
    encryption_field: u16,
    /// This field is message specific extension
    /// related to Handshake Type field.  The value MUST be set to 0
    /// except for the following cases.  (1) If the handshake control
    /// packet is the INDUCTION message, this field is sent back by the
    /// Listener. (2) In the case of a CONCLUSION message, this field
    /// value should contain a combination of Extension Type values.  For
    /// more details, see Section 4.3.1.
    extension_field: ExtensionField,
    /// The sequence number of the very first data packet to be sent.
    initial_packet_sequence_number: u32,
    /// This value is typically set
    /// to 1500, which is the default Maximum Transmission Unit (MTU) size
    /// for Ethernet, but can be less.
    maximum_transmission_unit_size: u32,
    /// The value of this field is the
    /// maximum number of data packets allowed to be "in flight" (i.e. the
    /// number of sent packets for which an ACK control packet has not yet
    /// been received).
    maximum_flow_window_size: u32,
    /// This field indicates the handshake packet
    /// type.  The possible values are described in Table 4.  For more
    /// details refer to Section 4.3.
    handshake_type: HandshakeType,
    /// This field holds the ID of the source SRT
    /// socket from which a handshake packet is issued.
    srt_socket_id: u32,
    /// Randomized value for processing a handshake.
    /// The value of this field is specified by the handshake message
    /// type.  See Section 4.3.
    syn_cookie: u32,
    /// IPv4 or IPv6 address of the packet's
    /// sender.  The value consists of four 32-bit fields.  In the case of
    /// IPv4 addresses, fields 2, 3 and 4 are filled with zeroes.
    peer_ip_address: u128,
    extensions: Extensions,
}

impl HandshakePacket {
    fn encode_body<W>(&self, mut writer: W) -> Result<(), Error>
    where
        W: Write,
    {
        self.version.encode(&mut writer)?;
        self.encryption_field.encode(&mut writer)?;
        self.extension_field.encode(&mut writer)?;
        self.initial_packet_sequence_number.encode(&mut writer)?;
        self.maximum_transmission_unit_size.encode(&mut writer)?;
        self.maximum_flow_window_size.encode(&mut writer)?;
        self.handshake_type.encode(&mut writer)?;
        self.srt_socket_id.encode(&mut writer)?;
        self.syn_cookie.encode(&mut writer)?;
        self.peer_ip_address.encode(&mut writer)?;
        self.extensions.encode(writer)?;

        Ok(())
    }

    fn decode_body<R>(mut reader: R) -> Result<Self, Error>
    where
        R: Read,
    {
        let version = u32::decode(&mut reader)?;
        let encryption_field = u16::decode(&mut reader)?;
        let extension_field = ExtensionField::decode(&mut reader)?;
        let initial_packet_sequence_number = u32::decode(&mut reader)?;
        let maximum_transmission_unit_size = u32::decode(&mut reader)?;
        let maximum_flow_window_size = u32::decode(&mut reader)?;
        let handshake_type = HandshakeType::decode(&mut reader)?;
        let srt_socket_id = u32::decode(&mut reader)?;
        let syn_cookie = u32::decode(&mut reader)?;
        let peer_ip_address = u128::decode(&mut reader)?;
        let extensions = Extensions::decode(reader)?;

        Ok(Self {
            header: Header::default(),
            version,
            encryption_field,
            extension_field,
            initial_packet_sequence_number,
            maximum_transmission_unit_size,
            maximum_flow_window_size,
            handshake_type,
            srt_socket_id,
            syn_cookie,
            peer_ip_address,
            extensions,
        })
    }
}

impl IsPacket for HandshakePacket {
    type Error = Error;

    fn upcast(self) -> Packet {
        let mut body = Vec::new();
        self.encode_body(&mut body).unwrap();

        Packet {
            header: self.header,
            body,
        }
    }

    fn downcast(mut packet: Packet) -> Result<Self, Self::Error> {
        let header = packet.header.as_control()?;
        if header.control_type() != ControlPacketType::Handshake {
            return Err(Error::InvalidControlType(header.control_type().to_u16()));
        }

        let mut this = Self::decode_body(&packet.body[..])?;
        this.header = packet.header;

        Ok(this)
    }
}

impl Encode for HandshakePacket {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.header.encode(&mut writer)?;
        self.version.encode(&mut writer)?;
        self.encryption_field.encode(&mut writer)?;
        self.extension_field.encode(&mut writer)?;
        self.initial_packet_sequence_number.encode(&mut writer)?;
        self.maximum_transmission_unit_size.encode(&mut writer)?;
        self.maximum_flow_window_size.encode(&mut writer)?;
        self.handshake_type.encode(&mut writer)?;
        self.srt_socket_id.encode(&mut writer)?;
        self.syn_cookie.encode(&mut writer)?;
        self.peer_ip_address.encode(&mut writer)?;
        self.extensions.encode(&mut writer)?;

        Ok(())
    }
}

impl Decode for HandshakePacket {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let header = Header::decode(&mut reader)?;

        let version = u32::decode(&mut reader)?;
        let encryption_field = u16::decode(&mut reader)?;
        let extension_field = ExtensionField::decode(&mut reader)?;
        let initial_packet_sequence_number = u32::decode(&mut reader)?;
        let maximum_transmission_unit_size = u32::decode(&mut reader)?;
        let maximum_flow_window_size = u32::decode(&mut reader)?;
        let handshake_type = HandshakeType::decode(&mut reader)?;
        let srt_socket_id = u32::decode(&mut reader)?;
        let syn_cookie = u32::decode(&mut reader)?;
        let peer_ip_address = u128::decode(&mut reader)?;
        let extensions = Extensions::decode(reader)?;

        Ok(Self {
            header,
            version,
            encryption_field,
            extension_field,
            initial_packet_sequence_number,
            maximum_transmission_unit_size,
            maximum_flow_window_size,
            handshake_type,
            srt_socket_id,
            syn_cookie,
            peer_ip_address,
            extensions,
        })
    }
}

pub enum EncryptionField {
    None,
    AES128,
    AES192,
    AES256,
}

// +============+================+
// | Value      | Handshake Type |
// +============+================+
// | 0xFFFFFFFD |      DONE      |
// +------------+----------------+
// | 0xFFFFFFFE |   AGREEMENT    |
// +------------+----------------+
// | 0xFFFFFFFF |   CONCLUSION   |
// +------------+----------------+
// | 0x00000000 |    WAVEHAND    |
// +------------+----------------+
// | 0x00000001 |   INDUCTION    |
// +------------+----------------+
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct HandshakeType(u32);

impl HandshakeType {
    pub const DONE: Self = Self(0xFFFFFFFD);
    pub const AGREEMENT: Self = Self(0xFFFFFFFE);
    pub const CONCLUSION: Self = Self(0xFFFFFFFF);
    pub const WAVEHAND: Self = Self(0x00000000);
    pub const INDUCTION: Self = Self(0x00000001);

    // REJECTION CODES (https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-4.3)

    /// Unknown reason
    pub const REJ_UNKNOWN: Self = Self(1000);

    /// System function error
    pub const REJ_SYSTEM: Self = Self(1001);

    /// Rejected by peer
    pub const REJ_PEER: Self = Self(1002);

    /// Resource allocation problem
    pub const REJ_RESOURCE: Self = Self(1003);

    /// incorrect data in handshake
    pub const REJ_ROGUE: Self = Self(1004);

    /// listener's backlog exceeded
    pub const REJ_BACKLOG: Self = Self(1005);

    /// internal program error
    pub const REJ_IPE: Self = Self(1006);

    /// socket is closing
    pub const REJ_CLOSE: Self = Self(1007);

    /// peer is older version than agent's min
    pub const REJ_VERSION: Self = Self(1008);

    /// rendezvous cookie collision
    pub const REJ_RDVCOOKIE: Self = Self(1009);

    /// wrong password
    pub const REJ_BADSECRET: Self = Self(1010);

    /// password required or unexpected
    pub const REJ_INSECURE: Self = Self(1011);

    /// Stream flag collision
    pub const REJ_MESSAGEAPI: Self = Self(1012);

    /// incompatible congestion-controller type
    pub const REJ_CONGESTION: Self = Self(1013);

    /// incompatible packet filter
    pub const REJ_FILTER: Self = Self(1014);

    /// incompatible group
    pub const REJ_GROUP: Self = Self(1015);

    pub fn to_u32(self) -> u32 {
        self.0
    }

    pub fn from_u32(n: u32) -> Option<Self> {
        match n {
            n if n == Self::DONE.0 => Some(Self::DONE),
            n if n == Self::AGREEMENT.0 => Some(Self::AGREEMENT),
            n if n == Self::CONCLUSION.0 => Some(Self::CONCLUSION),
            n if n == Self::WAVEHAND.0 => Some(Self::WAVEHAND),
            n if n == Self::INDUCTION.0 => Some(Self::INDUCTION),
            n if n == Self::REJ_UNKNOWN.0 => Some(Self::REJ_UNKNOWN),
            n if n == Self::REJ_SYSTEM.0 => Some(Self::REJ_SYSTEM),
            n if n == Self::REJ_PEER.0 => Some(Self::REJ_PEER),
            n if n == Self::REJ_RESOURCE.0 => Some(Self::REJ_RESOURCE),
            n if n == Self::REJ_ROGUE.0 => Some(Self::REJ_ROGUE),
            n if n == Self::REJ_BACKLOG.0 => Some(Self::REJ_BACKLOG),
            n if n == Self::REJ_IPE.0 => Some(Self::REJ_IPE),
            n if n == Self::REJ_CLOSE.0 => Some(Self::REJ_CLOSE),
            n if n == Self::REJ_VERSION.0 => Some(Self::REJ_VERSION),
            n if n == Self::REJ_RDVCOOKIE.0 => Some(Self::REJ_RDVCOOKIE),
            n if n == Self::REJ_BADSECRET.0 => Some(Self::REJ_BADSECRET),
            n if n == Self::REJ_INSECURE.0 => Some(Self::REJ_INSECURE),
            n if n == Self::REJ_MESSAGEAPI.0 => Some(Self::REJ_MESSAGEAPI),
            n if n == Self::REJ_CONGESTION.0 => Some(Self::REJ_CONGESTION),
            n if n == Self::REJ_FILTER.0 => Some(Self::REJ_FILTER),
            n if n == Self::REJ_GROUP.0 => Some(Self::REJ_GROUP),
            _ => None,
        }
    }

    pub fn is_done(self) -> bool {
        self == Self::DONE
    }

    pub fn is_agreement(self) -> bool {
        self == Self::AGREEMENT
    }

    pub fn is_conclusion(self) -> bool {
        self == Self::CONCLUSION
    }

    pub fn is_wavehand(self) -> bool {
        self == Self::WAVEHAND
    }

    pub fn is_induction(self) -> bool {
        self == Self::INDUCTION
    }

    pub fn is_rejection(self) -> bool {
        self.0 >= Self::REJ_UNKNOWN.0 && self.0 <= Self::REJ_GROUP.0
    }
}

impl Encode for HandshakeType {
    type Error = Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.0.encode(writer)?;
        Ok(())
    }
}

impl Decode for HandshakeType {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let n = u32::decode(reader)?;
        match Self::from_u32(n) {
            Some(this) => Ok(this),
            None => Err(Error::InvalidHandshakeType(n)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    header: Header,
    body: Vec<u8>,
}

impl IsPacket for Packet {
    type Error = Infallible;

    #[inline]
    fn downcast(packet: Packet) -> Result<Self, Self::Error> {
        Ok(packet)
    }

    #[inline]
    fn upcast(self) -> Packet {
        self
    }
}

impl Packet {
    /// Returns the total size (incl. header) of the packet.
    pub fn size(&self) -> usize {
        Header::SIZE + self.body.len()
    }

    pub fn downcast<T>(self) -> Result<T, Error>
    where
        T: IsPacket<Error = Error>,
    {
        T::downcast(self)
    }
}

impl Encode for Packet {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.header.encode(&mut writer)?;
        self.body.encode(&mut writer)?;
        Ok(())
    }

    fn size_hint(&self) -> usize {
        Encode::size_hint(&self.header) + Encode::size_hint(self.body.as_slice())
    }
}

impl Decode for Packet {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let header = Header::decode(&mut reader)?;
        let body = Vec::decode(reader)?;

        Ok(Self { header, body })
    }
}

/// Header for a control packet.
#[derive(Debug)]
pub struct ControlHeader<'a> {
    header: &'a mut Header,
}

impl<'a> ControlHeader<'a> {
    pub fn control_type(&self) -> ControlPacketType {
        let bits = self.header.seg0.bits(1..16).0;

        ControlPacketType::from_u32(bits).unwrap()
    }

    pub fn set_control_type(&mut self, type_: ControlPacketType) {
        self.header.seg0.set_bits(1..16, type_.to_u16() as u32)
    }
}

/// Header for a data packet.
pub struct DataHeader<'a> {
    header: &'a mut Header,
}

impl<'a> DataHeader<'a> {
    pub fn packet_sequence_number(&self) -> u32 {
        self.header.seg0.bits(1..32).0
    }

    pub fn set_packet_sequence_number(&mut self, n: u32) {
        self.header.seg0.set_bits(1..32, n);
    }

    pub fn packet_position(&self) -> PacketPosition {
        match self.header.seg1.bits(0..2).0 {
            0b10 => PacketPosition::First,
            0b00 => PacketPosition::Middle,
            0b01 => PacketPosition::Last,
            0b11 => PacketPosition::Full,
            _ => unreachable!(),
        }
    }

    pub fn set_packet_position(&mut self, pos: PacketPosition) {
        let val = match pos {
            PacketPosition::First => 0b10,
            PacketPosition::Middle => 0b00,
            PacketPosition::Last => 0b01,
            PacketPosition::Full => 0b11,
        };

        self.header.seg1.set_bits(0..2, val);
    }

    pub fn is_ordered(&self) -> bool {
        self.header.seg1.bits(2) != 0
    }

    pub fn set_ordered(&mut self, n: bool) {
        self.header.seg1.set_bits(2, n as u32)
    }

    pub fn kk(&self) -> ! {
        unimplemented!()
    }

    pub fn set_kk(&mut self) -> ! {
        unimplemented!()
    }

    pub fn is_retransmitted(&self) -> bool {
        self.header.seg1.bits(5) != 0
    }

    pub fn set_retransmitted(&mut self, n: bool) {
        self.header.seg1.set_bits(5, n as u32);
    }

    pub fn message_number(&self) -> u32 {
        self.header.seg1.bits(6..32).0
    }

    pub fn set_message_number(&mut self, n: u32) {
        self.header.seg1.set_bits(6..32, n);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PacketPosition {
    /// The packet is the first packet of the data stream.
    First,
    /// The packet is in the middle of the data stream.
    Middle,
    /// The packet is the last packet of the data stream.
    Last,
    /// The packet contains a full data stream.
    Full,
}

#[derive(Clone, Debug, Default)]
pub struct AckPacket {
    header: Header,
    /// This field
    /// contains the sequence number of the last data packet being
    /// acknowledged plus one.  In other words, if it the sequence number
    /// of the first unacknowledged packet.
    last_acknowledged_packet_sequence_number: u32,
    /// RTT value, in microseconds, estimated by the receiver
    /// based on the previous ACK/ACKACK packet pair exchange.
    rtt: u32,
    /// The variance of the RTT estimate, in
    /// microseconds.
    rtt_variance: u32,
    /// Available size of the receiver's
    /// buffer, in packets.
    avaliable_buffer_size: u32,
    /// The rate at which packets are being
    /// received, in packets per second.
    packets_receiving_rate: u32,
    /// Estimated bandwidth of the link,
    /// in packets per second.
    estimated_link_capacity: u32,
    /// Estimated receiving rate, in bytes per
    /// second.
    receiving_rate: u32,
}

impl AckPacket {
    /// This field contains the sequential
    /// number of the full acknowledgment packet starting from 1.
    pub fn acknowledgement_number(&self) -> u32 {
        self.header.seg1.0 .0
    }
}

impl Encode for AckPacket {
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

#[derive(Clone, Debug, Default)]
#[repr(transparent)]
pub struct Extensions(pub Vec<HandshakeExtension>);

impl Extensions {
    pub fn hsreq(&self) -> Option<HandshakeExtensionMessage> {
        for ext in &self.0 {
            if let ExtensionContent::Handshake(ext) = ext.extension_content {
                return Some(ext);
            }
        }

        None
    }

    pub fn remove_hsreq(&mut self) -> Option<HandshakeExtensionMessage> {
        let mut index: usize = 0;

        while index < self.0.len() {
            let ext = unsafe { self.0.get_unchecked(index) };

            if ext.extension_type == ExtensionType::HSREQ {
                if let ExtensionContent::Handshake(ext) = ext.extension_content {
                    self.0.remove(index);
                    return Some(ext);
                }
            }

            index += 1;
        }

        None
    }

    pub fn remove_stream_id(&mut self) -> Option<StreamIdExtension> {
        let mut index: usize = 0;

        while index < self.0.len() {
            let ext = unsafe { self.0.get_unchecked(index) };

            if ext.extension_type == ExtensionType::SID {
                if let ExtensionContent::StreamId(ext) = ext.extension_content.clone() {
                    self.0.remove(index);
                    return Some(ext);
                }
            }

            index += 1;
        }

        None
    }
}

impl Encode for Extensions {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        for ext in &self.0 {
            ext.encode(&mut writer)?;
        }

        Ok(())
    }
}

impl Decode for Extensions {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        // TODO: Preallocate
        let mut extensions = Vec::new();
        loop {
            match HandshakeExtension::decode(&mut reader) {
                Ok(ext) => {
                    extensions.push(ext);
                }
                Err(Error::Io(err)) => {
                    if err.kind() == ErrorKind::UnexpectedEof {
                        return Ok(Self(extensions));
                    } else {
                        return Err(err.into());
                    }
                }
                Err(err) => return Err(err),
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PeerIpAddress([u8; 8]);

impl PeerIpAddress {}

/// A packet type.
pub trait IsPacket: Sized {
    type Error;

    /// Upcasts this packet type into a generic [`Packet`].
    fn upcast(self) -> Packet;

    /// Attempts to downcast a generic [`Packet`] into this packet type. Returns an error if the
    /// `packet` cannot be downcasted into this type.
    ///
    /// Note that `downcast` only reads from the front. Any bytes unnecessary for the downcasting
    /// process will be dropped.
    fn downcast(packet: Packet) -> Result<Self, Self::Error>;
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ExtensionType(u16);

impl ExtensionType {
    pub const HSREQ: Self = Self(1);
    pub const HSRSP: Self = Self(2);
    pub const KMREQ: Self = Self(3);
    pub const KMRSP: Self = Self(4);
    pub const SID: Self = Self(5);
    pub const CONGESTION: Self = Self(6);
    pub const FILTER: Self = Self(7);
    pub const GROUP: Self = Self(8);

    pub const fn to_u16(self) -> u16 {
        self.0
    }

    pub const fn from_u16(n: u16) -> Option<Self> {
        match n {
            n if n == Self::HSREQ.0 => Some(Self::HSREQ),
            n if n == Self::HSRSP.0 => Some(Self::HSRSP),
            n if n == Self::KMREQ.0 => Some(Self::KMREQ),
            n if n == Self::KMRSP.0 => Some(Self::KMRSP),
            n if n == Self::SID.0 => Some(Self::SID),
            n if n == Self::CONGESTION.0 => Some(Self::CONGESTION),
            n if n == Self::FILTER.0 => Some(Self::FILTER),
            n if n == Self::GROUP.0 => Some(Self::GROUP),
            _ => None,
        }
    }
}

impl Encode for ExtensionType {
    type Error = Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.to_u16().encode(writer)?;
        Ok(())
    }
}

impl Decode for ExtensionType {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let n = u16::decode(reader)?;
        match Self::from_u16(n) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidExtensionType(n)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HandshakeExtension {
    pub extension_type: ExtensionType,
    /// Length of the content **IN FOUR-BYTE GROUPS**. In order word to get the length of 3
    /// multiply by 3: `let bytes = extension_length * 3;`.
    pub extension_length: u16,
    pub extension_content: ExtensionContent,
}

impl HandshakeExtension {}

impl Encode for HandshakeExtension {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.extension_type.encode(&mut writer)?;
        self.extension_length.encode(&mut writer)?;

        match &self.extension_content {
            ExtensionContent::Handshake(ext) => ext.encode(writer),
            ExtensionContent::KeyMaterial(ext) => ext.encode(writer),
            ExtensionContent::StreamId(ext) => ext.encode(writer),
            ExtensionContent::Group(_) => Ok(()),
        }
    }
}

impl Decode for HandshakeExtension {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let extension_type = ExtensionType::decode(&mut reader)?;
        let extension_length = u16::decode(&mut reader)?;

        let extension_content = match extension_type {
            ExtensionType::HSREQ | ExtensionType::HSRSP => {
                ExtensionContent::Handshake(HandshakeExtensionMessage::decode(reader)?)
            }
            ExtensionType::SID => ExtensionContent::StreamId(StreamIdExtension::decode(reader)?),
            _ => return Err(Error::UnsupportedExtension(extension_type)),
        };

        Ok(Self {
            extension_type,
            extension_length,
            extension_content,
        })
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct HandshakeExtensionMessage {
    pub srt_version: u32,
    pub srt_flags: HandshakeExtensionFlags,
    pub receiver_tsbpd_delay: u16,
    pub sender_tsbpd_delay: u16,
}

impl Encode for HandshakeExtensionMessage {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.srt_version.encode(&mut writer)?;
        self.srt_flags.encode(&mut writer)?;
        self.receiver_tsbpd_delay.encode(&mut writer)?;
        self.sender_tsbpd_delay.encode(&mut writer)?;

        Ok(())
    }
}

impl Decode for HandshakeExtensionMessage {
    type Error = Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let srt_version = u32::decode(&mut reader)?;
        let srt_flags = HandshakeExtensionFlags::decode(&mut reader)?;
        let receiver_tsbpd_delay = u16::decode(&mut reader)?;
        let sender_tsbpd_delay = u16::decode(&mut reader)?;

        Ok(Self {
            srt_version,
            srt_flags,
            receiver_tsbpd_delay,
            sender_tsbpd_delay,
        })
    }
}

/// https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-3.2.1.1.1
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct HandshakeExtensionFlags(u32);

impl HandshakeExtensionFlags {
    /// TSBPDSND flag defines if the TSBPD mechanism (Section 4.5) will be
    /// used for sending.
    pub const TSBPDSND: Self = Self(1);

    /// TSBPDRCV flag defines if the TSBPD mechanism (Section 4.5) will be
    /// used for receiving.
    pub const TSBPDRCV: Self = Self(1 << 1);

    /// CRYPT flag MUST be set.  It is a legacy flag that indicates the
    /// party understands KK field of the SRT Packet (Figure 3).
    pub const CRYPT: Self = Self(1 << 2);

    /// TLPKTDROP flag should be set if too-late packet drop mechanism
    /// will be used during transmission.  See Section 4.6.
    pub const TLPKTDROP: Self = Self(1 << 3);

    /// PERIODICNAK flag set indicates the peer will send periodic NAK
    /// packets.  See Section 4.8.2.
    pub const PERIODICNAK: Self = Self(1 << 4);

    /// REXMITFLG flag MUST be set.  It is a legacy flag that indicates
    /// the peer understands the R field of the SRT DATA Packet
    /// (Figure 3).
    pub const REXMITFLG: Self = Self(1 << 5);

    /// STREAM flag identifies the transmission mode (Section 4.2) to be
    /// used in the connection.  If the flag is set, the buffer mode
    /// (Section 4.2.2) is used.  Otherwise, the message mode
    /// (Section 4.2.1) is used.
    pub const STREAM: Self = Self(1 << 6);

    /// PACKET_FILTER flag indicates if the peer supports packet filter.
    pub const PACKET_FILTER: Self = Self(1 << 7);

    pub const fn to_u32(self) -> u32 {
        self.0
    }

    // TODO: FILTER ONLY VALID BITS
    pub const fn from_u32(n: u32) -> Option<Self> {
        Some(Self(n))
    }
}

impl BitAnd for HandshakeExtensionFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for HandshakeExtensionFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl Encode for HandshakeExtensionFlags {
    type Error = Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.0.encode(writer)?;
        Ok(())
    }
}

impl Decode for HandshakeExtensionFlags {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        Ok(Self(u32::decode(reader)?))
    }
}

#[derive(Clone, Debug)]
pub struct KeyMaterialExtension {
    // TOOO: impl
}

impl Encode for KeyMaterialExtension {
    type Error = Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        Ok(())
    }
}

impl Decode for KeyMaterialExtension {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        Ok(Self {})
    }
}

/// A UTF-8 string with up to 512 bytes.
///
/// The string is internally encoded little-endian words (u32).
#[derive(Clone, Debug, Default)]
pub struct StreamIdExtension {
    pub content: String,
}

impl StreamIdExtension {
    pub fn parse(&self) -> Result<StandardStreamId, StandardStreamIdError> {
        self.content.parse()
    }
}

impl Encode for StreamIdExtension {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        let mut buf = self.content.as_bytes();

        while buf.len() >= 4 {
            // Copy the chunk from the buffer.
            let mut bytes = [0; 4];

            // SAFETY: `bytes` always has exactly 4 bytes of space.
            unsafe { std::ptr::copy_nonoverlapping(buf.as_ptr(), bytes.as_mut_ptr(), 4) };

            bytes.reverse();

            writer.write_all(&bytes)?;

            buf = &buf[4..];
        }

        // Write the remaining chunk.
        if buf.len() > 0 {
            // Create a new buffer initialized with zero. Copy the remaining buffer into the new
            // buffer. All remaining bytes stay zero as required.
            let mut bytes = [0; 4];

            // SAFETY: `buf` is guaranteed to have less than 4 bytes remaining, meaning
            // we always have enough room in the new buffer.
            debug_assert!(buf.len() < 4);
            unsafe {
                std::ptr::copy_nonoverlapping(buf.as_ptr(), bytes.as_mut_ptr(), buf.len());
            }

            bytes.reverse();

            writer.write_all(&bytes)?;
        }

        Ok(())
    }
}

impl Decode for StreamIdExtension {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let mut vec = Vec::decode(reader)?;
        let mut buf = &mut *vec;

        let mut string = String::with_capacity(buf.len());

        while let Some(mut bytes) = buf.get_mut(0..4) {
            // Remove any trailing filler.
            while bytes.starts_with(&[0]) {
                bytes = &mut bytes[1..];
            }

            bytes.reverse();
            let chunk = match std::str::from_utf8(bytes) {
                Ok(c) => c,
                Err(err) => return Err(Error::FromUtf8Error(err)),
            };

            string.push_str(chunk);
            buf = &mut buf[4..];
        }

        Ok(Self { content: string })
    }
}

#[derive(Debug)]
pub enum StandardStreamIdError {
    InvalidPrefix,
    InvalidKeyValueFormat,
}

/// The default, recommended structure for the [`StreamIdExtension`].
///
/// Note that the nested block syntax is currently unsupported.
///
/// See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#appendix-B.1
#[derive(Clone, Debug)]
pub struct StandardStreamId {
    map: HashMap<String, String>,
}

impl StandardStreamId {
    pub fn user(&self) -> Option<&str> {
        self.map.get("u").map(|s| s.as_str())
    }

    pub fn resource(&self) -> Option<&str> {
        self.map.get("r").map(|s| s.as_str())
    }

    pub fn host(&self) -> Option<&str> {
        self.map.get("h").map(|s| s.as_str())
    }

    pub fn session(&self) -> Option<&str> {
        self.map.get("s").map(|s| s.as_str())
    }

    pub fn type_(&self) -> Option<&str> {
        self.map.get("t").map(|s| s.as_str())
    }

    pub fn mode(&self) -> Option<&str> {
        self.map.get("m").map(|s| s.as_str())
    }
}

impl FromStr for StandardStreamId {
    type Err = StandardStreamIdError;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        // Strip the common prefix.
        s = match s.strip_prefix("#!::") {
            Some(s) => s,
            None => return Err(StandardStreamIdError::InvalidPrefix),
        };

        let mut map = HashMap::new();

        for pair in s.split(',') {
            let (key, val) = match pair.split_once('=') {
                Some((key, val)) => (key, val),
                None => return Err(StandardStreamIdError::InvalidKeyValueFormat),
            };

            map.insert(key.to_owned(), val.to_owned());
        }

        Ok(Self { map })
    }
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct ExtensionField(u16);

impl ExtensionField {
    pub const NONE: Self = Self(0);

    /// The initial value for the INDUCTION phase.
    pub const INDUCTION: Self = Self(2);
    /// The srt magic `0x4A17` for the CONCLUSION phase.
    pub const SRT_MAGIC: Self = Self(0x4A17);

    pub const HSREQ: Self = Self(1);
    pub const KMREQ: Self = Self(1 << 1);
    pub const CONFIG: Self = Self(1 << 2);

    pub fn is_magic(&self) -> bool {
        *self == Self::SRT_MAGIC
    }

    pub const fn hsreq(&self) -> bool {
        self.0 & Self::HSREQ.0 != 0
    }

    pub const fn kmreg(&self) -> bool {
        self.0 & Self::KMREQ.0 != 0
    }

    pub const fn config(&self) -> bool {
        self.0 & Self::CONFIG.0 != 0
    }
}

impl Encode for ExtensionField {
    type Error = io::Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.0.encode(writer)
    }
}

impl Decode for ExtensionField {
    type Error = io::Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        Ok(Self(u16::decode(reader)?))
    }
}

impl Debug for ExtensionField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hsreq = if self.hsreq() { "HSREQ, " } else { "" };
        let kmreq = if self.kmreg() { "KMREQ, " } else { "" };
        let config = if self.config() { "CONFIG" } else { "" };

        write!(f, "ExtensionField {{ {}{}{} }}", hsreq, kmreq, config)
    }
}

/// See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-3.2.1
#[derive(Clone, Debug)]
pub enum ExtensionContent {
    Handshake(HandshakeExtensionMessage),
    KeyMaterial(KeyMaterialExtension),
    StreamId(StreamIdExtension),
    /// Unimplemented in current standart.
    Group(()),
}

impl ExtensionContent {
    pub fn len(&self) -> u32 {
        match self {
            Self::Handshake(_) => 3,
            // Unimplemented
            Self::KeyMaterial(_) => 0,
            Self::StreamId(ext) => {
                let len = ext.content.len() as u32;
                match len % 3 {
                    0 => len,
                    1 => len + 2,
                    2 => len + 1,
                    _ => unreachable!(),
                }
            }
            // unimplemented
            Self::Group(_) => 0,
        }
    }
}

impl From<HandshakeExtensionMessage> for ExtensionContent {
    fn from(src: HandshakeExtensionMessage) -> Self {
        Self::Handshake(src)
    }
}

#[cfg(test)]
mod tests {
    use crate::proto::{Decode, Encode};

    use super::StreamIdExtension;

    #[test]
    fn test_streamid_extension() {
        let buf = [
            0x3a, 0x3a, 0x21, 0x23, 0x65, 0x72, 0x3d, 0x6d, 0x73, 0x65, 0x75, 0x71, 0x3d, 0x72,
            0x2c, 0x74, 0x35, 0x33, 0x32, 0x31,
        ];

        let ext = StreamIdExtension::decode(&buf[..]).unwrap();
        assert_eq!(ext.content, "#!::m=request,r=1235");

        assert_eq!(ext.encode_to_vec().unwrap(), buf);
    }
}
