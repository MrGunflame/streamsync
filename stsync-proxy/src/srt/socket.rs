use std::io::{ErrorKind, IoSlice, Result};
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};

use socket2::{Domain, Protocol, SockRef, Socket, Type};
use tokio::net::UdpSocket;

use crate::proto::Encode;

use super::IsPacket;

#[derive(Debug)]
pub struct SrtSocket {
    socket: UdpSocket,
}

impl SrtSocket {
    pub fn new(addr: SocketAddr) -> Result<Self> {
        let domain = match addr {
            SocketAddr::V4(_) => Domain::IPV4,
            SocketAddr::V6(_) => Domain::IPV6,
        };

        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_nonblocking(true)?;
        socket.bind(&addr.into())?;
        socket.set_recv_buffer_size(500_000_000)?;

        let socket = UdpSocket::from_std(socket.into())?;

        Ok(Self { socket })
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf).await
    }

    pub async fn send_to<T>(
        &self,
        packet: T,
        addr: SocketAddr,
    ) -> std::result::Result<usize, super::Error>
    where
        T: IsPacket,
    {
        let packet = packet.upcast();

        let header = packet.header.encode_to_vec()?;
        let body = packet.body;

        let header = IoSlice::new(&header);
        let body = IoSlice::new(&body);

        self.send_to_vectored(&[header, body], addr)
            .await
            .map_err(From::from)
    }

    pub async fn send_to_vectored(&self, bufs: &[IoSlice<'_>], addr: SocketAddr) -> Result<usize> {
        loop {
            self.socket.writable().await?;

            match self.as_socket().send_to_vectored(bufs, &addr.into()) {
                Ok(n) => return Ok(n),
                Err(err) if err.kind() != ErrorKind::WouldBlock => return Err(err),
                _ => (),
            }
        }
    }

    #[inline]
    pub fn recv_buffer_size(&self) -> Result<usize> {
        self.as_socket().recv_buffer_size()
    }

    #[inline]
    pub fn send_buffer_size(&self) -> Result<usize> {
        self.as_socket().send_buffer_size()
    }

    #[inline]
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
    }

    #[inline]
    fn as_socket(&self) -> SockRef {
        SockRef::from(self)
    }
}

impl AsRawFd for SrtSocket {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}
