//! Secure Reliable Transport (SRT) implementation.
//!
//! https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01

use std::io::{self, Read, Write};

use tokio::net::UdpSocket;

use crate::proto::{Decode, Encode};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("invalid handshake type: {0}")]
    InvalidHandshakeType(u32),
}

/// SRT header followed directly by UDP header.
#[derive(Copy, Clone, Debug, Default)]
pub struct Header {
    oct0: u32,
    oct1: u32,
    timestamp: u32,
    destination_socket_id: u32,
}

impl Header {
    pub fn flag(&self) -> PacketType {
        // First BE bit.
        if self.oct0 & (1 << 31) == 0 {
            PacketType::Data
        } else {
            PacketType::Control
        }
    }
}

impl Encode for Header {
    type Error = Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.oct0.encode(&mut writer)?;
        self.oct1.encode(&mut writer)?;
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
        let oct0 = u32::decode(&mut reader)?;
        let oct1 = u32::decode(&mut reader)?;
        let timestamp = u32::decode(&mut reader)?;
        let destination_socket_id = u32::decode(&mut reader)?;

        Ok(Self {
            oct0,
            oct1,
            timestamp,
            destination_socket_id,
        })
    }
}

pub struct DataPacket {
    header: Header,
    // TODO: impl
    data: Vec<u8>,
}

impl DataPacket {
    pub fn packet_sequence_number(&self) -> u32 {
        self.header.oct0 >> 1
    }

    pub fn packet_position_flag(&self) -> u8 {
        self.header.oct1 as u8 & 0b11
    }

    pub fn order_flag(&self) -> OrderFlag {
        match (self.header.oct1 as u8 >> 2) & 0b1 {
            0 => OrderFlag::NotInOrder,
            1 => OrderFlag::InOrder,
            _ => unreachable!(),
        }
    }

    pub fn encryption_flag(&self) -> EncryptionFlag {
        match (self.header.oct1 as u8 >> 3) & 0b11 {
            0b00 => EncryptionFlag::None,
            0b01 => EncryptionFlag::Even,
            0b11 => EncryptionFlag::Odd,
            _ => unreachable!(),
        }
    }

    pub fn retransmission_flag(&self) -> u8 {
        (self.header.oct1 as u8 >> 5) & 0b1
    }

    pub fn message_number(&self) -> u32 {
        // 26 bits
        self.header.oct1 >> 6
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PacketType {
    Data,
    Control,
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
        match self.header.oct0 as u16 >> 1 {
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

    pub fn subtype(&self) -> ControlSubType {
        unimplemented!()
    }
}

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

impl HandshakePacket {}

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
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
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

pub async fn handshake(mut stream: UdpSocket) -> Result<Connection, Error> {
    // Ethernet frame size for now.
    // TODO: Only stack-allocate the required size for HandshakePacket.
    let mut buf = [0; 1500];

    // Induction phase
    let (_, addr) = stream.recv_from(&mut buf).await?;

    let packet = HandshakePacket::decode(&buf[..])?;

    println!("{:?}", packet);

    assert_eq!(packet.header.flag(), PacketType::Control);
    assert_eq!(packet.version, 4);
    assert_eq!(packet.encryption_field, 0);
    assert_eq!(packet.extension_field, 2);
    assert_eq!(packet.handshake_type, HandshakeType::Induction);
    assert_eq!(packet.syn_cookie, 0);

    let caller_socket_id = packet.srt_socket_id;
    let listener_socket_id = 69u32;

    println!("Got valid INDUCTION from caller");

    let mut resp = HandshakePacket::default();
    resp.header.oct0 = 1 << 31;
    resp.header.timestamp = packet.header.timestamp + 1;

    resp.initial_packet_sequence_number = 12345;
    resp.maximum_transmission_unit_size = 1500;
    resp.maximum_flow_window_size = 8192;
    resp.peer_ip_address += u32::from_be_bytes([127, 0, 0, 1]) as u128;

    // SRT
    resp.version = 5;
    resp.encryption_field = 0;
    resp.extension_field = 0x4A17;
    resp.handshake_type = HandshakeType::Induction;
    resp.srt_socket_id = listener_socket_id;
    // "random"
    resp.syn_cookie = 420;

    let mut buf = Vec::new();
    resp.encode(&mut buf)?;
    stream.send_to(&buf, addr).await?;

    println!("{:?}", resp);
    println!("{:?}", buf);
    println!("Send INDUCTION to caller");

    // Conclusion

    let mut buf = [0; 1500];
    let (_, addr) = stream.recv_from(&mut buf).await?;

    let packet = HandshakePacket::decode(&buf[..])?;

    println!("{:?}", packet);

    assert_eq!(packet.version, 5);
    assert_eq!(packet.handshake_type, HandshakeType::Conclusion);
    assert_eq!(packet.srt_socket_id, caller_socket_id);
    assert_eq!(packet.syn_cookie, 69);
    assert_eq!(packet.encryption_field, 0);

    println!("Got Valid CONCLUSION from caller");

    stream.send_to(&buf, addr).await?;

    Ok(Connection {})
}

pub struct Connection {}
