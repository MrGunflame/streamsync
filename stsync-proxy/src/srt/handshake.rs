use std::sync::Arc;

use tokio::net::UdpSocket;

use super::conn::Connection;
use super::state::State;
use super::{Error, HandshakePacket, HandshakeType, PacketType};
use crate::proto::{Decode, Encode};

pub async fn handshake(stream: &Arc<UdpSocket>, state: State) -> Result<u32, Error> {
    // Ethernet frame size for now.
    // TODO: Only stack-allocate the required size for HandshakePacket.
    let mut buf = [0; 1500];

    // Induction phase
    let (_, addr) = stream.recv_from(&mut buf).await?;

    let packet = HandshakePacket::decode(&buf[..])?;

    println!("{:?}", packet);

    assert_eq!(packet.header.packet_type(), PacketType::Control);
    assert_eq!(packet.version, 4);
    assert_eq!(packet.encryption_field, 0);
    assert_eq!(packet.extension_field, 2);
    assert_eq!(packet.handshake_type, HandshakeType::Induction);
    assert_eq!(packet.syn_cookie, 0);

    let caller_socket_id = packet.srt_socket_id;
    let listener_socket_id = 69u32;

    println!("Got valid INDUCTION from caller");

    let mut resp = HandshakePacket::default();
    resp.header.seg0.set_bits(0, 1);
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
    let (len, addr) = stream.recv_from(&mut buf).await?;

    let packet = HandshakePacket::decode(&buf[..])?;

    println!("{:?}", packet);

    assert_eq!(packet.version, 5);
    assert_eq!(packet.handshake_type, HandshakeType::Conclusion);
    assert_eq!(packet.srt_socket_id, caller_socket_id);
    assert_eq!(packet.syn_cookie, 420);
    assert_eq!(packet.encryption_field, 0);

    println!("Got Valid CONCLUSION from caller");

    stream.send_to(&buf[..len], addr).await?;

    let socket = stream.clone();
    tokio::task::spawn(async move {
        loop {
            // 10MS
            tokio::time::sleep(std::time::Duration::new(0, 10_000_000)).await;
        }
    });

    Ok(caller_socket_id)
}
