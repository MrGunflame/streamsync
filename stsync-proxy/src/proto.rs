use std::io::{self, Read, Write};
use std::mem;
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
        let mut buf = Vec::new();
        self.encode(&mut buf)?;
        Ok(buf)
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
            }

            impl Decode for $t {
                type Error = io::Error;

                fn decode<R>(mut reader: R) -> Result<Self, Self::Error>
                where
                    R: Read
                {
                    let mut buf = [0;mem::size_of::<Self>()];
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

impl Encode for Vec<u8> {
    type Error = io::Error;

    fn encode<W>(&self, mut writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        writer.write_all(&self)
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

        // Shift bits to the correct position.
        let bits = val.into() << self.0.bits() - range.end;

        // Subtract the current value of the masked bits, then add the new bits.
        self.0 = (self.0 - self.bits(range)) + bits;
    }
}

impl<T> Encode for Bits<T>
where
    T: Bytes + Encode,
{
    type Error = T::Error;

    fn encode<W>(&self, writer: W) -> Result<(), Self::Error>
    where
        W: Write,
    {
        self.0.encode(writer)
    }
}

impl<T> Decode for Bits<T>
where
    T: Bytes + Decode,
{
    type Error = T::Error;

    fn decode<R>(reader: R) -> Result<Self, Self::Error>
    where
        R: Read,
    {
        Ok(Self(T::decode(reader)?))
    }
}

pub trait IntoBitRange {
    fn into_bit_range(self) -> Range<usize>;
}

impl IntoBitRange for Range<usize> {
    fn into_bit_range(self) -> Range<usize> {
        self
    }
}

impl IntoBitRange for usize {
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

            impl Encode for $id {
                type Error = <$t as Encode>::Error;

                fn encode<W>(&self, writer:W) -> Result<(), Self::Error>
                where
                    W: Write
                {
                    self.0.encode(writer)
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
    }
}
