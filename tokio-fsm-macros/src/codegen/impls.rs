use proc_macro2::TokenStream;
use quote::quote;

use crate::validation::FsmStructure;
use crate::validation::HandlerReturnKind;

pub fn render_spawn(fsm: &FsmStructure) -> TokenStream {
    let fsm_name = &fsm.fsm_name;
    let handle_name = fsm.handle_ident();
    let task_name = fsm.task_ident();
    let state_enum_name = fsm.state_enum_ident();
    let initial_state = &fsm.initial_state;
    let channel_size = fsm.channel_size;
    let context_type = &fsm.context_type;

    quote! {
        #[must_use = "FSM task must be retained or it will be aborted immediately"]
        pub fn spawn(context: #context_type) -> (#handle_name, #task_name) {
            Self::spawn_named_with_token(None, context, ::tokio_fsm::tokio_util::sync::CancellationToken::new())
        }

        #[must_use = "FSM task must be retained or it will be aborted immediately"]
        pub fn spawn_named(name: &str, context: #context_type) -> (#handle_name, #task_name) {
            Self::spawn_named_with_token(Some(name.to_string()), context, ::tokio_fsm::tokio_util::sync::CancellationToken::new())
        }

        #[must_use = "FSM task must be retained or it will be aborted immediately"]
        pub fn spawn_with_token(context: #context_type, token: ::tokio_fsm::tokio_util::sync::CancellationToken) -> (#handle_name, #task_name) {
            Self::spawn_named_with_token(None, context, token)
        }

        #[must_use = "FSM task must be retained or it will be aborted immediately"]
        pub fn spawn_named_with_token(name: Option<String>, context: #context_type, token: ::tokio_fsm::tokio_util::sync::CancellationToken) -> (#handle_name, #task_name) {
            let (event_tx, event_rx) = tokio::sync::mpsc::channel(#channel_size);
            let (state_tx, state_rx) = tokio::sync::watch::channel(#state_enum_name::#initial_state);

            let fsm = #fsm_name {
                state: #state_enum_name::#initial_state,
                context,
                name: name.clone(), // Clone for the FSM instance
            };

            // CancellationToken is a cheap Arc-clone
            let handle_token = token.clone();

            let handle = tokio::spawn(fsm.run(event_rx, token, state_tx));

            (
                #handle_name {
                    event_tx,
                    state_rx,
                    token: handle_token,
                    name: name, // Move the original name into the handle
                },
                #task_name { handle },
            )
        }
    }
}

pub fn render_run(fsm: &FsmStructure) -> TokenStream {
    let event_enum_name = fsm.event_enum_ident();
    let state_enum_name = fsm.state_enum_ident();
    let context_type = &fsm.context_type;
    let error_type = &fsm.error_type;
    let fsm_name_str = fsm.fsm_name.to_string();

    let event_arms = build_event_arms(fsm);
    let timeout_logic = build_timeout_handler(fsm);

    let tracing_span = if fsm.tracing {
        quote! {
            let span = ::tokio_fsm::tracing::info_span!(
                "fsm",
                name = #fsm_name_str,
                fsm_id = self.name.as_deref().unwrap_or("unnamed")
            );
            let _guard = span.enter();
        }
    } else {
        quote! {}
    };

    let tracing_cancellation = if fsm.tracing {
        quote! {
            ::tokio_fsm::tracing::info!("FSM received external cancellation");
        }
    } else {
        quote! {}
    };

    let unhandled_event_log = if fsm.tracing {
        quote! {
            ::tokio_fsm::tracing::warn!(state = ?self.state, event = ?event, "Event dropped: No handler for this state");
        }
    } else {
        quote! {}
    };

    let unmatched_arm = if fsm.tracing {
        quote! {
            (state, event) => {
                #unhandled_event_log
            }
        }
    } else {
        quote! {
            (_, _) => {}
        }
    };

    quote! {
        async fn run(
            mut self,
            mut events: tokio::sync::mpsc::Receiver<#event_enum_name>,
            token: ::tokio_fsm::tokio_util::sync::CancellationToken,
            state_tx: tokio::sync::watch::Sender<#state_enum_name>,
        ) -> Result<#context_type, #error_type> {
            #tracing_span

            let sleep = tokio::time::sleep(tokio::time::Duration::from_secs(0));
            tokio::pin!(sleep);
            self.reset_timeout_for_current_state(sleep.as_mut());

            loop {
                tokio::select! {
                    _ = &mut sleep => {
                        #timeout_logic
                    }
                    _ = token.cancelled() => {
                        #tracing_cancellation
                        return Ok(self.context);
                    }
                    event = events.recv() => {
                        let Some(event) = event else { break };
                        match (self.state, event) {
                            #(#event_arms)*
                            #unmatched_arm
                        }
                    }
                }
            }

            Ok(self.context)
        }
    }
}

pub fn render_handle_impl(fsm: &FsmStructure) -> TokenStream {
    let handle_name = fsm.handle_ident();
    let event_enum_name = fsm.event_enum_ident();
    let state_enum_name = fsm.state_enum_ident();
    let timeout_reset_impl = render_timeout_reset_impl(fsm);

    quote! {
        impl #handle_name {
            /// Sends an event to the FSM.
            pub async fn send(&self, event: #event_enum_name) -> Result<(), tokio::sync::mpsc::error::SendError<#event_enum_name>> {
                self.event_tx.send(event).await
            }

            /// Attempts to send an event without awaiting capacity.
            pub fn try_send(&self, event: #event_enum_name) -> Result<(), tokio::sync::mpsc::error::TrySendError<#event_enum_name>> {
                self.event_tx.try_send(event)
            }

            /// Returns the current state of the FSM.
            pub fn current_state(&self) -> #state_enum_name {
                *self.state_rx.borrow()
            }

            /// Waits for the FSM to reach the specified state.
            pub async fn wait_for_state(&self, target: #state_enum_name) -> Result<(), tokio::sync::watch::error::RecvError> {
                let mut rx = self.state_rx.clone(); // Cheap watch::Receiver clone
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

        #timeout_reset_impl
    }
}

pub fn render_task_impl(fsm: &FsmStructure) -> TokenStream {
    let task_name = fsm.task_ident();
    let context_type = &fsm.context_type;
    let error_type = &fsm.error_type;

    quote! {
        impl std::future::Future for #task_name {
            type Output = Result<#context_type, tokio_fsm::TaskError<#error_type>>;

            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
                match std::pin::Pin::new(&mut self.handle).poll(cx) {
                    std::task::Poll::Ready(Ok(Ok(res))) => std::task::Poll::Ready(Ok(res)),
                    std::task::Poll::Ready(Ok(Err(e))) => std::task::Poll::Ready(Err(tokio_fsm::TaskError::Fsm(e))),
                    std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(tokio_fsm::TaskError::Join(e))),
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }
        }
    }
}

pub fn render_task_drop(fsm: &FsmStructure) -> TokenStream {
    let task_name = fsm.task_ident();

    let abort_log = if fsm.tracing {
        let fsm_name = fsm.fsm_name.to_string();
        quote! {
            ::tokio_fsm::tracing::warn!(fsm = #fsm_name, "FSM task dropped before completion; aborting execution. Did you forget to retain the task handle?");
        }
    } else {
        quote! {}
    };

    quote! {
        impl Drop for #task_name {
            fn drop(&mut self) {
                #abort_log
                self.handle.abort();
            }
        }
    }
}

// --- Event loop logic (previously in logic.rs) ---

/// Builds state-gated match arms for the event loop.
fn build_event_arms(fsm: &FsmStructure) -> Vec<TokenStream> {
    let mut arms = Vec::new();
    let event_enum = fsm.event_enum_ident();
    let state_enum = fsm.state_enum_ident();

    for handler in &fsm.handlers {
        if let Some(ref event) = handler.event {
            let event_name = &event.name;
            let event_name_str = event_name.to_string();
            let method_name = &handler.method.sig.ident;

            let timeout_reset = quote! {
                self.reset_timeout_for_current_state(sleep.as_mut());
            };

            // Payload handling
            let (payload_pattern, payload_call) = if handler.has_payload {
                (quote! { (payload) }, quote! { (payload) })
            } else {
                (quote! {}, quote! { () })
            };

            let tracing_log = if fsm.tracing {
                quote! {
                    ::tokio_fsm::tracing::info!(from = ?old_state, to = ?self.state, event = #event_name_str, "Transition successful");
                }
            } else {
                quote! {}
            };

            let arm_inner = match handler.return_kind {
                Some(HandlerReturnKind::Transition) => {
                    quote! {
                        let old_state = self.state;
                        let transition = self.#method_name #payload_call .await;
                        self.state = transition.into_state().into();
                        let _ = state_tx.send(self.state);
                        #tracing_log
                        #timeout_reset
                    }
                }
                Some(HandlerReturnKind::ResultTransition) => {
                    quote! {
                        let old_state = self.state;
                        match self.#method_name #payload_call .await {
                            Ok(transition) => {
                                self.state = transition.into_state().into();
                                let _ = state_tx.send(self.state);
                                #tracing_log
                                #timeout_reset
                            }
                            Err(transition) => {
                                self.state = transition.into_state().into();
                                let _ = state_tx.send(self.state);
                                #tracing_log
                                #timeout_reset
                            }
                        }
                    }
                }
                Some(HandlerReturnKind::ResultError) => {
                    quote! {
                        let old_state = self.state;
                        let transition = match self.#method_name #payload_call .await {
                            Ok(transition) => transition,
                            Err(error) => return Err(error),
                        };
                        self.state = transition.into_state().into();
                        let _ = state_tx.send(self.state);
                        #tracing_log
                        #timeout_reset
                    }
                }
                None => quote! {},
            };

            // Generate one match arm per source state (state-gated)
            for source_state in &handler.source_states {
                arms.push(quote! {
                    (#state_enum::#source_state, #event_enum::#event_name #payload_pattern) => {
                        #arm_inner
                    }
                });
            }
        }
    }

    arms
}

/// Builds the timeout handler block for the run loop.
fn build_timeout_handler(fsm: &FsmStructure) -> TokenStream {
    if let Some(handler) = fsm.handlers.iter().find(|h| h.is_timeout_handler) {
        let name = &handler.method.sig.ident;
        match handler.return_kind {
            Some(HandlerReturnKind::Transition) => quote! {
                let transition = self.#name().await;
                self.state = transition.into_state().into();
                let _ = state_tx.send(self.state);
                self.reset_timeout_for_current_state(sleep.as_mut());
            },
            Some(HandlerReturnKind::ResultTransition) => quote! {
                match self.#name().await {
                    Ok(transition) => {
                        self.state = transition.into_state().into();
                        let _ = state_tx.send(self.state);
                        self.reset_timeout_for_current_state(sleep.as_mut());
                    }
                    Err(transition) => {
                        self.state = transition.into_state().into();
                        let _ = state_tx.send(self.state);
                        self.reset_timeout_for_current_state(sleep.as_mut());
                    }
                }
            },
            Some(HandlerReturnKind::ResultError) => quote! {
                let transition = match self.#name().await {
                    Ok(transition) => transition,
                    Err(error) => return Err(error),
                };
                self.state = transition.into_state().into();
                let _ = state_tx.send(self.state);
                self.reset_timeout_for_current_state(sleep.as_mut());
            },
            None => quote! {},
        }
    } else {
        quote! {}
    }
}

fn render_timeout_reset_impl(fsm: &FsmStructure) -> TokenStream {
    let fsm_name = &fsm.fsm_name;
    let state_enum_name = fsm.state_enum_ident();

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
                    sleep.reset(tokio::time::Instant::now() + std::time::Duration::new(#secs, #nanos));
                }
            });
        }
    }

    quote! {
        impl #fsm_name {
            fn reset_timeout_for_current_state(
                &self,
                sleep: std::pin::Pin<&mut tokio::time::Sleep>,
            ) {
                const NO_TIMEOUT_SECS: u64 = 3_153_600_000;

                match self.state {
                    #(#timeout_arms)*
                    _ => {
                        sleep.reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(NO_TIMEOUT_SECS));
                    }
                }
            }
        }
    }
}
