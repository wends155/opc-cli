//! Pure-Rust Data Transfer Objects (DTOs) and connector abstraction traits.
//!
//! Decouples domain logic and worker thread orchestration from low-level
//! Win32 COM interfaces and native FFI structs.

use crate::com::iterator::StringIterator;
use crate::errors::{OpcError, OpcResult};
use crate::types::{
    BrowseDirection, BrowseType, ClientGroupHandle, ClientItemHandle, NamespaceType, OpcQuality,
    OpcServerEndpoint, OpcServerInfo, OpcValue, ServerGroupHandle, ServerIdentifier,
    ServerItemHandle, normalize_host,
};

// ── Pure-Rust Data Transfer Objects ────────────────────────────────

/// Specifies group removal behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupRemovalMode {
    /// Normal removal; server may reject if outstanding references or asynchronous operations exist.
    #[default]
    Normal,
    /// Force group removal even if outstanding references exist (`bForce = TRUE`).
    Force,
}

impl GroupRemovalMode {
    /// Returns `true` if this mode specifies forceful removal.
    #[inline]
    #[must_use]
    pub const fn is_force(self) -> bool {
        matches!(self, Self::Force)
    }
}

impl From<bool> for GroupRemovalMode {
    #[inline]
    fn from(force: bool) -> Self {
        if force { Self::Force } else { Self::Normal }
    }
}

impl From<GroupRemovalMode> for bool {
    #[inline]
    fn from(mode: GroupRemovalMode) -> Self {
        mode.is_force()
    }
}

impl From<GroupRemovalMode> for windows::core::BOOL {
    #[inline]
    fn from(mode: GroupRemovalMode) -> Self {
        Self::from(mode.is_force())
    }
}

/// Pairing of a server item handle and its target value for writing.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemWrite {
    /// Server-assigned handle identifying the target item.
    pub handle: ServerItemHandle,
    /// Value to write.
    pub value: OpcValue,
}

impl ItemWrite {
    /// Creates a new `ItemWrite` pairing.
    #[inline]
    #[must_use]
    pub const fn new(handle: ServerItemHandle, value: OpcValue) -> Self {
        Self { handle, value }
    }
}

/// Definition of an item to be added to an OPC group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupItemDef {
    /// Fully qualified tag identifier.
    pub item_id: String,
    /// Handle assigned by the client for this item.
    pub client_handle: ClientItemHandle,
    /// Whether the item should be activated immediately.
    pub active: bool,
}

/// Result of adding an item to an OPC group.
#[derive(Debug)]
pub struct GroupItemResult {
    /// Server-assigned handle for this item.
    pub server_handle: ServerItemHandle,
    /// Canonical data type reported by the server.
    pub canonical_type: u16,
    /// Error if adding this specific item failed.
    pub error: Option<OpcError>,
}

/// Synchronous read result for an item in an OPC group using strong domain types.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupItemState {
    /// Client handle associated with this item.
    pub client_handle: ClientItemHandle,
    /// Decoded strongly-typed value.
    pub value: OpcValue,
    /// Decoded 16-bit OPC quality.
    pub quality: OpcQuality,
    /// Timestamp reported by the server or acquisition time.
    pub timestamp: std::time::SystemTime,
}

/// Data source target for synchronous reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataSource {
    /// Read from server cache.
    Cache,
    /// Read directly from physical device.
    #[default]
    Device,
}

/// Configuration parameters for adding an OPC group.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupConfig<'a> {
    /// Requested name of the group.
    pub name: &'a str,
    /// Whether the group is initially active.
    pub active: bool,
    /// Requested update rate in milliseconds.
    pub update_rate_ms: u32,
    /// Client-assigned group handle.
    pub client_handle: ClientGroupHandle,
    /// Time zone bias in minutes from UTC.
    pub time_bias: i32,
    /// Percent deadband for analog items.
    pub percent_deadband: f32,
    /// Locale identifier.
    pub locale_id: u32,
}

impl<'a> GroupConfig<'a> {
    /// Creates an ephemeral, active group configuration with standard defaults.
    #[must_use]
    pub const fn ephemeral(name: &'a str) -> Self {
        Self {
            name,
            active: true,
            update_rate_ms: 1000,
            client_handle: ClientGroupHandle::new(1),
            time_bias: 0,
            percent_deadband: 0.0,
            locale_id: 0,
        }
    }

    /// Sets the requested update rate in milliseconds.
    #[must_use]
    pub const fn with_update_rate(mut self, update_rate_ms: u32) -> Self {
        self.update_rate_ms = update_rate_ms;
        self
    }

    /// Sets the client-assigned group handle.
    #[must_use]
    pub const fn with_client_handle(mut self, client_handle: ClientGroupHandle) -> Self {
        self.client_handle = client_handle;
        self
    }

    /// Sets whether the group is active.
    #[must_use]
    pub const fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Sets the percent deadband for analog items.
    #[must_use]
    pub const fn with_percent_deadband(mut self, percent_deadband: f32) -> Self {
        self.percent_deadband = percent_deadband;
        self
    }
}

/// Output wrapper returned when an OPC group is added.
pub struct CreatedGroup<G> {
    /// Connected group instance.
    pub group: G,
    /// Server-assigned handle for the group.
    pub server_handle: ServerGroupHandle,
    /// Revised update rate in milliseconds provided by the server.
    pub revised_update_rate_ms: u32,
}

// ── Connector & Facade Traits ───────────────────────────────────────

/// Factory for connecting to OPC DA servers.
pub trait ServerConnector: Send + Sync {
    /// The server facade type returned by [`Self::connect`].
    type Server: ConnectedServer;

    /// Enumerate all OPC DA server ProgIDs on the specified host.
    ///
    /// Pass `"localhost"` or `""` for local server discovery.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if server enumeration fails.
    fn enumerate_servers(&self, host: &str) -> OpcResult<Vec<String>>;

    /// Enumerate all OPC DA servers on the target host with rich catalog details.
    ///
    /// The default implementation falls back to [`Self::enumerate_servers`] and synthesizes
    /// [`OpcServerInfo`] records with zeroed CLSIDs and `user_type: None`.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if server enumeration fails.
    fn enumerate_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        let servers = self.enumerate_servers(host)?;
        let host_opt = normalize_host(Some(host));
        Ok(servers
            .into_iter()
            .map(|prog_id| {
                OpcServerInfo::new(
                    prog_id,
                    windows::core::GUID::zeroed(),
                    None,
                    host_opt.clone(),
                )
            })
            .collect())
    }

    /// Connect to an OPC DA server specified by an [`OpcServerEndpoint`].
    ///
    /// The default implementation delegates to [`Self::connect_identifier`] with `&endpoint.identifier`.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if connection fails.
    fn connect_endpoint(&self, endpoint: &OpcServerEndpoint) -> OpcResult<Self::Server> {
        self.connect_identifier(&endpoint.identifier)
    }

    /// Connect to an OPC DA server specified by a [`ServerIdentifier`].
    ///
    /// # Errors
    /// Returns an [`OpcError`] if connection fails.
    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server>;

    /// Connect to the named OPC DA server and return a server facade.
    ///
    /// The default implementation delegates to [`Self::connect_identifier`] with a [`ServerIdentifier::ProgId`].
    ///
    /// # Errors
    /// Returns an [`OpcError`] if connection fails.
    fn connect(&self, server_name: &str) -> OpcResult<Self::Server> {
        self.connect_identifier(&ServerIdentifier::ProgId(server_name.to_string()))
    }
}

/// Facade over a connected OPC DA server instance.
pub trait ConnectedServer {
    /// The group facade type returned by [`Self::add_group`].
    type Group: ConnectedGroup;

    /// Query the server's namespace organization type.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if querying organization fails.
    fn query_organization(&self) -> OpcResult<NamespaceType>;

    /// Browse the server's address space for item IDs of the given type.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if browsing fails.
    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> OpcResult<StringIterator>;

    /// Change the current browse position (e.g., navigate into/out of branches).
    ///
    /// # Errors
    /// Returns an [`OpcError`] if navigation fails.
    fn change_browse_position(&self, direction: BrowseDirection, name: &str) -> OpcResult<()>;

    /// Resolve a browse name to its fully-qualified item ID.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if resolution fails.
    fn get_item_id(&self, item_name: &str) -> OpcResult<String>;

    /// Add a new OPC group to this server connection using idiomatic parameters.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if group creation fails.
    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>>;

    /// Remove an OPC group by its server-assigned handle.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if group removal fails.
    fn remove_group(
        &self,
        server_group: ServerGroupHandle,
        mode: GroupRemovalMode,
    ) -> OpcResult<()>;
}

/// Facade over an OPC DA group for item management and I/O.
pub trait ConnectedGroup {
    /// Add items to this group for monitoring using pure-Rust definitions.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if adding items fails.
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>>;

    /// Perform a synchronous read of the given server handles, returning pure Rust states.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if read fails.
    fn read(
        &self,
        source: DataSource,
        server_handles: &[ServerItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>;

    /// Write values to the given items using strongly-typed [`ItemWrite`] pairs.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if write fails.
    fn write(&self, items: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_config_ephemeral_and_builders() {
        let config = GroupConfig::ephemeral("TestGroup")
            .with_update_rate(500)
            .with_client_handle(ClientGroupHandle::new(42))
            .with_active(false)
            .with_percent_deadband(0.5);

        assert_eq!(config.name, "TestGroup");
        assert!(!config.active);
        assert_eq!(config.update_rate_ms, 500);
        assert_eq!(config.client_handle, ClientGroupHandle::new(42));
        assert!((config.percent_deadband - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.time_bias, 0);
        assert_eq!(config.locale_id, 0);
    }

    #[test]
    fn test_group_removal_mode_conversions() {
        assert_eq!(GroupRemovalMode::default(), GroupRemovalMode::Normal);
        assert!(!GroupRemovalMode::Normal.is_force());
        assert!(GroupRemovalMode::Force.is_force());
        assert_eq!(GroupRemovalMode::from(false), GroupRemovalMode::Normal);
        assert_eq!(GroupRemovalMode::from(true), GroupRemovalMode::Force);
        assert!(!bool::from(GroupRemovalMode::Normal));
        assert!(bool::from(GroupRemovalMode::Force));
        assert_eq!(
            windows::core::BOOL::from(GroupRemovalMode::Normal),
            windows::core::BOOL(0)
        );
        assert_eq!(
            windows::core::BOOL::from(GroupRemovalMode::Force),
            windows::core::BOOL(1)
        );
    }

    #[test]
    fn test_item_write_construction() {
        let write = ItemWrite::new(ServerItemHandle::new(10), OpcValue::Int(42));
        assert_eq!(write.handle, ServerItemHandle::new(10));
        assert_eq!(write.value, OpcValue::Int(42));
    }
}
