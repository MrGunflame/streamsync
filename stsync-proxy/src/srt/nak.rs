use crate::session::SessionManager;

use super::{
    proto::{DropRequest, Nak},
    server::SrtConnStream,
    state::State,
    Error,
};

pub async fn nak<S>(packet: Nak, stream: SrtConnStream<'_>, _state: State<S>) -> Result<(), Error>
where
    S: SessionManager,
{
    stream
        .conn
        .metrics
        .packets_dropped
        .add(packet.lost_packet_sequence_numbers.len() as usize);

    // We drop all NAKs for now.
    let dropreq = DropRequest::builder()
        .message_number(0)
        .first_packet_sequence_number(packet.lost_packet_sequence_numbers.first())
        .last_packet_sequence_number(packet.lost_packet_sequence_numbers.last())
        .build();

    stream.send(dropreq).await?;
    Ok(())
}
