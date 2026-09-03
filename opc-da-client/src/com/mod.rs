//! Windows COM subsystem for OPC DA communication.
//!
//! Consolidates all COM-related functionality:
//! lifecycle management, thread affinity, memory wrappers,
//! connection traits, and the concrete OPC DA client.

#![allow(warnings)]
#![allow(clippy::all, clippy::pedantic, clippy::restriction)]

pub mod client;
pub mod connector;
pub mod guard;
pub mod iterator;
pub use crate::raw::memory;
pub mod worker;

pub use client::OpcDaClient;
pub use connector::{
    ComConnector, ComGroup, ComServer, ConnectedGroup, ConnectedServer, ServerConnector,
};
pub use guard::ComGuard;
pub use iterator::{GuidIterator, StringIterator};
pub use memory::{LocalPointer, RemoteArray, RemotePointer};
pub use worker::{ComRequest, ComWorker};
