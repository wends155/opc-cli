//! OPC DA 3.0 server and group implementations.
//!
//! This module implements the OPC DA 3.0 interfaces for servers and groups,
//! providing access to the latest features of the OPC DA specification.

use windows::core::Interface as _;

use crate::opc_da::errors::{OpcError, OpcResult};

use super::{
    ClientTrait,
    traits::{
        AsyncIo2Trait, AsyncIo3Trait, BrowseTrait, CommonTrait, ConnectionPointContainerTrait,
        GroupStateMgt2Trait, GroupStateMgtTrait, ItemDeadbandMgtTrait, ItemIoTrait, ItemMgtTrait,
        ItemSamplingMgtTrait, ServerTrait, SyncIo2Trait, SyncIoTrait,
    },
};

/// Client for OPC DA 3.0 servers.
#[derive(Debug)]
pub struct Client;

impl ClientTrait<Server> for Client {
    const CATALOG_ID: windows::core::GUID = crate::bindings::da::CATID_OPCDAServer30::IID;
}

/// An OPC DA 3.0 server implementation.
///
/// Provides access to OPC DA 3.0 server interfaces including:
/// - `IOPCServer` for basic server operations
/// - `IOPCCommon` for server status and locale management
/// - `IOPCBrowse` for browsing the server address space
/// - `IOPCItemIO` for direct item read/write operations
pub struct Server {
    pub(crate) server: crate::bindings::da::IOPCServer,
    pub(crate) common: crate::bindings::comn::IOPCCommon,
    pub(crate) connection_point_container: windows::Win32::System::Com::IConnectionPointContainer,
    pub(crate) browse: crate::bindings::da::IOPCBrowse,
    pub(crate) item_io: crate::bindings::da::IOPCItemIO,
}

impl TryFrom<windows::core::IUnknown> for Server {
    type Error = windows::core::Error;

    fn try_from(value: windows::core::IUnknown) -> windows::core::Result<Self> {
        Ok(Self {
            server: value.cast()?,
            common: value.cast()?,
            connection_point_container: value.cast()?,
            browse: value.cast()?,
            item_io: value.cast()?,
        })
    }
}

impl ServerTrait<Group> for Server {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCServer> {
        Ok(&self.server)
    }
}

impl CommonTrait for Server {
    fn interface(&self) -> OpcResult<&crate::bindings::comn::IOPCCommon> {
        Ok(&self.common)
    }
}

impl ConnectionPointContainerTrait for Server {
    fn interface(&self) -> OpcResult<&windows::Win32::System::Com::IConnectionPointContainer> {
        Ok(&self.connection_point_container)
    }
}

impl BrowseTrait for Server {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCBrowse> {
        Ok(&self.browse)
    }
}

impl ItemIoTrait for Server {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCItemIO> {
        Ok(&self.item_io)
    }
}

/// Iterator over OPC DA 3.0 groups.
pub type GroupIterator = super::GroupIterator<Group>;

/// An OPC DA 3.0 group implementation.
///
/// Provides access to OPC DA 3.0 group interfaces including:
/// - `IOPCItemMgt` for item management
/// - `IOPCGroupStateMgt` and `IOPCGroupStateMgt2` for group state management
/// - `IOPCSyncIO` and `IOPCSyncIO2` for synchronous operations
/// - `IOPCAsyncIO2` and `IOPCAsyncIO3` for asynchronous operations
/// - `IOPCItemSamplingMgt` for item sampling control
/// - `IOPCItemDeadbandMgt` for deadband management
#[derive(Debug)]
pub struct Group {
    pub(crate) item_mgt: crate::bindings::da::IOPCItemMgt,
    pub(crate) group_state_mgt: crate::bindings::da::IOPCGroupStateMgt,
    pub(crate) group_state_mgt2: crate::bindings::da::IOPCGroupStateMgt2,
    pub(crate) sync_io: crate::bindings::da::IOPCSyncIO,
    pub(crate) sync_io2: crate::bindings::da::IOPCSyncIO2,
    pub(crate) async_io2: crate::bindings::da::IOPCAsyncIO2,
    pub(crate) async_io3: crate::bindings::da::IOPCAsyncIO3,
    pub(crate) item_sampling_mgt: Option<crate::bindings::da::IOPCItemSamplingMgt>,
    pub(crate) item_deadband_mgt: crate::bindings::da::IOPCItemDeadbandMgt,
    pub(crate) connection_point_container: windows::Win32::System::Com::IConnectionPointContainer,
}

impl TryFrom<windows::core::IUnknown> for Group {
    type Error = windows::core::Error;

    fn try_from(value: windows::core::IUnknown) -> windows::core::Result<Self> {
        Ok(Self {
            item_mgt: value.cast()?,
            group_state_mgt: value.cast()?,
            group_state_mgt2: value.cast()?,
            sync_io: value.cast()?,
            sync_io2: value.cast()?,
            async_io2: value.cast()?,
            async_io3: value.cast()?,
            item_deadband_mgt: value.cast()?,
            item_sampling_mgt: value.cast().ok(),
            connection_point_container: value.cast()?,
        })
    }
}

impl ItemMgtTrait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCItemMgt> {
        Ok(&self.item_mgt)
    }
}

impl GroupStateMgtTrait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCGroupStateMgt> {
        Ok(&self.group_state_mgt)
    }
}

impl GroupStateMgt2Trait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCGroupStateMgt2> {
        Ok(&self.group_state_mgt2)
    }
}

impl SyncIoTrait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCSyncIO> {
        Ok(&self.sync_io)
    }
}

impl SyncIo2Trait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCSyncIO2> {
        Ok(&self.sync_io2)
    }
}

impl AsyncIo2Trait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCAsyncIO2> {
        Ok(&self.async_io2)
    }
}

impl AsyncIo3Trait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCAsyncIO3> {
        Ok(&self.async_io3)
    }
}

impl ItemDeadbandMgtTrait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCItemDeadbandMgt> {
        Ok(&self.item_deadband_mgt)
    }
}

impl ItemSamplingMgtTrait for Group {
    fn interface(&self) -> OpcResult<&crate::bindings::da::IOPCItemSamplingMgt> {
        self.item_sampling_mgt.as_ref().ok_or_else(|| {
            OpcError::NotImplemented("IOPCItemSamplingMgt not supported".to_string())
        })
    }
}

impl ConnectionPointContainerTrait for Group {
    fn interface(&self) -> OpcResult<&windows::Win32::System::Com::IConnectionPointContainer> {
        Ok(&self.connection_point_container)
    }
}
