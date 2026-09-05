#![allow(unsafe_code, unreachable_pub)]
#![doc = include_str!("../README.md")]
//! # opc-da-client
//!
//! Backend-agnostic OPC DA client library for Rust — async, trait-based,
//! with transparent COM management.
//!
//! ## Quick Start
//!
//! ```no_run
//! use opc_da_client::{OpcDaClient, OpcProvider, OpcResult};
//!
//! # #[tokio::main]
//! # async fn main() -> OpcResult<()> {
//! let client = OpcDaClient::default();
//! let servers = client.list_servers("localhost").await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Feature Flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `opc-da-backend` | ✅ | Native OPC DA backend via `windows-rs` |
//! | `test-support` | ❌ | Enables `MockOpcProvider` and `MockServerConnector` mock suites |
//!
//! ## Platform
//!
//! **Windows only** — OPC DA is built on COM/DCOM.

pub mod errors;
mod provider;
pub mod types;

#[cfg(feature = "opc-da-backend")]
pub(crate) mod raw;

#[cfg(feature = "opc-da-backend")]
pub mod com;

// Stable public API
pub use errors::{OpcError, OpcResult};
pub use provider::{
    DisplayOptionOpcValue, DisplayOptionTimestamp, OpcProvider, OpcQuality, OpcValue,
    OpcValueOptionExt, QualityLimit, QualityMajor, QualitySubstatus, SystemTimeOptionExt,
    TagCollector, TagValue, WriteResult,
};
pub use types::{
    BrowseDirection, BrowseType, GroupHandle, ItemHandle, OpcServerEndpoint, OpcServerInfo,
    ServerIdentifier,
};

// Backend re-exports (conditional)
#[cfg(feature = "opc-da-backend")]
pub use com::{
    client::OpcDaClient,
    connector::ComConnector,
    discovery::{OpcServerRegistration, OpcServerType, inspect_local_registration},
};

// Test support re-export
#[cfg(feature = "test-support")]
pub use provider::MockOpcProvider;

#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
pub use com::connector::{MockConnectedGroup, MockConnectedServer, MockServerConnector};

/// Type alias for an [`OpcDaClient`] instantiated with [`MockServerConnector`].
#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
pub type MockOpcDaClient = com::client::OpcDaClient<com::connector::MockServerConnector>;
