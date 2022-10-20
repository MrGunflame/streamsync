use crate::sink::MultiSink;

use super::{proto::AckAck, server::SrtConnStream, state::State, Error};

pub async fn ackack<S>(
    packet: AckAck,
    mut stream: SrtConnStream<'_, S>,
    state: State<S>,
) -> Result<(), Error>
where
    S: MultiSink,
{
    // Calculate the RTT since the last ACK. We need to be careful not to underflow if the client
    // sends a bad timestamp.
    let mut time_sent = None;
    while let Some((seq, ts)) = stream.conn.inflight_acks.pop_front() {
        if packet.acknowledgement_number() == seq {
            time_sent = Some(ts);
        }
    }

    match time_sent {
        Some(ts) => {
            let rtt = match ts
                .elapsed()
                .as_micros()
                .checked_sub(packet.header.timestamp as u128)
            {
                Some(rtt) => rtt,
                None => {
                    tracing::debug!("Received ACKACK before ACK was sent, that's not right!");
                    return Ok(());
                }
            };

            tracing::debug!("Got ACKACK with RTT {}", rtt);

            stream.conn.update_rtt(rtt as u32);

            state.pool.insert(stream.conn);
        }
        None => (),
    }

    Ok(())
}
