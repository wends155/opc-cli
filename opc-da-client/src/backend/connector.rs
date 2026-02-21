//! Abstractions for OPC DA server connectivity.
//!
//! Defines the [`ServerConnector`], [`ConnectedServer`], and [`ConnectedGroup`]
//! traits that decouple [`super::opc_da::OpcDaWrapper`] from concrete COM types.
//! This enables mock implementations for unit testing without a live COM server.

pub use crate::bindings::da::tagOPCITEMDEF;
pub use crate::bindings::da::{tagOPCITEMRESULT, tagOPCITEMSTATE};
use crate::opc_da::client::StringIterator;
pub use crate::opc_da::utils::RemoteArray;
pub use windows::Win32::System::Variant::VARIANT;

/// Factory for connecting to OPC DA servers.
///
/// Abstracts the concrete `v2::Client` usage so that tests can inject mocks
/// that return pre-configured server/group results without a live COM runtime.
///
/// # Errors
///
/// All methods return `anyhow::Result` — implementations should wrap COM errors
/// with contextual messages.
pub trait ServerConnector: Send + Sync {
    /// The server facade type returned by [`Self::connect`].
    type Server: ConnectedServer;

    /// Enumerate all OPC DA server ProgIDs on the local machine.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM registry enumeration fails.
    fn enumerate_servers(&self) -> anyhow::Result<Vec<String>>;

    /// Connect to the named OPC DA server and return a server facade.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM server cannot be created or connected.
    fn connect(&self, server_name: &str) -> anyhow::Result<Self::Server>;
}

/// Facade over a connected OPC DA server instance.
///
/// Wraps namespace browsing and group management operations in Rust-native types.
///
/// # Errors
///
/// All methods return `anyhow::Result` — COM errors are propagated with context.
pub trait ConnectedServer {
    /// The group facade type returned by [`Self::add_group`].
    type Group: ConnectedGroup;

    /// Query the server's namespace organization type.
    ///
    /// Returns `OPC_NS_FLAT` or `OPC_NS_HIERARCHICAL` as a `u32`.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM call fails.
    fn query_organization(&self) -> anyhow::Result<u32>;

    /// Browse the server's address space for item IDs of the given type.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM browse call fails.
    fn browse_opc_item_ids(
        &self,
        browse_type: u32,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> anyhow::Result<StringIterator>;

    /// Change the current browse position (e.g., navigate into/out of branches).
    ///
    /// # Errors
    ///
    /// Returns an error if the position change is rejected by the server.
    fn change_browse_position(&self, direction: u32, name: &str) -> anyhow::Result<()>;

    /// Resolve a browse name to its fully-qualified item ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot resolve the item name.
    fn get_item_id(&self, item_name: &str) -> anyhow::Result<String>;

    /// Add a new OPC group to this server connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the group creation fails.
    #[allow(clippy::too_many_arguments)]
    fn add_group(
        &self,
        name: &str,
        active: bool,
        update_rate: u32,
        client_handle: u32,
        time_bias: i32,
        percent_deadband: f32,
        locale_id: u32,
        revised_update_rate: &mut u32,
        server_handle: &mut u32,
    ) -> anyhow::Result<Self::Group>;

    /// Remove an OPC group by its server-assigned handle.
    ///
    /// # Errors
    ///
    /// Returns an error if the group removal fails.
    fn remove_group(&self, server_group: u32, force: bool) -> anyhow::Result<()>;
}

/// Facade over an OPC DA group for item management and I/O.
///
/// # Errors
///
/// All methods return `anyhow::Result` — COM errors are propagated with context.
pub trait ConnectedGroup {
    /// Add items to this group for monitoring.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM `AddItems` call fails.
    fn add_items(
        &self,
        items: &[tagOPCITEMDEF],
    ) -> anyhow::Result<(
        RemoteArray<tagOPCITEMRESULT>,
        RemoteArray<windows::core::HRESULT>,
    )>;

    /// Perform a synchronous read of the given server handles.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM `Read` call fails.
    fn read(
        &self,
        source: crate::bindings::da::tagOPCDATASOURCE,
        server_handles: &[u32],
    ) -> anyhow::Result<(
        RemoteArray<tagOPCITEMSTATE>,
        RemoteArray<windows::core::HRESULT>,
    )>;

    /// Write values to the given server handles.
    ///
    /// # Errors
    ///
    /// Returns an error if the COM `Write` call fails.
    fn write(
        &self,
        server_handles: &[u32],
        values: &[VARIANT],
    ) -> anyhow::Result<RemoteArray<windows::core::HRESULT>>;
}

// ── COM-backed implementations ──────────────────────────────────────

use crate::opc_da::client::v2::{Client, Group, Server};
use crate::opc_da::client::{
    BrowseServerAddressSpaceTrait, ClientTrait, ItemMgtTrait, ServerTrait, SyncIoTrait,
};
use anyhow::Context;

/// Real COM-backed [`ServerConnector`] implementation.
///
/// Uses `v2::Client` to enumerate and connect to OPC DA servers via Windows COM.
pub struct ComConnector;

impl ServerConnector for ComConnector {
    type Server = ComServer;

    fn enumerate_servers(&self) -> anyhow::Result<Vec<String>> {
        let client = Client;
        let guid_iter = client
            .get_servers()
            .context("Failed to enumerate OPC DA servers from registry")?;

        let mut servers = Vec::new();
        for guid in guid_iter.flatten() {
            // SAFETY: `crate::opc_da::GUID` and `windows::core::GUID` are both
            // `#[repr(C)]` structs with identical layout (4-byte, 2-byte, 2-byte,
            // 8-byte array). This is validated by a `const_assert_eq!` in
            // `opc_da/client/iterator.rs`.
            let win_guid: windows::core::GUID = unsafe { std::mem::transmute_copy(&guid) };
            if win_guid == windows::core::GUID::zeroed() {
                continue;
            }

            if let Ok(progid) = crate::helpers::guid_to_progid(&win_guid)
                && !progid.is_empty()
            {
                servers.push(progid);
            }
        }
        servers.sort();
        servers.dedup();
        Ok(servers)
    }

    fn connect(&self, server_name: &str) -> anyhow::Result<Self::Server> {
        let opc_server = crate::helpers::connect_server(server_name)?;
        Ok(ComServer(opc_server))
    }
}

/// COM-backed [`ConnectedServer`] wrapping a `v2::Server`.
pub struct ComServer(Server);

impl ConnectedServer for ComServer {
    type Group = ComGroup;

    fn query_organization(&self) -> anyhow::Result<u32> {
        Ok(self.0.query_organization()?.0.cast_unsigned())
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: u32,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> anyhow::Result<StringIterator> {
        #[allow(clippy::cast_possible_wrap)]
        let enum_str = self.0.browse_opc_item_ids(
            crate::bindings::da::tagOPCBROWSETYPE(browse_type as i32),
            filter,
            data_type,
            access_rights,
        )?;
        Ok(StringIterator::new(enum_str))
    }

    fn change_browse_position(&self, direction: u32, name: &str) -> anyhow::Result<()> {
        #[allow(clippy::cast_possible_wrap)]
        Ok(self.0.change_browse_position(
            crate::bindings::da::tagOPCBROWSEDIRECTION(direction as i32),
            name,
        )?)
    }

    fn get_item_id(&self, item_name: &str) -> anyhow::Result<String> {
        Ok(self.0.get_item_id(item_name)?)
    }

    fn add_group(
        &self,
        name: &str,
        active: bool,
        update_rate: u32,
        client_handle: u32,
        time_bias: i32,
        percent_deadband: f32,
        locale_id: u32,
        revised_update_rate: &mut u32,
        server_handle: &mut u32,
    ) -> anyhow::Result<Self::Group> {
        let group = self.0.add_group(
            name,
            active,
            client_handle,
            update_rate,
            locale_id,
            time_bias,
            percent_deadband,
            revised_update_rate,
            server_handle,
        )?;
        Ok(ComGroup(group))
    }

    fn remove_group(&self, server_group: u32, force: bool) -> anyhow::Result<()> {
        Ok(self.0.remove_group(server_group, force)?)
    }
}

/// COM-backed [`ConnectedGroup`] wrapping a `v2::Group`.
pub struct ComGroup(Group);

impl ConnectedGroup for ComGroup {
    fn add_items(
        &self,
        items: &[tagOPCITEMDEF],
    ) -> anyhow::Result<(
        RemoteArray<tagOPCITEMRESULT>,
        RemoteArray<windows::core::HRESULT>,
    )> {
        Ok(self.0.add_items(items)?)
    }

    fn read(
        &self,
        source: crate::bindings::da::tagOPCDATASOURCE,
        server_handles: &[u32],
    ) -> anyhow::Result<(
        RemoteArray<tagOPCITEMSTATE>,
        RemoteArray<windows::core::HRESULT>,
    )> {
        Ok(self.0.read(source, server_handles)?)
    }

    fn write(
        &self,
        server_handles: &[u32],
        values: &[VARIANT],
    ) -> anyhow::Result<RemoteArray<windows::core::HRESULT>> {
        Ok(self.0.write(server_handles, values)?)
    }
}
