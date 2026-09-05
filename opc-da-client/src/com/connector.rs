//! Windows COM OPC DA server and group connector facade.
//!
//! Submodules:
//! - [`traits`]: Pure-Rust DTOs and abstract connector traits.
//! - [`server`]: Concrete COM server connector and server facade.
//! - [`group`]: Concrete COM group and synchronous I/O operations.
//! - [`mock`]: Pure-Rust mock infrastructure for testing.

pub mod group;
#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod server;
pub mod traits;

const _: () = assert!(
    std::mem::size_of::<windows::core::GUID>() == 16,
    "windows::core::GUID must be 16 bytes for COM compatibility"
);
const _: () = assert!(
    std::mem::align_of::<windows::core::GUID>() >= 4,
    "windows::core::GUID must be at least 4-byte aligned"
);

pub use crate::com::iterator::{GuidIterator, StringIterator};
pub use crate::errors::{OpcError, OpcResult};
pub use crate::provider::{OpcQuality, OpcValue};
pub use crate::raw::memory::{LocalPointer, RemoteArray, RemotePointer};
pub use crate::types::{
    BrowseDirection, BrowseType, GroupHandle, ItemHandle, OpcServerEndpoint, OpcServerInfo,
    ServerIdentifier,
};

pub use group::ComGroup;
#[cfg(any(test, feature = "test-support"))]
pub use mock::{
    MockAddItemsFn, MockConnectedGroup, MockConnectedServer, MockReadFn, MockServerConnector,
    MockState, MockWriteFn,
};
pub use server::{ComConnector, ComServer};
pub use traits::{
    ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
    GroupItemResult, GroupItemState, ServerConnector,
};
