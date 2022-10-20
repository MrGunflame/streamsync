use std::borrow::Borrow;
use std::collections::HashSet;
use std::future::Future;
use std::io::Write;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

use futures::Stream;
use serde::de::IntoDeserializer;
use tokio::net::UdpSocket;

use super::state::State;
use crate::proto::{Bits, Decode, Encode};
use crate::srt::proto::{Keepalive, LightAck};
use crate::srt::{AckPacket, ControlPacketType, PacketType};

use super::{Error, Packet};

pub async fn serve(socket: UdpSocket) -> Result<(), ()> {
    let state = State {
        connections: Arc::default(),
    };

    let mut frame_count = 0usize;
    let mut sequence_num = 12345;
    let mut remote_socket_id = 0;

    let mut file = std::fs::File::create("out.ts").unwrap();

    let socket = Arc::new(socket);

    loop {
        let mut buf = [0; 1500];
        let (len, addr) = socket.recv_from(&mut buf).await.unwrap();
        println!("Accept {:?}", addr);

        let mut packet = match Packet::decode(&buf[..len]) {
            Ok(packet) => packet,
            Err(err) => {
                println!("Failed to decode packet: {}", err);
                continue;
            }
        };

        println!("RECV {:?}", packet);

        match packet.header.packet_type() {
            PacketType::Data => {
                println!("handle data");
                println!("Body: {}", packet.body.len());

                file.write_all(&packet.body).unwrap();

                let seqnum = packet.header.seg0.0 .0;
                println!("SEQNUM: {:?}", seqnum);

                // frame_count += 1;
                // if frame_count == 64 {
                //     let ack = LightAck::builder()
                //         .last_acknowledged_packet_sequence_number(seqnum + 1)
                //         .build();

                //     socket
                //         .send_to(&ack.encode_to_vec().unwrap(), addr)
                //         .await
                //         .unwrap();

                //     println!("Send LightACK");
                // }

                let mut ack = AckPacket::default();
                ack.header.set_packet_type(PacketType::Control);
                ack.header.timestamp = packet.header.timestamp + 1;
                ack.header.destination_socket_id = remote_socket_id;
                let mut header = ack.header.as_control().unwrap();
                header.set_control_type(ControlPacketType::Ack);
                ack.header.seg1 = Bits((sequence_num).into());
                ack.last_acknowledged_packet_sequence_number = seqnum + 1;
                ack.rtt = 100_000;
                ack.rtt_variance = 50_000;
                ack.avaliable_buffer_size = 5000;
                ack.packets_receiving_rate = 1500;
                ack.estimated_link_capacity = 5000;
                ack.receiving_rate = 500000;

                sequence_num += 1;

                let mut buf = Vec::new();
                ack.encode(&mut buf).unwrap();

                socket.send_to(&buf, addr).await.unwrap();

                //
            }
            PacketType::Control => {
                println!("control");

                let state = state.clone();

                match packet.header.as_control().unwrap().control_type() {
                    ControlPacketType::Handshake => {
                        remote_socket_id =
                            super::handshake::handshake(&socket, state).await.unwrap();
                    }
                    ControlPacketType::Keepalive => {
                        println!("keepalive");

                        let mut keepalive = Keepalive::builder().build();
                        keepalive.header.destination_socket_id = remote_socket_id;
                        keepalive.header.timestamp = packet.header.timestamp + 1;

                        socket
                            .send_to(&keepalive.encode_to_vec().unwrap(), addr)
                            .await
                            .unwrap();
                    }
                    _ => {
                        println!("other control packet");
                    }
                }
            }
        }
    }
}
