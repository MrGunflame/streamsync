use proc_macro::TokenStream;

mod packet;

#[proc_macro_derive(Packet)]
pub fn packet(input: TokenStream) -> TokenStream {
    packet::packet(input)
}
