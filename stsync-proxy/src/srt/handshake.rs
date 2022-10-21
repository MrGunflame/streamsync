use std::sync::atomic::{AtomicU32, Ordering};

use super::server::{SrtConnStream, SrtStream};
use super::state::{Connection, State};
use super::{Error, HandshakePacket, HandshakeType, PacketType};
use crate::proto::Encode;
use crate::sink::MultiSink;

static SRV_SOCKET_ID: AtomicU32 = AtomicU32::new(0);

/// Only continue if lhs == rhs, otherwise return from the current function.
macro_rules! srt_assert {
    ($lhs:expr, $rhs:expr) => {
        if $lhs != $rhs {
            tracing::trace!(concat!(stringify!($lhs), " was not {:?}"), $rhs);

            return Ok(());
        }
    };
}

/// Handles a new incoming handshake. This is the only packet that should be handled on
/// unknown connection streams.
pub async fn handshake_new<S>(
    packet: HandshakePacket,
    stream: SrtStream<'_>,
    state: State<S>,
) -> Result<(), Error>
where
    S: MultiSink,
{
    tracing::trace!("INDUCTION");

    // Check if the handshake induction is valid.
    srt_assert!(packet.version, 4);
    srt_assert!(packet.encryption_field, 0);
    srt_assert!(packet.extension_field, 2);
    srt_assert!(packet.handshake_type, HandshakeType::Induction);
    srt_assert!(packet.syn_cookie, 0);

    let client_socket_id = packet.srt_socket_id;
    let server_socket_id = SRV_SOCKET_ID.fetch_add(1, Ordering::Relaxed);

    let client_seqnum = packet.initial_packet_sequence_number;
    let server_seqnum = state.random();

    let syn_cookie = state.random();

    // Respond with our own induction.
    // TODO: Advertise AES block size here
    let mut resp = HandshakePacket::default();
    resp.header.set_packet_type(PacketType::Control);
    resp.header.timestamp = 0;
    resp.header.destination_socket_id = client_socket_id;
    // SRT magic
    resp.version = 5;
    resp.extension_field = 0x4A17;
    resp.srt_socket_id = server_socket_id;
    resp.handshake_type = HandshakeType::Induction;
    resp.syn_cookie = syn_cookie;

    resp.initial_packet_sequence_number = server_seqnum;
    // Ethernet frame size
    resp.maximum_transmission_unit_size = 1500;
    resp.maximum_flow_window_size = 8192;
    // TODO: Correct IP handling
    resp.peer_ip_address = u32::from_be_bytes([127, 0, 0, 1]) as u128;

    stream.send(resp).await?;

    // Mark the connection as alive.
    let mut conn = Connection::new(server_socket_id, client_socket_id);
    conn.client_sequence_number = client_seqnum;
    conn.server_sequence_number = server_seqnum;
    conn.syn_cookie = syn_cookie;
    state.pool.insert(conn);

    tracing::debug!("Adding new client with state INDUCTION");
    Ok(())
}

pub async fn handshake<S>(
    mut packet: HandshakePacket,
    mut stream: SrtConnStream<'_, S>,
    state: State<S>,
) -> Result<(), Error>
where
    S: MultiSink,
{
    // A client may not know that we know them.
    if packet.handshake_type != HandshakeType::Conclusion {
        return handshake_new(packet, stream.stream, state).await;
    }

    srt_assert!(packet.handshake_type, HandshakeType::Conclusion);
    srt_assert!(packet.version, 5);
    srt_assert!(packet.encryption_field, 0);
    srt_assert!(packet.syn_cookie, stream.conn.syn_cookie);

    // Remove the syn_cookie.
    packet.srt_socket_id = stream.conn.server_socket_id.0;
    packet.syn_cookie = 0;

    let buf = packet.encode_to_vec()?;
    stream.send(packet).await?;

    // Update the conn
    stream.conn.state = HandshakeType::Done;

    state.pool.insert(stream.conn);
    Ok(())
}

// pub async fn handshake(stream: &Arc<UdpSocket>, state: State) -> Result<Connection, Error> {
//     let start_time = Instant::now();

//     // Ethernet frame size for now.
//     // TODO: Only stack-allocate the required size for HandshakePacket.
//     let mut buf = [0; 1500];

//     // Induction phase
//     let (_, addr) = stream.recv_from(&mut buf).await?;

//     let packet = HandshakePacket::decode(&buf[..])?;

//     println!("{:?}", packet);

//     assert_eq!(packet.header.packet_type(), PacketType::Control);
//     assert_eq!(packet.version, 4);
//     assert_eq!(packet.encryption_field, 0);
//     assert_eq!(packet.extension_field, 2);
//     assert_eq!(packet.handshake_type, HandshakeType::Induction);
//     assert_eq!(packet.syn_cookie, 0);

//     let caller_initial_sequence_number = packet.initial_packet_sequence_number;

//     let caller_socket_id = packet.srt_socket_id;
//     let listener_socket_id = 69u32;

//     println!("Got valid INDUCTION from caller");

//     let mut resp = HandshakePacket::default();
//     resp.header.seg0.set_bits(0, 1);
//     resp.header.timestamp = packet.header.timestamp + 1;

//     resp.initial_packet_sequence_number = 12345;
//     resp.maximum_transmission_unit_size = 1500;
//     resp.maximum_flow_window_size = 8192;
//     resp.peer_ip_address += u32::from_be_bytes([127, 0, 0, 1]) as u128;

//     // SRT
//     resp.version = 5;
//     resp.encryption_field = 0;
//     resp.extension_field = 0x4A17;
//     resp.handshake_type = HandshakeType::Induction;
//     resp.srt_socket_id = listener_socket_id;
//     // "random"
//     resp.syn_cookie = 420;

//     let mut buf = Vec::new();
//     resp.encode(&mut buf)?;
//     stream.send_to(&buf, addr).await?;

//     println!("{:?}", resp);
//     println!("{:?}", buf);
//     println!("Send INDUCTION to caller");

//     // Conclusion

//     let mut buf = [0; 1500];
//     let (len, addr) = stream.recv_from(&mut buf).await?;

//     let packet = HandshakePacket::decode(&buf[..])?;

//     println!("{:?}", packet);

//     assert_eq!(packet.version, 5);
//     assert_eq!(packet.handshake_type, HandshakeType::Conclusion);
//     assert_eq!(packet.srt_socket_id, caller_socket_id);
//     assert_eq!(packet.syn_cookie, 420);
//     assert_eq!(packet.encryption_field, 0);

//     println!("Got Valid CONCLUSION from caller");

//     stream.send_to(&buf[..len], addr).await?;

//     let socket = stream.clone();
//     tokio::task::spawn(async move {
//         loop {
//             // 10MS
//             tokio::time::sleep(std::time::Duration::new(0, 10_000_000)).await;
//         }
//     });

//     Ok(Connection {
//         start_time,
//         server_socket_id: SocketId(listener_socket_id),
//         client_socket_id: SocketId(caller_socket_id),
//         client_sequence_number: caller_initial_sequence_number,
//         rtt: 100_000,
//         rtt_variance: 50_000,
//         state: HandshakeType::Done,
//     })
// }
