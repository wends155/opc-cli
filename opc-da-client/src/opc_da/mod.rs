//! OPC DA protocol implementation.
//!
//! This module provides the COM-based OPC DA client, including trait definitions,
//! version-specific implementations, error types, and type definitions.

#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::module_name_repetitions,
    clippy::needless_pass_by_value,
    clippy::unreadable_literal
)]
#[allow(clippy::missing_errors_doc)]
pub mod client;
pub mod com_utils;
pub mod errors;
pub mod typedefs;
