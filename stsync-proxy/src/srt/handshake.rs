//! SRT handshake
//!
//! Currently only the Caller-Listener handshake process is supported.
//!
//! See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-4.3

use super::conn::Connection;
use super::server::SrtStream;
use super::state::{ConnectionId, State};
use super::IsPacket;
use super::{Error, HandshakePacket, HandshakeType, PacketType};
use crate::session::SessionManager;
use crate::srt::ExtensionField;

/// Only continue if lhs == rhs, otherwise return from the current function.
macro_rules! srt_assert {
    ($lhs:expr, $rhs:expr) => {
        if $lhs != $rhs {
            tracing::trace!(concat!(stringify!($lhs), " was not {:?}"), $rhs);

            return Ok(());
        }
    };
}

pub async fn handshake<S>(
    packet: HandshakePacket,
    stream: SrtStream<'_>,
    state: &State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    match packet.handshake_type {
        HandshakeType::INDUCTION => handshake_induction(packet, stream, state).await,
        HandshakeType::CONCLUSION => handshake_conclusion(packet, stream, state).await,
        t => {
            tracing::debug!("Unsupported handshake type {:?}", t);
            Ok(())
        }
    }
}

async fn handshake_induction<S>(
    packet: HandshakePacket,
    stream: SrtStream<'_>,
    state: &State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    tracing::trace!("INDUCTION");
    debug_assert!(packet.handshake_type.is_induction());

    srt_assert!(packet.version, 4);
    srt_assert!(packet.encryption_field, 0);
    srt_assert!(packet.extension_field, ExtensionField::INDUCTION);
    srt_assert!(packet.syn_cookie, 0);

    let client_socket_id = packet.srt_socket_id;
    let server_socket_id = packet.srt_socket_id;

    let client_seqnum = packet.initial_packet_sequence_number;
    let server_seqnum = client_seqnum;

    let syn_cookie = state.random();

    let mut resp = HandshakePacket::default();
    resp.header.set_packet_type(PacketType::Control);
    resp.header.timestamp = 0;
    resp.header.destination_socket_id = client_socket_id;

    resp.handshake_type = HandshakeType::INDUCTION;
    resp.version = 5;
    resp.extension_field = ExtensionField::SRT_MAGIC;
    resp.syn_cookie = syn_cookie;
    resp.srt_socket_id = server_socket_id;
    resp.initial_packet_sequence_number = server_seqnum;

    resp.maximum_transmission_unit_size = state.config.mtu;
    resp.maximum_flow_window_size = state.config.flow_window;

    stream.send(resp).await?;

    let id = ConnectionId {
        addr: stream.addr,
        server_socket_id: server_socket_id.into(),
        client_socket_id: client_socket_id.into(),
    };

    // SAFETY: We guarantee that `state` outlives the connection.
    let (conn, handle) =
        unsafe { Connection::new(id, state, stream.socket, client_seqnum, syn_cookie) };

    tokio::task::spawn(async move {
        tracing::trace!("Spawned new connection");

        if let Err(err) = conn.await {
            tracing::debug!("Failed to serve connection: {}", err);
        }
    });

    tracing::debug!("Adding new client");
    state.pool.insert(handle);

    Ok(())
}

async fn handshake_conclusion<S>(
    packet: HandshakePacket,
    stream: SrtStream<'_>,
    state: &State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    tracing::trace!("CONCLUSION");
    debug_assert!(packet.handshake_type.is_conclusion());

    srt_assert!(packet.version, 5);
    srt_assert!(packet.encryption_field, 0);

    let conn = match state.pool.find_client_id(stream.addr, packet.srt_socket_id) {
        Some(conn) => conn,
        None => {
            tracing::debug!(
                "Unknown socket id {} from peer {}",
                packet.srt_socket_id,
                stream.addr
            );

            return Ok(());
        }
    };

    let _ = conn.send(packet.upcast()).await;

    Ok(())
}
