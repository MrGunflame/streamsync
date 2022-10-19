pub struct Header {}

// +-------------+                           +-------------+
// |    Client   |       TCP/IP Network      |    Server   |
// +-------------+            |              +-------------+
//       |                    |                     |
//  Uninitialized             |               Uninitialized
//       |          C0        |                     |
//       |------------------->|         C0          |
//       |                    |-------------------->|
//       |          C1        |                     |
//       |------------------->|         S0          |
//       |                    |<--------------------|
//       |                    |         S1          |
//  Version sent              |<--------------------|
//       |          S0        |                     |
//       |<-------------------|                     |
//       |          S1        |                     |
//       |<-------------------|                Version sent
//       |                    |         C1          |
//       |                    |-------------------->|
//       |          C2        |                     |
//       |------------------->|         S2          |
//       |                    |<--------------------|
//    Ack sent                |                  Ack Sent
//       |          S2        |                     |
//       |<-------------------|                     |
//       |                    |         C2          |
//       |                    |-------------------->|
//  Handshake Done            |               Handshake Done
//       |                    |                     |

/// Send by client, then answered by server.
struct Chunk0 {
    /// Version (8 bits): In C0, this field identifies the RTMP version requested by the client. In S0, this field identifies the RTMP version selected by the server. The version defined by this specification is 3. Values 0-2 are deprecated values used by earlier proprietary products; 4-31 are reserved for future implementations; and 32-255 are not allowed (to allow distinguishing RTMP from text-based protocols, which always start with a printable character). A server that does not recognize the client’s requested version SHOULD respond with 3. The client MAY choose to degrade to version 3, or to abandon the handshake.
    version: u8,
}

struct Chunk1 {
    /// Time (4 bytes): This field contains a timestamp, which SHOULD be used as the epoch for all future chunks sent from this endpoint. This may be 0, or some arbitrary value. To synchronize multiple chunkstreams, the endpoint may wish to send the current value of the other chunkstream’s timestamp.
    time: u32,
    /// Zero (4 bytes): This field MUST be all 0s.
    zero: Zeroed<u32>,
    /// Random data (1528 bytes): This field can contain any arbitrary values. Since each endpoint has to distinguish between the response to the handshake it has initiated and the handshake initiated by its peer,this data SHOULD send something sufficiently random. But there is no need for cryptographically-secure randomness, or even dynamic values.
    random: [u8; 1528],
}

pub struct Zeroed<T>(T);

pub trait Zeroable {}

struct Chunk2 {
    /// Time (4 bytes): This field MUST contain the timestamp sent by the peer in S1 (for C2) or C1 (for S2).
    time: u32,
    /// Time2 (4 bytes): This field MUST contain the timestamp at which the previous packet(s1 or c1) sent by the peer was read.
    time2: u32,
    /// Random echo (1528 bytes): This field MUST contain the random data field sent by the peer in S1 (for C2) or S2 (for C1). Either peer can use the time and time2 fields together with the current timestamp as a quick estimate of the bandwidth and/or latency of the connection, but this is unlikely to be useful.
    random: [u8; 1528],
}
