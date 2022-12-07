# Streamsync

The current approach is to avoid any direct stream sychronisation and instead uses a 
low-latency custom streaming server implementation.

The current implementation purely accepts a stream, and broadcasts it directly to all
subscribers. If needed it is possible to hook directly into the data stream to add custom
delays depending on the streams.

The server uses the SRT (*Secure Reliable Transport*) protocol to wrap the MPEG-TS stream. SRT is 
based on UDP which means the transport stream is unreliable. This is a good thing as this
means we never need to stall the whole stream due to lost packets. The amount of video 
details lost is not significant unless you reach a very high packet loss ratio (> 10%).

## Building

There are two ways of building: a native binary or docker.

### Native binary

**Make sure you have a stable rust toolchain installed.**

```
cargo build --release --bin stsync-proxy
```

### Docker

A `Dockerfile` is included to build the docker image from scratch. The provided `Makefile`
includes a `docker` target that tags the docker image automatically:

```
cd stsync-proxy && make docker
```

Alternatively you can build the docker image manually:

```
docker build --rm -t stsync-proxy -f stsync-proxy/Dockerfile .
```

## Configuration

The only option currently supported is `RUST_LOG` which defines the loglevel of the server.
Possible values are `trace`, `debug`, `info`, `warn`, `error` and `off`.

## System Resources

The server does not impose any direct requirements, but the number of runnable streams
heavily depends on the avaliable resources. The most important requirement is raw UDP
throughput; the most important resources are network bandwidth and CPU performance. Server
performance scales almost linearly with thread count.

As system exhaustion increases you will see increasing packet loss and with that loss of
video data.

With the default MTU of 1500 and default buffer size of 8192, the memory usage for a single
stream will be roughly 12MB.

## Running the server

### Docker

After building the docker image (using the command above) you can run a docker container. **It is recommended to run the container
in `network=host` mode due to the significant overhead of dockers NAT. Note that this also exposes the HTTP server on 9998 by default.**

```
docker run -d --name streamsync -e RUST_LOG0=info --network=host stsync-proxy:latest
```

The default configuration binds the SRT server to `0.0.0.0:9999`. It additionally binds a
HTTP server to `0.0.0.0:9998`. The HTTP server is currently only requires to read server
and connection metrics (see below).

The SRT server follows the normal caller-listener handshake procedure (where the server is 
the listener). A StreamID needs to be provided when connecting to the server. The format 
used is the [recommended syntax](https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#appendix-B.1) defined. The syntax looks as follows:

`#!::key1=value1,key2=value2,etc...`

The nested syntax is unsupported. Providing an invalid StreamID syntax will result in a 
`REJ_ROGUE` rejection. From the standard keys three keys are understood and always **required**:

| Key | Type                    | Note |
| --- | ----------------------- | ----
| r   | hex-encoded value       | (Resource Name). A unique identifier to a stream. |
| s   | hex-encoded value       | (Session ID). An arbitrary key for a client. This serves as a authorization token. |
| m   | `request \| publish` | (Mode). What a client wants to do with a stream. `publish` means the client wants to publish a stream. `request` means a client wants to view a stream. |

## Acquiring a session key

A session id/session key is a hex-encoded integer that allows access to a single stream. 
Session keys have a limited lifetime of 5 minutes and are single use. This is to prevent
potential man-in-the-middle attacks.

Before acquiring a session key you need an access token. These defined in `config.json`.
**They should only be transmitted over a secure connection.** You can then make a HTTP POST
request to `/v1/streams/:id/sessions`, with the access token attached as a Bearer token in
the `Authorization` header.

The returned response contains the stream and session id:
```
{
    "resource_id": "1",
    "session_id": "91bf7a9ed500c8ce"
}
```

The session key expires 5 minutes after being issued.

## Publishing via FFmpeg

FFmpeg supports streaming over SRT. For example to stream a `test.ts` file to `127.0.0.1:9999` you can use the following command:

```
ffmpeg -re -i test.ts -acodec copy -vcodec copy -f mpegts 'srt://127.0.0.1:9999?streamid=#!::r=1,s=1,m=publish'
```
Note the use of the `-re` flag, without that flag FFmpeg will not stream but transcode the 
file to the given output. Adjust `r=1` and `s=1` to your resource and session id accordingly.

## Requesting (Viewing) via ffplay

ffplay is a really simple video player akin to libvlc or cvlc using FFmpeg under the hood.
Using ffplay one can view a SRT stream using the following command:

```
ffplay 'srt://127.0.0.1:9999?streamid=#!::r=1,s=1,m=request'
```

Again, adjust `r=1` and `s=1` to your resource and session id accordingly.

## Publishing via OBS

OBS uses FFmpeg under the hood, so the FFmpeg section applies here aswell. To publish a 
stream go into `Settings > Stream`, select `Service: Custom...` and enter the same output url
as required by FFmpeg into the `Server` field:

```
srt://127.0.0.1:9999?streamid=#!::r=1,s=1,m=publish
```

Again, adjust `r=1` and `s=1` to your resource and session id accordingly.

## Requesting via VLC

VLC supports the SRT protocol. Unfortunately VLC (3) currently does not support entering a 
StreamID, making it not possible to use VLC to view a stream currently.

## Tuning OBS for low-latency

While the streaming server is easily capable of transmitting sub-second streams, a chunk of
the latency actually comes on the sending end. With some tuning it is possible to bring down
the OBS streaming latency to `< 1s`. This section is currently limited to the `x264` encoder
as that is always avaliable.

| Key          | Value | Note |
| ------------ | ----- | ---- |
| Rate Control | `CBR` | Streams with a constant bitrate. This should always be prefered option for streaming. |
| Bitrate      | ?    | This value heavily depends on your resolution and framerate. |
| Keyframe Interval | `1` | |
| CPU Usage Preset | `superfast` | |
| Profile | `high` | |
| Tune | `zerolatency` | |
| x264 Options | *empty* | |

Most hardware encoders have similar options.

## Server and Connection Monitoring

The included HTTP server (bound on `0.0.0.0:9998` by default) includes a prometheus 
compatible metrics endpoint at `/v1/metrics`. It currently exposes the following metrics:

### Server metrics

The server metrics are associated with the main server process and are always avaliable.

| Name                      | Labels | Note |
| ------------------------- | ------ | ---- |
| `srt_connections_total`   | *None* | An ever-increasing counter of connections made to the server. |
| `srt_connections_current` | mode={`handshake`\|`request`\|`publish`} | The number of active connections in each mode. The `handshake` mode is only used while the connection is still being established. |

### Connection metrics

The connection metrics are avaliable for each connection. They are not shown when there are
no connections currently. All metrics share a `id` label that uniquely identifies the connection.

| Name                               | Labels | Note |
| ---------------------------------- | ------ | ---------------- |
| `srt_connection_ctrl_packets_sent` | *None* | The number of control packets sent to the remote peer. | 
| `srt_connection_ctrl_packets_recv` | *None* | The number of control packets received from the remote peer. |
| `srt_connection_ctrl_packets_lost` | *None* | The number of control packets lost. *Note that this metric is purely an estimation.* |
| `srt_connection_ctrl_bytes_sent`   | *None* | The number of bytes sent to the remote peer in control packets. |
| `srt_connection_ctrl_bytes_recv`   | *None* | The number of bytes received from the remote peer in control packets. |
| `srt_connection_ctrl_bytes_lost`   | *None* | The number of bytes lost in control packets. *Note that this metrics is purely an estimation.* |
| `srt_connection_data_packets_sent` | *None* | The number of data packets sent to the remote peer. |
| `srt_connection_data_packets_recv` | *None* | The number of data packets received from the remote peer. |
| `srt_connection_data_packets_lost` | *None* | The number of data packets lost. |
| `srt_connection_data_bytes_sent`   | *None* | The number of bytes sent to the remote peer in data packets. |
| `srt_connection_data_bytes_recv`   | *None* | The number of bytes received from the remote peer in data packets. |
| `srt_connection_data_bytes_lost`   | *None* | The number of bytes lost in data packets. *This metric is an estimation based on the number of lost data packets and the MTU.* |
| `srt_connection_rtt`               | *None* | The round-trip time to the remote peer. |
| `srt_connection_rtt_variance`      | *None* | The variance in round-trip time to the remote peer. |

### Todo list

- [ ] Workers
- [ ] TSDBD (especially for the reciver)
- [ ] AES encryption
- [ ] A secure way to bootstrap the AES encryption and exchange resource/session ids
- [ ] Potentially an OBS plugin that automates configuration
