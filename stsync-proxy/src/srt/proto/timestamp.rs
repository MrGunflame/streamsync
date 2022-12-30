use std::io::Write;
use std::time::Duration;

use bytes::Buf;

use crate::proto::{Decode, Encode};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
