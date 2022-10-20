//! Secure Reliable Transport (SRT) implementation.
//!
//! https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01
mod ack;
mod conn;
mod data;
mod handshake;
pub mod proto;
pub mod server;
mod shutdown;
pub mod state;

use conn::Connection;

use std::{
    convert::Infallible,
    io::{self, Read, Write},
};

use tokio::net::UdpSocket;

use crate::proto::{Bits, Decode, Encode, U32};

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

#[derive(Clone, Debug)]
pub struct DataPacket {
    header: Header,
    data: Vec<u8>,
}

impl DataPacket {
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
    extension_field: u16,
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
    /// The value of this field is used to process
    /// an integrated handshake.  Each extension can have a pair of
    /// request and response types.
    extension_type: u16,
    /// The length of the Extension Contents
    /// field in four-byte blocks.
    extension_length: u16,
    /// Extension Contents: variable length.  The payload of the extension.
    extension_contents: Vec<u8>,
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
        self.extension_type.encode(&mut writer)?;
        self.extension_length.encode(&mut writer)?;

        writer.write_all(&self.extension_contents)?;
        Ok(())
    }

    fn decode_body<R>(mut reader: R) -> Result<Self, Error>
    where
        R: Read,
    {
        let version = u32::decode(&mut reader)?;
        let encryption_field = u16::decode(&mut reader)?;
        let extension_field = u16::decode(&mut reader)?;
        let initial_packet_sequence_number = u32::decode(&mut reader)?;
        let maximum_transmission_unit_size = u32::decode(&mut reader)?;
        let maximum_flow_window_size = u32::decode(&mut reader)?;
        let handshake_type = HandshakeType::decode(&mut reader)?;
        let srt_socket_id = u32::decode(&mut reader)?;
        let syn_cookie = u32::decode(&mut reader)?;
        let peer_ip_address = u128::decode(&mut reader)?;
        // let extension_type = u16::decode(&mut reader)?;
        // let extension_length = u16::decode(&mut reader)?;
        // TODO: impl
        // let extension_contents = Vec::new();

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
            extension_type: 0,
            extension_length: 0,
            extension_contents: vec![],
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
        self.extension_type.encode(&mut writer)?;
        self.extension_length.encode(&mut writer)?;

        writer.write_all(&self.extension_contents)?;
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
        let extension_field = u16::decode(&mut reader)?;
        let initial_packet_sequence_number = u32::decode(&mut reader)?;
        let maximum_transmission_unit_size = u32::decode(&mut reader)?;
        let maximum_flow_window_size = u32::decode(&mut reader)?;
        let handshake_type = HandshakeType::decode(&mut reader)?;
        let srt_socket_id = u32::decode(&mut reader)?;
        let syn_cookie = u32::decode(&mut reader)?;
        let peer_ip_address = u128::decode(&mut reader)?;
        let extension_type = u16::decode(&mut reader)?;
        let extension_length = u16::decode(&mut reader)?;
        // TODO: impl
        let extension_contents = Vec::new();

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
            extension_type,
            extension_length,
            extension_contents,
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
pub enum HandshakeType {
    #[default]
    Induction,
    Wavehand,
    Conclusion,
    Agreement,
    Done,
}

impl Encode for HandshakeType {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        let val: u32 = match self {
            Self::Done => 0xFFFFFFFD,
            Self::Agreement => 0xFFFFFFFE,
            Self::Conclusion => 0xFFFFFFFF,
            Self::Wavehand => 0x00000000,
            Self::Induction => 0x00000001,
        };

        writer.write_all(&val.to_be_bytes())?;
        Ok(())
    }
}

impl Decode for HandshakeType {
    type Error = Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        match u32::decode(reader).unwrap() {
            0xFFFFFFFD => Ok(Self::Done),
            0xFFFFFFFE => Ok(Self::Agreement),
            0xFFFFFFFF => Ok(Self::Conclusion),
            0x00000000 => Ok(Self::Wavehand),
            0x00000001 => Ok(Self::Induction),
            val => Err(Error::InvalidHandshakeType(val)),
        }
    }
}

// 0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+- SRT Header +-+-+-+-+-+-+-+-+-+-+-+-+-+
// |1|         Control Type        |            Reserved           |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                   Type-specific Information                   |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                           Timestamp                           |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                     Destination Socket ID                     |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
pub struct Keepalive {
    header: Header,
}

#[derive(Clone, Debug)]
pub struct Packet {
    header: Header,
    body: Vec<u8>,
}

impl Packet {
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
    header: &'a Header,
}

impl<'a> DataHeader<'a> {
    // pub fn packet_position(&self) -> PacketPosition {}
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
