//! Canonical domain types for OPC DA client operations.
//!
//! Provides type-safe representations of handles, group states, server statuses,
//! namespace browsing types, and OPC DA quality flags.

pub(crate) mod batch;
pub(crate) mod browse;
pub(crate) mod clsid;
pub(crate) mod collection;
pub(crate) mod collector;
pub(crate) mod handles;
pub(crate) mod quality;
pub(crate) mod server;
pub(crate) mod value;
pub(crate) mod vartype;
pub(crate) mod write_batch;

pub use batch::*;
pub use browse::*;
pub use clsid::*;
pub use collection::*;
pub use collector::*;
pub use handles::*;
pub use quality::*;
pub use server::*;
pub use value::*;
pub use vartype::*;
pub use write_batch::*;
