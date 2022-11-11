use cipher::generic_array::{ArrayLength, GenericArray};
use cipher::typenum::{PartialDiv, PartialQuot, U16};
use cipher::StreamCipher;
use ctr::flavors::CtrFlavor;

#[derive(Clone, Debug)]
pub struct Counter {
    block_counter: u16,
    packet_index: u32,
    zeroed: [u8; 10],
}

impl Counter {}

impl<B> CtrFlavor<B> for Counter
where
    B: ArrayLength<u8> + PartialDiv<U16>,
    PartialQuot<B, U16>: ArrayLength<u128>,
{
    type CtrNonce = Nonce;
    type Backend = u128;

    const NAME: &'static str = "128";

    fn remaining(cn: &Self::CtrNonce) -> Option<usize> {}

    fn next_block(cn: &mut Self::CtrNonce) -> GenericArray<u8, B> {}

    fn current_block(cn: &Self::CtrNonce) -> GenericArray<u8, B> {}

    fn from_nonce(block: &GenericArray<u8, B>) -> Self::CtrNonce {}

    fn set_from_backend(cn: &mut Self::CtrNonce, v: Self::Backend) {
        cn.0 = v;
    }

    fn as_backend(cn: &Self::CtrNonce) -> Self::Backend {
        cn.0
    }
}

#[derive(Clone)]
pub struct Nonce(u128);
