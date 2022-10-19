use std::io::Read;
use std::mem;

use crate::proto::Decode;

/// RTCP packet header.
///
/// 64-bit aligned.
#[derive(Copy, Clone, Debug)]
pub struct Header {
    // 0-1 - Version
    // 2   - P
    // 3-7 - RC
    pub oct0: u8,
    pub pt: u8,
    pub length: u16,
    pub ssrc: u32,
}

impl Header {
    /// Identifies the version of RTP, which is the same in RTCP packets as in RTP data packets.
    /// The version defined by this specification is two (2).
    pub fn version(&self) -> u8 {
        self.oct0 & 0b0000_0011
    }

    /// Indicates if there are extra padding bytes at the end of the RTP packet. Padding may be
    /// used to fill up a block of certain size, for example as required by an encryption algorithm.
    /// The last byte of the padding contains the number of padding bytes that were added
    /// (including itself).
    pub fn padding(&self) -> bool {
        self.oct0 & 0b0000_0100 != 0
    }

    /// RC (Reception report count): (5 bits) The number of reception report blocks contained in this packet. A value of zero is valid.
    pub fn reception_report_count(&self) -> u8 {
        self.oct0 >> 3
    }

    pub fn packet_type(&self) -> u8 {
        self.pt
    }

    /// Buffer size (not memory aligned)
    const fn size() -> usize {
        mem::size_of::<u8>() + mem::size_of::<u8>() + mem::size_of::<u16>() + mem::size_of::<u32>()
    }
}

// impl Decode for Header {
//     fn decode<R>(mut reader: R) -> Self
//     where
//         R: Read,
//     {
//         let mut buf = [0; Self::size()];
//         reader.read_exact(&mut buf).unwrap();

//         let oct0 = buf[0];
//         let pt = buf[1];
//         let length = u16::from_be_bytes([buf[2], buf[3]]);
//         let ssrc = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);

//         Self {
//             oct0,
//             pt,
//             length,
//             ssrc,
//         }
//     }
// }

#[derive(Clone, Debug)]
pub enum PacketType {
    SenderReport,
    ReceiverReport,
    SourceDescription,
    Goodbye,
    ApplicationSpecificMessage,
}
