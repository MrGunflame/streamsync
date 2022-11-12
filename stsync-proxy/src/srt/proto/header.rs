use std::ops::{Deref, DerefMut};

use crate::srt::{ControlPacketType, Error, PacketType};

use super::Header;

macro_rules! header_impl {
    ($id:ident, $typ:expr $(, $doc:tt)?) => {
        $(
            #[doc = $doc]
        )?
        #[derive(Copy, Clone, Debug)]
        pub struct $id(Header);

        impl TryFrom<Header> for $id {
            type Error = Error;

            fn try_from(mut header: Header) -> Result<Self, Self::Error> {
                let ctrl = header.as_control()?;
                if ctrl.control_type() != $typ {
                    Err(Error::InvalidControlType($typ.to_u16()))
                } else {
                    Ok(Self(header))
                }
            }
        }

        impl Default for $id {
            fn default() -> Self {
                let mut header = Header::default();
                header.set_packet_type(PacketType::Control);
                header.as_control_unchecked().set_control_type($typ);

                Self(header)
            }
        }

        impl Deref for $id {
            type Target = Header;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl DerefMut for $id {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl From<$id> for Header {
            fn from(src: $id) -> Self {
                src.0
            }
        }
    };
}

header_impl!(HandshakeHeader, ControlPacketType::Handshake);
header_impl!(
    KeepaliveHeader,
    ControlPacketType::Keepalive,
    "Header for the [`Keepalive`] packet."
);
header_impl!(AckHeader, ControlPacketType::Ack);
header_impl!(NakHeader, ControlPacketType::Nak);
header_impl!(ShutdownHeader, ControlPacketType::Shutdown);
header_impl!(AckAckHeader, ControlPacketType::AckAck);
header_impl!(DropRequestHeader, ControlPacketType::DropReq);
