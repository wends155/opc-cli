//! Win32 COM server connection, category enumeration, and namespace navigation.
//!
//! Provides the concrete [`ComConnector`] and [`ComServer`] types implementing
//! the [`ServerConnector`] and [`ConnectedServer`] traits.

use crate::com::connector::group::ComGroup;
use crate::com::connector::traits::{ConnectedServer, CreatedGroup, GroupConfig, ServerConnector};
use crate::com::discovery::guid_to_progid;
use crate::com::iterator::{GuidIterator, StringIterator};
use crate::errors::{OpcError, OpcResult};
use crate::raw::bindings::da::{
    OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_TO, OPC_BROWSE_UP, OPC_FLAT, OPC_LEAF,
};
use crate::raw::memory::{LocalPointer, RemotePointer};
use crate::types::{BrowseDirection, BrowseType, GroupHandle, OpcServerInfo, ServerIdentifier};
use windows::Win32::System::Com::{CLSCTX_ALL, CLSIDFromProgID, CoCreateInstance};
use windows::core::Interface;

/// Resolve an OPC DA server [`ServerIdentifier`] to a connected `opc_da` Server instance.
///
/// If connecting via [`ServerIdentifier::Clsid`], instantiates directly via `CoCreateInstance`.
/// If connecting via [`ServerIdentifier::ProgId`], resolves the ProgID via the Windows registry.
///
/// # Errors
///
/// Returns `Err` if the `ProgID` cannot be resolved or the server
/// cannot be instantiated.
#[tracing::instrument(level = "info", err)]
pub(crate) fn connect_server_identifier(
    identifier: &ServerIdentifier,
) -> OpcResult<crate::raw::bindings::da::IOPCServer> {
    let clsid_raw = match identifier {
        ServerIdentifier::Clsid(guid) => *guid,
        ServerIdentifier::ProgId(server_name) => {
            let server_lp = LocalPointer::from(server_name.as_str());
            // SAFETY: Calling COM function CLSIDFromProgID with a null-terminated wide string.
            match unsafe { CLSIDFromProgID(server_lp.as_pcwstr()) } {
                Ok(guid) => guid,
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        server = %server_name,
                        "Failed to resolve ProgID to CLSID"
                    );
                    return Err(OpcError::connection_failed(server_name, e));
                }
            }
        }
    };

    let server_desc = identifier.to_string();
    // SAFETY: Calling COM function CoCreateInstance with valid CLSID to instantiate IOPCServer.
    let server: crate::raw::bindings::da::IOPCServer =
        unsafe { CoCreateInstance(&raw const clsid_raw, None, CLSCTX_ALL) }.inspect_err(|e| {
            let err = OpcError::from(e.clone());
            crate::log_opc_err!(&err, crate::errors::OpcOperation::Connect, server = %server_desc);
        })?;
    tracing::debug!(server = %server_desc, "Connected to OPC DA server");
    Ok(server)
}

/// Real COM-backed server connector implementation.
pub struct ComConnector;

impl ServerConnector for ComConnector {
    type Server = ComServer;

    #[tracing::instrument(level = "info", skip(self), err)]
    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        tracing::debug!("Enumerating OPC DA Server classes via COM Component Categories Manager");
        // SAFETY: Calling COM function CLSIDFromProgID with static wide string literal.
        let id = unsafe { CLSIDFromProgID(windows::core::w!("OPC.ServerList.1"))? };

        // SAFETY: Calling COM function CoCreateInstance to instantiate IOPCServerList interface.
        let servers: crate::raw::bindings::comn::IOPCServerList =
            unsafe { CoCreateInstance(&raw const id, None, CLSCTX_ALL)? };

        let versions = [crate::raw::bindings::da::CATID_OPCDAServer20::IID];

        // SAFETY: Calling COM method EnumClassesOfCategories with valid version GUID slice.
        let iter = unsafe {
            servers
                .EnumClassesOfCategories(&versions, &versions)
                .inspect_err(|e| tracing::warn!(error = ?e, "Failed to enumerate server classes"))?
        };

        let guid_iter = GuidIterator::new(iter);

        let mut servers = Vec::new();
        for guid in guid_iter.flatten() {
            if guid == windows::core::GUID::zeroed() {
                continue;
            }

            if let Ok(progid) = guid_to_progid(&guid)
                && !progid.is_empty()
            {
                servers.push(progid);
            }
        }
        servers.sort();
        servers.dedup();
        Ok(servers)
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn enumerate_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        tracing::debug!("Enumerating OPC DA Server catalog details via OpcServerListCatalog");
        let catalog = crate::com::discovery::OpcServerListCatalog::new()?;
        catalog.enumerate_details(host)
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        let server = connect_server_identifier(identifier)?;

        let common: crate::raw::bindings::comn::IOPCCommon = server.cast()?;
        let item_properties: crate::raw::bindings::da::IOPCItemProperties = server.cast()?;
        let server_public_groups: Option<crate::raw::bindings::da::IOPCServerPublicGroups> =
            server.cast().ok();
        let browse_server_address_space: Option<
            crate::raw::bindings::da::IOPCBrowseServerAddressSpace,
        > = server.cast().ok();

        Ok(ComServer {
            server,
            common,
            item_properties,
            server_public_groups,
            browse_server_address_space,
        })
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn connect(&self, server_name: &str) -> OpcResult<Self::Server> {
        self.connect_identifier(&ServerIdentifier::from(server_name))
    }
}

/// COM-backed [`ConnectedServer`].
#[allow(dead_code)]
pub struct ComServer {
    pub(crate) server: crate::raw::bindings::da::IOPCServer,
    pub(crate) common: crate::raw::bindings::comn::IOPCCommon,
    pub(crate) item_properties: crate::raw::bindings::da::IOPCItemProperties,
    pub(crate) server_public_groups: Option<crate::raw::bindings::da::IOPCServerPublicGroups>,
    pub(crate) browse_server_address_space:
        Option<crate::raw::bindings::da::IOPCBrowseServerAddressSpace>,
}

impl ConnectedServer for ComServer {
    type Group = ComGroup;

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn query_organization(&self) -> OpcResult<u32> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        // SAFETY: Calling COM interface method QueryOrganization.
        let org = unsafe { iface.QueryOrganization()? };
        Ok(org.0.cast_unsigned())
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
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
        let filter_ptr = LocalPointer::from(filter.unwrap_or_default());
        let raw_type = match browse_type {
            BrowseType::Branch => OPC_BRANCH,
            BrowseType::Leaf => OPC_LEAF,
            BrowseType::Flat => OPC_FLAT,
        };
        // SAFETY: Calling COM interface method BrowseOPCItemIDs with valid parameters.
        let output = unsafe {
            iface.BrowseOPCItemIDs(raw_type, filter_ptr.as_pcwstr(), data_type, access_rights)?
        };
        Ok(StringIterator::new(output))
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn change_browse_position(&self, direction: BrowseDirection, name: &str) -> OpcResult<()> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        let name_ptr = LocalPointer::from(name);
        let raw_dir = match direction {
            BrowseDirection::Up => OPC_BROWSE_UP,
            BrowseDirection::Down => OPC_BROWSE_DOWN,
            BrowseDirection::To => OPC_BROWSE_TO,
        };
        // SAFETY: Calling COM interface method ChangeBrowsePosition with valid parameters.
        unsafe {
            iface.ChangeBrowsePosition(raw_dir, name_ptr.as_pcwstr())?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        let iface = self.browse_server_address_space.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCBrowseServerAddressSpace not supported".to_string())
        })?;
        let item_data_id = LocalPointer::from(item_name);
        // SAFETY: Calling COM interface method GetItemID with valid item_data_id string.
        let output = unsafe { iface.GetItemID(item_data_id.as_pwstr())? };
        RemotePointer::from(output).into_string()
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        let mut group = None;
        let group_name = LocalPointer::from(config.name);
        let group_name = group_name.as_pcwstr();

        let mut raw_server_handle = 0u32;
        let mut revised_update_rate = 0u32;
        // SAFETY: Calling COM interface method AddGroup with valid parameters and output pointers.
        unsafe {
            self.server.AddGroup(
                group_name,
                config.active,
                config.update_rate_ms,
                config.client_handle.0,
                &raw const config.time_bias,
                &raw const config.percent_deadband,
                config.locale_id,
                &raw mut raw_server_handle,
                &raw mut revised_update_rate,
                &crate::raw::bindings::da::IOPCItemMgt::IID,
                &raw mut group,
            )?;
        }

        match group {
            None => Err(OpcError::Com {
                source: windows::core::Error::new(
                    windows::Win32::Foundation::E_POINTER,
                    "Failed to add group, returned null",
                ),
            }),
            Some(group) => {
                let unknown: windows::core::IUnknown = group.cast()?;
                let group: ComGroup = unknown.try_into()?;

                Ok(CreatedGroup {
                    group,
                    server_handle: GroupHandle(raw_server_handle),
                    revised_update_rate_ms: revised_update_rate,
                })
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()> {
        // SAFETY: Calling COM interface method RemoveGroup with server handle.
        unsafe {
            self.server.RemoveGroup(server_group.0, force)?;
        }
        Ok(())
    }
}
