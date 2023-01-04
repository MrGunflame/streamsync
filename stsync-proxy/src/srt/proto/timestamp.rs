use std::cmp::Ordering;
use std::io::Write;
use std::time::Duration;

use bytes::Buf;

use crate::proto::{Decode, Encode};
use crate::utils::serial;

const WRAP_PERIOD: u32 = u32::MAX - Duration::from_secs(30).as_micros() as u32;

/// A SRT timestamp.
///
/// The timestamp wraps around every 01:11:35 hours (`0xFFFF_FFFF`). The wrapping period starts
/// 30 seconds before that point (`0xFE36_3c7F`). The [`PartialEq`] and [`PartialOrd`]
/// implementations mimic this behaivoir.
///
/// See https://datatracker.ietf.org/doc/html/draft-sharabayko-srt-01#section-4.5.1.1 for more
/// details.
#[derive(Copy, Clone, Debug, Default, Hash)]
#[repr(transparent)]
pub struct Timestamp {
    /// Timestamp relative to connection start in microseconds.
    timestamp: u32,
}

impl Timestamp {
    /// Creates a new `Timestamp` based on the number of `micros` since start of the stream.
    #[inline]
    pub const fn from_micros(micros: u32) -> Self {
        Self { timestamp: micros }
    }

    /// Returns the `Timestamp` as microseconds since stream start.
    #[inline]
    pub const fn as_micros(self) -> u32 {
        self.timestamp
    }

    /// Returns the `Timestamp` as a [`Duration`] since stream start.
    #[inline]
    pub const fn to_duration(self) -> Duration {
        Duration::from_micros(self.timestamp as u64)
    }

    /// Returns `true` if the delivered timestamp is within the wrapping period.
    #[inline]
    pub fn is_wrapping(self) -> bool {
        self.timestamp >= WRAP_PERIOD
    }
}

impl From<Timestamp> for Duration {
    #[inline]
    fn from(value: Timestamp) -> Self {
        value.to_duration()
    }
}

impl Encode for Timestamp {
    type Error = <u32 as Encode>::Error;

    #[inline]
    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.timestamp.encode(writer)
    }
}

impl Decode for Timestamp {
    type Error = <u32 as Decode>::Error;

    #[inline]
    fn decode<B>(bytes: &mut B) -> Result<Self, Self::Error>
    where
        B: Buf,
    {
        let timestamp = u32::decode(bytes)?;
        Ok(Self { timestamp })
    }
}

impl PartialEq for Timestamp {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Ord::cmp(self, other).is_eq()
    }
}

impl Eq for Timestamp {}

impl PartialOrd for Timestamp {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        // FIXME: This is not strictly correct and uses the the serial comparasion instead.
        // Instead using WRAP_PERIOD would be better.
        serial::cmp::<32>(self.timestamp, other.timestamp)
    }
}
