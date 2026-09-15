//! Windows COM subsystem for OPC DA communication.
//!
//! Consolidates all COM-related functionality:
//! lifecycle management, thread affinity, memory wrappers,
//! connection traits, and the concrete OPC DA client.

// Always compiled — pure-Rust initializer traits + worker subsystem
pub(crate) mod guard;
pub(crate) mod worker;

// Windows COM backend — only compiled with opc-da-backend feature
#[cfg(feature = "opc-da-backend")]
pub mod connector;
#[cfg(feature = "opc-da-backend")]
pub mod discovery;
#[cfg(feature = "opc-da-backend")]
pub(crate) mod iterator;
#[cfg(feature = "opc-da-backend")]
pub(crate) mod security;
#[cfg(feature = "opc-da-backend")]
pub(crate) mod variant;
