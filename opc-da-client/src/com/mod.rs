//! Windows COM subsystem for OPC DA communication.
//!
//! Consolidates all COM-related functionality:
//! lifecycle management, thread affinity, memory wrappers,
//! connection traits, and the concrete OPC DA client.

#![allow(warnings)]

pub mod client;
pub mod connector;
pub mod guard;
pub mod iterator;
pub mod memory;
pub mod worker;

pub use client::OpcDaClient;
pub use connector::{
    ComConnector, ComGroup, ComServer, ConnectedGroup, ConnectedServer, ServerConnector,
};
pub use guard::ComGuard;
pub use iterator::{GuidIterator, StringIterator};
pub use memory::{LocalPointer, RemoteArray, RemotePointer};
pub use worker::{ComRequest, ComWorker};
