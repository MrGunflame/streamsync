use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

use futures::Sink;
use tokio::net::UdpSocket;

use crate::srt::proto::Nak;
use crate::{
    session::SessionManager,
    srt::{proto::Ack, server::SrtStream},
};

use super::state::Connection;
use super::{proto::AckAck, server::SrtConnStream, state::State, Error};

pub async fn ack<S>(packet: Ack, stream: SrtConnStream<'_>, state: State<S>) -> Result<(), Error>
where
    S: SessionManager,
{
    let acknum = packet.acknowledgement_number();

    let avaliable_buffer_size = packet.avaliable_buffer_size;

    // TODO: This should probably just be a CAS.
    let old = stream
        .conn
        .buffers_avail
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |_| {
            Some(avaliable_buffer_size)
        })
        .unwrap();

    if avaliable_buffer_size > 0 && old == 0 {
        stream.conn.buffers_waker.notify_waiters();
    }

    let ackack = AckAck::builder().acknowledgement_number(acknum).build();

    stream.send(ackack).await?;
    Ok(())
}

pub async fn ackack<S>(
    packet: AckAck,
    stream: SrtConnStream<'_>,
    state: State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    // Calculate the RTT since the last ACK. We need to be careful not to underflow if the client
    // sends a bad timestamp.
    let mut time_sent = None;
    let mut inflight_acks = stream.conn.inflight_acks.lock();
    while let Some((seq, ts)) = inflight_acks.pop_front() {
        if packet.acknowledgement_number() == seq {
            time_sent = Some(ts);
        }
    }
    drop(inflight_acks);

    match time_sent {
        Some(ts) => {
            let rtt = ts.elapsed().as_micros() as u32;

            tracing::debug!("Got ACKACK with RTT {}", rtt);

            stream.conn.rtt.update(rtt);
        }
        None => (),
    }

    Ok(())
}

pub async fn send_ack(stream: &SrtConnStream<'_>) -> Result<(), Error> {
    tracing::trace!("Sending ACK");

    // Send periodic NAK report instead of ACK.
    // Send a NAK with all lost seqnums instead.
    // {
    //     let packet = {
    //         let lost_packets = stream.conn.lost_packets.lock().unwrap();
    //         if !lost_packets.is_empty() {
    //             Some(
    //                 Nak::builder()
    //                     .lost_packet_sequence_numbers(&*lost_packets)
    //                     .build(),
    //             )
    //         } else {
    //             None
    //         }
    //     };

    //     if let Some(packet) = packet {
    //         stream.send(packet).await?;
    //         return Ok(());
    //     }
    // }

    let ack = {
        let mut inflight_acks = stream.conn.inflight_acks.lock();

        let (rtt, rtt_variance) = stream.conn.rtt.load();

        // let timespan = stream.conn.start_time.elapsed().as_secs().max(1) as usize;
        // let packet_recv_rate = stream.conn.metrics.packets_recv.load() / timespan;
        // let bytes_recv_rate = stream.conn.metrics.bytes_recv.load() / timespan;

        let client_seqnum = stream.conn.client_sequence_number.load(Ordering::Acquire);

        let mut ack = Ack::builder()
            .last_acknowledged_packet_sequence_number(client_seqnum)
            .rtt(rtt)
            .rtt_variance(rtt_variance)
            .avaliable_buffer_size(5000)
            // .packets_receiving_rate(packet_recv_rate as u32)
            // .estimated_link_capacity(packet_recv_rate as u32)
            // .receiving_rate(bytes_recv_rate as u32)
            .build();

        let server_sequence_number = stream
            .conn
            .server_sequence_number
            .fetch_add(1, Ordering::AcqRel);

        ack.set_acknowledgement_number(server_sequence_number);

        let now = Instant::now();

        ack.header.destination_socket_id = stream.conn.client_socket_id.0;
        ack.header.timestamp = stream.conn.timestamp();

        inflight_acks.push_back(server_sequence_number, now);

        ack
    };

    stream.send(ack).await?;
    Ok(())
}
