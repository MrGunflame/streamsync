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
            let rtt = ts.elapsed().as_micros() as u32;

            tracing::debug!("Got ACKACK with RTT {}", rtt);

            stream.conn.update_rtt(rtt);

            state.pool.insert(stream.conn);
        }
        None => (),
    }

    Ok(())
}
