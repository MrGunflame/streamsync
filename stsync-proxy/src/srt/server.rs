use futures::FutureExt;
use socket2::{Domain, Protocol, Socket, Type};
use std::future::Future;
use std::io::{self};
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::net::UdpSocket;

use super::config::Config;
use super::state::State;
use crate::proto::{Decode, Encode};
use crate::session::SessionManager;
use crate::srt::state::ConnectionId;

use super::{Error, IsPacket, Packet};

pub struct Server<S>
where
    S: SessionManager,
{
    pub state: State<S>,
    socket: Arc<UdpSocket>,
    fut: Pin<Box<dyn Future<Output = Result<(), Error>>>>,
}

impl<S> Server<S>
where
    S: SessionManager,
{
    pub fn new(session_manager: S, config: Config) -> Result<Self, io::Error> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_recv_buffer_size(500_000_000)?;
        socket.bind(&SocketAddr::from_str("0.0.0.0:9999").unwrap().into())?;

        let rx = socket.recv_buffer_size()?;
        let tx = socket.send_buffer_size()?;

        let socket = UdpSocket::from_std(socket.into())?;

        tracing::info!("Listening on {}", socket.local_addr()?);
        tracing::info!("Socket Recv-Q: {}, Send-Q: {}", rx, tx);

        let socket = Arc::new(socket);

        let state = State::new(session_manager, config);

        let fut = {
            let state = state.clone();
            let socket = socket.clone();
            Box::pin(async move {
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
                        break Err(err);
                    }
                }
            })
        };

        Ok(Self { state, socket, fut })
    }
}

impl<S> Future for Server<S>
where
    S: SessionManager,
{
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.fut.poll_unpin(cx)
    }
}

async fn handle_message<S>(
    packet: Packet,
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
