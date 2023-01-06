use bytes::BytesMut;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use std::future::Future;
use std::io::{self};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::task::JoinHandle;
use tracing::{event, span, Level};

use super::config::Config;
use super::state::State;
use crate::proto::Decode;
use crate::session::SessionManager;
use crate::srt::socket::SrtSocket;
use crate::srt::state::ConnectionId;

use super::{Error, IsPacket, Packet};

pub struct Server<S>
where
    S: SessionManager,
{
    pub state: State<S>,
    workers: FuturesUnordered<Worker>,
}

impl<S> Server<S>
where
    S: SessionManager,
{
    pub fn new<C>(session_manager: S, config: C) -> Result<Self, io::Error>
    where
        C: Into<Config>,
    {
        let config = config.into();

        let socket = SrtSocket::new(config.bind)?;

        let rx = socket.recv_buffer_size()?;
        let tx = socket.send_buffer_size()?;

        tracing::info!("Srt socket listening on {}", socket.local_addr()?);
        tracing::info!("Socket Recv-Q: {}, Send-Q: {}", rx, tx);

        let num_workers = config.workers.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        });

        let socket = Arc::new(socket);
        let state = State::new(session_manager, config);

        let workers = FuturesUnordered::new();
        for i in 0..num_workers {
            workers.push(Worker::new(i, socket.clone(), state.clone()));
        }

        tracing::info!("Spawned {} worker threads", num_workers);

        Ok(Self { state, workers })
    }
}

impl<S> Future for Server<S>
where
    S: SessionManager,
{
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match self.workers.poll_next_unpin(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(err)),
                _ => (),
            }
        }
    }
}

unsafe impl<S> Send for Server<S> where S: SessionManager + Send {}

async fn handle_message<S>(
    packet: Packet,
    addr: SocketAddr,
    socket: &SrtSocket,
    state: &State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    let stream = SrtStream { socket, addr };

    // A destination socket id of 0 indicates a handshake request.
    if packet.header.destination_socket_id == 0 {
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
            handle.send(packet).await;
        }
        None => {
            tracing::debug!("Received packet from unknown client {}", id);
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct SrtStream<'a> {
    pub socket: &'a SrtSocket,
    pub addr: SocketAddr,
}

impl<'a> SrtStream<'a> {
    pub async fn send<T>(&self, packet: T) -> Result<(), Error>
    where
        T: IsPacket,
    {
        self.socket.send_to(packet, self.addr).await.map(|_| ())
    }
}

#[derive(Debug)]
struct Worker {
    handle: JoinHandle<Result<(), Error>>,
}

impl Worker {
    pub fn new<S>(ident: usize, socket: Arc<SrtSocket>, state: State<S>) -> Self
    where
        S: SessionManager,
    {
        let resource_span = span!(Level::TRACE, "Worker");

        let handle = tokio::task::spawn(async move {
            event!(
                parent: &resource_span,
                Level::INFO,
                "Created worker {}",
                ident
            );

            loop {
                let mut buf = BytesMut::zeroed(1500);
                let (len, addr) = socket.recv_from(&mut buf).await?;
                tracing::trace!("[{}] Got {} bytes from {}", ident, len, addr);
                buf.truncate(len);

                let packet = match Packet::decode(&mut buf) {
                    Ok(packet) => packet,
                    Err(err) => {
                        tracing::debug!("[{}] Failed to decode packet: {}", ident, err);
                        continue;
                    }
                };

                let socket = socket.clone();
                handle_message(packet, addr, &socket, &state).await?;
            }
        });

        Self { handle }
    }
}

impl Future for Worker {
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.handle.poll_unpin(cx).map(|res| res.unwrap())
    }
}
