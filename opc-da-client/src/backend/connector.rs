pub use crate::bindings::da::tagOPCITEMDEF;
use crate::opc_da::client::StringIterator;
pub use crate::bindings::da::{tagOPCITEMSTATE, tagOPCITEMRESULT};
pub use crate::opc_da::utils::RemoteArray;
pub use windows::Win32::System::Variant::VARIANT;

pub trait ServerConnector: Send + Sync {
    type Server: ConnectedServer;

    fn enumerate_servers(&self) -> anyhow::Result<Vec<String>>;
    fn connect(&self, server_name: &str) -> anyhow::Result<Self::Server>;
}

pub trait ConnectedServer {
    type Group: ConnectedGroup;

    fn query_organization(&self) -> anyhow::Result<u32>;
    
    fn browse_opc_item_ids(
        &self,
        browse_type: u32,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> anyhow::Result<StringIterator>;
    
    fn change_browse_position(&self, direction: u32, name: &str) -> anyhow::Result<()>;
    
    fn get_item_id(&self, item_name: &str) -> anyhow::Result<String>;

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
    
    fn remove_group(&self, server_group: u32, force: bool) -> anyhow::Result<()>;
}

pub trait ConnectedGroup {
    fn add_items(
        &self,
        items: &[tagOPCITEMDEF],
    ) -> anyhow::Result<(
        RemoteArray<tagOPCITEMRESULT>,
        RemoteArray<windows::core::HRESULT>,
    )>;

    fn read(
        &self,
        source: crate::bindings::da::tagOPCDATASOURCE,
        server_handles: &[u32]
    ) -> anyhow::Result<(
        RemoteArray<tagOPCITEMSTATE>,
        RemoteArray<windows::core::HRESULT>,
    )>;

    fn write(
        &self,
        server_handles: &[u32],
        values: &[VARIANT],
    ) -> anyhow::Result<RemoteArray<windows::core::HRESULT>>;
}

use crate::opc_da::client::v2::{Client, Server, Group};
use crate::opc_da::client::{ServerTrait, BrowseServerAddressSpaceTrait, ClientTrait, ItemMgtTrait, SyncIoTrait};
use anyhow::Context;

pub struct ComConnector;

impl ServerConnector for ComConnector {
    type Server = ComServer;

    fn enumerate_servers(&self) -> anyhow::Result<Vec<String>> {
        let client = Client;
        let guid_iter = client.get_servers().context("Failed to enumerate OPC DA servers from registry")?;
        
        let mut servers = Vec::new();
        for guid in guid_iter.flatten() {
            let win_guid: windows::core::GUID = unsafe { std::mem::transmute_copy(&guid) };
            if win_guid == windows::core::GUID::zeroed() {
                continue;
            }

            if let Ok(progid) = crate::helpers::guid_to_progid(&win_guid) {
                if !progid.is_empty() {
                    servers.push(progid);
                }
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

pub struct ComServer(Server);

impl ConnectedServer for ComServer {
    type Group = ComGroup;

    fn query_organization(&self) -> anyhow::Result<u32> {
        Ok(self.0.query_organization()?.0 as u32)
    }
    
    fn browse_opc_item_ids(
        &self,
        browse_type: u32,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> anyhow::Result<StringIterator> {
        let enum_str = self.0.browse_opc_item_ids(crate::bindings::da::tagOPCBROWSETYPE(browse_type as i32), filter, data_type, access_rights)?;
        Ok(StringIterator::new(enum_str))
    }
    
    fn change_browse_position(&self, direction: u32, name: &str) -> anyhow::Result<()> {
        Ok(self.0.change_browse_position(crate::bindings::da::tagOPCBROWSEDIRECTION(direction as i32), name)?)
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
            name, active, client_handle, update_rate, locale_id, time_bias, percent_deadband, revised_update_rate, server_handle
        )?;
        Ok(ComGroup(group))
    }
    
    fn remove_group(&self, server_group: u32, force: bool) -> anyhow::Result<()> {
        Ok(self.0.remove_group(server_group, force)?)
    }
}

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
        server_handles: &[u32]
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
