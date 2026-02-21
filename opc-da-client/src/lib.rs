#![allow(unsafe_code, unreachable_pub)]
//! # opc-da-client
//!
//! Backend-agnostic OPC DA client library.
//!
//! ## Features
//! - `opc-da-backend` (default): Uses the `opc_da` crate for COM interaction
//! - `test-support`: Enables `MockOpcProvider` via `mockall`

mod com_guard;
mod helpers;
mod provider;

#[cfg(feature = "opc-da-backend")]
#[allow(warnings)]
mod bindings;

#[cfg(feature = "opc-da-backend")]
#[allow(warnings)]
mod opc_da;

#[cfg(feature = "opc-da-backend")]
mod backend;

// Stable public API
pub use com_guard::ComGuard;
pub use helpers::friendly_com_hint;
pub use provider::{OpcProvider, OpcValue, TagValue, WriteResult};

// Backend re-exports (conditional)
#[cfg(feature = "opc-da-backend")]
pub use backend::{connector::ComConnector, opc_da::OpcDaWrapper};

// Test support re-export
#[cfg(feature = "test-support")]
pub use provider::MockOpcProvider;
