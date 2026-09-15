//! Windows COM subsystem for OPC DA communication.
//!
//! Consolidates all COM-related functionality:
//! lifecycle management, thread affinity, memory wrappers,
//! connection traits, and the concrete OPC DA client.

pub mod connector;
pub mod discovery;
pub(crate) mod guard;
pub(crate) mod iterator;
pub(crate) mod security;
pub(crate) mod variant;
pub(crate) mod worker;
