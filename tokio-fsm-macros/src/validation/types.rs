use std::time::Duration;

use syn::{Ident, Type};

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
    pub return_kind: Option<HandlerReturnKind>,

    // Derived semantic fields (previously in IR)
    /// Source states this handler is valid in.
    pub source_states: Vec<Ident>,
    /// Whether the event carries a payload argument.
    pub has_payload: bool,
    /// Parsed timeout duration for the target state, if any.
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerReturnKind {
    Transition,
    ResultTransition,
    ResultError,
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
