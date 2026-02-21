//! OPC DA 1.0 server and group implementations.
//!
//! This module implements the original OPC DA 1.0 interfaces for servers and groups,
//! providing basic functionality for legacy systems.

use windows::core::Interface as _;

use super::{
    ClientTrait,
    traits::{
        AsyncIoTrait, BrowseServerAddressSpaceTrait, DataObjectTrait, GroupStateMgtTrait,
        ItemMgtTrait, PublicGroupStateMgtTrait, ServerPublicGroupsTrait, ServerTrait, SyncIoTrait,
    },
};

/// Client for OPC DA 1.0 servers.
#[derive(Debug)]
pub struct Client;

impl ClientTrait<Server> for Client {
    const CATALOG_ID: windows::core::GUID = crate::bindings::da::CATID_OPCDAServer10::IID;
}

/// An OPC DA 1.0 server implementation.
///
/// Provides access to OPC DA 1.0 server interfaces including:
/// - `IOPCServer` for basic server operations
/// - `IOPCServerPublicGroups` for public group management
/// - `IOPCBrowseServerAddressSpace` for browsing the address space
pub struct Server {
    pub(crate) server: crate::bindings::da::IOPCServer,
    pub(crate) server_public_groups: Option<crate::bindings::da::IOPCServerPublicGroups>,
    pub(crate) browse_server_address_space:
        Option<crate::bindings::da::IOPCBrowseServerAddressSpace>,
}

impl TryFrom<windows::core::IUnknown> for Server {
    type Error = windows::core::Error;

    fn try_from(value: windows::core::IUnknown) -> windows::core::Result<Self> {
        Ok(Self {
            server: value.cast()?,
            server_public_groups: value.cast().ok(),
            browse_server_address_space: value.cast().ok(),
        })
    }
}

impl ServerTrait<Group> for Server {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCServer> {
        Ok(&self.server)
    }
}

impl ServerPublicGroupsTrait for Server {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCServerPublicGroups> {
        self.server_public_groups.as_ref().ok_or_else(|| {
            windows::core::Error::new(
                windows::Win32::Foundation::E_NOTIMPL,
                "IOPCServerPublicGroups not supported",
            )
        })
    }
}

impl BrowseServerAddressSpaceTrait for Server {
    fn interface(
        &self,
    ) -> windows::core::Result<&crate::bindings::da::IOPCBrowseServerAddressSpace> {
        self.browse_server_address_space.as_ref().ok_or_else(|| {
            windows::core::Error::new(
                windows::Win32::Foundation::E_NOTIMPL,
                "IOPCBrowseServerAddressSpace not supported",
            )
        })
    }
}

/// Iterator over OPC DA 1.0 groups.
pub type GroupIterator = super::GroupIterator<Group>;

/// An OPC DA 1.0 group implementation.
///
/// Provides access to OPC DA 1.0 group interfaces including:
/// - `IOPCItemMgt` for item management
/// - `IOPCGroupStateMgt` for group state management
/// - `IOPCPublicGroupStateMgt` for public group operations
/// - `IOPCSyncIO` for synchronous operations
/// - `IOPCAsyncIO` for asynchronous operations
/// - `IDataObject` for data transfer
pub struct Group {
    pub(crate) item_mgt: crate::bindings::da::IOPCItemMgt,
    pub(crate) group_state_mgt: crate::bindings::da::IOPCGroupStateMgt,
    pub(crate) public_group_state_mgt: Option<crate::bindings::da::IOPCPublicGroupStateMgt>,
    pub(crate) sync_io: crate::bindings::da::IOPCSyncIO,
    pub(crate) async_io: crate::bindings::da::IOPCAsyncIO,
    pub(crate) data_object: windows::Win32::System::Com::IDataObject,
}

impl TryFrom<windows::core::IUnknown> for Group {
    type Error = windows::core::Error;

    fn try_from(value: windows::core::IUnknown) -> windows::core::Result<Self> {
        Ok(Self {
            item_mgt: value.cast()?,
            group_state_mgt: value.cast()?,
            public_group_state_mgt: value.cast().ok(),
            sync_io: value.cast()?,
            async_io: value.cast()?,
            data_object: value.cast()?,
        })
    }
}

impl ItemMgtTrait for Group {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCItemMgt> {
        Ok(&self.item_mgt)
    }
}

impl GroupStateMgtTrait for Group {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCGroupStateMgt> {
        Ok(&self.group_state_mgt)
    }
}

impl PublicGroupStateMgtTrait for Group {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCPublicGroupStateMgt> {
        self.public_group_state_mgt.as_ref().ok_or_else(|| {
            windows::core::Error::new(
                windows::Win32::Foundation::E_NOTIMPL,
                "IOPCPublicGroupStateMgt not supported",
            )
        })
    }
}

impl SyncIoTrait for Group {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCSyncIO> {
        Ok(&self.sync_io)
    }
}

impl AsyncIoTrait for Group {
    fn interface(&self) -> windows::core::Result<&crate::bindings::da::IOPCAsyncIO> {
        Ok(&self.async_io)
    }
}

impl DataObjectTrait for Group {
    fn interface(&self) -> windows::core::Result<&windows::Win32::System::Com::IDataObject> {
        Ok(&self.data_object)
    }
}
