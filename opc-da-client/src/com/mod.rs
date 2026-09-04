//! Windows COM subsystem for OPC DA communication.
//!
//! Consolidates all COM-related functionality:
//! lifecycle management, thread affinity, memory wrappers,
//! connection traits, and the concrete OPC DA client.

pub mod client;
pub mod connector;
pub mod guard;
pub mod iterator;
pub(crate) use crate::raw::memory;
pub(crate) mod variant;
pub mod worker;

pub use client::OpcDaClient;
pub use connector::{
    ComConnector, ComGroup, ComServer, ConnectedGroup, ConnectedServer, ServerConnector,
};
