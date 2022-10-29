use crate::session::SessionManager;

use super::{
    proto::{DropRequest, Nak, SequenceNumbers},
    server::SrtConnStream,
    state::State,
    Error,
};

pub async fn nak<S>(packet: Nak, stream: SrtConnStream<'_>, state: State<S>) -> Result<(), Error>
where
    S: SessionManager,
{
    // We drop all NAKs for now.
    let dropreq = DropRequest::builder()
        .message_number(0)
        .first_packet_sequence_number(packet.lost_packet_sequence_numbers.first())
        .last_packet_sequence_number(packet.lost_packet_sequence_numbers.last())
        .build();

    stream.send(dropreq).await?;
    Ok(())
}
