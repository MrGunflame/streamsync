use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use futures::{Sink, Stream, StreamExt};
use tokio::net::{ToSocketAddrs, UdpSocket};

use super::state::{Connection, SocketId, State};
use crate::proto::{Bits, Decode, Encode};
use crate::srt::proto::{Keepalive, LightAck};
use crate::srt::{AckPacket, ControlPacketType, PacketType};

use super::{Error, Packet};

pub async fn serve<A>(addr: A) -> Result<(), io::Error>
where
    A: ToSocketAddrs,
{
    let socket = UdpSocket::bind(addr).await?;
    tracing::info!("Listing on {}", socket.local_addr()?);

    let state = State::new();

    let mut file = std::fs::File::create("out.ts").unwrap();

    let socket = Arc::new(socket);

    let buffer = Buffer::default();
    let mut buffer2 = buffer.clone();
    tokio::task::spawn(async move {
        while let Some(buf) = buffer2.next().await {
            file.write_all(&buf).unwrap();
        }
    });

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

async fn handle_message(
    mut packet: Packet,
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    state: State,
) -> Result<(), Error> {
    let stream = SrtStream {
        socket: &socket,
        addr,
    };

    tracing::debug!("{}", packet.header.destination_socket_id);
    tracing::debug!("{:?}", state.pool);

    let stream = match state
        .pool
        .get(SocketId(packet.header.destination_socket_id))
    {
        Some(conn) => SrtConnStream { stream, conn },
        // New Packet from unknown client.
        None => {
            match packet.downcast() {
                Ok(packet) => {
                    super::handshake::handshake_new(packet, stream, state).await?;
                }
                Err(err) => {
                    // tracing::debug!("Failed to downcast packet: {}", err);
                }
            }

            return Ok(());
        }
    };

    tracing::debug!(
        "Found message from existing client {}",
        stream.conn.server_socket_id.0
    );

    match packet.header.packet_type() {
        PacketType::Control => match packet.header.as_control().unwrap().control_type() {
            ControlPacketType::Handshake => {
                if let Ok(packet) = packet.downcast() {
                    super::handshake::handshake(packet, stream, state).await?;
                }
            }
            _ => {
                tracing::warn!("Unhandled control packet");
            }
        },
        PacketType::Data => {
            println!("got data");
        }
    }

    Ok(())
}

#[derive(Copy, Clone, Debug)]
pub struct SrtStream<'a> {
    socket: &'a UdpSocket,
    addr: SocketAddr,
}

impl<'a> SrtStream<'a> {
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.socket.send_to(buf, self.addr).await
    }
}

/// A SrtStream with an associated connection.
pub struct SrtConnStream<'a> {
    pub stream: SrtStream<'a>,
    pub conn: Connection,
}

impl<'a> SrtConnStream<'a> {
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.stream.send(buf).await
    }
}

/// An ordered buffer.
#[derive(Clone, Debug, Default)]
pub struct Buffer {
    /// The seqnum that the buffer head is currently waiting for.
    next_seqnum: Arc<Mutex<u32>>,
    /// Ready buffer. This buffer is ordered and ready to be consumed.
    buf: Arc<Mutex<Vec<u8>>>,
    /// Internal queue. This buffers all segments that require a previous segment.
    queue: Arc<Mutex<HashMap<u32, Vec<u8>>>>,
    waker: Arc<Mutex<Option<Waker>>>,
    is_closed: Arc<AtomicBool>,
}

impl Buffer {
    pub fn push(&self, seqnum: u32, buf: Vec<u8>) {
        tracing::trace!("Buffer::push({}, buf)", seqnum);

        // Correct segment.
        let mut next_seqnum = self.next_seqnum.lock().unwrap();
        if *next_seqnum == seqnum {
            tracing::debug!("{} is in order", seqnum);

            let mut out = self.buf.lock().unwrap();
            out.extend(buf);

            *next_seqnum += 1;

            // Push all segments that are waiting on seqnum.
            loop {
                let mut queue = self.queue.lock().unwrap();
                match queue.remove(&next_seqnum) {
                    Some(buf) => {
                        out.extend(buf);
                        *next_seqnum += 1;
                    }
                    None => break,
                }
            }

            // Wake the a stream waker if avaliable.
            self.wake_rx();

            return;
        }

        tracing::debug!("{} is out of order (missing {})", seqnum, next_seqnum);
        // Missing a segment
        let mut queue = self.queue.lock().unwrap();
        queue.insert(seqnum, buf);
    }

    pub fn set_seqnum(&self, seqnum: u32) {
        *self.next_seqnum.lock().unwrap() = seqnum;
    }

    pub fn close(&self) {
        self.is_closed.store(true, Ordering::Release);

        self.wake_rx();
    }

    fn wake_rx(&self) {
        let waker = self.waker.lock().unwrap();
        if let Some(waker) = waker.as_ref() {
            waker.wake_by_ref();
        }
    }
}

impl Stream for Buffer {
    type Item = Vec<u8>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = self.buf.lock().unwrap();
        if buf.len() > 0 {
            // Swap out the current buffer with a empty one. Return the swapped, filled buffer.
            let out = std::mem::replace(&mut *buf, Vec::new());

            return Poll::Ready(Some(out));
        }

        // Check if the sink is closed.
        if self.is_closed.load(Ordering::Acquire) {
            return Poll::Ready(None);
        }

        let mut waker = self.waker.lock().unwrap();

        let update_waker = match &*waker {
            Some(waker) => !waker.will_wake(cx.waker()),
            None => true,
        };

        if update_waker {
            *waker = Some(cx.waker().clone());
        }

        Poll::Pending
    }
}

async fn keepalive(packet: Packet, state: &Arc<State>, socket: &Arc<UdpSocket>) {
    let mut resp = Keepalive::builder().build();
}
