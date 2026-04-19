//! Validation and semantic analysis for FSM structure.
//!
//! This module is the first layer of the macro pipeline. It:
//! 1. Parses the `impl` block to extract states, events, and handlers
//! 2. Derives semantic fields (timeout durations, payload presence, result
//!    types)
//! 3. Validates the FSM graph (reachability from initial state)

use std::{collections::HashMap, time::Duration};

use darling::FromMeta;
use petgraph::{algo::has_path_connecting, graph::DiGraph};
use quote::format_ident;
use syn::{Error, FnArg, GenericArgument, Ident, ImplItem, PathArguments, ReturnType, Type};

use crate::attrs;

/// Represents a discovered state in the FSM.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    pub name: Ident,
}

/// Represents a discovered event in the FSM.
#[derive(Debug, Clone)]
pub struct Event {
    pub name: Ident,
    pub payload_type: Option<Type>,
}

/// Represents a handler method in the FSM, including all derived semantic
/// fields.
#[derive(Debug, Clone)]
pub struct Handler {
    pub method: syn::ImplItemFn,
    pub event: Option<Event>,
    pub is_timeout_handler: bool,
    pub return_states: Vec<State>,

    // Derived semantic fields (previously in IR)
    /// Source states this handler is valid in.
    pub source_states: Vec<Ident>,
    /// Whether the event carries a payload argument.
    pub has_payload: bool,
    /// Whether the return type is `Result<Transition<A>, Transition<B>>`.
    pub is_result: bool,
    /// Parsed timeout duration for the target state, if any.
    pub timeout: Option<Duration>,
}

/// The complete FSM structure after parsing and validation.
#[derive(Debug)]
pub struct FsmStructure {
    pub fsm_name: Ident,
    pub initial_state: Ident,
    pub channel_size: usize,
    pub context_type: Type,
    pub error_type: Type,
    pub states: Vec<State>,
    pub events: Vec<Event>,
    pub handlers: Vec<Handler>,
    pub tracing: bool,
    pub serde: bool,
}

impl FsmStructure {
    // --- Ident helpers (previously in helpers.rs) ---

    pub fn state_enum_ident(&self) -> Ident {
        format_ident!("{}State", self.fsm_name)
    }

    pub fn event_enum_ident(&self) -> Ident {
        format_ident!("{}Event", self.fsm_name)
    }

    pub fn handle_ident(&self) -> Ident {
        format_ident!("{}Handle", self.fsm_name)
    }

    pub fn task_ident(&self) -> Ident {
        format_ident!("{}Task", self.fsm_name)
    }

    // --- Parsing ---

    /// Parse the impl block and extract the complete FSM structure.
    pub fn parse(args: attrs::FsmArgs, impl_block: &syn::ItemImpl) -> syn::Result<Self> {
        let fsm_name = match &*impl_block.self_ty {
            syn::Type::Path(path) => path
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
        let mut event_names: Vec<Ident> = Vec::new();
        let mut events: Vec<Event> = Vec::new();
        let mut state_names: Vec<Ident> = Vec::new();

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

    /// Validate the FSM graph for reachability.
    ///
    /// Constructs a directed graph where:
    /// - **Nodes**: FSM states
    /// - **Edges**: Transitions from declared source states to return states
    ///
    /// Checks:
    /// 1. All declared states exist as nodes
    /// 2. All states are reachable from the initial state
    fn validate(&self) -> syn::Result<()> {
        let mut graph = DiGraph::<&Ident, ()>::new();
        let mut nodes = HashMap::new();

        for state in &self.states {
            let node = graph.add_node(&state.name);
            nodes.insert(&state.name, node);
        }

        let initial_node = nodes.get(&self.initial_state).ok_or_else(|| {
            syn::Error::new_spanned(
                &self.initial_state,
                "Initial state not found in discovered states",
            )
        })?;

        // 1. Validate timeout contract
        let timeout_handlers: Vec<_> = self
            .handlers
            .iter()
            .filter(|h| h.is_timeout_handler)
            .collect();
        if timeout_handlers.len() > 1 {
            return Err(syn::Error::new_spanned(
                &timeout_handlers[1].method.sig.ident,
                "Multiple #[on_timeout] handlers are not allowed",
            ));
        }
        let has_timeout_handler = !timeout_handlers.is_empty();

        // 2. Identify states that arm a timeout
        let mut states_with_timeout = std::collections::HashSet::new();
        for handler in &self.handlers {
            if handler.timeout.is_some() {
                if !has_timeout_handler {
                    return Err(syn::Error::new_spanned(
                        &handler.method.sig.ident,
                        "#[state_timeout] requires an #[on_timeout] handler",
                    ));
                }
                for target in &handler.return_states {
                    states_with_timeout.insert(&target.name);
                }
            }
        }

        // 3. Build reachability graph
        for handler in &self.handlers {
            for target in &handler.return_states {
                let target_node = nodes.get(&target.name).ok_or_else(|| {
                    syn::Error::new_spanned(
                        &target.name,
                        format!(
                            "Target state '{}' not found in discovered states",
                            target.name
                        ),
                    )
                })?;

                if handler.is_timeout_handler {
                    // Timeout handlers only fire from states that have a timeout armed.
                    for &state_name in &states_with_timeout {
                        let source_node = nodes.get(state_name).unwrap();
                        graph.add_edge(*source_node, *target_node, ());
                    }
                } else {
                    // State-gated: add edges only from declared source states
                    for source_ident in &handler.source_states {
                        let source_node = nodes.get(source_ident).ok_or_else(|| {
                            syn::Error::new_spanned(
                                source_ident,
                                format!(
                                    "Source state '{}' in #[state(...)] not found in FSM states",
                                    source_ident
                                ),
                            )
                        })?;
                        graph.add_edge(*source_node, *target_node, ());
                    }
                }
            }
        }

        // Check reachability from initial state to all other states
        for (&state_name, &node) in &nodes {
            if !has_path_connecting(&graph, *initial_node, node, None) {
                return Err(syn::Error::new_spanned(
                    state_name,
                    format!(
                        "State '{}' is unreachable from initial state '{}'",
                        state_name, self.initial_state
                    ),
                ));
            }
        }

        Ok(())
    }
}

impl Handler {
    /// Parse a method into a Handler with all semantic fields derived.
    fn parse(method: &syn::ImplItemFn) -> syn::Result<Self> {
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

/// Extract state names from a return type (Transition<State> or
/// Result<Transition<State>, Transition<State>>).
fn extract_return_states(output: &ReturnType) -> syn::Result<Vec<State>> {
    let return_type = match output {
        ReturnType::Type(_, ty) => ty.as_ref(),
        ReturnType::Default => return Ok(Vec::new()),
    };

    let mut states = Vec::new();
    extract_states_recursive(return_type, &mut states)?;
    Ok(states)
}

fn extract_states_recursive(ty: &Type, states: &mut Vec<State>) -> syn::Result<()> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
    {
        if segment.ident == "Transition" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
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
        } else if segment.ident == "Result"
            && let PathArguments::AngleBracketed(args) = &segment.arguments
        {
            for arg in &args.args {
                if let GenericArgument::Type(inner_ty) = arg {
                    extract_states_recursive(inner_ty, states)?;
                }
            }
        }
    }
    Ok(())
}
