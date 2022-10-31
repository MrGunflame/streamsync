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
use futures::StreamExt;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{Interval, MissedTickBehavior};

use crate::proto::Encode;
use crate::session::{LiveStream, SessionManager};
use crate::srt::proto::Nak;
use crate::utils::Shared;

use super::proto::{Ack, AckAck, DropRequest, Keepalive, Shutdown};
use super::sink::OutputSink;
use super::state::{ConnectionId, State};
use super::{
    ControlPacketType, DataPacket, Error, ExtensionField, ExtensionType, HandshakeExtension,
    HandshakePacket, IsPacket, Packet, PacketPosition, PacketType,
};

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

    pub incoming: mpsc::Receiver<Packet>,
    pub socket: Arc<UdpSocket>,

    /// Time of the first sent packet.
    pub start_time: Instant,

    pub server_socket_id: u32,
    pub client_socket_id: u32,

    pub server_sequence_number: u32,
    pub client_sequence_number: u32,

    pub mode: ConnectionMode<S>,

    pub inflight_acks: AckQueue,
    pub rtt: Rtt,

    pub tick_interval: TickInterval,

    /// Timestamp of the last packet received by the peer.
    pub last_time: Instant,

    /// Self-referential struct.
    pub poll_state: PollState,
}

impl<S> Connection<S>
where
    S: SessionManager,
{
    pub fn state(&self) -> &State<S> {
        unsafe { self.state.as_ref() }
    }

    pub fn timestamp(&self) -> u32 {
        self.start_time.elapsed().as_micros() as u32
    }

    pub fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Read));

        let mut timeout = tokio::time::sleep_until((self.last_time + Duration::new(5, 0)).into());
        let timeout = unsafe { Pin::new_unchecked(&mut timeout) };

        match self.incoming.poll_recv(cx) {
            Poll::Pending => match timeout.poll(cx) {
                Poll::Pending => match &mut self.mode {
                    ConnectionMode::Request { stream, .. } => match stream.poll_next_unpin(cx) {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(Some(buf)) => {
                            let this: &'static mut Self =
                                unsafe { std::mem::transmute(self.as_mut()) };

                            let fut = Box::pin(async move {
                                this.send_bytes(buf).await;
                            });

                            self.poll_state = PollState::Write(fut);
                            Poll::Ready(())
                        }
                        Poll::Ready(None) => {
                            self.as_mut().init_close();
                            Poll::Ready(())
                        }
                    },
                    ConnectionMode::Publish(_) => {
                        match Pin::new(&mut self.tick_interval).poll(cx) {
                            Poll::Pending => Poll::Pending,
                            Poll::Ready(()) => {
                                let this: &'static mut Self =
                                    unsafe { std::mem::transmute(self.as_mut()) };
                                let fut = Box::pin(async move {
                                    if let Err(err) = this.send_ack().await {
                                        tracing::debug!("Failed to send ACK: {}", err);
                                    }
                                });

                                self.poll_state = PollState::Write(fut);
                                Poll::Ready(())
                            }
                        }
                    }
                    _ => Poll::Pending,
                },
                // Close the connection.
                Poll::Ready(_) => {
                    self.as_mut().init_close();
                    Poll::Ready(())
                }
            },
            Poll::Ready(Some(packet)) => {
                let this: &'static mut Self = unsafe { std::mem::transmute(self.as_mut()) };
                let fut = Box::pin(async move {
                    if let Err(err) = this.handle_packet(packet).await {
                        tracing::debug!("Failed to handle packet: {}", err);
                    }
                });

                self.poll_state = PollState::Write(fut);
                Poll::Ready(())
            }
            Poll::Ready(None) => {
                self.as_mut().init_close();
                Poll::Ready(())
            }
        }
    }

    fn init_read(&mut self) {
        self.poll_state = PollState::Read;
    }

    fn init_close(mut self: Pin<&mut Self>) {
        let this: &'static mut Self = unsafe { std::mem::transmute(self.as_mut()) };
        let fut = Box::pin(async move { this.close().await });

        self.poll_state = PollState::Close(fut);
    }

    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Write(_)));

        match &mut self.poll_state {
            PollState::Write(fut) => match fut.as_mut().poll(cx).map(|_| ()) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(()) => {
                    self.init_read();
                    Poll::Ready(())
                }
            },
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    pub fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        #[cfg(debug_assertions)]
        assert!(matches!(self.poll_state, PollState::Close(_)));

        match &mut self.poll_state {
            PollState::Close(fut) => fut.as_mut().poll(cx).map(|_| ()),
            _ => unsafe { hint::unreachable_unchecked() },
        }
    }

    pub async fn handle_packet(&mut self, mut packet: Packet) -> Result<(), Error> {
        // Update connection stats.
        self.last_time = Instant::now();

        match packet.header.packet_type() {
            PacketType::Data => match packet.downcast() {
                Ok(packet) => self.handle_data(packet).await,
                Err(err) => {
                    tracing::debug!("Failed to downcast packet: {}", err);
                    Ok(())
                }
            },
            PacketType::Control => match packet.header.as_control_unchecked().control_type() {
                ControlPacketType::Handshake => match packet.downcast() {
                    Ok(packet) => self.handle_handshake(packet).await,
                    Err(err) => {
                        tracing::debug!("Failed to downcast packet: {}", err);
                        Ok(())
                    }
                },
                ControlPacketType::Keepalive => match packet.downcast() {
                    Ok(packet) => self.handle_keepalive(packet).await,
                    Err(err) => {
                        tracing::debug!("Failed to downcast packet: {}", err);
                        Ok(())
                    }
                },
                ControlPacketType::Ack => match packet.downcast() {
                    Ok(packet) => self.handle_ack(packet).await,
                    Err(err) => {
                        tracing::debug!("Failed to downcast packet: {}", err);
                        Ok(())
                    }
                },
                ControlPacketType::Nak => match packet.downcast() {
                    Ok(packet) => self.handle_nak(packet).await,
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
                    Ok(packet) => self.handle_shutdown(packet).await,
                    Err(err) => {
                        tracing::debug!("Failed to downcast packet: {}", err);
                        Ok(())
                    }
                },
                ControlPacketType::AckAck => match packet.downcast() {
                    Ok(packet) => self.handle_ackack(packet).await,
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
            },
        }
    }

    pub async fn send_ack(&mut self) -> Result<(), Error> {
        let packet = Ack::builder()
            .acknowledgement_number(self.server_sequence_number)
            .last_acknowledged_packet_sequence_number(self.client_sequence_number)
            .rtt(self.rtt.rtt)
            .rtt_variance(self.rtt.rtt_variance)
            .avaliable_buffer_size(5000)
            .packets_receiving_rate(5000)
            .estimated_link_capacity(5000)
            .receiving_rate(5000)
            .build();

        self.send(packet).await
    }

    pub async fn send_bytes(&mut self, bytes: Bytes) -> Result<(), Error> {
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

        self.send(packet).await
    }

    pub async fn send<T>(&self, packet: T) -> Result<(), Error>
    where
        T: IsPacket,
    {
        let mut packet = packet.upcast();
        packet.header.timestamp = self.timestamp();
        packet.header.destination_socket_id = self.id.client_socket_id.0;

        let buf = packet.encode_to_vec()?;
        self.socket.send_to(&buf, self.id.addr).await?;
        Ok(())
    }

    /// Closes the connection.
    pub async fn close(&mut self) -> Result<(), Error> {
        if let ConnectionMode::Publish(sink) = &mut self.mode {
            let _ = sink.close().await;
        }

        self.send(Shutdown::builder().build()).await
    }

    pub async fn handle_data(&mut self, packet: DataPacket) -> Result<(), Error> {
        // Only handle data packets from peers that are publishing.
        let tx = match &mut self.mode {
            ConnectionMode::Publish(tx) => tx,
            _ => return Ok(()),
        };

        let seqnum = packet.packet_sequence_number();

        let _ = tx.push(packet).await;

        if self.client_sequence_number != seqnum {
            // TODO: LOST PACKET
        }

        self.client_sequence_number += 1;

        Ok(())
    }

    pub async fn handle_shutdown(&mut self, _packet: Shutdown) -> Result<(), Error> {
        self.close().await
    }

    pub async fn handle_ackack(&mut self, packet: AckAck) -> Result<(), Error> {
        while let Some((seq, ts)) = self.inflight_acks.pop_front() {
            if packet.acknowledgement_number() == seq {
                let rtt = ts.elapsed().as_micros() as u32;
                tracing::trace!("Received ACKACK with RTT {}", rtt);

                self.rtt.update(rtt);

                return Ok(());
            }
        }

        Ok(())
    }

    pub async fn handle_keepalive(&mut self, _packet: Keepalive) -> Result<(), Error> {
        self.send(Keepalive::builder().build()).await
    }

    pub async fn handle_ack(&mut self, packet: Ack) -> Result<(), Error> {
        // We only accpet ACK packets when the peer requests a stream.
        if let ConnectionMode::Request { .. } = self.mode {
            // Reply with an ACKACK.
            let packet = AckAck::builder()
                .acknowledgement_number(packet.acknowledgement_number())
                .build();

            self.send(packet).await
        } else {
            Ok(())
        }
    }

    pub async fn handle_handshake(&mut self, mut packet: HandshakePacket) -> Result<(), Error> {
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
            ext.sender_tsbpd_delay = self.start_time.elapsed().as_micros() as u16;

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

            match sid.mode() {
                Some("request") => match resource_id {
                    Some(id) => {
                        tracing::info!("Peer {} wants to request resource {:?}", self.id, id);

                        let stream = self.state().session_manager.request(id).unwrap();

                        self.mode = ConnectionMode::Request {
                            stream,
                            message_number: 1,
                        };
                    }
                    None => return Ok(()),
                },
                Some("publish") => {
                    tracing::info!(
                        "Peer {} wants to publish to resource {:?}",
                        self.id,
                        resource_id
                    );

                    let sink = self.state().session_manager.publish(resource_id).unwrap();

                    self.mode = ConnectionMode::Publish(OutputSink::new(self, sink));
                }
                _ => return Ok(()),
            }
        }

        self.send(packet).await
    }

    pub async fn handle_nak(&mut self, _packet: Nak) -> Result<(), Error> {
        Ok(())
    }

    pub async fn handle_dropreq(&mut self, _packet: DropRequest) -> Result<(), Error> {
        Ok(())
    }

    pub async fn handle_peer_error(&mut self, _packet: Packet) -> Result<(), Error> {
        Ok(())
    }
}

impl<S> Future for Connection<S>
where
    S: SessionManager,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match &self.poll_state {
                PollState::Read => match self.as_mut().poll_read(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(()) => continue,
                },
                PollState::Write(_) => match self.as_mut().poll_write(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(()) => continue,
                },
                PollState::Close(_) => match self.poll_close(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(()) => return Poll::Ready(()),
                },
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
        self.state().pool.remove(self.id);
    }
}

pub struct ConnectionHandle {
    tx: mpsc::Sender<Packet>,
}

#[derive(Clone, Debug, Default)]
pub struct AckQueue {
    inner: VecDeque<(u32, Instant)>,
}

impl AckQueue {
    pub fn new() -> Self {
        Self {
            inner: VecDeque::new(),
        }
    }

    pub fn push_back(&mut self, seq: u32, ts: Instant) {
        self.inner.push_back((seq, ts));
    }

    pub fn pop_front(&mut self) -> Option<(u32, Instant)> {
        self.inner.pop_front()
    }

    pub fn last(&self) -> Option<(u32, Instant)> {
        if self.inner.is_empty() {
            None
        } else {
            self.inner.get(self.inner.len() - 1).copied()
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
        stream: LiveStream<S::Stream>,
        message_number: u32,
    },
}

impl<S> ConnectionMode<S>
where
    S: SessionManager,
{
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

#[derive(Default)]
pub enum PollState {
    #[default]
    Read,
    Write(Pin<Box<dyn Future<Output = ()>>>),
    Close(Pin<Box<dyn Future<Output = Result<(), Error>>>>),
}

#[cfg(test)]
mod tests {
    use super::Rtt;

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
}
