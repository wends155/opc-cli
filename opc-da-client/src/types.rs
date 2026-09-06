//! Canonical domain types for OPC DA client operations.
//!
//! Provides type-safe representations of handles, group states, server statuses,
//! namespace browsing types, and OPC DA quality flags.

pub mod batch;
pub mod browse;
pub mod collection;
pub mod collector;
pub mod handles;
pub mod quality;
pub mod server;
pub mod value;

#[cfg(test)]
mod tests;

pub use batch::*;
pub use browse::*;
pub use collection::*;
pub use collector::*;
pub use handles::*;
pub use quality::*;
pub use server::*;
pub use value::*;
