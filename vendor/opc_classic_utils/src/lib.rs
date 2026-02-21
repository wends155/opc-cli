#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::cargo_common_metadata,
    clippy::needless_pass_by_value,
    clippy::unreadable_literal
)]
//! OPC Classic utilities and common functionality
//!
//! This crate provides shared utilities for OPC Classic implementations,
//! including automatic memory management and common traits.

pub mod memory;

pub use memory::*;
