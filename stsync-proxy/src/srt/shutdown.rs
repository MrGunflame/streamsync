use crate::sink::MultiSink;

use super::{proto::Shutdown, server::SrtConnStream, state::State, Error};

pub async fn shutdown<S>(
    _packet: Shutdown,
    stream: SrtConnStream<'_, S>,
    state: State<S>,
) -> Result<(), Error>
where
    S: MultiSink,
{
    tracing::debug!("Shutting down conn {}", stream.conn.server_socket_id.0);

    super::server::close_metrics(&stream.conn);

    stream.conn.close().await;
    state.pool.remove(stream.conn);
    Ok(())
}
