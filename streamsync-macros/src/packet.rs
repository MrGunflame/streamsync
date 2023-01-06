use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Field, Fields};

pub(crate) fn packet(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = input.ident;

    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields,
            Fields::Unnamed(_) => panic!("Unnamed fields are currently not supported"),
            Fields::Unit => panic!("Unit structs are currently not supported"),
        },
        _ => panic!("#[derive(Packet)] only supports structs"),
    };

    let mut pkt_fields = PacketFields::default();
    for field in fields.named {
        if field.ident.as_ref().unwrap() == "header" {
            pkt_fields.header = Some(field);
        } else {
            pkt_fields.fields.push(field);
        }
    }

    let decode_body_fn = expand_decode_body_impl(&pkt_fields);
    let encode_body_fn = expand_encode_body_impl(&pkt_fields);

    let downcast_fn = expand_downcast_impl(&pkt_fields);
    let upcast_fn = expand_upcast_impl(&pkt_fields);

    let expanded = quote! {
        #[automatically_derived]
        impl #ident {
            #decode_body_fn
            #encode_body_fn
        }

        impl crate::srt::IsPacket for #ident {
            type Error = crate::srt::Error;

            #downcast_fn
            #upcast_fn
        }
    };

    TokenStream::from(expanded)
}

#[derive(Debug, Default)]
struct PacketFields {
    header: Option<Field>,
    fields: Vec<Field>,
}

fn expand_decode_body_impl(pkt_fields: &PacketFields) -> TokenStream2 {
    let header_ty = pkt_fields.header.clone().unwrap().ty;

    let fn_impl: TokenStream2 = pkt_fields
        .fields
        .iter()
        .map(|field| {
            let ident = field.ident.clone().unwrap();
            let ty = field.ty.clone();

            quote! {
                #ident: #ty::decode(&mut bytes)?,
            }
        })
        .collect();

    quote! {
        fn decode_body<B>(mut bytes: B, header: #header_ty) -> Result<Self, crate::srt::Error>
        where
            B: ::bytes::Buf,
        {
            Ok(Self {
                header,
                #fn_impl
            })
        }
    }
}

fn expand_encode_body_impl(pkt_fields: &PacketFields) -> TokenStream2 {
    let fn_impl: TokenStream2 = pkt_fields
        .fields
        .iter()
        .map(|field| {
            let ident = field.ident.clone().unwrap();

            quote! {
                self.#ident.encode(&mut writer)?;
            }
        })
        .collect();

    quote! {
        fn encode_body<W>(&self, mut writer: W) -> Result<(), crate::srt::Error>
        where
            W: std::io::Write,
        {
            #fn_impl
            Ok(())
        }
    }
}

fn expand_downcast_impl(pkt_fields: &PacketFields) -> TokenStream2 {
    assert!(pkt_fields.header.is_some());

    quote! {
        fn downcast(mut packet: crate::srt::Packet) -> Result<Self, Self::Error> {
            let header = packet.header.try_into()?;
            Self::decode_body(&mut packet.body, header)
        }
    }
}

fn expand_upcast_impl(pkt_fields: &PacketFields) -> TokenStream2 {
    assert!(pkt_fields.header.is_some());

    quote! {
        fn upcast(self) -> crate::srt::Packet {
            let mut body = Vec::new();
            self.encode_body(&mut body).unwrap();

            crate::srt::Packet {
                header: self.header.into(),
                body: body.into(),
            }
        }
    }
}
