use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::net::UdpSocket;

use super::Packet;

pub struct Connection {}
