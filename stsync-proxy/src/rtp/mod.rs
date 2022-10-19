use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;

#[derive(Clone, Debug)]
pub struct Header<'a> {
    // Byte 0
    pub version: Bits<'a, BigEndian, 2>,
    pub padding: Bits<'a, BigEndian, 1>,
    pub extension: Bits<'a, BigEndian, 1>,
    pub csrc_count: Bits<'a, BigEndian, 4>,
    // Byte 1
    pub marker: Bits<'a, BigEndian, 1>,
    pub payload_type: Bits<'a, BigEndian, 7>,
    // Byte 2-3
    pub sequence: Bits<'a, BigEndian, 16>,
    pub timestamp: Bits<'a, BigEndian, 32>,
    pub ssrc: Bits<'a, BigEndian, 32>,
    pub csrc: &'a [u32],
}

pub struct Bits<'a, T: ByteOrder, const N: usize> {
    bytes: &'a [u8],
    _marker: PhantomData<T>,
}

impl<'a, T: ByteOrder, const N: usize> Clone for Bits<'a, T, N> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: ByteOrder, const N: usize> Copy for Bits<'a, T, N> {}

impl<'a, T: ByteOrder, const N: usize> Bits<'a, T, N> {}

impl<'a, T: ByteOrder, const N: usize> Debug for Bits<'a, T, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.bytes.fmt(f)
    }
}

pub trait ByteOrder {}

pub struct BigEndian;

impl ByteOrder for BigEndian {}
