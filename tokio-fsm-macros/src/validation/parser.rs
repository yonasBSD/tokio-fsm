use darling::FromMeta;
use syn::{Error, FnArg, GenericArgument, ImplItem, ItemImpl, PathArguments, ReturnType, Type};

use super::types::{Event, FsmStructure, Handler, State};
use crate::attrs;

impl Handler {
    /// Parse a method into a Handler with all semantic fields derived.
    pub fn parse(method: &syn::ImplItemFn) -> syn::Result<Self> {
        let mut event: Option<Event> = None;
        let mut is_timeout_handler = false;
        let mut state_timeout_attr = None;
        let mut source_states = Vec::new();

        // Parse attributes
        for attr in &method.attrs {
            if attr.path().is_ident("on") {
                let on_attr: attrs::OnAttr = attrs::OnAttr::from_meta(&attr.meta)?;
                let payload_type = if method.sig.inputs.len() > 1 {
                    if let FnArg::Typed(pat_type) = &method.sig.inputs[1] {
                        Some((*pat_type.ty).clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                // Multiple #[on(...)] attributes are allowed for multi-state handlers
                source_states.push(on_attr.state);
                if let Some(ref existing_event) = event {
                    if existing_event.name != on_attr.event {
                        return Err(Error::new_spanned(
                            &on_attr.event,
                            format!(
                                "Handler method '{}' handles multiple different events: '{}' and '{}'",
                                method.sig.ident, existing_event.name, on_attr.event
                            ),
                        ));
                    }
                } else {
                    event = Some(Event {
                        name: on_attr.event,
                        payload_type,
                    });
                }
            } else if attr.path().is_ident("on_timeout") {
                is_timeout_handler = true;
            } else if attr.path().is_ident("state_timeout") {
                state_timeout_attr = Some(attrs::StateTimeoutAttr::from_meta(&attr.meta)?);
            }
        }

        // Derive: has_payload
        let has_payload = event
            .as_ref()
            .map(|e| e.payload_type.is_some())
            .unwrap_or(false);

        // Derive: is_result
        let is_result = match &method.sig.output {
            syn::ReturnType::Type(_, ty) => {
                if let syn::Type::Path(path) = ty.as_ref() {
                    path.path
                        .segments
                        .last()
                        .map(|seg| seg.ident == "Result")
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            syn::ReturnType::Default => false,
        };

        // Derive: timeout (fail loudly on invalid duration)
        let timeout = if let Some(ref st) = state_timeout_attr {
            let duration_str = st.duration.value();
            let parsed = humantime::parse_duration(&duration_str).map_err(|e| {
                syn::Error::new_spanned(
                    &st.duration,
                    format!("Invalid duration '{}': {}", duration_str, e),
                )
            })?;
            Some(parsed)
        } else {
            None
        };

        // Extract return states from return type
        let return_states = extract_return_states(&method.sig.output)?;

        Ok(Self {
            method: method.clone(),
            event,
            is_timeout_handler,
            return_states,
            source_states,
            has_payload,
            is_result,
            timeout,
        })
    }
}

pub fn extract_return_states(output: &ReturnType) -> syn::Result<Vec<State>> {
    let return_type = match output {
        ReturnType::Type(_, ty) => ty.as_ref(),
        ReturnType::Default => return Ok(Vec::new()),
    };

    let mut states = Vec::new();
    extract_states_recursive(return_type, &mut states)?;
    Ok(states)
}

fn extract_states_recursive(ty: &Type, states: &mut Vec<State>) -> syn::Result<()> {
    let Type::Path(path) = ty else { return Ok(()) };
    let Some(segment) = path.path.segments.last() else {
        return Ok(());
    };

    match segment.ident.to_string().as_str() {
        "Transition" => {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return Ok(());
            };
            for arg in &args.args {
                if let GenericArgument::Type(Type::Path(inner_path)) = arg
                    && let Some(state_seg) = inner_path.path.segments.last()
                {
                    states.push(State {
                        name: state_seg.ident.clone(),
                    });
                }
            }
        }
        "Result" => {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return Ok(());
            };
            for arg in &args.args {
                if let GenericArgument::Type(inner_ty) = arg {
                    extract_states_recursive(inner_ty, states)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

impl FsmStructure {
    /// Parse the impl block and extract the complete FSM structure.
    pub fn parse(args: attrs::FsmArgs, impl_block: &ItemImpl) -> syn::Result<Self> {
        let fsm_name = match &*impl_block.self_ty {
            Type::Path(path) => path
                .path
                .segments
                .last()
                .ok_or_else(|| Error::new_spanned(&impl_block.self_ty, "Expected FSM type name"))?
                .ident
                .clone(),
            _ => {
                return Err(Error::new_spanned(
                    &impl_block.self_ty,
                    "Expected type path for FSM",
                ));
            }
        };

        let initial_state = args.initial;

        // Extract associated types
        let mut context_type = None;
        let mut error_type = None;

        for item in &impl_block.items {
            if let ImplItem::Type(ty) = item {
                if ty.ident == "Context" {
                    context_type = Some(ty.ty.clone());
                } else if ty.ident == "Error" {
                    error_type = Some(ty.ty.clone());
                }
            }
        }

        let context_type = context_type.ok_or_else(|| {
            Error::new_spanned(impl_block, "Missing associated type: type Context = ...")
        })?;
        let error_type = error_type.ok_or_else(|| {
            Error::new_spanned(impl_block, "Missing associated type: type Error = ...")
        })?;

        // Parse methods
        let mut handlers = Vec::new();
        let mut event_names: Vec<syn::Ident> = Vec::new();
        let mut events: Vec<Event> = Vec::new();
        let mut state_names: Vec<syn::Ident> = Vec::new();

        state_names.push(initial_state.clone());

        for item in &impl_block.items {
            if let ImplItem::Fn(method) = item {
                let handler = Handler::parse(method)?;

                // Collect states from return types
                for state in &handler.return_states {
                    if !state_names.iter().any(|s| s == &state.name) {
                        state_names.push(state.name.clone());
                    }
                }

                // Collect source states
                for state in &handler.source_states {
                    if !state_names.iter().any(|s| s == state) {
                        state_names.push(state.clone());
                    }
                }

                // Collect events and validate payload consistency
                if let Some(ref event) = handler.event {
                    if let Some(existing_event) = events.iter().find(|e| e.name == event.name) {
                        if existing_event.payload_type != event.payload_type {
                            let expected = existing_event
                                .payload_type
                                .as_ref()
                                .map(|ty| quote::quote!(#ty).to_string())
                                .unwrap_or_else(|| "None".to_string());
                            let actual = event
                                .payload_type
                                .as_ref()
                                .map(|ty| quote::quote!(#ty).to_string())
                                .unwrap_or_else(|| "None".to_string());
                            return Err(Error::new_spanned(
                                &event.name,
                                format!(
                                    "Event '{}' has inconsistent payload types across handlers: expected '{}', found '{}'",
                                    event.name, expected, actual
                                ),
                            ));
                        }
                    } else {
                        event_names.push(event.name.clone());
                        events.push(event.clone());
                    }
                }

                handlers.push(handler);
            }
        }

        let states: Vec<State> = state_names.into_iter().map(|name| State { name }).collect();

        let fsm = Self {
            fsm_name,
            initial_state,
            channel_size: args.channel_size,
            context_type,
            error_type,
            states,
            events,
            handlers,
            tracing: args.tracing,
            serde: args.serde,
        };

        fsm.validate()?;

        Ok(fsm)
    }
}
