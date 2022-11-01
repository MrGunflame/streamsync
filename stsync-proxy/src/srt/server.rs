use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::future::Future;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::{ToSocketAddrs, UdpSocket};

use super::config::Config;
use super::state::{Connection, SocketId, State};
use crate::proto::{Bits, Decode, Encode};
use crate::session::SessionManager;
use crate::srt::proto::{Keepalive, LightAck};
use crate::srt::state::ConnectionId;

use super::{Error, IsPacket, Packet};

pub fn serve<A, S>(
    addr: A,
    session_manager: S,
    config: Config,
) -> (State<S>, impl Future<Output = Result<(), io::Error>>)
where
    A: ToSocketAddrs,
    S: SessionManager + 'static,
{
    let state = State::new(session_manager, config);

    let fut = {
        // NOTE: This `state` instance MUST outlive all connections in the pool. Only after the
        // pool is emptied (all handles are closed) and no new udp frames are accepted may the
        // state be dropped.
        let state = state.clone();

        async move {
            let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            socket.set_recv_buffer_size(500_000_000)?;
            socket.bind(&SocketAddr::from_str("0.0.0.0:9999").unwrap().into())?;

            let rx = socket.recv_buffer_size()?;
            let tx = socket.send_buffer_size()?;

            let socket = UdpSocket::from_std(socket.into())?;
            tracing::info!("Listing on {}", socket.local_addr()?);

            tracing::info!("Socket Recv-Q: {}, Send-Q: {}", rx, tx);

            let socket = Arc::new(socket);

            loop {
                let mut buf = [0; 1500];
                let (len, addr) = socket.recv_from(&mut buf).await.unwrap();
                tracing::trace!("Got {} bytes from {}", len, addr);

                let packet = match Packet::decode(&buf[..len]) {
                    Ok(packet) => packet,
                    Err(err) => {
                        println!("Failed to decode packet: {}", err);
                        continue;
                    }
                };

                let socket = socket.clone();
                if let Err(err) = handle_message(packet, addr, socket, &state).await {
                    tracing::error!("Error serving connection: {}", err);
                }
            }
        }
    };

    (state, fut)
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
    state: &State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    let stream = SrtStream {
        socket,
        addr,
        _marker: &PhantomData,
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

    let id = ConnectionId {
        addr: stream.addr,
        server_socket_id: packet.header.destination_socket_id.into(),
        client_socket_id: packet.header.destination_socket_id.into(),
    };

    match state.pool.get(id) {
        Some(handle) => {
            let _ = handle.send(packet).await;
        }
        None => {
            tracing::debug!("Received packet from unknown client {}", id);
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct SrtStream<'a> {
    pub socket: Arc<UdpSocket>,
    pub addr: SocketAddr,
    pub _marker: &'a PhantomData<()>,
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
#[derive(Clone)]
pub struct SrtConnStream<'a> {
    pub stream: SrtStream<'a>,
    pub conn: Arc<Connection>,
}

impl<'a> SrtConnStream<'a> {
    pub fn new(stream: SrtStream<'a>, conn: Arc<Connection>) -> Self {
        Self { stream, conn }
    }

    pub async fn send<T>(&self, packet: T) -> Result<(), Error>
    where
        T: IsPacket,
    {
        let mut packet = packet.upcast();
        packet.header.timestamp = self.conn.timestamp();
        packet.header.destination_socket_id = self.conn.client_socket_id.0;

        self.stream.send(packet).await
    }
}

async fn keepalive(_packet: Keepalive, stream: SrtConnStream<'_>) -> Result<(), Error> {
    let resp = Keepalive::builder().build();
    stream.send(resp).await?;
    Ok(())
}
