use crate::helpers::{
    filetime_to_string, friendly_com_hint, guid_to_progid, is_known_iterator_bug,
    quality_to_string, variant_to_string,
};
use crate::provider::{OpcProvider, TagValue};
use anyhow::{Context, Result};
use async_trait::async_trait;
use opc_da::client::v2::Client;
use opc_da::client::{
    BrowseServerAddressSpaceTrait, ClientTrait, ItemMgtTrait, ServerTrait, StringIterator,
    SyncIoTrait,
};
use opc_da_bindings::{
    OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_UP, OPC_DS_DEVICE, OPC_LEAF, OPC_NS_FLAT, tagOPCITEMDEF,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::System::Com::{
    CLSIDFromProgID, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
};
use windows::core::PCWSTR;

/// Concrete [`OpcProvider`] implementation using the `opc_da` crate.
///
/// Handles COM threading (MTA) and per-call initialization/teardown
/// via `tokio::task::spawn_blocking`.
pub struct OpcDaWrapper;

impl OpcDaWrapper {
    /// Creates a new `OpcDaWrapper`.
    pub fn new() -> Self {
        OpcDaWrapper
    }
}

impl Default for OpcDaWrapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Recursively browse the OPC DA hierarchical address space depth-first.
fn browse_recursive(
    server: &opc_da::client::v2::Server,
    tags: &mut Vec<String>,
    max_tags: usize,
    progress: &Arc<AtomicUsize>,
    tags_sink: &Arc<std::sync::Mutex<Vec<String>>>,
    depth: usize,
) -> Result<()> {
    const MAX_DEPTH: usize = 50;
    if depth > MAX_DEPTH || tags.len() >= max_tags {
        if depth > MAX_DEPTH {
            tracing::warn!(depth, "Max browse depth reached, truncating");
        }
        return Ok(());
    }

    let branch_enum = server
        .browse_opc_item_ids(OPC_BRANCH, Some(""), 0, 0)
        .context("Failed to browse branches at current position")?;

    let branch_iter = StringIterator::new(branch_enum);
    let branches: Vec<String> = branch_iter
        .filter_map(|r| match r {
            Ok(name) => Some(name),
            Err(e) => {
                if is_known_iterator_bug(&e) {
                    tracing::trace!(error = ?e, "Branch iteration: known crate bug, skipping");
                } else {
                    tracing::warn!(error = ?e, "Branch iteration error, skipping");
                }
                None
            }
        })
        .collect();

    for branch in branches {
        if tags.len() >= max_tags {
            break;
        }
        if let Err(e) = server.change_browse_position(OPC_BROWSE_DOWN, &branch) {
            tracing::warn!(branch = %branch, error = ?e, "Failed to enter branch, skipping");
            continue;
        }

        let recurse_result =
            browse_recursive(server, tags, max_tags, progress, tags_sink, depth + 1);

        if let Err(e) = server.change_browse_position(OPC_BROWSE_UP, "") {
            tracing::error!(branch = %branch, error = ?e, "Failed to navigate back up!");
            return Err(anyhow::anyhow!(
                "Browse position corrupted: failed to navigate up from branch '{}'",
                branch
            ));
        }
        recurse_result?;
    }

    if let Ok(leaf_enum) = server.browse_opc_item_ids(OPC_LEAF, Some(""), 0, 0) {
        let leaf_iter = StringIterator::new(leaf_enum);
        for leaf_res in leaf_iter {
            if tags.len() >= max_tags {
                break;
            }
            let browse_name = match leaf_res {
                Ok(name) => name,
                Err(e) => {
                    if is_known_iterator_bug(&e) {
                        tracing::trace!(error = ?e, "Leaf iteration: known crate bug, skipping");
                    } else {
                        tracing::warn!(error = ?e, "Leaf iteration error, skipping");
                    }
                    continue;
                }
            };

            let tag = match server.get_item_id(&browse_name) {
                Ok(item_id) => item_id,
                Err(_) => browse_name,
            };
            tags.push(tag.clone());
            if let Ok(mut sink) = tags_sink.lock() {
                sink.push(tag);
            }
            progress.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(())
}

#[async_trait]
impl OpcProvider for OpcDaWrapper {
    async fn list_servers(&self, _host: &str) -> Result<Vec<String>> {
        tokio::task::spawn_blocking(move || {
            let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let result = (|| {
                let client = Client;
                let guid_iter = client
                    .get_servers()
                    .context("Failed to enumerate OPC DA servers from registry")?;

                let mut servers = Vec::new();
                for guid in guid_iter.flatten() {
                    let win_guid: windows::core::GUID = unsafe { std::mem::transmute_copy(&guid) };
                    if win_guid == windows::core::GUID::zeroed() {
                        continue;
                    }

                    if let Ok(progid) = guid_to_progid(&win_guid)
                        && !progid.is_empty()
                    {
                        servers.push(progid);
                    }
                }
                servers.sort();
                servers.dedup();
                Ok(servers)
            })();
            if com_init.is_ok() {
                unsafe {
                    CoUninitialize();
                }
            }
            result
        })
        .await?
    }

    async fn browse_tags(
        &self,
        server: &str,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
        tags_sink: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Result<Vec<String>> {
        let server_name = server.to_string();
        tokio::task::spawn_blocking(move || {
            let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let result = (|| {
                let clsid_raw = unsafe {
                    let server_wide: Vec<u16> = server_name.encode_utf16().chain(std::iter::once(0)).collect();
                    CLSIDFromProgID(PCWSTR(server_wide.as_ptr())).with_context(|| {
                        format!("Failed to resolve ProgID '{}' to CLSID", server_name)
                    })?
                };
                let clsid = unsafe { std::mem::transmute_copy(&clsid_raw) };

                let client = Client;
                let opc_server = client
                    .create_server(clsid, opc_da::def::ClassContext::All)
                    .map_err(|e| {
                        let hint = friendly_com_hint(&anyhow::anyhow!("{:?}", e))
                            .unwrap_or("Check DCOM configuration and server status");
                        tracing::error!(error = ?e, server = %server_name, hint, "create_server failed");
                        e
                    })?;

                let org = opc_server.query_organization().context("Failed to query namespace organization")?;
                let mut tags = Vec::new();

                if org == OPC_NS_FLAT {
                    let enum_string = opc_server.browse_opc_item_ids(OPC_LEAF, Some(""), 0, 0)?;
                    let string_iter = StringIterator::new(enum_string);
                    for tag_res in string_iter {
                        if tags.len() >= max_tags { break; }
                        let tag = tag_res.map_err(|e| anyhow::anyhow!("Tag iteration error: {:?}", e))?;
                        tags.push(tag.clone());
                        if let Ok(mut sink) = tags_sink.lock() { sink.push(tag); }
                        progress.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    browse_recursive(&opc_server, &mut tags, max_tags, &progress, &tags_sink, 0)?;
                }
                Ok(tags)
            })();
            if com_init.is_ok() {
                unsafe { CoUninitialize(); }
            }
            result
        })
        .await?
    }

    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> Result<Vec<TagValue>> {
        let server_name = server.to_string();
        tokio::task::spawn_blocking(move || {
            let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let result = (|| {
                let clsid_raw = unsafe {
                    let server_wide: Vec<u16> = server_name
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    CLSIDFromProgID(PCWSTR(server_wide.as_ptr()))
                        .context("ProgID to CLSID failed")?
                };
                let clsid = unsafe { std::mem::transmute_copy(&clsid_raw) };

                let client = Client;
                let opc_server = client.create_server(clsid, opc_da::def::ClassContext::All)?;

                let mut revised_update_rate = 0u32;
                let mut server_handle = 0u32;
                let group = opc_server.add_group(
                    "opc-da-client-read",
                    true,
                    0,
                    1000,
                    0,
                    0,
                    0.0,
                    &mut revised_update_rate,
                    &mut server_handle,
                )?;

                let item_defs: Vec<tagOPCITEMDEF> = tag_ids
                    .iter()
                    .enumerate()
                    .map(|(idx, tag_id)| {
                        let item_id_wide: Vec<u16> =
                            tag_id.encode_utf16().chain(std::iter::once(0)).collect();
                        tagOPCITEMDEF {
                            szAccessPath: windows::core::PWSTR::null(),
                            szItemID: windows::core::PWSTR(item_id_wide.as_ptr() as *mut _),
                            bActive: windows::Win32::Foundation::TRUE,
                            hClient: idx as u32,
                            dwBlobSize: 0,
                            pBlob: std::ptr::null_mut(),
                            vtRequestedDataType: 0,
                            wReserved: 0,
                        }
                    })
                    .collect();

                let (results, errors) = group.add_items(&item_defs)?;
                let mut server_handles = Vec::new();
                let mut valid_indices = Vec::new();

                for (idx, (_item_result, error)) in results
                    .as_slice()
                    .iter()
                    .zip(errors.as_slice().iter())
                    .enumerate()
                {
                    if error.is_ok() {
                        server_handles.push(_item_result.hServer);
                        valid_indices.push(idx);
                    }
                }

                if server_handles.is_empty() {
                    return Err(anyhow::anyhow!("No valid items to read"));
                }

                let (item_states, read_errors) = group.read(OPC_DS_DEVICE, &server_handles)?;
                let mut tag_values = Vec::new();
                let item_states_slice = item_states.as_slice();
                let read_errors_slice = read_errors.as_slice();

                for (i, idx) in valid_indices.iter().enumerate() {
                    let state = &item_states_slice[i];
                    let read_error = &read_errors_slice[i];
                    let value_str = if read_error.is_ok() {
                        variant_to_string(&state.vDataValue)
                    } else {
                        format!("Error: {:?}", read_error)
                    };

                    tag_values.push(TagValue {
                        tag_id: tag_ids[*idx].clone(),
                        value: value_str,
                        quality: quality_to_string(state.wQuality),
                        timestamp: filetime_to_string(&state.ftTimeStamp),
                    });
                }

                let _ = opc_server.remove_group(server_handle, true);
                Ok(tag_values)
            })();
            if com_init.is_ok() {
                unsafe {
                    CoUninitialize();
                }
            }
            result
        })
        .await?
    }
}
