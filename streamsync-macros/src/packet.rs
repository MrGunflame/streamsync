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

    let expanded = quote! {
        impl #ident {
            #decode_body_fn
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
        fn decode_body<B>(mut bytes: B, header: crate::srt::Header) -> Result<Self, crate::srt::Error>
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
