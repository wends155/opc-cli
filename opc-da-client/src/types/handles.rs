//! Type-safe handle representations for OPC groups and items.

use std::fmt;

/// Type-safe client-assigned group handle.
///
/// In OPC DA COM interfaces (`IOPCServer::AddGroup`), `hClientGroup` is provided by the client application.
/// It cannot be used directly where a server-assigned group handle is required.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClientGroupHandle(u32);

impl ClientGroupHandle {
    /// Creates a new `ClientGroupHandle` from a raw 32-bit unsigned integer.
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

impl From<u32> for ClientGroupHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<ClientGroupHandle> for u32 {
    #[inline]
    fn from(handle: ClientGroupHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for ClientGroupHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type-safe server-assigned group handle.
///
/// In OPC DA COM interfaces (`IOPCServer::RemoveGroup`, `IOPCGroupStateMgt`), operations targeting groups
/// require the server-assigned handle (`phServerGroup`). Passing a [`ClientGroupHandle`] instead is
/// prevented at compile-time by the type system.
///
/// # Examples
///
/// ```
/// use opc_da_client::ServerGroupHandle;
/// let handle = ServerGroupHandle::new(123u32);
/// assert_eq!(handle.as_raw(), 123u32);
/// ```
///
/// Compile-fail test ensuring [`ClientGroupHandle`] cannot be passed to a function expecting [`ServerGroupHandle`]:
/// ```compile_fail
/// use opc_da_client::{ClientGroupHandle, ServerGroupHandle};
/// fn remove_group(_handle: ServerGroupHandle) {}
/// let client = ClientGroupHandle::new(1);
/// remove_group(client);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ServerGroupHandle(u32);

impl ServerGroupHandle {
    /// Creates a new `ServerGroupHandle` from a raw 32-bit unsigned integer.
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

impl From<u32> for ServerGroupHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<ServerGroupHandle> for u32 {
    #[inline]
    fn from(handle: ServerGroupHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for ServerGroupHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Legacy type alias for [`ServerGroupHandle`].
///
/// Deprecated in favor of explicit [`ServerGroupHandle`] or [`ClientGroupHandle`] to ensure type safety.
#[deprecated(
    since = "0.2.1",
    note = "Use ServerGroupHandle or ClientGroupHandle for type-safe handle domain separation"
)]
pub type GroupHandle = ServerGroupHandle;

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
