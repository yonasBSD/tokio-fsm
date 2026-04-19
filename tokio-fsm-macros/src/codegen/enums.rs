use proc_macro2::TokenStream;
use quote::quote;

use crate::validation::FsmStructure;

pub fn render_state_enum(fsm: &FsmStructure) -> TokenStream {
    let states: Vec<_> = fsm.states.iter().map(|s| &s.name).collect();
    let state_enum_name = fsm.state_enum_ident();

    let state_structs: Vec<_> = fsm
        .states
        .iter()
        .map(|s| {
            let name = &s.name;
            let enum_name = &state_enum_name;
            quote! {
                #[derive(Debug, Clone, Copy)]
                pub struct #name;
                impl From<#name> for #enum_name {
                    fn from(_: #name) -> Self {
                        #enum_name::#name
                    }
                }
            }
        })
        .collect();

    let serde_derive = if fsm.serde {
        quote! { #[derive(::tokio_fsm::serde::Serialize, ::tokio_fsm::serde::Deserialize)] }
    } else {
        quote! {}
    };

    quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #serde_derive
        pub enum #state_enum_name {
            #(#states,)*
        }

        #(#state_structs)*
    }
}

pub fn render_event_enum(fsm: &FsmStructure) -> TokenStream {
    let variants: Vec<TokenStream> = fsm
        .events
        .iter()
        .map(|event| {
            let event_name = &event.name;
            if let Some(ref payload_type) = event.payload_type {
                quote! { #event_name(#payload_type), }
            } else {
                quote! { #event_name, }
            }
        })
        .collect();

    let event_enum_name = fsm.event_enum_ident();

    let serde_derive = if fsm.serde {
        quote! { #[derive(::tokio_fsm::serde::Serialize, ::tokio_fsm::serde::Deserialize)] }
    } else {
        quote! {}
    };

    quote! {
        #[derive(Debug, Clone)]
        #serde_derive
        pub enum #event_enum_name {
            #(#variants)*
        }
    }
}
