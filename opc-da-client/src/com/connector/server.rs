//! Win32 COM server connection, category enumeration, and namespace navigation.
//!
//! Provides the concrete [`ComConnector`] and [`ComServer`] types implementing
//! the [`ServerConnector`] and [`ConnectedServer`] traits.

use crate::com::connector::group::ComGroup;
use crate::com::connector::traits::{ConnectedServer, CreatedGroup, GroupConfig, ServerConnector};
use crate::com::iterator::StringIterator;
use crate::errors::{OpcError, OpcResult};
use crate::raw::bindings::da::{
    OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_TO, OPC_BROWSE_UP, OPC_FLAT, OPC_LEAF,
};
use crate::raw::memory::{LocalPointer, RemotePointer};
use crate::types::{BrowseDirection, BrowseType, GroupHandle, OpcServerInfo, ServerIdentifier};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_REMOTE_SERVER, CLSIDFromProgID, COAUTHINFO, COSERVERINFO, CoCreateInstance,
    CoCreateInstanceEx, CoSetProxyBlanket, EOAC_NONE, MULTI_QI, RPC_C_AUTHN_LEVEL, RPC_C_IMP_LEVEL,
};
use windows::core::Interface;

/// Standard OPC Foundation OPCEnum CLSID for remote server discovery.
pub const CLSID_OPC_SERVER_LIST: windows::core::GUID =
    windows::core::GUID::from_u128(0x1348_6d51_4821_11d2_a494_3cb3_06c1_0000);

pub const RPC_C_AUTHN_LEVEL_CONNECT: u32 = 2;
pub const RPC_C_AUTHN_LEVEL_PKT_INTEGRITY: u32 = 5;
pub const RPC_C_AUTHN_WINNT: u32 = 10;
pub const RPC_C_AUTHZ_NONE: u32 = 0;
pub const RPC_C_IMP_LEVEL_IMPERSONATE: u32 = 3;

/// Returns the RPC authentication level depending on whether legacy DCOM is requested.
#[inline]
#[must_use]
pub const fn authn_level_for(legacy_dcom: bool) -> u32 {
    if legacy_dcom {
        RPC_C_AUTHN_LEVEL_CONNECT
    } else {
        RPC_C_AUTHN_LEVEL_PKT_INTEGRITY
    }
}

/// Applies DCOM security blanketing to a COM interface proxy.
///
/// In modern Windows environments (post-KB5004442), DCOM RPC requires `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` (5)
/// by default, or `RPC_C_AUTHN_LEVEL_CONNECT` (2) if `legacy_dcom` is enabled.
pub fn apply_proxy_blanket<T: Interface>(proxy: &T, legacy_dcom: bool) -> OpcResult<()> {
    let authn_level = authn_level_for(legacy_dcom);
    let unk: windows::core::IUnknown = match proxy.cast() {
        Ok(u) => u,
        Err(e) => {
            tracing::debug!(error = ?e, "Interface does not cast to IUnknown for proxy blanketing");
            return Ok(());
        }
    };
    // SAFETY: Calling CoSetProxyBlanket on valid COM interface pointer with standard NT security.
    let hr = unsafe {
        CoSetProxyBlanket(
            &unk,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL(authn_level),
            RPC_C_IMP_LEVEL(RPC_C_IMP_LEVEL_IMPERSONATE),
            None,
            EOAC_NONE,
        )
    };
    if let Err(e) = hr {
        tracing::warn!(
            error = ?e,
            authn_level,
            "Failed to set COM proxy blanket (continuing without security blanket)"
        );
    }
    Ok(())
}

/// Resolve an [`OpcServerEndpoint`](crate::types::OpcServerEndpoint) to a connected COM [`crate::raw::bindings::da::IOPCServer`] instance,
/// supporting both local COM activation and remote DCOM activation via `CoCreateInstanceEx`.
#[tracing::instrument(level = "info", err)]
pub(crate) fn connect_endpoint(
    endpoint: &crate::types::OpcServerEndpoint,
    legacy_dcom: bool,
) -> OpcResult<crate::raw::bindings::da::IOPCServer> {
    let clsid_raw = match &endpoint.identifier {
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

    let server_desc = endpoint.to_string();
    let is_remote_host = endpoint
        .host
        .as_deref()
        .filter(|h| !h.is_empty() && !h.eq_ignore_ascii_case("localhost") && *h != "127.0.0.1");

    let server: crate::raw::bindings::da::IOPCServer = if let Some(host) = is_remote_host {
        let host_lp = LocalPointer::from(host);
        let authn_level = authn_level_for(legacy_dcom);
        let auth_info = COAUTHINFO {
            dwAuthnSvc: RPC_C_AUTHN_WINNT,
            dwAuthzSvc: RPC_C_AUTHZ_NONE,
            pwszServerPrincName: windows::core::PWSTR::null(),
            dwAuthnLevel: authn_level,
            dwImpersonationLevel: RPC_C_IMP_LEVEL_IMPERSONATE,
            pAuthIdentityData: std::ptr::null_mut(),
            dwCapabilities: 0,
        };
        let server_info = COSERVERINFO {
            dwReserved1: 0,
            pwszName: host_lp.as_pwstr(),
            pAuthInfo: (&raw const auth_info).cast_mut(),
            dwReserved2: 0,
        };
        let mqi = MULTI_QI {
            pIID: &crate::raw::bindings::da::IOPCServer::IID,
            pItf: std::mem::ManuallyDrop::new(None),
            hr: windows::core::HRESULT(0),
        };
        let mut mqi_slice = [mqi];

        // SAFETY: Calling CoCreateInstanceEx with remote host and valid MULTI_QI.
        unsafe {
            CoCreateInstanceEx(
                &raw const clsid_raw,
                None,
                CLSCTX_REMOTE_SERVER,
                Some(&raw const server_info),
                &mut mqi_slice,
            )
        }
        .inspect_err(|e| {
            let err = OpcError::from(e.clone());
            crate::log_opc_err!(&err, crate::errors::OpcOperation::Connect, server = %server_desc);
        })?;

        let [mut result_mqi] = mqi_slice;
        if result_mqi.hr.is_err() {
            let err = OpcError::from(windows::core::Error::from_hresult(result_mqi.hr));
            crate::log_opc_err!(&err, crate::errors::OpcOperation::Connect, server = %server_desc);
            return Err(err);
        }

        // SAFETY: CoCreateInstanceEx succeeded with S_OK and populated result_mqi.pItf with a valid COM pointer.
        let unk =
            unsafe { std::mem::ManuallyDrop::take(&mut result_mqi.pItf) }.ok_or_else(|| {
                OpcError::Internal("CoCreateInstanceEx returned null interface pointer".into())
            })?;

        // Apply proxy blanket to IOPCServer
        apply_proxy_blanket(&unk, legacy_dcom)?;

        unk.cast()?
    } else {
        // SAFETY: Calling COM function CoCreateInstance with valid CLSID to instantiate IOPCServer locally.
        let s: crate::raw::bindings::da::IOPCServer = unsafe {
            CoCreateInstance(&raw const clsid_raw, None, CLSCTX_ALL)
        }
        .inspect_err(|e| {
            let err = OpcError::from(e.clone());
            crate::log_opc_err!(&err, crate::errors::OpcOperation::Connect, server = %server_desc);
        })?;
        s
    };

    tracing::debug!(server = %server_desc, "Connected to OPC DA server");
    Ok(server)
}

/// Resolve an OPC DA server [`ServerIdentifier`] to a connected COM [`crate::raw::bindings::da::IOPCServer`] instance.
#[allow(dead_code)]
#[tracing::instrument(level = "info", err)]
pub(crate) fn connect_server_identifier(
    identifier: &ServerIdentifier,
) -> OpcResult<crate::raw::bindings::da::IOPCServer> {
    connect_endpoint(
        &crate::types::OpcServerEndpoint::from(identifier.clone()),
        false,
    )
}

/// Real COM-backed server connector implementation.
#[derive(Default, Clone)]
pub struct ComConnector;

impl ComConnector {
    /// Connects to an [`OpcServerEndpoint`](crate::types::OpcServerEndpoint) with optional `legacy_dcom` authentication override.
    pub fn connect_endpoint(
        &self,
        endpoint: &crate::types::OpcServerEndpoint,
        legacy_dcom: bool,
    ) -> OpcResult<ComServer> {
        let server = connect_endpoint(endpoint, legacy_dcom)?;

        let common: crate::raw::bindings::comn::IOPCCommon = server.cast()?;
        let _ = apply_proxy_blanket(&common, legacy_dcom);

        let item_properties: crate::raw::bindings::da::IOPCItemProperties = server.cast()?;
        let _ = apply_proxy_blanket(&item_properties, legacy_dcom);

        let server_public_groups: Option<crate::raw::bindings::da::IOPCServerPublicGroups> =
            server.cast().ok();
        if let Some(ref spg) = server_public_groups {
            let _ = apply_proxy_blanket(spg, legacy_dcom);
        }

        let browse_server_address_space: Option<
            crate::raw::bindings::da::IOPCBrowseServerAddressSpace,
        > = server.cast().ok();
        if let Some(ref bsas) = browse_server_address_space {
            let _ = apply_proxy_blanket(bsas, legacy_dcom);
        }

        Ok(ComServer {
            server,
            common,
            item_properties,
            server_public_groups,
            browse_server_address_space,
            legacy_dcom,
        })
    }
}

impl ServerConnector for ComConnector {
    type Server = ComServer;

    #[tracing::instrument(level = "info", skip(self), err)]
    fn enumerate_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        let details = self.enumerate_server_details(host)?;
        let mut servers: Vec<String> = details.into_iter().map(|d| d.prog_id).collect();
        servers.sort();
        servers.dedup();
        Ok(servers)
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn enumerate_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        tracing::debug!("Enumerating OPC DA Server catalog details via OpcServerListCatalog");
        let catalog = crate::com::discovery::OpcServerListCatalog::new(Some(host), false)?;
        catalog.enumerate_details(host)
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        self.connect_endpoint(
            &crate::types::OpcServerEndpoint::from(identifier.clone()),
            false,
        )
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
    pub(crate) legacy_dcom: bool,
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
        let group_name_buf = LocalPointer::from(config.name);
        let group_name_ptr = group_name_buf.as_pcwstr();

        let mut raw_server_handle = 0u32;
        let mut revised_update_rate = 0u32;
        // SAFETY: Calling COM interface method AddGroup with valid parameters and output pointers.
        unsafe {
            self.server.AddGroup(
                group_name_ptr,
                config.active,
                config.update_rate_ms,
                config.client_handle.as_raw(),
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
                    server_handle: GroupHandle::new(raw_server_handle),
                    revised_update_rate_ms: revised_update_rate,
                })
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()> {
        // SAFETY: Calling COM interface method RemoveGroup with server handle.
        unsafe {
            self.server.RemoveGroup(server_group.as_raw(), force)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clsid_opc_server_list_constant() {
        assert_eq!(
            CLSID_OPC_SERVER_LIST,
            windows::core::GUID::from_u128(0x1348_6d51_4821_11d2_a494_3cb3_06c1_0000)
        );
        let formatted = format!("{CLSID_OPC_SERVER_LIST:?}");
        assert!(
            formatted
                .to_lowercase()
                .contains("13486d51-4821-11d2-a494-3cb306c10000")
        );
    }

    #[test]
    fn test_authn_level_selection() {
        assert_eq!(authn_level_for(false), RPC_C_AUTHN_LEVEL_PKT_INTEGRITY);
        assert_eq!(authn_level_for(true), RPC_C_AUTHN_LEVEL_CONNECT);
    }
}
