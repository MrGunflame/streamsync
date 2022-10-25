use std::sync::atomic::Ordering;

use crate::sink::MultiSink;

use super::{server::SrtConnStream, state::State, DataPacket, Error};

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
    //     // Append the lost sequence number.
    //     let mut lost_packets = stream.conn.lost_packets.lock().unwrap();
    //     lost_packets.push(client_sequence_number);
    // }

    let mut lost_packets = stream.conn.lost_packets.lock().unwrap();
    lost_packets.retain(|seq| *seq != seqnum);

    stream
        .conn
        .client_sequence_number
        .fetch_add(1, Ordering::AcqRel);

    Ok(())
}
