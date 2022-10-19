use std::io::{self, Read, Write};
use std::mem;

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
