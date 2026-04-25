use proc_macro2::TokenStream;
use quote::quote;

use crate::validation::FsmStructure;

pub fn render_handle_impl(fsm: &FsmStructure) -> TokenStream {
    let handle_name = fsm.handle_ident();
    let event_enum_name = fsm.event_enum_ident();
    let state_enum_name = fsm.state_enum_ident();

    quote! {
        impl #handle_name {
            /// Sends an event to the FSM.
            pub async fn send(&self, event: #event_enum_name) -> Result<(), ::tokio_fsm::tokio::sync::mpsc::error::SendError<#event_enum_name>> {
                self.event_tx.send(event).await
            }

            /// Attempts to send an event without awaiting capacity.
            pub fn try_send(&self, event: #event_enum_name) -> Result<(), ::tokio_fsm::tokio::sync::mpsc::error::TrySendError<#event_enum_name>> {
                self.event_tx.try_send(event)
            }

            /// Returns the current state of the FSM.
            pub fn current_state(&self) -> #state_enum_name {
                *self.state_rx.borrow()
            }

            /// Waits for the FSM to reach the specified state.
            pub async fn wait_for_state(&self, target: #state_enum_name) -> Result<(), ::tokio_fsm::tokio::sync::watch::error::RecvError> {
                let mut rx = self.state_rx.clone();
                while *rx.borrow_and_update() != target {
                    rx.changed().await?;
                }
                Ok(())
            }

            /// Shuts down the FSM immediately.
            pub fn shutdown(&self) {
                self.token.cancel();
            }

            /// Returns the cancellation token for this FSM.
            pub fn token(&self) -> &::tokio_fsm::tokio_util::sync::CancellationToken {
                &self.token
            }

            /// Returns the name of the FSM instance, if provided.
            pub fn name(&self) -> Option<&str> {
                self.name.as_deref()
            }
        }
    }
}
