//! Pure-Rust Data Transfer Objects (DTOs) and connector abstraction traits.
//!
//! Decouples domain logic and worker thread orchestration from low-level
//! Win32 COM interfaces and native FFI structs.

use crate::com::iterator::StringIterator;
use crate::errors::{OpcError, OpcResult};
use crate::provider::{OpcQuality, OpcValue};
use crate::types::{
    BrowseDirection, BrowseType, GroupHandle, ItemHandle, OpcServerInfo, ServerIdentifier,
};

// ── Pure-Rust Data Transfer Objects ────────────────────────────────

/// Definition of an item to be added to an OPC group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupItemDef {
    /// Fully qualified tag identifier.
    pub item_id: String,
    /// Handle assigned by the client for this item.
    pub client_handle: ItemHandle,
    /// Whether the item should be activated immediately.
    pub active: bool,
}

/// Result of adding an item to an OPC group.
#[derive(Debug)]
pub struct GroupItemResult {
    /// Server-assigned handle for this item.
    pub server_handle: ItemHandle,
    /// Canonical data type reported by the server.
    pub canonical_type: u16,
    /// Error if adding this specific item failed.
    pub error: Option<OpcError>,
}

/// Synchronous read result for an item in an OPC group using strong domain types.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupItemState {
    /// Client handle associated with this item.
    pub client_handle: ItemHandle,
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
    pub client_handle: GroupHandle,
    /// Time zone bias in minutes from UTC.
    pub time_bias: i32,
    /// Percent deadband for analog items.
    pub percent_deadband: f32,
    /// Locale identifier.
    pub locale_id: u32,
}

/// Output wrapper returned when an OPC group is added.
pub struct CreatedGroup<G> {
    /// Connected group instance.
    pub group: G,
    /// Server-assigned handle for the group.
    pub server_handle: GroupHandle,
    /// Revised update rate in milliseconds provided by the server.
    pub revised_update_rate_ms: u32,
}

// ── Connector & Facade Traits ───────────────────────────────────────

/// Factory for connecting to OPC DA servers.
pub trait ServerConnector: Send + Sync {
    /// The server facade type returned by [`Self::connect`].
    type Server: ConnectedServer;

    /// Enumerate all OPC DA server ProgIDs on the local machine.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if server enumeration fails.
    fn enumerate_servers(&self) -> OpcResult<Vec<String>>;

    /// Enumerate all OPC DA servers on the target host with rich catalog details.
    ///
    /// The default implementation falls back to [`Self::enumerate_servers`] and synthesizes
    /// [`OpcServerInfo`] records with zeroed CLSIDs and `user_type: None`.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if server enumeration fails.
    fn enumerate_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        let servers = self.enumerate_servers()?;
        let host_opt =
            if host.is_empty() || host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
                None
            } else {
                Some(host.to_string())
            };
        Ok(servers
            .into_iter()
            .map(|prog_id| OpcServerInfo {
                prog_id,
                clsid: windows::core::GUID::zeroed(),
                user_type: None,
                host: host_opt.clone(),
            })
            .collect())
    }

    /// Connect to the named OPC DA server and return a server facade.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if connection fails.
    fn connect(&self, server_name: &str) -> OpcResult<Self::Server>;

    /// Connect to an OPC DA server specified by a [`ServerIdentifier`].
    ///
    /// The default implementation delegates to [`Self::connect`] using the string representation.
    ///
    /// # Errors
    /// Returns an [`OpcError`] if connection fails.
    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        match identifier {
            ServerIdentifier::ProgId(prog_id) => self.connect(prog_id),
            ServerIdentifier::Clsid(guid) => {
                self.connect(&crate::types::format_guid_bracketed(guid))
            }
        }
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
    fn query_organization(&self) -> OpcResult<u32>;

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
    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()>;
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
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>;

    /// Write values to the given server handles using pure Rust [`OpcValue`].
    ///
    /// # Errors
    /// Returns an [`OpcError`] if write fails.
    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>>;
}
