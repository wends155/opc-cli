//! Abstractions for OPC DA server connectivity.
//!
//! Defines the [`ServerConnector`], [`ConnectedServer`], and [`ConnectedGroup`]
//! traits that decouple [`super::client::OpcDaClient`] and [`super::worker::ComWorker`]
//! from concrete COM types. This enables mock implementations for unit testing
//! without a live COM server or native Windows allocators.

#![allow(
    clippy::borrow_as_ptr,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::type_complexity,
    clippy::derivable_impls,
    clippy::field_reassign_with_default,
    clippy::approx_constant,
    clippy::unreadable_literal
)]

use windows::Win32::System::Com::{CLSIDFromProgID, ProgIDFromCLSID};
use windows::core::{Interface, PCWSTR};

const _: () = assert!(
    std::mem::size_of::<windows::core::GUID>() == 16,
    "windows::core::GUID must be 16 bytes for COM compatibility"
);
const _: () = assert!(
    std::mem::align_of::<windows::core::GUID>() >= 4,
    "windows::core::GUID must be at least 4-byte aligned"
);

pub use crate::com::iterator::{GuidIterator, StringIterator};
pub use crate::errors::{OpcError, OpcResult};
pub use crate::provider::{OpcQuality, OpcValue};
pub use crate::raw::memory::{LocalPointer, RemoteArray, RemotePointer};
pub use crate::types::{
    BrowseDirection, BrowseType, GroupHandle, ItemHandle, OpcServerEndpoint, OpcServerInfo,
    ServerIdentifier,
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
    fn enumerate_servers(&self) -> OpcResult<Vec<String>>;

    /// Enumerate all OPC DA servers on the target host with rich catalog details.
    ///
    /// The default implementation falls back to [`Self::enumerate_servers`] and synthesizes
    /// [`OpcServerInfo`] records with zeroed CLSIDs and `user_type: None`.
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
    fn connect(&self, server_name: &str) -> OpcResult<Self::Server>;

    /// Connect to an OPC DA server specified by a [`ServerIdentifier`].
    ///
    /// The default implementation delegates to [`Self::connect`] using the string representation.
    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        match identifier {
            ServerIdentifier::ProgId(prog_id) => self.connect(prog_id),
            ServerIdentifier::Clsid(guid) => self.connect(&format!(
                "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
                guid.data1,
                guid.data2,
                guid.data3,
                guid.data4[0],
                guid.data4[1],
                guid.data4[2],
                guid.data4[3],
                guid.data4[4],
                guid.data4[5],
                guid.data4[6],
                guid.data4[7]
            )),
        }
    }
}

/// Facade over a connected OPC DA server instance.
pub trait ConnectedServer {
    /// The group facade type returned by [`Self::add_group`].
    type Group: ConnectedGroup;

    /// Query the server's namespace organization type.
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

    /// Add a new OPC group to this server connection using idiomatic parameters.
    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>>;

    /// Remove an OPC group by its server-assigned handle.
    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()>;
}

/// Facade over an OPC DA group for item management and I/O.
pub trait ConnectedGroup {
    /// Add items to this group for monitoring using pure-Rust definitions.
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>>;

    /// Perform a synchronous read of the given server handles, returning pure Rust states.
    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>;

    /// Write values to the given server handles using pure Rust [`OpcValue`].
    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>>;
}

// ── COM-backed implementations ──────────────────────────────────────

/// Helper to convert GUID to `ProgID` using Windows API
#[tracing::instrument(level = "debug", err)]
pub(crate) fn guid_to_progid(guid: &windows::core::GUID) -> OpcResult<String> {
    // SAFETY: `ProgIDFromCLSID` is a Win32 FFI call that allocates a PWSTR via COM allocator.
    let progid = unsafe { ProgIDFromCLSID(guid) }?;

    if progid.is_null() {
        return Ok(String::new());
    }

    RemotePointer::from(progid).into_string()
}

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
            // SAFETY: `server_wide` is null-terminated and lives until end of scope.
            unsafe {
                let server_wide: Vec<u16> = server_name
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                match CLSIDFromProgID(PCWSTR(server_wide.as_ptr())) {
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
        }
    };

    let server_desc = identifier.to_string();
    // SAFETY: Calling COM function CoCreateInstance with valid CLSID to instantiate IOPCServer.
    let server: crate::raw::bindings::da::IOPCServer = unsafe {
        windows::Win32::System::Com::CoCreateInstance(
            &raw const clsid_raw,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        )
    }
    .inspect_err(|e| {
        let err = OpcError::from(e.clone());
        crate::log_opc_err!(&err, crate::errors::OpcOperation::Connect, server = %server_desc);
    })?;
    tracing::debug!(server = %server_desc, "Connected to OPC DA server");
    Ok(server)
}

/// Resolve an OPC DA server `ProgID` or CLSID string to a connected `opc_da` Server instance.
///
/// Converts the `ProgID` string to a `CLSID` via the Windows registry (or parses
/// direct CLSID syntax), then creates and returns a connected server handle.
///
/// # Errors
///
/// Returns `Err` if the `ProgID` cannot be resolved or the server
/// cannot be instantiated.
#[allow(dead_code)]
#[tracing::instrument(level = "info", err)]
pub(crate) fn connect_server(server_name: &str) -> OpcResult<crate::raw::bindings::da::IOPCServer> {
    connect_server_identifier(&ServerIdentifier::from(server_name))
}

/// Real COM-backed server connector implementation.
pub struct ComConnector;

impl ServerConnector for ComConnector {
    type Server = ComServer;

    #[tracing::instrument(level = "info", skip(self), err)]
    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        tracing::debug!("Enumerating OPC DA Server classes via COM Component Categories Manager");
        // SAFETY: Calling COM function CLSIDFromProgID with static wide string literal.
        let id = unsafe {
            windows::Win32::System::Com::CLSIDFromProgID(windows::core::w!("OPC.ServerList.1"))?
        };

        // SAFETY: Calling COM function CoCreateInstance to instantiate IOPCServerList interface.
        let servers: crate::raw::bindings::comn::IOPCServerList = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &raw const id,
                None,
                windows::Win32::System::Com::CLSCTX_ALL,
            )?
        };

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
        Ok(org.0 as u32)
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
        let raw_type =
            crate::raw::bindings::da::tagOPCBROWSETYPE((browse_type as u32).cast_signed());
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
        let raw_dir =
            crate::raw::bindings::da::tagOPCBROWSEDIRECTION((direction as u32).cast_signed());
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
                &config.time_bias,
                &config.percent_deadband,
                config.locale_id,
                &mut raw_server_handle,
                &mut revised_update_rate,
                &crate::raw::bindings::da::IOPCItemMgt::IID,
                &mut group,
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

/// COM-backed [`ConnectedGroup`].
#[allow(dead_code)]
pub struct ComGroup {
    pub(crate) item_mgt: crate::raw::bindings::da::IOPCItemMgt,
    pub(crate) group_state_mgt: crate::raw::bindings::da::IOPCGroupStateMgt,
    pub(crate) public_group_state_mgt: Option<crate::raw::bindings::da::IOPCPublicGroupStateMgt>,
    pub(crate) sync_io: crate::raw::bindings::da::IOPCSyncIO,
    pub(crate) async_io: Option<crate::raw::bindings::da::IOPCAsyncIO>,
    pub(crate) async_io2: crate::raw::bindings::da::IOPCAsyncIO2,
    pub(crate) connection_point_container: windows::Win32::System::Com::IConnectionPointContainer,
    pub(crate) data_object: Option<windows::Win32::System::Com::IDataObject>,
}

impl ConnectedGroup for ComGroup {
    #[tracing::instrument(level = "debug", skip(self, items), err)]
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

        let len = items.len().try_into()?;
        tracing::debug!(
            item_count = len,
            "Adding items to OPC group natively via IOPCItemMgt"
        );

        let mut wide_names: Vec<Vec<u16>> = Vec::with_capacity(items.len());
        for item in items {
            let wide: Vec<u16> = item
                .item_id
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            wide_names.push(wide);
        }

        let mut item_defs: Vec<crate::raw::bindings::da::tagOPCITEMDEF> =
            Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            let wide_ptr = wide_names[i].as_ptr();
            item_defs.push(crate::raw::bindings::da::tagOPCITEMDEF {
                szAccessPath: windows::core::PWSTR::null(),
                szItemID: windows::core::PWSTR(wide_ptr.cast_mut()),
                bActive: item.active.into(),
                hClient: item.client_handle.0,
                vtRequestedDataType: 0,
                dwBlobSize: 0,
                pBlob: std::ptr::null_mut(),
                wReserved: 0,
            });
        }

        let mut results = RemoteArray::new(len);
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method AddItems with valid item definition array and output buffers.
        unsafe {
            self.item_mgt.AddItems(
                len,
                item_defs.as_ptr(),
                results.as_mut_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        let results_slice = results.as_slice();
        let errors_slice = errors.as_slice();
        let mut group_results = Vec::with_capacity(items.len());

        for i in 0..items.len() {
            let err = if errors_slice[i].is_ok() {
                None
            } else {
                Some(OpcError::Com {
                    source: windows::core::Error::from_hresult(errors_slice[i]),
                })
            };
            group_results.push(GroupItemResult {
                server_handle: ItemHandle(results_slice[i].hServer),
                canonical_type: results_slice[i].vtCanonicalDataType,
                error: err,
            });
        }

        Ok(group_results)
    }

    #[tracing::instrument(level = "debug", skip(self, server_handles), err)]
    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        if server_handles.is_empty() {
            return Err(OpcError::InvalidState(
                "server_handles cannot be empty".to_string(),
            ));
        }

        let len = server_handles.len().try_into()?;
        let native_source = match source {
            DataSource::Cache => crate::raw::bindings::da::OPC_DS_CACHE,
            DataSource::Device => crate::raw::bindings::da::OPC_DS_DEVICE,
        };

        let mut item_values = RemoteArray::new(len);
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method Read with valid server handle array and output buffers.
        unsafe {
            self.sync_io.Read(
                native_source,
                len,
                server_handles.as_ptr().cast(),
                item_values.as_mut_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        let values_slice = item_values.as_slice();
        let errors_slice = errors.as_slice();
        let mut states = Vec::with_capacity(server_handles.len());

        for i in 0..server_handles.len() {
            let err = errors_slice[i];
            if err.is_ok() {
                let state = &values_slice[i];
                let value = crate::com::variant::variant_to_opc_value(&state.vDataValue);
                let quality = OpcQuality::from(state.wQuality);
                let timestamp =
                    crate::raw::memory::TryFromNative::try_from_native(&state.ftTimeStamp)
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                states.push(Ok(GroupItemState {
                    client_handle: ItemHandle(state.hClient),
                    value,
                    quality,
                    timestamp,
                }));
            } else {
                states.push(Err(OpcError::Com {
                    source: windows::core::Error::from_hresult(err),
                }));
            }
        }

        Ok(states)
    }

    #[tracing::instrument(level = "debug", skip(self, server_handles, values), err)]
    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        if server_handles.len() != values.len() {
            return Err(OpcError::InvalidState(
                "server_handles and values must have the same length".to_string(),
            ));
        }

        let len = server_handles.len().try_into()?;
        let variants: Vec<windows::Win32::System::Variant::VARIANT> = values
            .iter()
            .map(crate::com::variant::opc_value_to_variant)
            .collect();
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method Write with valid server handle and VARIANT arrays.
        unsafe {
            self.sync_io.Write(
                len,
                server_handles.as_ptr().cast(),
                variants.as_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        let errors_slice = errors.as_slice();
        let results = errors_slice
            .iter()
            .map(|&hr| {
                if hr.is_ok() {
                    Ok(())
                } else {
                    Err(OpcError::Com {
                        source: windows::core::Error::from_hresult(hr),
                    })
                }
            })
            .collect();

        Ok(results)
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

// ── Reusable Pure-Rust Mock Infrastructure ──────────────────────────

/// Shared state flags controlling mock failure injection and connection monitoring.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct MockState {
    /// Number of successful connection invocations.
    pub connect_count: std::sync::atomic::AtomicUsize,
    /// Injects failure on server connection and server enumeration.
    pub should_fail_connect: std::sync::atomic::AtomicBool,
    /// Injects write errors on item write operations.
    pub should_fail_write: std::sync::atomic::AtomicBool,
    /// Simulates general connection drop errors.
    pub should_fail_connection: std::sync::atomic::AtomicBool,
    /// Simulates RPC server unavailable error (0x800706BA) triggering connection eviction.
    pub should_fail_with_connection_error: std::sync::atomic::AtomicBool,
    /// Simulates worker thread panic on request handling.
    pub should_panic_on_request: std::sync::atomic::AtomicBool,
    /// Number of times remove_group has been invoked.
    pub remove_group_count: std::sync::atomic::AtomicUsize,
}

/// Pure-Rust mock implementation of [`ConnectedGroup`] for testing.
///
/// Supports customizable closures for `add_items`, `read`, and `write` operations.
#[cfg(any(test, feature = "test-support"))]
pub struct MockConnectedGroup {
    pub state: std::sync::Arc<MockState>,
    pub add_items_fn:
        Option<Box<dyn Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync>>,
    pub read_fn: Option<
        Box<
            dyn Fn(DataSource, &[ItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
                + Send
                + Sync,
        >,
    >,
    pub write_fn: Option<
        Box<
            dyn Fn(&[ItemHandle], &[OpcValue]) -> OpcResult<Vec<Result<(), OpcError>>>
                + Send
                + Sync,
        >,
    >,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for MockConnectedGroup {
    fn default() -> Self {
        Self {
            state: std::sync::Arc::new(MockState::default()),
            add_items_fn: None,
            read_fn: None,
            write_fn: None,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ConnectedGroup for MockConnectedGroup {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        if let Some(f) = &self.add_items_fn {
            f(items)
        } else {
            Ok(items
                .iter()
                .enumerate()
                .map(|(i, _)| GroupItemResult {
                    server_handle: ItemHandle((i + 1) as u32),
                    canonical_type: 8,
                    error: None,
                })
                .collect())
        }
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        if let Some(f) = &self.read_fn {
            f(source, server_handles)
        } else {
            Ok(server_handles
                .iter()
                .map(|&h| {
                    Ok(GroupItemState {
                        client_handle: h,
                        value: OpcValue::Int(42),
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        }
    }

    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        if self
            .state
            .should_fail_connection
            .load(std::sync::atomic::Ordering::Relaxed)
            || self
                .state
                .should_fail_with_connection_error
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            // RPC server unavailable (0x800706BA) triggers connection eviction
            return Err(OpcError::Com {
                source: windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800706BA_u32 as i32,
                )),
            });
        }

        if self
            .state
            .should_fail_write
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(server_handles
                .iter()
                .map(|_| {
                    Err(OpcError::Com {
                        source: windows::core::Error::from_hresult(
                            windows::Win32::Foundation::E_FAIL,
                        ),
                    })
                })
                .collect());
        }

        if let Some(f) = &self.write_fn {
            f(server_handles, values)
        } else {
            Ok(server_handles.iter().map(|_| Ok(())).collect())
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ConnectedGroup for std::sync::Arc<MockConnectedGroup> {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        (**self).add_items(items)
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        (**self).read(source, server_handles)
    }

    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        (**self).write(server_handles, values)
    }
}

/// Pure-Rust mock implementation of [`ConnectedServer`] for testing.
///
/// Supports in-memory tag browsing via [`StringIterator::from_vec`] and configurable group handling.
#[cfg(any(test, feature = "test-support"))]
pub struct MockConnectedServer {
    /// Mock group associated with this server instance.
    pub group: std::sync::Arc<MockConnectedGroup>,
    /// Shared failure injection state.
    pub state: std::sync::Arc<MockState>,
    /// Flag indicating if connection drop should be simulated.
    pub should_fail_connection: std::sync::atomic::AtomicBool,
    /// Simulated tag IDs yielded during browse operations.
    pub tags: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Namespace organization type (1 = Hierarchical, 2 = Flat).
    pub organization: std::sync::atomic::AtomicU32,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for MockConnectedServer {
    fn default() -> Self {
        let state = std::sync::Arc::new(MockState::default());
        Self {
            group: std::sync::Arc::new(MockConnectedGroup {
                state: state.clone(),
                add_items_fn: None,
                read_fn: None,
                write_fn: None,
            }),
            state,
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ConnectedServer for MockConnectedServer {
    type Group = std::sync::Arc<MockConnectedGroup>;

    fn query_organization(&self) -> OpcResult<u32> {
        Ok(self.organization.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn browse_opc_item_ids(
        &self,
        _browse_type: BrowseType,
        _filter: Option<&str>,
        _data_type: u16,
        _access_rights: u32,
    ) -> OpcResult<StringIterator> {
        let tags = self.tags.lock()?;
        Ok(StringIterator::from_vec(tags.clone()))
    }

    fn change_browse_position(&self, _direction: BrowseDirection, _name: &str) -> OpcResult<()> {
        Ok(())
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        Ok(item_name.to_string())
    }

    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        if self
            .state
            .should_panic_on_request
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            std::panic::panic_any("Simulated worker panic");
        }

        if self
            .should_fail_connection
            .load(std::sync::atomic::Ordering::Relaxed)
            || self
                .state
                .should_fail_connection
                .load(std::sync::atomic::Ordering::Relaxed)
            || self
                .state
                .should_fail_with_connection_error
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            // RPC server unavailable (0x800706BA) triggers connection eviction
            return Err(OpcError::Com {
                source: windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800706BA_u32 as i32,
                )),
            });
        }

        Ok(CreatedGroup {
            group: self.group.clone(),
            server_handle: GroupHandle(1),
            revised_update_rate_ms: config.update_rate_ms,
        })
    }

    fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
        self.state
            .remove_group_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// Pure-Rust mock implementation of [`ServerConnector`] for testing and test-support.
///
/// Exports configurable server enumeration and mock server connections without Windows COM interfaces.
#[cfg(any(test, feature = "test-support"))]
pub struct MockServerConnector {
    /// Mock server yielded on connection.
    pub server: std::sync::Arc<MockConnectedServer>,
    /// Shared failure injection state.
    pub state: std::sync::Arc<MockState>,
    /// Simulated server ProgIDs returned by server enumeration.
    pub servers: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Simulated server details returned by structured server enumeration.
    pub server_details: std::sync::Arc<std::sync::Mutex<Vec<OpcServerInfo>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for MockServerConnector {
    fn default() -> Self {
        let state = std::sync::Arc::new(MockState::default());
        let server = std::sync::Arc::new(MockConnectedServer {
            group: std::sync::Arc::new(MockConnectedGroup {
                state: state.clone(),
                add_items_fn: None,
                read_fn: None,
                write_fn: None,
            }),
            state: state.clone(),
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        });
        let default_details = vec![OpcServerInfo {
            prog_id: "Matrikon.OPC.Simulation.1".to_string(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Matrikon OPC Simulation Server".to_string()),
            host: None,
        }];
        Self {
            server,
            state,
            servers: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Matrikon.OPC.Simulation.1".to_string(),
            ])),
            server_details: std::sync::Arc::new(std::sync::Mutex::new(default_details)),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl MockServerConnector {
    /// Creates a new `MockServerConnector` with default simulation settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `MockServerConnector` with the provided shared mock state.
    ///
    /// # Arguments
    /// * `state` - Shared atomic flags controlling mock failure injection.
    #[must_use]
    pub fn with_state(state: std::sync::Arc<MockState>) -> Self {
        let server = std::sync::Arc::new(MockConnectedServer {
            group: std::sync::Arc::new(MockConnectedGroup {
                state: state.clone(),
                add_items_fn: None,
                read_fn: None,
                write_fn: None,
            }),
            state: state.clone(),
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        });
        let default_details = vec![OpcServerInfo {
            prog_id: "Mock.Server.1".to_string(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Mock Server 1".to_string()),
            host: None,
        }];
        Self {
            server,
            state,
            servers: std::sync::Arc::new(std::sync::Mutex::new(vec!["Mock.Server.1".to_string()])),
            server_details: std::sync::Arc::new(std::sync::Mutex::new(default_details)),
        }
    }

    /// Overrides simulated tag IDs returned during browse operations.
    ///
    /// # Arguments
    /// * `tags` - Vector of tag identifier strings.
    #[must_use]
    pub fn with_tags(self, tags: Vec<String>) -> Self {
        if let Ok(mut guard) = self.server.tags.lock() {
            *guard = tags;
        }
        self
    }

    /// Overrides simulated server ProgIDs returned during enumeration.
    ///
    /// Also synchronizes synthesized [`OpcServerInfo`] records into `server_details`.
    ///
    /// # Arguments
    /// * `servers` - Vector of server ProgID strings.
    #[must_use]
    pub fn with_servers(self, servers: Vec<String>) -> Self {
        let synthesized: Vec<OpcServerInfo> = servers
            .iter()
            .map(|s| OpcServerInfo {
                prog_id: s.clone(),
                clsid: windows::core::GUID::zeroed(),
                user_type: None,
                host: None,
            })
            .collect();
        if let Ok(mut guard) = self.servers.lock() {
            *guard = servers;
        }
        if let Ok(mut guard) = self.server_details.lock() {
            *guard = synthesized;
        }
        self
    }

    /// Overrides simulated structured server details returned during enumeration.
    ///
    /// Also synchronizes the ProgIDs into `servers`.
    ///
    /// # Arguments
    /// * `details` - Vector of [`OpcServerInfo`] records.
    #[must_use]
    pub fn with_server_details(self, details: Vec<OpcServerInfo>) -> Self {
        let prog_ids: Vec<String> = details.iter().map(|d| d.prog_id.clone()).collect();
        if let Ok(mut guard) = self.server_details.lock() {
            *guard = details;
        }
        if let Ok(mut guard) = self.servers.lock() {
            *guard = prog_ids;
        }
        self
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ServerConnector for MockServerConnector {
    type Server = std::sync::Arc<MockConnectedServer>;

    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        let servers = self.servers.lock()?;
        Ok(servers.clone())
    }

    fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        let details = self.server_details.lock()?;
        Ok(details.clone())
    }

    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        self.connect(&identifier.to_string())
    }

    fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Connection("Mock connection failed".into()));
        }

        self.state
            .connect_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self.server.clone())
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ConnectedServer for std::sync::Arc<MockConnectedServer> {
    type Group = std::sync::Arc<MockConnectedGroup>;

    fn query_organization(&self) -> OpcResult<u32> {
        (**self).query_organization()
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> OpcResult<StringIterator> {
        (**self).browse_opc_item_ids(browse_type, filter, data_type, access_rights)
    }

    fn change_browse_position(&self, direction: BrowseDirection, name: &str) -> OpcResult<()> {
        (**self).change_browse_position(direction, name)
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        (**self).get_item_id(item_name)
    }

    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        (**self).add_group(config)
    }

    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()> {
        (**self).remove_group(server_group, force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_group_defaults() {
        let group = MockConnectedGroup::default();
        let defs = vec![
            GroupItemDef {
                item_id: "Random.Int4".to_string(),
                client_handle: ItemHandle(0),
                active: true,
            },
            GroupItemDef {
                item_id: "Random.Real8".to_string(),
                client_handle: ItemHandle(1),
                active: true,
            },
        ];

        let results = group.add_items(&defs).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].server_handle, ItemHandle(1));
        assert!(results[0].error.is_none());
        assert_eq!(results[1].server_handle, ItemHandle(2));
        assert!(results[1].error.is_none());

        let states = group
            .read(DataSource::Device, &[ItemHandle(1), ItemHandle(2)])
            .unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].as_ref().unwrap().client_handle, ItemHandle(1));
        assert_eq!(states[0].as_ref().unwrap().value, OpcValue::Int(42));
        assert_eq!(states[0].as_ref().unwrap().quality, OpcQuality::GOOD);

        let write_res = group
            .write(&[ItemHandle(1)], &[OpcValue::Int(100)])
            .unwrap();
        assert_eq!(write_res.len(), 1);
        assert!(write_res[0].is_ok());
    }

    #[test]
    fn test_mock_group_custom_handlers() {
        let mut group = MockConnectedGroup::default();
        group.read_fn = Some(Box::new(|source, handles| {
            assert_eq!(source, DataSource::Cache);
            Ok(handles
                .iter()
                .map(|&h| {
                    Ok(GroupItemState {
                        client_handle: h,
                        value: OpcValue::Float(3.14),
                        quality: OpcQuality::UNCERTAIN,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        }));

        let states = group.read(DataSource::Cache, &[ItemHandle(99)]).unwrap();
        assert_eq!(states.len(), 1);
        let s = states[0].as_ref().unwrap();
        assert_eq!(s.client_handle, ItemHandle(99));
        assert_eq!(s.value, OpcValue::Float(3.14));
        assert_eq!(s.quality, OpcQuality::UNCERTAIN);
    }

    #[test]
    fn test_mock_server_add_group_and_eviction() {
        let server = MockConnectedServer::default();
        let config = GroupConfig {
            name: "test_group",
            active: true,
            update_rate_ms: 500,
            client_handle: GroupHandle(10),
            time_bias: 0,
            percent_deadband: 0.0,
            locale_id: 0,
        };

        let created = server.add_group(&config).unwrap();
        assert_eq!(created.server_handle, GroupHandle(1));
        assert_eq!(created.revised_update_rate_ms, 500);

        server
            .should_fail_connection
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(server.add_group(&config).is_err());
    }

    #[test]
    fn test_group_item_def_and_state_cloning() {
        let def = GroupItemDef {
            item_id: "Tag1".to_string(),
            client_handle: ItemHandle(42),
            active: true,
        };
        let cloned_def = def.clone();
        assert_eq!(def, cloned_def);

        let state = GroupItemState {
            client_handle: ItemHandle(42),
            value: OpcValue::Bool(true),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        };
        let cloned_state = state.clone();
        assert_eq!(state, cloned_state);
        assert_eq!(state.value.to_string(), "true");
    }

    #[test]
    fn test_mock_connector_browse() {
        let server = MockConnectedServer::default();
        let iter = server
            .browse_opc_item_ids(BrowseType::Leaf, None, 0, 0)
            .expect("MockConnectedServer should support browse");
        let tags: Vec<String> = iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert!(!tags.is_empty(), "Mock browse should return simulated tags");
    }

    #[test]
    fn test_guid_to_progid_zeroed_guid_returns_com_error() {
        let zeroed = windows::core::GUID::zeroed();
        let result = guid_to_progid(&zeroed);
        assert!(result.is_err());
        if let Err(OpcError::Com { .. }) = result {
            // Expected: structured COM error preserved
        } else {
            panic!("Expected OpcError::Com, got: {result:?}");
        }
    }

    #[test]
    fn test_mock_server_connector_server_details() {
        use crate::types::OpcServerInfo;
        let mock = MockServerConnector::new().with_server_details(vec![OpcServerInfo {
            prog_id: "Custom.Mock.1".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Custom Mock Title".into()),
            host: None,
        }]);
        let details = mock.enumerate_server_details("localhost").unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].display_name(), "Custom Mock Title");
    }
}
