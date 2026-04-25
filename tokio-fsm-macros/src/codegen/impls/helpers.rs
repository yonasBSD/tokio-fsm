//! Shared TokenStream builders and private FSM helper generation.
//!
//! These helpers keep branching on parsed handler semantics in codegen while
//! centralizing the repetitive "apply next state" tail logic in one generated
//! private method.

use proc_macro2::TokenStream;
use quote::quote;

use crate::validation::{FsmStructure, HandlerReturnKind};

pub fn render_fsm_private_helpers(fsm: &FsmStructure) -> TokenStream {
    let state_enum_name = fsm.state_enum_ident();

    let tracing_log = if fsm.tracing {
        quote! {
            if let Some(event_name) = event_name {
                ::tokio_fsm::tracing::info!(
                    from = ?old_state,
                    to = ?self.state,
                    event = event_name,
                    "Transition successful"
                );
            }
        }
    } else {
        quote! {
            let _ = event_name;
        }
    };

    let mut seen_states = Vec::new();
    let mut timeout_arms = Vec::new();

    for handler in &fsm.handlers {
        let Some(duration) = handler.timeout else {
            continue;
        };

        let secs = duration.as_secs();
        let nanos = duration.subsec_nanos();

        for target in &handler.return_states {
            if seen_states
                .iter()
                .any(|state: &syn::Ident| state == &target.name)
            {
                continue;
            }
            seen_states.push(target.name.clone());

            let state_name = &target.name;
            timeout_arms.push(quote! {
                #state_enum_name::#state_name => {
                    sleep.reset(::tokio_fsm::tokio::time::Instant::now() + std::time::Duration::new(#secs, #nanos));
                }
            });
        }
    }

    quote! {
        fn apply_transition(
            &mut self,
            next: #state_enum_name,
            state_tx: &::tokio_fsm::tokio::sync::watch::Sender<#state_enum_name>,
            sleep: std::pin::Pin<&mut ::tokio_fsm::tokio::time::Sleep>,
            event_name: Option<&str>,
        ) {
            let old_state = self.state;
            self.state = next;
            let _ = state_tx.send(self.state);
            #tracing_log
            self.reset_timeout_for_current_state(sleep);
        }

        fn reset_timeout_for_current_state(
            &self,
            sleep: std::pin::Pin<&mut ::tokio_fsm::tokio::time::Sleep>,
        ) {
            const NO_TIMEOUT_SECS: u64 = 3_153_600_000;

            match self.state {
                #(#timeout_arms)*
                _ => {
                    sleep.reset(::tokio_fsm::tokio::time::Instant::now() + ::tokio_fsm::tokio::time::Duration::from_secs(NO_TIMEOUT_SECS));
                }
            }
        }
    }
}

pub fn render_cancellable_call(call: TokenStream) -> TokenStream {
    quote! {
        ::tokio_fsm::tokio::select! {
            _ = token.cancelled() => return Ok(self.context),
            result = #call => result,
        }
    }
}

pub fn render_transition_dispatch(
    return_kind: HandlerReturnKind,
    call: TokenStream,
    event_name: TokenStream,
) -> TokenStream {
    let apply_transition = |transition: TokenStream| {
        quote! {
            self.apply_transition(#transition.into_state().into(), &state_tx, sleep.as_mut(), #event_name);
        }
    };

    match return_kind {
        HandlerReturnKind::Transition => {
            let apply = apply_transition(quote! { transition });
            quote! {
                let transition = #call;
                #apply
            }
        }
        HandlerReturnKind::ResultTransition => {
            let apply_ok = apply_transition(quote! { transition });
            let apply_err = apply_transition(quote! { transition });
            quote! {
                match #call {
                    Ok(transition) => {
                        #apply_ok
                    }
                    Err(transition) => {
                        #apply_err
                    }
                }
            }
        }
        HandlerReturnKind::ResultError => {
            let apply = apply_transition(quote! { transition });
            quote! {
                let transition = match #call {
                    Ok(transition) => transition,
                    Err(error) => return Err(error),
                };
                #apply
            }
        }
    }
}
