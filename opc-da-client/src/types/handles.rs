//! Type-safe handle representations for OPC groups and items.

use std::fmt;

/// Opaque handle for an OPC group.
///
/// This wrapper type enhances type safety when interacting with OPC COM interfaces,
/// preventing accidental mixing of group and item handles.
///
/// # Examples
///
/// ```
/// use opc_da_client::GroupHandle;
/// let handle = GroupHandle::new(123u32);
/// assert_eq!(handle.as_raw(), 123u32);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GroupHandle(u32);

impl GroupHandle {
    /// Creates a new `GroupHandle` from a raw 32-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw 32-bit handle value.
    #[inline]
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for GroupHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<GroupHandle> for u32 {
    #[inline]
    fn from(handle: GroupHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for GroupHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type-safe client-assigned item handle.
///
/// In OPC DA COM interfaces, `hClient` is provided by the client application to identify an item
/// across asynchronous or synchronous notifications. It cannot be used directly where a server handle
/// is required.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClientItemHandle(u32);

impl ClientItemHandle {
    /// Creates a new `ClientItemHandle` from a raw 32-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw 32-bit handle value.
    #[inline]
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for ClientItemHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<ClientItemHandle> for u32 {
    #[inline]
    fn from(handle: ClientItemHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for ClientItemHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type-safe server-assigned item handle.
///
/// In OPC DA COM interfaces (`IOPCSyncIO::Read`, `IOPCSyncIO::Write`, `IOPCItemMgt::RemoveItems`),
/// operations targeting items require the server-assigned handle (`phServer`). Passing a [`ClientItemHandle`]
/// instead is prevented at compile-time by the type system.
///
/// # Examples
///
/// ```
/// use opc_da_client::ServerItemHandle;
/// let handle = ServerItemHandle::new(456u32);
/// assert_eq!(handle.as_raw(), 456u32);
/// ```
///
/// Compile-fail test ensuring [`ClientItemHandle`] cannot be passed to a function expecting [`ServerItemHandle`]:
/// ```compile_fail
/// use opc_da_client::{ClientItemHandle, ServerItemHandle};
/// fn read_item(handle: ServerItemHandle) {}
/// let client = ClientItemHandle::new(1);
/// read_item(client);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ServerItemHandle(u32);

impl ServerItemHandle {
    /// Creates a new `ServerItemHandle` from a raw 32-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw 32-bit handle value.
    #[inline]
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for ServerItemHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<ServerItemHandle> for u32 {
    #[inline]
    fn from(handle: ServerItemHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for ServerItemHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Legacy type alias for [`ServerItemHandle`].
///
/// Deprecated in favor of explicit [`ServerItemHandle`] or [`ClientItemHandle`] to ensure type safety.
#[deprecated(
    since = "0.2.0",
    note = "Use ServerItemHandle or ClientItemHandle for type-safe handle domain separation"
)]
pub type ItemHandle = ServerItemHandle;
