//! Windows COM OPC DA server and group connector facade.
//!
//! Submodules:
//! - [`server`]: Concrete COM server connector and server facade.
//! - [`group`]: Concrete COM group and synchronous I/O operations.

pub mod group;
pub mod server;

const _: () = assert!(
    std::mem::size_of::<windows::core::GUID>() == 16,
    "windows::core::GUID must be 16 bytes for COM compatibility"
);
const _: () = assert!(
    std::mem::align_of::<windows::core::GUID>() >= 4,
    "windows::core::GUID must be at least 4-byte aligned"
);

pub use server::ComConnector;
