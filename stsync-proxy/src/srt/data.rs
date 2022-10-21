use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use crate::{proto::Encode, sink::MultiSink, srt::proto::Nak};

use super::{proto::Ack, server::SrtConnStream, state::State, DataPacket, Error};

pub async fn handle_data<S>(
    packet: DataPacket,
    stream: SrtConnStream<'_, S>,
    state: State<S>,
) -> Result<(), Error>
where
    S: MultiSink,
{
    let seqnum = packet.packet_sequence_number();
    tracing::debug!("SEQNUM {}", seqnum);

    if let Err(err) = stream.conn.write_sink(&state, packet).await {
        tracing::error!("Failed to write to sink");
    }

    let client_sequence_number = stream.conn.client_sequence_number.load(Ordering::Acquire);
    // if client_sequence_number != seqnum {
    //     // We lost a packet. Send a NAK.
    //     tracing::trace!("Lost packet {}", client_sequence_number);
    //     // TODO: NAK
    //     let nak = Nak::builder()
    //         .lost_packet_sequence_number(client_sequence_number)
    //         .build();

    //     stream.send(nak).await?;
    //     return Ok(());
    // }

    let mut inflight_acks = stream.conn.inflight_acks.lock().unwrap();
    // Full ACK every 10ms.
    let should_send_ack = match inflight_acks.last() {
        // Wait 10ms after the last ACK.
        Some((_, ts)) => ts.elapsed() >= Duration::from_millis(10),
        // No ACK sent yet. We sent the first ACK 10ms after connection start.
        None => stream.conn.start_time.elapsed() >= Duration::from_millis(10),
    };

    if should_send_ack {
        tracing::trace!("Sending full ACK");

        let (rtt, rtt_variance) = stream.conn.rtt.load();

        let mut ack = Ack::builder()
            .last_acknowledged_packet_sequence_number(seqnum + 1)
            .rtt(rtt)
            .rtt_variance(rtt_variance)
            .avaliable_buffer_size(5000)
            .packets_receiving_rate(1500)
            .estimated_link_capacity(5000)
            .receiving_rate(5000)
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

        stream.send(ack).await?;
    }

    stream
        .conn
        .client_sequence_number
        .fetch_add(1, Ordering::AcqRel);

    Ok(())
}
