//! Windows COM OPC DA server and group connector facade.
//!
//! Submodules:
//! - [`traits`]: Pure-Rust DTOs and abstract connector traits.
//! - [`server`]: Concrete COM server connector and server facade.
//! - [`group`]: Concrete COM group and synchronous I/O operations.
//! - `mock`: Pure-Rust mock infrastructure for testing (enabled via `test-support`).

pub mod group;
pub mod server;

#[allow(unused_imports)]
#[cfg(any(test, feature = "test-support"))]
pub use crate::connector::mock;
#[allow(unused_imports)]
pub use crate::connector::traits;

const _: () = assert!(
    std::mem::size_of::<windows::core::GUID>() == 16,
    "windows::core::GUID must be 16 bytes for COM compatibility"
);
const _: () = assert!(
    std::mem::align_of::<windows::core::GUID>() >= 4,
    "windows::core::GUID must be at least 4-byte aligned"
);

#[allow(unused_imports)]
#[cfg(any(test, feature = "test-support"))]
pub use crate::connector::mock::{
    MockConnectedGroup, MockConnectedServer, MockServerConnector, MockState,
};
#[allow(unused_imports)]
pub use crate::connector::traits::{
    ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
    GroupItemResult, GroupItemState, GroupRemovalMode, ItemWrite, ServerBackend,
    ServerCatalogDiscovery, ServerConnector,
};
pub use server::ComConnector;
