use std::borrow::Borrow;
use std::collections::VecDeque;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::hint;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::sink::{Close, Feed};
use futures::{pin_mut, FutureExt, SinkExt, StreamExt};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{Interval, MissedTickBehavior};
use tracing::{event, Level, Span};

use crate::proto::Encode;
use crate::session::{LiveStream, SessionManager};
use crate::srt::proto::Nak;
use crate::srt::HandshakeType;
use crate::utils::Shared;

use super::metrics::ConnectionMetrics;
use super::proto::{Ack, AckAck, DropRequest, Keepalive, Shutdown};
use super::sink::OutputSink;
use super::state::{ConnectionId, State};
use super::stream::SrtStream;
use super::{
    ControlPacketType, DataPacket, Error, ExtensionField, ExtensionType, HandshakeExtension,
    HandshakePacket, IsPacket, Packet, PacketPosition, PacketType,
};

pub type Result<T> = std::result::Result<T, Error>;

/// A `Connection` is a single future representing a logical SRT stream.
///
/// # Safety
///
/// When a `Connection` is created, it contains a shared reference to the global [`State`]. This
/// reference is direct and not reference counted. **While the `Connection` exists the contained
/// [`State`] must not be dropped. The [`State`] must also not be borrowed mutably**. Shared
/// borrows are still allowed.
pub struct Connection<S>
where
    S: SessionManager,
{
    pub id: ConnectionId,
    pub state: Shared<State<S>>,
    pub metrics: Arc<ConnectionMetrics>,

    pub incoming: mpsc::Receiver<Packet>,
    pub socket: Arc<UdpSocket>,

    /// Time of the first sent packet.
    pub start_time: Instant,

    pub server_socket_id: u32,
    pub client_socket_id: u32,

    pub server_sequence_number: u32,
    pub client_sequence_number: u32,

    pub mode: ConnectionMode<S>,

    pub inflight_acks: LossList,
    pub loss_list: LossList,
    pub rtt: Rtt,

    pub tick_interval: TickInterval,

    /// Timestamp of the last packet received by the peer.
    pub last_time: Instant,

    /// Self-referential struct.
    pub poll_state: PollState<S>,

    /// Maximum transmission unit, the maximum size for an Ethernet frame. The default is 1500,
    /// which is the maximum size for an Ethernet frame.
    pub mtu: u16,

    pub queue: TransmissionQueue,

    pub resource_span: Span,
}

impl<S> Connection<S>
where
    S: SessionManager,
{
    /// Returns a reference to the [`State`] that owns this `Connection`.
    #[inline]
    pub fn state(&self) -> &State<S> {
        // SAFETY: When a `Connection` is created the caller guarantees that the provided
        // `&State<S>` reference outlives the `Connection` instance.
        unsafe { self.state.as_ref() }
    }

    pub fn timestamp(&self) -> u32 {
        self.start_time.elapsed().as_micros() as u32
    }

    pub fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        tracing::trace!("Connection.poll_read");

        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Read));

        // Empty the sending queue before doing anything else.
        if let Some(packet) = self.queue.pop() {
            // Update connection stats.
            match packet.header.packet_type() {
                PacketType::Data => {
                    self.metrics.data_packets_sent.add(1);
                    self.metrics.data_bytes_sent.add(packet.size());
                }
                PacketType::Control => {
                    self.metrics.ctrl_packets_sent.add(1);
                    self.metrics.ctrl_bytes_sent.add(packet.size());
                }
            }

            let socket = self.socket.clone();
            let addr = self.id.addr;
            let fut = Box::pin(async move {
                let buf = packet.encode_to_vec()?;
                socket.send_to(&buf, addr).await?;
                Ok(())
            });

            self.poll_state = PollState::Write(fut);

            // Immediately move into write state.
            return Poll::Ready(Ok(()));
        }

        match self.incoming.poll_recv(cx) {
            Poll::Ready(Some(packet)) => {
                self.handle_packet(packet)?;
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(None) => {
                self.close()?;
                return Poll::Ready(Ok(()));
            }
            Poll::Pending => (),
        }

        match self.tick_interval.poll_unpin(cx) {
            Poll::Ready(()) => {
                self.tick()?;
                // return Poll::Ready(Ok(()));
            }
            Poll::Pending => (),
        }

        let timestamp = self.timestamp();
        let this = unsafe { self.get_unchecked_mut() };

        if let ConnectionMode::Request {
            stream,
            message_number,
        } = &mut this.mode
        {
            let mut count = 0;
            while let Poll::Ready(res) = stream.poll_next_unpin(cx) {
                match res {
                    Some(buf) => {
                        let mut packet = DataPacket::default();
                        packet.header.set_packet_type(PacketType::Data);
                        packet
                            .header()
                            .set_packet_sequence_number(this.server_sequence_number);
                        packet.header().set_message_number(*message_number);
                        packet.header().set_ordered(true);
                        packet.header().set_packet_position(PacketPosition::Full);
                        packet.data = buf.into();

                        this.server_sequence_number += 1;
                        *message_number += 1;

                        let mut packet = packet.upcast();
                        packet.header.timestamp = timestamp;
                        packet.header.destination_socket_id = this.id.client_socket_id.0;

                        this.queue.push(packet);
                        count += 1;
                    }
                    None => {
                        this.close()?;
                        return Poll::Ready(Ok(()));
                    }
                }
            }

            if count > 0 {
                return Poll::Ready(Ok(()));
            }

            // match stream.poll_next_unpin(cx) {
            //     Poll::Ready(Some(buf)) => {}
            //     Poll::Ready(None) => {
            //         self.close()?;
            //         return Poll::Ready(Ok(()));
            //     }
            //     Poll::Pending => (),
            // }
        }

        Poll::Pending
    }

    fn init_read(&mut self) {
        self.poll_state = PollState::Read;
    }

    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        event!(parent: &self.resource_span, Level::TRACE, "Connection.poll_write");

        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Write(_)));

        match &mut self.poll_state {
            PollState::Write(fut) => match fut.as_mut().poll(cx).map(|_| ()) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(()) => {
                    self.init_read();
                    Poll::Ready(Ok(()))
                }
            },
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    fn poll_write_sink(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        event!(parent: &self.resource_span, Level::TRACE, "Connection.poll_write_sink");

        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::WriteSink(_)));

        match &mut self.poll_state {
            PollState::WriteSink(fut) => {
                pin_mut!(fut);

                match fut.poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(_) => {
                        self.init_read();
                        Poll::Ready(Ok(()))
                    }
                }
            }
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    pub fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        event!(parent: &self.resource_span, Level::TRACE, "Connection.poll_close");

        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Close(_)));

        // TODO: Drain the transmission queue, the assume the connection is dead.

        match &mut self.poll_state {
            PollState::Close(fut) => {
                pin_mut!(fut);

                match fut.poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(err)) => Poll::Ready(Ok(())),
                    Poll::Ready(Ok(())) => {
                        tracing::debug!("Connection to {} closed", self.id);
                        self.poll_state = PollState::Closed;
                        Poll::Ready(Ok(()))
                    }
                }
            }
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    pub fn handle_packet(&mut self, mut packet: Packet) -> Result<()> {
        tracing::trace!("Connection.handle_packet");

        // Update connection stats.
        self.last_time = Instant::now();

        match packet.header.packet_type() {
            PacketType::Data => {
                // Update connection metrics.
                self.metrics.data_packets_recv.inc();
                self.metrics.data_bytes_recv.add(packet.size());

                match packet.downcast() {
                    Ok(packet) => self.handle_data(packet),
                    Err(err) => {
                        tracing::debug!("Failed to downcast packet: {}", err);
                        Ok(())
                    }
                }
            }
            PacketType::Control => {
                // Update connection metrics.
                self.metrics.ctrl_packets_recv.inc();
                self.metrics.ctrl_bytes_recv.add(packet.size());

                match packet.header.as_control_unchecked().control_type() {
                    ControlPacketType::Handshake => match packet.downcast() {
                        Ok(packet) => self.handle_handshake(packet),
                        Err(err) => {
                            tracing::debug!("Failed to downcast packet: {}", err);
                            Ok(())
                        }
                    },
                    ControlPacketType::Keepalive => match packet.downcast() {
                        Ok(packet) => self.handle_keepalive(packet),
                        Err(err) => {
                            tracing::debug!("Failed to downcast packet: {}", err);
                            Ok(())
                        }
                    },
                    ControlPacketType::Ack => match packet.downcast() {
                        Ok(packet) => self.handle_ack(packet),
                        Err(err) => {
                            tracing::debug!("Failed to downcast packet: {}", err);
                            Ok(())
                        }
                    },
                    ControlPacketType::Nak => match packet.downcast() {
                        Ok(packet) => self.handle_nak(packet),
                        Err(err) => {
                            tracing::debug!("Failed to downcast packet: {}", err);
                            Ok(())
                        }
                    },
                    ControlPacketType::CongestionWarning => {
                        tracing::warn!("Unhandled CongestionWarning");
                        Ok(())
                    }
                    ControlPacketType::Shutdown => match packet.downcast() {
                        Ok(packet) => self.handle_shutdown(packet),
                        Err(err) => {
                            tracing::debug!("Failed to downcast packet: {}", err);
                            Ok(())
                        }
                    },
                    ControlPacketType::AckAck => match packet.downcast() {
                        Ok(packet) => self.handle_ackack(packet),
                        Err(err) => {
                            tracing::debug!("Failed to downcast packet: {}", err);
                            Ok(())
                        }
                    },
                    ControlPacketType::DropReq => {
                        tracing::warn!("Unhandled DropReq");
                        Ok(())
                    }
                    ControlPacketType::PeerError => {
                        tracing::warn!("Unhandled PeerError");
                        Ok(())
                    }
                    ControlPacketType::UserDefined => Ok(()),
                }
            }
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        // Drop the connection after 15s of no response from the peer.
        if self.last_time.elapsed() >= Duration::from_secs(15) {
            return self.close();
        }

        // Send ACKs to the peer in publish mode.
        if self.mode.is_publish() {
            // Purge all lost packets.
            let packets_lost = self.loss_list.clear(self.rtt);
            self.metrics.data_packets_lost.add(packets_lost);
            self.metrics
                .data_bytes_lost
                .add(packets_lost * self.mtu as usize);

            let timespan = self.start_time.elapsed().as_secs() as u32;

            let packets_recv_rate;
            let bytes_recv_rate;
            if timespan == 0 {
                packets_recv_rate = 0;
                bytes_recv_rate = 0;
            } else {
                packets_recv_rate = self.metrics.data_packets_recv.get() as u32 / timespan;
                bytes_recv_rate = self.metrics.data_bytes_recv.get() as u32 / timespan;
            }

            let packet = Ack::builder()
                .acknowledgement_number(self.server_sequence_number)
                .last_acknowledged_packet_sequence_number(self.client_sequence_number)
                .rtt(self.rtt.rtt)
                .rtt_variance(self.rtt.rtt_variance)
                .avaliable_buffer_size(8192)
                .packets_receiving_rate(packets_recv_rate)
                .estimated_link_capacity(packets_recv_rate)
                .receiving_rate(bytes_recv_rate)
                .build();

            self.inflight_acks.push(self.server_sequence_number);

            self.server_sequence_number += 1;

            self.send(packet)?;
        }

        Ok(())
    }

    pub fn send_bytes(&mut self, bytes: Bytes) -> Result<()> {
        let message_number = match &mut self.mode {
            ConnectionMode::Request {
                stream: _,
                message_number,
            } => message_number,
            _ => return Ok(()),
        };

        let mut packet = DataPacket::default();
        packet.header.set_packet_type(PacketType::Data);
        packet
            .header()
            .set_packet_sequence_number(self.server_sequence_number);
        packet.header().set_message_number(*message_number);
        packet.header().set_ordered(true);
        packet.header().set_packet_position(PacketPosition::Full);
        packet.data = bytes.into();

        self.server_sequence_number += 1;
        *message_number += 1;

        self.send(packet)
    }

    /// Sends a packet to the peer.
    pub fn send<T>(&mut self, packet: T) -> Result<()>
    where
        T: IsPacket,
    {
        let mut packet = packet.upcast();
        packet.header.timestamp = self.timestamp();
        packet.header.destination_socket_id = self.id.client_socket_id.0;

        self.queue.push(packet);
        Ok(())
    }

    /// Closes the connection.
    pub fn close(&mut self) -> Result<()> {
        self.send(Shutdown::builder().build())?;

        if let ConnectionMode::Publish(sink) = &mut self.mode {
            #[cfg(debug_assertions)]
            assert!(matches!(self.poll_state, PollState::Read));

            let fut = sink.close();
            let fut = unsafe { std::mem::transmute(fut) };

            self.poll_state = PollState::Close(fut);
        } else {
            self.poll_state = PollState::Closed;
        }

        close_metrics(self);

        Ok(())
    }

    pub fn handle_data(&mut self, packet: DataPacket) -> Result<()> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Read));

        // Only handle data packets from peers that are publishing.
        let tx = match &mut self.mode {
            ConnectionMode::Publish(tx) => tx,
            _ => return Ok(()),
        };

        let seqnum = packet.packet_sequence_number();

        tracing::trace!("Received packet with sequence {}", seqnum);

        let fut = tx.feed(packet);
        let fut = unsafe { std::mem::transmute(fut) };
        self.poll_state = PollState::WriteSink(fut);

        // If the sequence number of the packet is not the next expected sequence number
        // and is not already a lost sequence number we lost all sequences up to the
        // received sequence. We move the sequence counter forward accordingly and register
        // all missing sequence numbers in case they are being received later out-of-order.
        if self.client_sequence_number != seqnum && self.loss_list.remove(seqnum).is_none() {
            self.loss_list.extend(self.client_sequence_number..seqnum);

            // We attempt to recover the lost packet by sending NAK right away. We don't
            // actually validate that it reaches its destination. If it gets lost we simply
            // skip the packet.
            // let mut builder = Nak::builder();
            // if self.loss_list.len() > 1 {
            //     unsafe {
            //         // SAFETY: We just extended `loss_list` by at least 1.
            //         let first = self.loss_list.first_unchecked();
            //         let last = self.loss_list.last_unchecked();

            //         builder = builder.lost_packet_sequence_numbers(first..=last);
            //     }
            // } else {
            //     // SAFETY: We just extended `loss_list` by at least 1.
            //     unsafe {
            //         let first = self.loss_list.first_unchecked();

            //         builder = builder.lost_packet_sequence_number(first);
            //     }
            // }

            // self.send(builder.build())?;
        }

        self.client_sequence_number = seqnum + 1;

        Ok(())
    }

    pub fn handle_shutdown(&mut self, _packet: Shutdown) -> Result<()> {
        self.close()
    }

    pub fn handle_ackack(&mut self, packet: AckAck) -> Result<()> {
        if let Some(ts) = self.inflight_acks.remove(packet.acknowledgement_number()) {
            let rtt = ts.elapsed().as_micros() as u32;
            tracing::trace!("Received ACKACK with RTT {}", rtt);

            self.rtt.update(rtt);

            self.metrics.rtt.set(self.rtt.rtt as usize);
            self.metrics
                .rtt_variance
                .set(self.rtt.rtt_variance as usize);
        }

        Ok(())
    }

    pub fn handle_keepalive(&mut self, _packet: Keepalive) -> Result<()> {
        self.send(Keepalive::builder().build())
    }

    pub fn handle_ack(&mut self, packet: Ack) -> Result<()> {
        // We only accpet ACK packets when the peer requests a stream.
        if let ConnectionMode::Request { .. } = self.mode {
            self.rtt.rtt = packet.rtt;
            self.rtt.rtt_variance = packet.rtt_variance;

            self.metrics.rtt.set(self.rtt.rtt as usize);
            self.metrics
                .rtt_variance
                .set(self.rtt.rtt_variance as usize);

            // Reply with an ACKACK.
            let packet = AckAck::builder()
                .acknowledgement_number(packet.acknowledgement_number())
                .build();

            self.send(packet)
        } else {
            Ok(())
        }
    }

    pub fn handle_handshake(&mut self, mut packet: HandshakePacket) -> Result<()> {
        tracing::trace!("Connection.handle_handshake");

        let syn_cookie = match self.mode {
            ConnectionMode::Induction { syn_cookie } => syn_cookie,
            _ => {
                // We already recognise the client, but the client doesn't. This is most likely
                // caused by a bad peer implementation. Since we don't retain the state from the
                // INDUCTION phase, we simply reset the connection and pretend it never existed.
                // TODO: Reset the connection.
                tracing::debug!("Connection already established, destroying");
                return Ok(());
            }
        };

        if packet.version != 5 {
            tracing::debug!(
                "Missmatched version {} in HS (expected 5), rejecting",
                packet.version
            );
            return Ok(());
        }

        if packet.encryption_field != 0 {
            tracing::debug!(
                "Missmatched encryption_field {} in HS (expected 0), rejecting",
                packet.encryption_field
            );
            return Ok(());
        }

        if packet.syn_cookie != syn_cookie {
            tracing::debug!(
                "Missmatched syn_cookie {} in HS (expected {}), rejecting",
                packet.syn_cookie,
                syn_cookie
            );

            return Ok(());
        }

        packet.syn_cookie = 0;
        packet.srt_socket_id = self.id.server_socket_id.0;
        packet.initial_packet_sequence_number = self.server_sequence_number;

        // Handle handshake extensions.
        if let Some(mut ext) = packet.extensions.remove_hsreq() {
            ext.sender_tsbpd_delay = ext.receiver_tsbpd_delay;

            packet.extensions.0.push(HandshakeExtension {
                extension_type: ExtensionType::HSRSP,
                extension_length: 3,
                extension_content: ext.into(),
            });

            packet.extension_field = ExtensionField::HSREQ;
        }

        // StreamId extension
        if let Some(ext) = packet.extensions.remove_stream_id() {
            tracing::info!("StreamId ext: {:?} (Parsed {:?})", ext, ext.parse());

            let sid = match ext.parse() {
                Ok(sid) => sid,
                Err(err) => {
                    tracing::debug!("Failed to parse StreamId extension: {:?}", err);
                    return Ok(());
                }
            };

            let resource_id = sid.resource().map(|id| id.parse().ok()).flatten();
            let session_id = sid.session().map(|id| id.parse().ok()).flatten();

            match sid.mode() {
                Some("request") => {
                    tracing::info!(
                        "Peer {} wants to request resource {:?} with key {:?}",
                        self.id,
                        resource_id,
                        session_id
                    );

                    let stream = match self
                        .state()
                        .session_manager
                        .request(resource_id, session_id)
                    {
                        Ok(stream) => stream,
                        Err(err) => {
                            let code =
                                if err.is_invalid_resource_id() || err.is_invalid_credentials() {
                                    HandshakeType::REJ_BADSECRET
                                } else {
                                    HandshakeType::REJ_UNKNOWN
                                };

                            return self.reject(code);
                        }
                    };

                    let stream = SrtStream::new(stream, 8192, self.client_sequence_number);

                    self.state().metrics.connections_handshake_current.dec();
                    self.state().metrics.connections_request_current.inc();

                    self.mode = ConnectionMode::Request {
                        stream,
                        message_number: 1,
                    };
                }
                Some("publish") => {
                    tracing::info!(
                        "Peer {} wants to publish to resource {:?} with key {:?}",
                        self.id,
                        resource_id,
                        session_id
                    );

                    let sink = match self
                        .state()
                        .session_manager
                        .publish(resource_id, session_id)
                    {
                        Ok(sink) => sink,
                        Err(err) => {
                            let code =
                                if err.is_invalid_resource_id() || err.is_invalid_credentials() {
                                    HandshakeType::REJ_BADSECRET
                                } else {
                                    HandshakeType::REJ_UNKNOWN
                                };

                            return self.reject(code);
                        }
                    };

                    self.state().metrics.connections_handshake_current.dec();
                    self.state().metrics.connections_publish_current.inc();

                    self.mode = ConnectionMode::Publish(OutputSink::new(sink));
                }
                _ => return self.reject(HandshakeType::REJ_ROGUE),
            }
        }

        self.send(packet)
    }

    pub fn reject(&mut self, reason: HandshakeType) -> Result<()> {
        tracing::debug!("Rejecting client {} with reason {:?}", self.id, reason);

        let mut packet = HandshakePacket::default();
        packet.header.set_packet_type(PacketType::Control);
        packet.version = 0x00010501;
        packet.encryption_field = 0;
        packet.extension_field = ExtensionField::NONE;
        packet.initial_packet_sequence_number = self.server_sequence_number;
        packet.maximum_transmission_unit_size = self.mtu as u32;
        packet.maximum_flow_window_size = 8192;
        packet.handshake_type = reason;
        packet.srt_socket_id = self.server_socket_id;
        packet.syn_cookie = 0;
        packet.peer_ip_address = 0;

        self.send(packet)
    }

    /// Handle an incoming [`Nak`] packet. This only responds if the connection is in [`Request`]
    /// mode, otherwise it does nothing.
    ///
    /// [`Request`]: ConnectionMode::Request
    pub fn handle_nak(&mut self, packet: Nak) -> Result<()> {
        let timestamp = self.timestamp();

        let stream = match &mut self.mode {
            ConnectionMode::Request { stream, .. } => stream,
            _ => return Ok(()),
        };

        for seq in packet.lost_packet_sequence_numbers.iter() {
            match stream.get(seq).map(|buf| buf.to_vec()) {
                Some(buf) => {
                    // TODO: Keep track of message numbers.
                    let mut packet = DataPacket::default();
                    packet.header().set_packet_sequence_number(seq);
                    packet.header().set_ordered(true);
                    packet.header().set_retransmitted(true);
                    packet.header().set_packet_position(PacketPosition::Full);
                    packet.data = buf;

                    let mut packet = packet.upcast();
                    packet.header.timestamp = timestamp;
                    packet.header.destination_socket_id = self.id.client_socket_id.0;

                    self.queue.push_prio(packet);
                }
                None => {
                    let dropreq = DropRequest::builder()
                        .message_number(0)
                        .first_packet_sequence_number(packet.lost_packet_sequence_numbers.first())
                        .last_packet_sequence_number(packet.lost_packet_sequence_numbers.last())
                        .build();

                    let mut packet = dropreq.upcast();
                    packet.header.timestamp = timestamp;
                    packet.header.destination_socket_id = self.id.client_socket_id.0;

                    self.queue.push(packet);
                }
            }
        }

        Ok(())
    }

    pub async fn handle_dropreq(&mut self, _packet: DropRequest) -> Result<()> {
        Ok(())
    }

    pub async fn handle_peer_error(&mut self, _packet: Packet) -> Result<()> {
        Ok(())
    }
}

impl<S> Future for Connection<S>
where
    S: SessionManager,
{
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match &self.poll_state {
                PollState::Read => match self.as_mut().poll_read(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    _ => (),
                },
                PollState::Write(_) => match self.as_mut().poll_write(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    _ => (),
                },
                PollState::WriteSink(_) => match self.as_mut().poll_write_sink(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    _ => (),
                },
                PollState::Close(_) => match self.as_mut().poll_close(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    _ => (),
                },
                PollState::Closed => return Poll::Ready(Ok(())),
            }
        }
    }
}

impl<S> Hash for Connection<S>
where
    S: SessionManager,
{
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<S> Borrow<ConnectionId> for Connection<S>
where
    S: SessionManager,
{
    #[inline]
    fn borrow(&self) -> &ConnectionId {
        &self.id
    }
}

impl<S> PartialEq for Connection<S>
where
    S: SessionManager,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<S> PartialEq<ConnectionId> for Connection<S>
where
    S: SessionManager,
{
    #[inline]
    fn eq(&self, other: &ConnectionId) -> bool {
        self.id == *other
    }
}

impl<S> Eq for Connection<S> where S: SessionManager {}

// SAFETY: We guarantee that `State` is `Send` + `Sync` and outlives this future when
// sent to another thread.
unsafe impl<S> Send for Connection<S> where S: SessionManager + Send {}
unsafe impl<S> Sync for Connection<S> where S: SessionManager + Sync {}

impl<S> Drop for Connection<S>
where
    S: SessionManager,
{
    fn drop(&mut self) {
        let state = self.state();

        state.pool.remove(self.id);
        state.conn_metrics.lock().remove(&self.id);

        match &self.mode {
            ConnectionMode::Induction { .. } => state.metrics.connections_handshake_current.dec(),
            ConnectionMode::Publish(_) => state.metrics.connections_publish_current.dec(),
            ConnectionMode::Request { .. } => state.metrics.connections_request_current.dec(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionHandle {
    pub id: ConnectionId,
    pub tx: mpsc::Sender<Packet>,
}

impl ConnectionHandle {
    pub async fn send(&self, packet: Packet) {
        let _ = self.tx.send(packet).await;
    }
}

impl Hash for ConnectionHandle {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Borrow<ConnectionId> for ConnectionHandle {
    #[inline]
    fn borrow(&self) -> &ConnectionId {
        &self.id
    }
}

impl PartialEq<ConnectionId> for ConnectionHandle {
    #[inline]
    fn eq(&self, other: &ConnectionId) -> bool {
        self.id == *other
    }
}

impl PartialEq for ConnectionHandle {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ConnectionHandle {}

/// A list to keep track of lost packets. Internally a `LossList` is a stack with all sequence
/// numbers sorted in ascending order. This sorting is not done automatically, it is only possible
/// to push new sequence numbers that are greater than the last one.
#[derive(Clone, Debug, Default)]
pub struct LossList {
    inner: Vec<(u32, Instant)>,
}

impl LossList {
    pub const fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Returns the number of sequence numbers in the `LossList`.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the `LossList` is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pushes a new lost sequence number onto the stack. The pushed sequence `seq` must be bigger
    /// than the last pushed sequence number on the stack.
    ///
    /// # Panics
    ///
    /// Panics when `debug_assertions` is enabled and the pushed sequence number `seq` is smaller
    /// than the last pushed sequence number on the stack.
    pub fn push(&mut self, seq: u32) {
        self.push_in(seq, Instant::now())
    }

    /// Clears all sequence numbers that should be considered lost from the stack. Returns the
    /// number of removed sequence numbers. [`Rtt`] is used to determine whether a sequence number
    /// is likely still inflight, or should be considered lost.
    pub fn clear(&mut self, rtt: Rtt) -> usize {
        self.clear_in(rtt, Instant::now())
    }

    /// Removes a sequence number from the `LossList`. Returns the [`Instant`] at which the
    /// sequence number was inserted.
    pub fn remove(&mut self, seq: u32) -> Option<Instant> {
        if let Ok(index) = self.inner.binary_search_by(|(n, _)| n.cmp(&seq)) {
            let (_, ts) = self.inner.remove(index);
            return Some(ts);
        }

        None
    }

    /// Returns the first sequence number in the `LossList` without doing a bounds check.
    ///
    /// # Safety
    ///
    /// This method results in undefined behavoir if `self.len() == 0`.
    #[inline]
    pub unsafe fn first_unchecked(&mut self) -> u32 {
        let (seq, _) = unsafe { self.inner.get_unchecked(0) };
        *seq
    }

    /// Returns the last sequence number in the `LossList` without doing a bounds check.
    ///
    /// # Safety
    ///
    /// This method results in undefined behavoir if `self.len() == 0`.
    #[inline]
    pub unsafe fn last_unchecked(&mut self) -> u32 {
        let (seq, _) = unsafe { self.inner.get_unchecked(self.len() - 1) };
        *seq
    }

    fn push_in(&mut self, seq: u32, now: Instant) {
        #[cfg(debug_assertions)]
        if let Some((n, _)) = self.inner.last() {
            if seq <= *n {
                panic!("Tried to push {} to LossList with last sequence {}", seq, n);
            }
        }

        self.inner.push((seq, now));
    }

    fn clear_in(&mut self, rtt: Rtt, now: Instant) -> usize {
        // TODO: A binary search could also be benefitial here.
        let mut num_removed = 0;
        while !self.is_empty() {
            let (_, ts) = unsafe { self.inner.get_unchecked(0) };

            let diff = (now - *ts).as_micros() as u32;
            if diff < rtt.rtt * 2 {
                return num_removed;
            }

            self.inner.remove(0);
            num_removed += 1;
        }

        num_removed
    }
}

impl Extend<u32> for LossList {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = u32>,
    {
        let iter = iter.into_iter();

        if let Some(len) = iter.size_hint().1 {
            self.inner.reserve(len);
        }

        let now = Instant::now();
        for seq in iter {
            self.push_in(seq, now);
        }
    }
}

pub enum ConnectionMode<S>
where
    S: SessionManager,
{
    Induction {
        syn_cookie: u32,
    },
    Publish(OutputSink<S>),
    Request {
        stream: SrtStream<LiveStream<S::Stream>>,
        message_number: u32,
    },
}

impl<S> ConnectionMode<S>
where
    S: SessionManager,
{
    pub fn is_publish(&self) -> bool {
        matches!(self, Self::Publish(_))
    }

    pub fn is_request(&self) -> bool {
        matches!(self, Self::Request { .. })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rtt {
    pub rtt: u32,
    pub rtt_variance: u32,
}

impl Rtt {
    #[inline]
    pub const fn new() -> Self {
        Self {
            rtt: 100_000,
            rtt_variance: 50_000,
        }
    }

    pub fn update(&mut self, new: u32) {
        self.rtt_variance = ((3.0 / 4.0) * self.rtt_variance as f32
            + (1.0 / 4.0) * self.rtt.abs_diff(new) as f32) as u32;

        self.rtt = ((7.0 / 8.0) * self.rtt as f32 + (1.0 / 8.0) * new as f32) as u32;
    }
}

impl Default for Rtt {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct TickInterval(Interval);

impl TickInterval {
    pub fn new() -> Self {
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        Self(interval)
    }
}

impl Default for TickInterval {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Future for TickInterval {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_tick(cx).map(|_| ())
    }
}

pub enum PollState<S>
where
    S: SessionManager,
{
    Read,
    Write(Pin<Box<dyn Future<Output = Result<()>>>>),
    WriteSink(Feed<'static, OutputSink<S>, DataPacket>),
    Close(Close<'static, OutputSink<S>, DataPacket>),
    Closed,
}

impl<S> Default for PollState<S>
where
    S: SessionManager,
{
    #[inline]
    fn default() -> Self {
        Self::Read
    }
}

/// A packet transmission queue.
#[derive(Clone, Debug, Default)]
pub struct TransmissionQueue {
    queue: VecDeque<Packet>,
    prio: VecDeque<Packet>,
}

impl TransmissionQueue {
    /// Pushes a new [`Packet`] onto the default queue.
    pub fn push(&mut self, packet: Packet) {
        self.queue.push_back(packet);
    }

    /// Pushes a new [`Packet`] onto the priority queue.
    pub fn push_prio(&mut self, packet: Packet) {
        self.prio.push_back(packet);
    }

    pub fn pop(&mut self) -> Option<Packet> {
        match self.prio.remove(0) {
            Some(packet) => Some(packet),
            None => self.queue.remove(0),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.prio.is_empty()
    }
}

pub fn close_metrics<S: SessionManager>(conn: &Connection<S>) {
    let m = &conn.metrics;

    tracing::info!(
        "
    | CTRL SENT | CTRL RECV | CTRL LOST | DATA SENT | DATA RECV | DATA LOST | RTT |
    | --------- | --------- | --------- | --------- | --------- | --------- | --- |
    | {}        | {}        | {}        | {}        | {}        | {}        | {}  |
    | {}        | {}        | {}        | {}        | {}        | {}        | {}  |
    ",
        m.ctrl_packets_sent,
        m.ctrl_packets_recv,
        m.ctrl_packets_lost,
        m.data_packets_sent,
        m.data_packets_recv,
        m.data_packets_lost,
        conn.rtt.rtt,
        m.ctrl_bytes_sent,
        m.ctrl_bytes_recv,
        m.ctrl_bytes_lost,
        m.data_bytes_sent,
        m.data_bytes_recv,
        m.data_bytes_lost,
        conn.rtt.rtt_variance,
    );
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{LossList, Rtt};

    #[test]
    fn test_rtt() {
        let mut rtt = Rtt::new();
        assert_eq!(rtt.rtt, 100_000);
        assert_eq!(rtt.rtt_variance, 50_000);

        rtt.update(100_000);
        assert_eq!(rtt.rtt, 100_000);
        assert_eq!(rtt.rtt_variance, 37_500);

        let mut rtt = Rtt::new();
        rtt.update(0);
        assert_eq!(rtt.rtt, 87_500);
        assert_eq!(rtt.rtt_variance, 62_500);
    }

    #[test]
    fn test_loss_list() {
        let now = Instant::now();

        let mut list = LossList::new();
        list.push_in(1, now);
        list.push_in(2, now + Duration::new(1, 0));
        list.push_in(3, now + Duration::new(2, 0));
        assert_eq!(list.len(), 3);

        let rtt = Rtt::new();
        assert_eq!(list.clear_in(rtt, now), 0);
        assert_eq!(list.len(), 3);
        assert_eq!(list.clear_in(rtt, now + Duration::from_millis(500)), 1);
        assert_eq!(list.len(), 2);
        assert_eq!(list.clear_in(rtt, now + Duration::from_secs(5)), 2);
        assert_eq!(list.len(), 0);
    }
}
