//! # opc-da-client
//!
//! Backend-agnostic OPC DA client library.
//!
//! ## Features
//! - `opc-da-backend` (default): Uses the `opc_da` crate for COM interaction
//! - `test-support`: Enables `MockOpcProvider` via `mockall`

mod helpers;
mod provider;

#[cfg(feature = "opc-da-backend")]
mod backend;

// Stable public API
pub use helpers::friendly_com_hint;
pub use provider::{OpcProvider, TagValue};

// Backend re-exports (conditional)
#[cfg(feature = "opc-da-backend")]
pub use backend::opc_da::OpcDaWrapper;

// Test support re-export
#[cfg(feature = "test-support")]
pub use provider::MockOpcProvider;
