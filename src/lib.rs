#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod core;

#[cfg(feature = "serde")]
#[doc(hidden)]
pub use serde;
#[doc(hidden)]
pub use tokio;
#[doc(inline)]
pub use tokio_fsm_macros::*;
#[doc(hidden)]
pub use tokio_util;
#[cfg(feature = "tracing")]
#[doc(hidden)]
pub use tracing;

#[cfg(feature = "serde")]
#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_fsm_serde_derive {
    () => {
        #[derive(::tokio_fsm::serde::Serialize, ::tokio_fsm::serde::Deserialize)]
    };
}

#[cfg(not(feature = "serde"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __tokio_fsm_serde_derive {
    () => {
        compile_error!("`#[fsm(serde = true)]` requires enabling the `serde` feature on the `tokio-fsm` dependency");
    };
}

#[doc(inline)]
pub use crate::core::*;
