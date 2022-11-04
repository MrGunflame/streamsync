use std::io::{self, Read, Write};
use std::mem::{self, MaybeUninit};
use std::ops::{Add, BitAnd, BitOr, Deref, DerefMut, Range, Shl, Shr, Sub};

pub trait Decode: Sized {
    type Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read;
}

pub trait Encode {
    type Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write;

    fn encode_to_vec(&self) -> Result<Vec<u8>, Self::Error> {
        let mut buf = Vec::with_capacity(self.size_hint());
        self.encode(&mut buf)?;
        Ok(buf)
    }

    /// Returns a hint about the expected size of `self` requires for encoding. The returned value
    /// is purely a hint and not a guarantee.
    #[inline]
    fn size_hint(&self) -> usize {
        0
    }
}

macro_rules! impl_uint_be {
    ($($t:ty),*$(,)?) => {
        $(
            impl Encode for $t {
                type Error = io::Error;

                fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
                where
                    W: Write
                {
                    writer.write_all(&self.to_be_bytes())
                }

                #[inline]
                fn size_hint(&self) -> usize {
                    mem::size_of::<Self>()
                }
            }

            impl Decode for $t {
                type Error = io::Error;

                fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
                where
                    R: Read
                {
                    let mut buf = [0 ;mem::size_of::<Self>()];
                    reader.read_exact(&mut buf)?;
                    Ok(Self::from_be_bytes(buf))
                }
            }
        )*
    };
}

impl_uint_be! {
    u8,
    u16,
    u32,
    u64,
    u128,
}

impl Encode for [u8] {
    type Error = io::Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        writer.write_all(&self)
    }

    fn size_hint(&self) -> usize {
        self.len()
    }
}

impl Decode for Vec<u8> {
    type Error = io::Error;

    fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        let mut buf = Vec::with_capacity(256);
        reader.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

/// A transparent wrapper around `T` used to directly manipulate bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Bits<T>(pub T)
where
    T: Bytes;

impl<T> Bits<T>
where
    T: Bytes,
{
    pub fn bits<R>(&self, range: R) -> T
    where
        R: IntoBitRange,
    {
        let range = range.into_bit_range();

        let num = range.len();
        let start = self.0.bits() - range.end;

        let mask = ((1 << num) - 1) << start;

        // let bits = self.0.bits() - 1 - range.start;

        // let bits = self.0.bits() - range.end;
        (self.0 & mask) >> start
    }

    pub fn set_bits<R, V>(&mut self, range: R, val: V)
    where
        R: IntoBitRange,
        V: Into<T>,
    {
        let range = range.into_bit_range();

        // Number of bits
        let num = range.len();
        let start = self.0.bits() - range.end;

        let mask = ((1 << num) - 1) << start;

        let curr = self.0 & mask;
        let new = val.into() << start;

        // Subtract the current value, then add the new value.
        self.0 = self.0 - curr + new;
    }
}

impl<T> Encode for Bits<T>
where
    T: Bytes + Encode,
{
    type Error = T::Error;

    #[inline]
    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.0.encode(writer)
    }

    #[inline]
    fn size_hint(&self) -> usize {
        Encode::size_hint(&self.0)
    }
}

impl<T> Decode for Bits<T>
where
    T: Bytes + Decode,
{
    type Error = T::Error;

    #[inline]
    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        Ok(Self(T::decode(reader)?))
    }
}

unsafe impl<T> Zeroable for Bits<T> where T: Bytes + Zeroable {}

pub trait IntoBitRange {
    fn into_bit_range(self) -> Range<usize>;
}

impl IntoBitRange for Range<usize> {
    #[inline]
    fn into_bit_range(self) -> Range<usize> {
        self
    }
}

impl IntoBitRange for usize {
    #[inline]
    fn into_bit_range(self) -> Range<usize> {
        self..self + 1
    }
}

pub trait Bytes:
    Add<Output = Self>
    + Sub<Output = Self>
    + Shl<usize, Output = Self>
    + Shr<usize, Output = Self>
    + BitOr<usize, Output = Self>
    + BitAnd<usize, Output = Self>
    + Default
    + Sized
    + Copy
{
    fn bits(&self) -> usize;
}

macro_rules! uint_newtype {
    ($($id:ident, $t:ty),*$(,)?) => {
        $(
            #[doc = concat!("A `", stringify!($t), "` that implements [`Bytes`] and all required traits.")]
            #[doc = ""]
            #[doc = concat!("This is transparent wrapper around `", stringify!($t), "`.")]
            #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
            #[repr(transparent)]
            pub struct $id(pub $t);

            impl Add for $id {
                type Output = Self;

                fn add(self, rhs: Self) -> Self::Output {
                    Self(self.0 + rhs.0)
                }
            }

            impl Sub for $id {
                type Output = Self;

                fn sub(self, rhs: Self) -> Self::Output {
                    Self(self.0 - rhs.0)
                }
            }

            impl Shl<usize> for $id {
                type Output = Self;

                fn shl(self, rhs: usize) -> Self::Output {
                    Self(self.0 << rhs)
                }
            }

            impl Shr<usize> for $id {
                type Output = Self;

                fn shr(self, rhs: usize) -> Self::Output {
                    Self(self.0 >> rhs)
                }
            }

            impl BitAnd<usize> for $id {
                type Output = Self;

                fn bitand(self, rhs: usize) -> Self::Output {
                    Self(self.0 & rhs as $t)
                }
            }

            impl BitOr<usize> for $id {
                type Output = Self;

                fn bitor(self, rhs: usize) -> Self::Output {
                    Self(self.0 | rhs as $t)
                }
            }

            impl PartialEq<$t> for $id {
                fn eq(&self, other: &$t) -> bool {
                    self.0 == *other
                }
            }

            impl Deref for $id {
                type Target = $t;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl DerefMut for $id {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }

            impl Bytes for $id {
                fn bits(&self) -> usize {
                    <$t>::BITS as usize
                }
            }

            impl From<$t> for $id {
                fn from(src: $t) -> Self {
                    Self(src)
                }
            }

            impl<'a> From<&'a $t> for $id {
                fn from(src: &'a $t) -> Self {
                    Self(*src)
                }
            }

            impl Encode for $id {
                type Error = <$t as Encode>::Error;

                fn encode<W>(&self, writer:W) -> Result<(), Self::Error>
                where
                    W: Write
                {
                    self.0.encode(writer)
                }

                fn size_hint(&self) -> usize {
                    Encode::size_hint(&self.0)
                }
            }

            impl Decode for $id {
                type Error = <$t as Decode>::Error;

                fn decode<R>(reader: R) -> Result<Self, Self::Error>
                where
                    R: Read
                {
                    Ok(Self(<$t>::decode(reader)?))
                }
            }

            unsafe impl Zeroable for $id {}
        )*
    };
}

uint_newtype! {
    U8, u8,
    U16, u16,
    U32, u32,
    U64, u64,
    U128, u128,
}

#[cfg(test)]
mod tests {
    use super::{Bits, U16, U32, U8};

    #[test]
    fn test_bits_be() {
        let bits = Bits(U8(0b1000_0000));
        assert_eq!(bits.bits(0..1), 1);
        assert_eq!(bits.bits(1..8), 0);

        let bits = Bits(U8(0b0111_1111));
        assert_eq!(bits.bits(0..1), 0);
        assert_eq!(bits.bits(1..8), 0b0111_1111);

        let mut bits = Bits(U8(0));
        bits.set_bits(0..1, 1);
        assert_eq!(bits.0, 0b1000_0000);

        bits.set_bits(5..7, 0b11);
        assert_eq!(bits.0, 0b1000_0110);

        let bits = Bits(U16(256 + 255));
        assert_eq!(bits.bits(0..7), 0);
        assert_eq!(bits.bits(7..8), 1);
        assert_eq!(bits.bits(8..16), 255);

        // let bits = Bits(U32(0x387249d9));
        // assert_eq!(bits.bits(0..1), 0);
        // assert_eq!(bits.bits(1..32), 947014105);

        let bits = Bits(U32(2147876864));
        assert_eq!(bits.bits(0..1), 1);
        assert_eq!(bits.bits(1..8), 0);
        assert_eq!(bits.bits(8..16), 6);
        assert_eq!(bits.bits(16..32), 0);

        let mut bits = Bits(U16(256));
        assert_eq!(bits.bits(0..7), 0);
        assert_eq!(bits.bits(7..8), 1);
        assert_eq!(bits.bits(8..16), 0);

        bits.set_bits(0..1, 1);
        assert_eq!(bits.bits(0..1), 1);
        assert_eq!(bits.bits(1..7), 0);
        assert_eq!(bits.bits(7..8), 1);
        assert_eq!(bits.bits(8..16), 0);

        bits.set_bits(0..1, 0);
        assert_eq!(bits.bits(0..1), 0);
        assert_eq!(bits.bits(1..7), 0);
        assert_eq!(bits.bits(7..8), 1);
        assert_eq!(bits.bits(8..16), 0);
    }
}

/// A type that can safely be initialized with a full zero-bit pattern.
///
/// # Safety
///
/// When implementing `Zeroable` the same safety guarantees as [`MaybeUninit::zeroed`] must be
/// given. This means the value must valid for a all zero-bit pattern.
///
/// In particular all integer types are `Zeroable`. In addition a struct that only contains fields
/// that are `Zeroable` can also be safely marked as `Zeroable`.
pub unsafe trait Zeroable: Sized {
    /// Creates a new, zeroed value.
    #[inline]
    fn zeroed() -> Self {
        // SAFETY: The implementor that a zeroed value is valid.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }
}

unsafe impl Zeroable for u8 {}
unsafe impl Zeroable for u16 {}
unsafe impl Zeroable for u32 {}
unsafe impl Zeroable for u64 {}

unsafe impl Zeroable for i8 {}
unsafe impl Zeroable for i16 {}
unsafe impl Zeroable for i32 {}
unsafe impl Zeroable for i64 {}

unsafe impl Zeroable for f32 {}
unsafe impl Zeroable for f64 {}
