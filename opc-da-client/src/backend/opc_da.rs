use crate::helpers::{
    filetime_to_string, friendly_com_hint, guid_to_progid, is_known_iterator_bug,
    opc_value_to_variant, quality_to_string, variant_to_string,
};
use crate::provider::{OpcProvider, OpcValue, TagValue, WriteResult};
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

/// Concrete [`OpcProvider`] implementation for Windows OPC DA.
///
/// Heavy-weight implementation that uses the `opc_da` crate for
/// native COM interop.
pub struct OpcDaWrapper;

impl OpcDaWrapper {
    /// Creates a new `OpcDaWrapper`.
    pub const fn new() -> Self {
        Self
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
                "Browse position corrupted: failed to navigate up from branch '{branch}'"
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
                Ok(item_id) => {
                    tracing::debug!(
                        browse_name = %browse_name,
                        item_id = %item_id,
                        "get_item_id resolved"
                    );
                    item_id
                }
                Err(e) => {
                    tracing::warn!(
                        browse_name = %browse_name,
                        error = ?e,
                        "get_item_id failed, using browse name as fallback"
                    );
                    browse_name
                }
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
            let _guard = crate::ComGuard::new()?;
            {
                let client = Client;
                let guid_iter = client
                    .get_servers()
                    .context("Failed to enumerate OPC DA servers from registry")?;

                let mut servers = Vec::new();
                for guid in guid_iter.flatten() {
                    // SAFETY: `opc_da::GUID` and `windows::core::GUID` have
                    // identical memory layout (128-bit, 4-2-2-8 fields).
                    // `transmute_copy` is used because the crates define
                    // distinct types with the same ABI.
                    // SAFETY: `opc_da::GUID` and `windows::core::GUID` are binary compatible
                    // 128-bit structures with identical field layouts.
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
            }
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
            // SAFETY: COM is initialized per-thread (MTA). This block runs
            // inside `spawn_blocking`, ensuring a dedicated OS thread.
            // `ComGuard` handles unconditional `CoUninitialize` on drop.
            let _guard = crate::ComGuard::new()?;
            {
                let opc_server = crate::helpers::connect_server(&server_name)?;

                let org = opc_server
                    .query_organization()
                    .context("Failed to query namespace organization")?;
                let mut tags = Vec::new();

                if org == OPC_NS_FLAT {
                    let enum_string = opc_server.browse_opc_item_ids(OPC_LEAF, Some(""), 0, 0)?;
                    let string_iter = StringIterator::new(enum_string);
                    for tag_res in string_iter {
                        if tags.len() >= max_tags {
                            break;
                        }
                        let tag =
                            tag_res.map_err(|e| anyhow::anyhow!("Tag iteration error: {e:?}"))?;
                        tags.push(tag.clone());
                        if let Ok(mut sink) = tags_sink.lock() {
                            sink.push(tag);
                        }
                        progress.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    browse_recursive(&opc_server, &mut tags, max_tags, &progress, &tags_sink, 0)?;
                }
                Ok(tags)
            }
        })
        .await?
    }

    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> Result<Vec<TagValue>> {
        let server_name = server.to_string();
        tokio::task::spawn_blocking(move || {
            let _guard = crate::ComGuard::new()?;
            {
                // SAFETY: `server_wide` null-termination and scope management.
                let opc_server = crate::helpers::connect_server(&server_name)?;

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

                // SAFETY: item_id_wides must outlive item_defs because
                // tagOPCITEMDEF.szItemID holds a raw pointer into each Vec.
                let item_id_wides: Vec<Vec<u16>> = tag_ids
                    .iter()
                    .map(|tag_id| tag_id.encode_utf16().chain(std::iter::once(0)).collect())
                    .collect();

                let item_defs: Vec<tagOPCITEMDEF> = item_id_wides
                    .iter()
                    .enumerate()
                    .map(|(idx, wide)| tagOPCITEMDEF {
                        szAccessPath: windows::core::PWSTR::null(),
                        szItemID: windows::core::PWSTR(wide.as_ptr().cast_mut()),
                        bActive: windows::Win32::Foundation::TRUE,
                        #[allow(clippy::cast_possible_truncation)]
                        hClient: idx as u32,
                        dwBlobSize: 0,
                        pBlob: std::ptr::null_mut(),
                        vtRequestedDataType: 0,
                        wReserved: 0,
                    })
                    .collect();

                let (results, errors) = group.add_items(&item_defs)?;
                let mut server_handles = Vec::new();
                let mut valid_indices = Vec::new();

                for (idx, (item_result, error)) in results
                    .as_slice()
                    .iter()
                    .zip(errors.as_slice().iter())
                    .enumerate()
                {
                    if error.is_ok() {
                        server_handles.push(item_result.hServer);
                        valid_indices.push(idx);
                    } else {
                        tracing::warn!(
                            tag = %tag_ids[idx],
                            error = ?error,
                            "read_tag_values: add_items rejected tag"
                        );
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

                    let (value_str, quality_str) = if read_error.is_ok() {
                        (
                            variant_to_string(&state.vDataValue),
                            quality_to_string(state.wQuality),
                        )
                    } else {
                        let hint = friendly_com_hint(&anyhow::anyhow!("{read_error:?}"));
                        let full_msg = hint.unwrap_or("Unknown OPC error");

                        tracing::warn!(
                            tag = %tag_ids[*idx],
                            error = ?read_error,
                            hint = %full_msg,
                            "read_tag_values: per-item read error"
                        );

                        ("Error".to_string(), format!("Bad — {full_msg}"))
                    };

                    tag_values.push(TagValue {
                        tag_id: tag_ids[*idx].clone(),
                        value: value_str,
                        quality: quality_str,
                        timestamp: filetime_to_string(state.ftTimeStamp),
                    });
                }

                let _ = opc_server.remove_group(server_handle, true);
                Ok(tag_values)
            }
        })
        .await?
    }

    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> Result<WriteResult> {
        let server_name = server.to_string();
        let tag = tag_id.to_string();
        tokio::task::spawn_blocking(move || {
            let _guard = crate::ComGuard::new()?;
            {
                // SAFETY: `server_wide` null-termination and scope management.
                let opc_server = crate::helpers::connect_server(&server_name)?;

                let mut revised_update_rate = 0u32;
                let mut server_handle = 0u32;
                let group = opc_server.add_group(
                    "opc-da-client-write",
                    true,
                    0,
                    1000,
                    0,
                    0,
                    0.0,
                    &mut revised_update_rate,
                    &mut server_handle,
                )?;

                // SAFETY: item_id_wide must outlive item_def because
                // tagOPCITEMDEF.szItemID holds a raw pointer into the Vec.
                let mut item_id_wide: Vec<u16> = tag.encode_utf16().chain(std::iter::once(0)).collect();
                let item_def = tagOPCITEMDEF {
                    szAccessPath: windows::core::PWSTR::null(),
                    szItemID: windows::core::PWSTR(item_id_wide.as_mut_ptr()),
                    bActive: windows::Win32::Foundation::TRUE,
                    hClient: 0,
                    dwBlobSize: 0,
                    pBlob: std::ptr::null_mut(),
                    vtRequestedDataType: 0,
                    wReserved: 0,
                };

                let (results, errors) = group.add_items(&[item_def])?;
                let item_res = results.as_slice().first().context("Server returned empty item results")?;
                let item_err = errors.as_slice().first().context("Server returned empty item errors")?;

                if let Err(e) = item_err.ok() {
                    tracing::warn!(server = %server_name, tag = %tag, error = ?e, "write_tag_value: failed to add tag to group");
                    return Ok(WriteResult {
                        tag_id: tag,
                        success: false,
                        error: Some(format!("Failed to add tag to group: {e:?}")),
                    });
                }

                let variant = opc_value_to_variant(&value);
                let write_errors = group.write(&[item_res.hServer], &[variant])?;
                let write_error = write_errors.as_slice().first().context("Server returned empty write errors")?;

                let write_result = if write_error.is_ok() {
                    tracing::info!(server = %server_name, tag = %tag, "write_tag_value: write completed successfully");
                    WriteResult {
                        tag_id: tag,
                        success: true,
                        error: None,
                    }
                } else {
                    let hint =
                        friendly_com_hint(&anyhow::anyhow!("{write_error:?}")).unwrap_or("");
                    tracing::error!(server = %server_name, tag = %tag, error = ?write_error, hint = %hint, "write_tag_value: server rejected write");
                    WriteResult {
                        tag_id: tag,
                        success: false,
                        error: Some(if hint.is_empty() {
                            format!("{write_error:?}")
                        } else {
                            format!("{write_error:?} ({hint})")
                        }),
                    }
                };

                let _ = opc_server.remove_group(server_handle, true);
                Ok(write_result)
            }
        })
        .await?
    }
}
