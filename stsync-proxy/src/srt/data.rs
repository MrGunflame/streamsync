use std::sync::atomic::Ordering;

use crate::session::SessionManager;

use super::{server::SrtConnStream, state::State, DataPacket, Error};

pub async fn handle_data<S>(
    packet: DataPacket,
    stream: SrtConnStream<'_>,
    state: State<S>,
) -> Result<(), Error>
where
    S: SessionManager,
{
    let seqnum = packet.packet_sequence_number();
    tracing::debug!("SEQNUM {}", seqnum);

    stream.conn.send(packet).await;

    let client_sequence_number = stream.conn.client_sequence_number.load(Ordering::Acquire);

    // Lost a packet.
    if client_sequence_number != seqnum {
        // Append the lost sequence number.
        let mut lost_packets = stream.conn.lost_packets.lock().unwrap();
        lost_packets.push(client_sequence_number);
    }

    let mut lost_packets = stream.conn.lost_packets.lock().unwrap();
    lost_packets.retain(|seq| *seq != seqnum);

    stream
        .conn
        .client_sequence_number
        .fetch_add(1, Ordering::AcqRel);

    Ok(())
}
