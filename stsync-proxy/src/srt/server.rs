use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use futures::{Sink, Stream, StreamExt};
use tokio::net::{ToSocketAddrs, UdpSocket};

use super::config::Config;
use super::state::{Connection, SocketId, State};
use crate::proto::{Bits, Decode, Encode};
use crate::sink::MultiSink;
use crate::srt::proto::{Keepalive, LightAck};
use crate::srt::state::ConnectionId;
use crate::srt::{ControlPacketType, PacketType};

use super::{Error, IsPacket, Packet};

pub async fn serve<A, S>(addr: A, sink: S, config: Config) -> Result<(), io::Error>
where
    A: ToSocketAddrs,
    S: MultiSink + 'static,
{
    let socket = UdpSocket::bind(addr).await?;
    tracing::info!("Listing on {}", socket.local_addr()?);

    let state = State::new(sink, config);

    // Clean regularly
    let state2 = state.clone();
    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::new(15, 0)).await;
            tracing::trace!("Tick cleanup");
            let num_removed = state2.pool.clean();
            tracing::debug!(
                "Purged {} connections ({} alive)",
                num_removed,
                state2.pool.len()
            );
        }
    });

    let socket = Arc::new(socket);

    loop {
        let mut buf = [0; 1500];
        let (len, addr) = socket.recv_from(&mut buf).await.unwrap();
        // println!("Accept {:?}", addr);

        let packet = match Packet::decode(&buf[..len]) {
            Ok(packet) => packet,
            Err(err) => {
                println!("Failed to decode packet: {}", err);
                continue;
            }
        };

        if let Err(err) = handle_message(packet, addr, socket.clone(), state.clone()).await {
            tracing::error!("Error serving connection: {}", err);
        }
    }
}

// // println!("handle data");
// // println!("Body: {}", packet.body.len());

// let seqnum = packet.header.seg0.0 .0;
// // println!("SEQNUM: {:?}", seqnum);

// buffer.push(seqnum, packet.body);

// // frame_count += 1;
// // if frame_count == 64 {
// //     let ack = LightAck::builder()
// //         .last_acknowledged_packet_sequence_number(seqnum + 1)
// //         .build();

// //     socket
// //         .send_to(&ack.encode_to_vec().unwrap(), addr)
// //         .await
// //         .unwrap();

// //     println!("Send LightACK");
// // }

// let mut ack = AckPacket::default();
// ack.header.set_packet_type(PacketType::Control);
// ack.header.timestamp = packet.header.timestamp + 1;
// ack.header.destination_socket_id = conn.unwrap().client_socket_id.0;
// let mut header = ack.header.as_control().unwrap();
// header.set_control_type(ControlPacketType::Ack);
// ack.header.seg1 = Bits((sequence_num).into());
// ack.last_acknowledged_packet_sequence_number = seqnum + 1;
// ack.rtt = 100_000;
// ack.rtt_variance = 50_000;
// ack.avaliable_buffer_size = 5000;
// ack.packets_receiving_rate = 1500;
// ack.estimated_link_capacity = 5000;
// ack.receiving_rate = 500000;

// sequence_num += 1;

// let mut buf = Vec::new();
// ack.encode(&mut buf).unwrap();

// socket.send_to(&buf, addr).await.unwrap();

async fn handle_message<S>(
    mut packet: Packet,
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    state: State<S>,
) -> Result<(), Error>
where
    S: MultiSink,
{
    let stream = SrtStream {
        socket: &socket,
        addr,
    };

    // A destination socket id of 0 indicates a handshake request.
    if packet.header.destination_socket_id == 0 {
        tracing::debug!("New connection from {}", addr);

        match packet.downcast() {
            Ok(packet) => {
                super::handshake::handshake(packet, stream, state).await?;
            }
            Err(err) => {
                tracing::debug!("Failed to downcast packet: {}", err);
            }
        }

        return Ok(());
    }

    let stream = match state.pool.get(ConnectionId {
        addr,
        socket_id: packet.header.destination_socket_id.into(),
    }) {
        Some(conn) => {
            *conn.last_packet_time.lock().unwrap() = Instant::now();
            conn.packets_recv.fetch_add(1, Ordering::Relaxed);
            conn.bytes_recv
                .fetch_add(packet.size() as u32, Ordering::Relaxed);

            conn.metrics.packets_recv.add(1);
            conn.metrics.bytes_recv.add(packet.size());

            SrtConnStream { stream, conn }
        }
        None => return Ok(()),
    };

    tracing::debug!(
        "Found message from existing client {}",
        stream.conn.server_socket_id.0
    );

    match packet.header.packet_type() {
        PacketType::Control => match packet.header.as_control().unwrap().control_type() {
            ControlPacketType::Handshake => {
                tracing::debug!("Got handshake request with non-zero desination socket id");
            }
            ControlPacketType::AckAck => match packet.downcast() {
                Ok(packet) => super::ack::ackack(packet, stream, state).await?,
                Err(err) => tracing::trace!("Failed to downcast packet to AckAck: {}", err),
            },
            ControlPacketType::Shutdown => match packet.downcast() {
                Ok(packet) => super::shutdown::shutdown(packet, stream, state).await?,
                Err(err) => tracing::trace!("Failed to downcast packet to Shutdown: {}", err),
            },
            ControlPacketType::Keepalive => match packet.downcast() {
                Ok(packet) => keepalive(packet, stream).await?,
                Err(err) => tracing::trace!("Failed to downcast packet to Keepalive: {}", err),
            },
            _ => {
                tracing::warn!("Unhandled control packet");
            }
        },
        PacketType::Data => {
            // println!("got data");

            match packet.downcast() {
                Ok(packet) => super::data::handle_data(packet, stream, state).await?,
                Err(err) => tracing::trace!("Failed to downcast to DataPacket: {}", err),
            }
        }
    }

    Ok(())
}

#[derive(Copy, Clone, Debug)]
pub struct SrtStream<'a> {
    pub socket: &'a UdpSocket,
    pub addr: SocketAddr,
}

impl<'a> SrtStream<'a> {
    pub async fn send<T>(&self, packet: T) -> Result<(), Error>
    where
        T: IsPacket,
    {
        let buf = packet.upcast().encode_to_vec()?;
        self.socket.send_to(&buf, self.addr).await?;
        Ok(())
    }
}

/// A SrtStream with an associated connection.
pub struct SrtConnStream<'a, M>
where
    M: MultiSink,
{
    pub stream: SrtStream<'a>,
    pub conn: Arc<Connection<M::Sink>>,
}

impl<'a, M> SrtConnStream<'a, M>
where
    M: MultiSink,
{
    pub async fn send<T>(&self, packet: T) -> Result<(), Error>
    where
        T: IsPacket,
    {
        let mut packet = packet.upcast();
        packet.header.timestamp = self.conn.timestamp();
        packet.header.destination_socket_id = self.conn.client_socket_id.0;

        self.conn.metrics.packets_sent.add(1);
        self.conn.metrics.bytes_sent.add(packet.size());

        self.stream.send(packet).await
    }
}

async fn keepalive<M>(_packet: Keepalive, stream: SrtConnStream<'_, M>) -> Result<(), Error>
where
    M: MultiSink,
{
    let resp = Keepalive::builder().build();
    stream.send(resp).await?;
    Ok(())
}

pub fn close_metrics<S>(conn: &Connection<S>)
where
    S: Sink<Vec<u8>>,
{
    tracing::info!("Connection to {} closed", conn.id.addr);
    tracing::info!(
        "Connection metrics:\n
        |  SENT  | RECV | DROP | RTT |\n
        | ------ | ---- | ---- | --- |\n
        | {}     | {}   | {}   | {}us |\n
        | {}     | {}   | -    | {}us |\n
        ",
        conn.metrics.packets_sent,
        conn.metrics.packets_recv,
        conn.metrics.packets_dropped,
        conn.rtt.load().0,
        conn.metrics.bytes_sent,
        conn.metrics.bytes_recv,
        conn.rtt.load().1,
    );
}
