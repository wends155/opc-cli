//! Abstractions for OPC DA server connectivity.
//!
//! Defines the [`ServerConnector`], [`ConnectedServer`], and [`ConnectedGroup`]
//! traits that decouple [`super::client::OpcDaClient`] from concrete COM types.
//! This enables mock implementations for unit testing without a live COM server.

#![allow(warnings)]
#![allow(clippy::all, clippy::pedantic, clippy::restriction)]

pub use crate::bindings::da::{tagOPCDATASOURCE, tagOPCITEMDEF, tagOPCITEMRESULT, tagOPCITEMSTATE};
pub use crate::com::iterator::{GuidIterator, StringIterator};
pub use crate::com::memory::{LocalPointer, RemoteArray, RemotePointer};
pub use crate::errors::{OpcError, OpcResult};
pub use crate::types::{BrowseDirection, BrowseType, GroupHandle, ItemHandle};
use anyhow::Context;
pub use windows::Win32::System::Variant::VARIANT;
use windows::core::Interface;

/// Factory for connecting to OPC DA servers.
///
/// Abstracts the concrete COM client usage so that tests can inject mocks
/// that return pre-configured server/group results without a live COM runtime.
pub trait ServerConnector: Send + Sync {
    /// The server facade type returned by [`Self::connect`].
    type Server: ConnectedServer;

    /// Enumerate all OPC DA server ProgIDs on the local machine.
    fn enumerate_servers(&self) -> OpcResult<Vec<String>>;

    /// Connect to the named OPC DA server and return a server facade.
    fn connect(&self, server_name: &str) -> OpcResult<Self::Server>;
}

/// Facade over a connected OPC DA server instance.
///
/// Wraps namespace browsing and group management operations in Rust-native types.
pub trait ConnectedServer {
    /// The group facade type returned by [`Self::add_group`].
    type Group: ConnectedGroup;

    /// Query the server's namespace organization type.
    ///
    /// Returns `OPC_NS_FLAT` (1) or `OPC_NS_HIERARCHIAL` (2) as a `u32`.
    fn query_organization(&self) -> OpcResult<u32>;

    /// Browse the server's address space for item IDs of the given type.
    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> OpcResult<StringIterator>;

    /// Change the current browse position (e.g., navigate into/out of branches).
    fn change_browse_position(&self, direction: BrowseDirection, name: &str) -> OpcResult<()>;

    /// Resolve a browse name to its fully-qualified item ID.
    fn get_item_id(&self, item_name: &str) -> OpcResult<String>;

    /// Add a new OPC group to this server connection.
    #[allow(clippy::too_many_arguments)]
    fn add_group(
        &self,
        name: &str,
        active: bool,
        update_rate: u32,
        client_handle: GroupHandle,
        time_bias: i32,
        percent_deadband: f32,
        locale_id: u32,
        revised_update_rate: &mut u32,
        server_handle: &mut GroupHandle,
    ) -> OpcResult<Self::Group>;

    /// Remove an OPC group by its server-assigned handle.
    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()>;
}

/// Facade over an OPC DA group for item management and I/O.
pub trait ConnectedGroup {
    /// Add items to this group for monitoring.
    fn add_items(
        &self,
        items: &[tagOPCITEMDEF],
    ) -> OpcResult<(
        RemoteArray<tagOPCITEMRESULT>,
        RemoteArray<windows::core::HRESULT>,
    )>;

    /// Perform a synchronous read of the given server handles.
    fn read(
        &self,
        source: tagOPCDATASOURCE,
        server_handles: &[ItemHandle],
    ) -> OpcResult<(
        RemoteArray<tagOPCITEMSTATE>,
        RemoteArray<windows::core::HRESULT>,
    )>;

    /// Write values to the given server handles.
    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[VARIANT],
    ) -> OpcResult<RemoteArray<windows::core::HRESULT>>;
}

// ── COM-backed implementations ──────────────────────────────────────

/// Real COM-backed server connector implementation.
///
/// Uses Windows COM to enumerate and connect to OPC DA servers.
pub struct ComConnector;

impl ServerConnector for ComConnector {
    type Server = ComServer;

    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        tracing::debug!("Enumerating OPC DA Server classes via COM Component Categories Manager");
        // SAFETY: Calling COM function CLSIDFromProgID with static wide string literal.
        let id = unsafe {
            windows::Win32::System::Com::CLSIDFromProgID(windows::core::w!("OPC.ServerList.1"))?
        };

        // SAFETY: Calling COM function CoCreateInstance to instantiate IOPCServerList interface.
        let servers: crate::bindings::comn::IOPCServerList = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &id,
                None,
                windows::Win32::System::Com::CLSCTX_ALL,
            )?
        };

        let versions = [crate::bindings::da::CATID_OPCDAServer20::IID];

        // SAFETY: Calling COM method EnumClassesOfCategories with valid version GUID slice.
        let iter = unsafe {
            servers
                .EnumClassesOfCategories(&versions, &versions)
                .map_err(|e| {
                    windows::core::Error::new(e.code(), "Failed to enumerate server classes")
                })?
        };

        let guid_iter = GuidIterator::new(iter);

        let mut servers = Vec::new();
        for guid in guid_iter.flatten() {
            if guid == windows::core::GUID::zeroed() {
                continue;
            }

            if let Ok(progid) = crate::helpers::guid_to_progid(&guid)
                && !progid.is_empty()
            {
                servers.push(progid);
            }
        }
        servers.sort();
        servers.dedup();
        Ok(servers)
    }

    fn connect(&self, server_name: &str) -> OpcResult<Self::Server> {
        let opc_server = crate::helpers::connect_server(server_name)?;
        let unknown: windows::core::IUnknown = opc_server.cast()?;

        Ok(ComServer {
            server: opc_server,
            common: unknown.cast()?,
            connection_point_container: unknown.cast()?,
            item_properties: unknown.cast()?,
            server_public_groups: unknown.cast().ok(),
            browse_server_address_space: unknown.cast().ok(),
        })
    }
}

/// COM-backed [`ConnectedServer`].
pub struct ComServer {
    pub(crate) server: crate::bindings::da::IOPCServer,
    pub(crate) common: crate::bindings::comn::IOPCCommon,
    pub(crate) connection_point_container: windows::Win32::System::Com::IConnectionPointContainer,
    pub(crate) item_properties: crate::bindings::da::IOPCItemProperties,
    pub(crate) server_public_groups: Option<crate::bindings::da::IOPCServerPublicGroups>,
    pub(crate) browse_server_address_space:
        Option<crate::bindings::da::IOPCBrowseServerAddressSpace>,
}

impl ConnectedServer for ComServer {
    type Group = ComGroup;

    fn query_organization(&self) -> OpcResult<u32> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        // SAFETY: Calling COM interface method QueryOrganization on valid interface pointer.
        unsafe { Ok(iface.QueryOrganization()?.0.cast_unsigned()) }
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> OpcResult<StringIterator> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        let filter_criteria = LocalPointer::from_option(filter);
        let raw_type = crate::bindings::da::tagOPCBROWSETYPE((browse_type as u32).cast_signed());
        // SAFETY: Calling COM interface method BrowseOPCItemIDs with valid filter string.
        let iter = unsafe {
            iface.BrowseOPCItemIDs(
                raw_type,
                filter_criteria.as_pwstr(),
                data_type,
                access_rights,
            )?
        };
        Ok(StringIterator::new(iter))
    }

    fn change_browse_position(&self, direction: BrowseDirection, name: &str) -> OpcResult<()> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        let pos = LocalPointer::from(name);
        let raw_dir = crate::bindings::da::tagOPCBROWSEDIRECTION((direction as u32).cast_signed());
        // SAFETY: Calling COM interface method ChangeBrowsePosition with valid position string.
        unsafe {
            iface.ChangeBrowsePosition(raw_dir, pos.as_pwstr())?;
        }
        Ok(())
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        let item_data_id = LocalPointer::from(item_name);
        // SAFETY: Calling COM interface method GetItemID with valid item_data_id string.
        let output = unsafe { iface.GetItemID(item_data_id.as_pwstr())? };
        let ptr = RemotePointer::from(output);
        ptr.try_into().map_err(OpcError::from)
    }

    fn add_group(
        &self,
        name: &str,
        active: bool,
        update_rate: u32,
        client_handle: GroupHandle,
        time_bias: i32,
        percent_deadband: f32,
        locale_id: u32,
        revised_update_rate: &mut u32,
        server_handle: &mut GroupHandle,
    ) -> OpcResult<Self::Group> {
        let mut group = None;
        let group_name = LocalPointer::from(name);
        let group_name = group_name.as_pcwstr();

        let mut raw_server_handle = 0u32;
        // SAFETY: Calling COM interface method AddGroup with valid parameters and output pointers.
        unsafe {
            self.server.AddGroup(
                group_name,
                active,
                update_rate,
                client_handle.0,
                &time_bias,
                &percent_deadband,
                locale_id,
                &mut raw_server_handle,
                revised_update_rate,
                &crate::bindings::da::IOPCItemMgt::IID,
                &mut group,
            )?;
        }
        *server_handle = GroupHandle(raw_server_handle);

        match group {
            None => Err(OpcError::Com {
                source: windows::core::Error::new(
                    windows::Win32::Foundation::E_POINTER,
                    "Failed to add group, returned null",
                ),
            }),
            Some(group) => {
                let unknown: windows::core::IUnknown = group.cast()?;
                unknown
                    .try_into()
                    .map_err(|source| OpcError::Com { source })
            }
        }
    }

    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()> {
        // SAFETY: Calling COM interface method RemoveGroup with server handle.
        unsafe {
            self.server.RemoveGroup(server_group.0, force)?;
        }
        Ok(())
    }
}

/// COM-backed [`ConnectedGroup`].
pub struct ComGroup {
    pub(crate) item_mgt: crate::bindings::da::IOPCItemMgt,
    pub(crate) group_state_mgt: crate::bindings::da::IOPCGroupStateMgt,
    pub(crate) public_group_state_mgt: Option<crate::bindings::da::IOPCPublicGroupStateMgt>,
    pub(crate) sync_io: crate::bindings::da::IOPCSyncIO,
    pub(crate) async_io: Option<crate::bindings::da::IOPCAsyncIO>,
    pub(crate) async_io2: crate::bindings::da::IOPCAsyncIO2,
    pub(crate) connection_point_container: windows::Win32::System::Com::IConnectionPointContainer,
    pub(crate) data_object: Option<windows::Win32::System::Com::IDataObject>,
}

impl ConnectedGroup for ComGroup {
    fn add_items(
        &self,
        items: &[tagOPCITEMDEF],
    ) -> OpcResult<(
        RemoteArray<tagOPCITEMRESULT>,
        RemoteArray<windows::core::HRESULT>,
    )> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

        let len = items.len().try_into()?;
        tracing::debug!(
            item_count = len,
            "Adding items to OPC group natively via IOPCItemMgt"
        );
        let mut results = RemoteArray::new(len);
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method AddItems with valid item definition array and output buffers.
        unsafe {
            self.item_mgt.AddItems(
                len,
                items.as_ptr(),
                results.as_mut_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        Ok((results, errors))
    }

    fn read(
        &self,
        source: tagOPCDATASOURCE,
        server_handles: &[ItemHandle],
    ) -> OpcResult<(
        RemoteArray<tagOPCITEMSTATE>,
        RemoteArray<windows::core::HRESULT>,
    )> {
        if server_handles.is_empty() {
            return Err(OpcError::InvalidState(
                "server_handles cannot be empty".to_string(),
            ));
        }

        let len = server_handles.len().try_into()?;

        let mut item_values = RemoteArray::new(len);
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method Read with valid server handle array and output buffers.
        unsafe {
            self.sync_io.Read(
                source,
                len,
                server_handles.as_ptr() as *const u32,
                item_values.as_mut_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        Ok((item_values, errors))
    }

    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[VARIANT],
    ) -> OpcResult<RemoteArray<windows::core::HRESULT>> {
        if server_handles.len() != values.len() {
            return Err(OpcError::InvalidState(
                "server_handles and values must have the same length".to_string(),
            ));
        }

        let len = server_handles.len().try_into()?;
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method Write with valid server handle and VARIANT arrays.
        unsafe {
            self.sync_io.Write(
                len,
                server_handles.as_ptr() as *const u32,
                values.as_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        Ok(errors)
    }
}

impl TryFrom<windows::core::IUnknown> for ComGroup {
    type Error = windows::core::Error;

    fn try_from(unknown: windows::core::IUnknown) -> Result<Self, Self::Error> {
        Ok(Self {
            item_mgt: unknown.cast()?,
            group_state_mgt: unknown.cast()?,
            public_group_state_mgt: unknown.cast().ok(),
            sync_io: unknown.cast()?,
            async_io: unknown.cast().ok(),
            async_io2: unknown.cast()?,
            connection_point_container: unknown.cast()?,
            data_object: unknown.cast().ok(),
        })
    }
}
