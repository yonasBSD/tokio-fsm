//! Attribute parsing for FSM macro.

use darling::FromMeta;
use syn::{Ident, LitStr};

/// Arguments for the `#[fsm]` attribute.
#[derive(Debug, FromMeta)]
pub struct FsmArgs {
    /// Initial state (required).
    pub initial: Ident,

    /// Channel size for event queue (default: 100). Must be greater than 0.
    #[darling(default = "default_channel_size", and_then = validate_channel_size)]
    pub channel_size: usize,
}

fn default_channel_size() -> usize {
    100
}

fn validate_channel_size(size: usize) -> darling::Result<usize> {
    if size == 0 {
        return Err(darling::Error::custom(
            "channel_size must be greater than 0",
        ));
    }
    Ok(size)
}

/// Arguments for the `#[on(state = Idle, event = Start)]` attribute.
#[derive(Debug, FromMeta)]
pub struct OnAttr {
    /// Source state this handler is valid in.
    pub state: Ident,
    /// Event that triggers this handler.
    pub event: Ident,
}

/// Arguments for the `#[state_timeout]` attribute.
#[derive(Debug, Clone, FromMeta)]
pub struct StateTimeoutAttr {
    /// Duration string (e.g., "30s", "5m").
    pub duration: LitStr,
}
