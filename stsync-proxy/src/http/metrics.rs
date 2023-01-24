use std::fmt::Write;

use hyper::{Body, Response};

use crate::http::Context;

pub(super) async fn metrics(ctx: Context) -> Response<Body> {
    let mut string = String::new();
    let guard = ctx.state.srt.conn_metrics.lock();
    let iter = guard.iter();

    writeln!(
        string,
        "srt_connections_total {}",
        ctx.state.srt.metrics.connections_total
    )
    .unwrap();

    for (mode, gauge) in [
        (
            "handshake",
            &ctx.state.srt.metrics.connections_handshake_current,
        ),
        (
            "request",
            &ctx.state.srt.metrics.connections_request_current,
        ),
        (
            "publish",
            &ctx.state.srt.metrics.connections_publish_current,
        ),
    ] {
        writeln!(
            string,
            "srt_connections_current{{mode=\"{}\"}} {}",
            mode, gauge
        )
        .unwrap();
    }

    for (id, metrics) in iter {
        let id = id.server_socket_id.0;

        writeln!(
            string,
            "srt_connection_data_packets_sent{{id=\"{}\"}} {}",
            id, metrics.data_packets_sent.original
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_data_bytes_sent{{id=\"{}\"}} {}",
            id, metrics.data_bytes_sent.original
        )
        .unwrap();

        for (ctr, label) in [
            (&metrics.data_packets_recv.original, "original"),
            (&metrics.data_packets_recv.retransmitted, "retransmitted"),
            (&metrics.data_packets_recv.dropped, "dropped"),
            (&metrics.data_packets_recv.lost, "lost"),
        ] {
            writeln!(
                string,
                "srt_connection_data_packets_recv{{id=\"{}\",type=\"{}\"}} {}",
                id, label, ctr
            )
            .unwrap();
        }

        for (ctr, label) in [
            (&metrics.data_bytes_recv.original, "original"),
            (&metrics.data_bytes_recv.retransmitted, "retransmitted"),
            (&metrics.data_bytes_recv.dropped, "dropped"),
            (&metrics.data_bytes_recv.lost, "lost"),
        ] {
            writeln!(
                string,
                "srt_connection_data_bytes_recv{{id=\"{}\",type=\"{}\"}} {}",
                id, label, ctr
            )
            .unwrap();
        }

        writeln!(
            string,
            "srt_connection_ctrl_packets_sent{{id=\"{}\"}} {}",
            id, metrics.ctrl_packets_sent
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_bytes_sent{{id=\"{}\"}} {}",
            id, metrics.ctrl_bytes_sent
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_packets_recv{{id=\"{}\"}} {}",
            id, metrics.ctrl_packets_recv
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_bytes_recv{{id=\"{}\"}} {}",
            id, metrics.ctrl_bytes_recv
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_packets_lost{{id=\"{}\"}} {}",
            id, metrics.ctrl_packets_lost
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_ctrl_bytes_lost{{id=\"{}\"}} {}",
            id, metrics.ctrl_bytes_lost
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_rtt{{id=\"{}\"}} {}",
            id, metrics.rtt
        )
        .unwrap();

        writeln!(
            string,
            "srt_connection_rtt_variance{{id=\"{}\"}} {}",
            id, metrics.rtt_variance
        )
        .unwrap();
    }

    Response::builder()
        .status(200)
        .body(Body::from(string))
        .unwrap()
}
