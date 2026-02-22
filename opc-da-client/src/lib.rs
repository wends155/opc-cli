#![allow(unsafe_code, unreachable_pub)]
//! # opc-da-client
//!
//! Backend-agnostic OPC DA client library for Rust — async, trait-based,
//! with RAII COM guard.
//!
//! ## Quick Start
//!
//! ```no_run
//! # use anyhow::Result;
//! use opc_da_client::{ComGuard, OpcDaWrapper, OpcProvider};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let _guard = ComGuard::new()?;
//! let client = OpcDaWrapper::default();
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
//! | `test-support` | ❌ | Enables `MockOpcProvider` via `mockall` |
//!
//! ## Platform
//!
//! **Windows only** — OPC DA is built on COM/DCOM.

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
pub use helpers::{format_hresult, friendly_com_hint};
pub use provider::{OpcProvider, OpcValue, TagValue, WriteResult};

// Backend re-exports (conditional)
#[cfg(feature = "opc-da-backend")]
pub use backend::{connector::ComConnector, opc_da::OpcDaWrapper};

// Test support re-export
#[cfg(feature = "test-support")]
pub use provider::MockOpcProvider;
