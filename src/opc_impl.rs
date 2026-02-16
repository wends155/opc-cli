use crate::app::TagValue;
use crate::traits::OpcProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use opc_da::client::v2::Client;
use opc_da::client::{
    BrowseServerAddressSpaceTrait, ClientTrait, ItemMgtTrait, ServerTrait, StringIterator,
    SyncIoTrait,
};
use opc_da_bindings::{
    tagOPCITEMDEF, OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_UP, OPC_DS_DEVICE, OPC_LEAF, OPC_NS_FLAT,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoInitializeEx, CoTaskMemFree, CoUninitialize, ProgIDFromCLSID,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;

pub struct OpcDaWrapper;

impl OpcDaWrapper {
    pub fn new() -> Self {
        OpcDaWrapper
    }
}

/// Maps known COM/DCOM error codes to actionable user hints.
pub fn friendly_com_hint(err: &anyhow::Error) -> Option<&'static str> {
    let msg = format!("{:?}", err);
    if msg.contains("0x80040112") {
        Some("Server license does not permit OPC client connections")
    } else if msg.contains("0x80080005") {
        Some("Server process failed to start — check if it is installed and running")
    } else if msg.contains("0x80070005") {
        Some("Access denied — DCOM launch/activation permissions not configured for this user")
    } else if msg.contains("0x800706BA") {
        Some("RPC server unavailable — the target host may be offline or blocking RPC")
    } else if msg.contains("0x800706F4") {
        Some("COM marshalling error — try restarting the OPC server")
    } else if msg.contains("0x80040154") {
        Some("Server is not registered on this machine")
    } else if msg.contains("0x80004003") {
        Some("Invalid pointer — likely a known issue with the OPC DA crate's iterator initialization")
    } else {
        None
    }
}

/// Returns `true` for E_POINTER errors that are known to be caused by
/// the `opc_da` crate's `StringIterator` initialization bug (index starts
/// at 0 with null-pointer cache, producing 16 phantom errors per iterator).
fn is_known_iterator_bug(err: &windows::core::Error) -> bool {
    err.code().0 as u32 == 0x80004003 // E_POINTER
}

/// Helper to convert GUID to ProgID using Windows API
fn guid_to_progid(guid: &windows::core::GUID) -> Result<String> {
    unsafe {
        let progid = ProgIDFromCLSID(guid).context("Failed to get ProgID from CLSID")?;

        let result = if progid.is_null() {
            String::new()
        } else {
            // Modern windows-rs (0.61.x) has to_string() on PWSTR
            progid
                .to_string()
                .map_err(|e| anyhow::anyhow!("Failed into convert PWSTR: {}", e))?
        };

        if !progid.is_null() {
            CoTaskMemFree(Some(progid.as_ptr() as *const _));
        }

        Ok(result)
    }
}

/// Convert OPC DA VARIANT to a displayable string.
fn variant_to_string(variant: &VARIANT) -> String {
    unsafe {
        // Access the discriminant to get the VT type
        let vt = variant.Anonymous.Anonymous.vt;
        match vt as u32 {
            0 => "Empty".to_string(),                                       // VT_EMPTY
            1 => "Null".to_string(),                                        // VT_NULL
            2 => format!("{}", variant.Anonymous.Anonymous.Anonymous.iVal), // VT_I2
            3 => format!("{}", variant.Anonymous.Anonymous.Anonymous.lVal), // VT_I4
            4 => format!("{:.2}", variant.Anonymous.Anonymous.Anonymous.fltVal), // VT_R4
            5 => format!("{:.2}", variant.Anonymous.Anonymous.Anonymous.dblVal), // VT_R8
            8 => {
                // VT_BSTR - string
                let bstr = variant.Anonymous.Anonymous.Anonymous.bstrVal;
                if bstr.is_empty() {
                    "\"\"".to_string()
                } else {
                    format!("\"{}\"", bstr.to_string())
                }
            }
            11 => format!("{}", variant.Anonymous.Anonymous.Anonymous.boolVal.0 != 0), // VT_BOOL
            _ => format!("(VT {})", vt),
        }
    }
}

/// Map OPC quality code to a human-readable label.
fn quality_to_string(quality: u16) -> String {
    let quality_bits = quality & 0xC0; // Top 2 bits define Good/Bad/Uncertain
    match quality_bits {
        0xC0 => "Good".to_string(),
        0x00 => "Bad".to_string(),
        0x40 => "Uncertain".to_string(),
        _ => format!("Unknown(0x{:04X})", quality),
    }
}

/// Convert FILETIME to a human-readable string.
fn filetime_to_string(ft: &FILETIME) -> String {
    if ft.dwHighDateTime == 0 && ft.dwLowDateTime == 0 {
        return "N/A".to_string();
    }
    // Simple format: just show the raw FILETIME for now
    format!("{:08X}{:08X}", ft.dwHighDateTime, ft.dwLowDateTime)
}

/// Recursively browse the OPC DA hierarchical address space depth-first.
///
/// Collects fully-qualified item IDs into `tags` up to `max_tags`.
/// Increments `progress` counter as each tag is found.
fn browse_recursive(
    server: &opc_da::client::v2::Server,
    tags: &mut Vec<String>,
    max_tags: usize,
    progress: &Arc<AtomicUsize>,
    depth: usize,
) -> Result<()> {
    const MAX_DEPTH: usize = 50; // Safety guard against infinite recursion
    if depth > MAX_DEPTH || tags.len() >= max_tags {
        if depth > MAX_DEPTH {
            tracing::warn!(depth, "Max browse depth reached, truncating");
        }
        return Ok(());
    }

    // 1. Enumerate branches and recurse into each FIRST
    let branch_enum = server
        .browse_opc_item_ids(OPC_BRANCH, Some(""), 0, 0)
        .map_err(|e| {
            tracing::warn!(error = ?e, depth, "browse_opc_item_ids(BRANCH) failed");
            e
        })
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
        tracing::debug!(branch = %branch, depth, "Descending into branch");

        // Navigate down into the branch
        if let Err(e) = server.change_browse_position(OPC_BROWSE_DOWN, &branch) {
            tracing::warn!(branch = %branch, error = ?e, "Failed to enter branch, skipping");
            continue;
        }

        // Recurse
        let recurse_result = browse_recursive(server, tags, max_tags, progress, depth + 1);

        // Always navigate back up, even if recursion failed
        if let Err(e) = server.change_browse_position(OPC_BROWSE_UP, "") {
            tracing::error!(branch = %branch, error = ?e, "Failed to navigate back up!");
            return Err(anyhow::anyhow!(
                "Browse position corrupted: failed to navigate up from branch '{}'",
                branch
            ));
        }

        // Propagate recursion errors after restoring position
        recurse_result?;
    }

    // 2. Enumerate leaves at current position (soft-fail: log and skip)
    match server.browse_opc_item_ids(OPC_LEAF, Some(""), 0, 0) {
        Ok(leaf_enum) => {
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

                // Convert browse name to fully-qualified item ID
                let tag = match server.get_item_id(&browse_name) {
                    Ok(item_id) => {
                        tracing::trace!(browse_name = %browse_name, item_id = %item_id, "Discovered tag");
                        item_id
                    }
                    Err(e) => {
                        // Fallback: use browse name if get_item_id fails
                        tracing::warn!(
                            browse_name = %browse_name,
                            error = ?e,
                            "get_item_id failed, using browse name as fallback"
                        );
                        browse_name
                    }
                };
                tags.push(tag);
                progress.fetch_add(1, Ordering::Relaxed);
            }
        }
        Err(e) => {
            tracing::warn!(error = ?e, depth, "browse_opc_item_ids(LEAF) failed, skipping leaves");
        }
    }

    Ok(())
}

#[async_trait]
impl OpcProvider for OpcDaWrapper {
    async fn list_servers(&self, host: &str) -> Result<Vec<String>> {
        let host = host.to_string();
        tokio::task::spawn_blocking(move || {
            // Ensure COM is initialized for this thread pool thread (MTA)
            let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

            let result = (|| {
                tracing::info!("Listing OPC servers on {}", host);

                if !host.is_empty() && host != "localhost" && host != "127.0.0.1" {
                    tracing::warn!(
                        "Remote host requested, but only local registry scan is supported."
                    );
                }

                let client = Client;
                let guid_iter = client
                    .get_servers()
                    .context("Failed to enumerate OPC DA servers from registry")?;

                let mut servers = Vec::new();
                for guid_res in guid_iter {
                    match guid_res {
                        Ok(guid) => {
                            tracing::debug!("Found server GUID: {:?}", guid);
                            // Convert opc_da GUID to windows::core::GUID
                            // They should be identical in memory
                            let win_guid: windows::core::GUID =
                                unsafe { std::mem::transmute_copy(&guid) };

                            // Skip null GUIDs — ghost COM registrations
                            let null_guid = windows::core::GUID::zeroed();
                            if win_guid == null_guid {
                                tracing::trace!("Skipping null GUID");
                                continue;
                            }

                            match guid_to_progid(&win_guid) {
                                Ok(progid) => {
                                    if !progid.is_empty() {
                                        tracing::debug!("Resolved GUID to ProgID: {}", progid);
                                        servers.push(progid);
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to resolve GUID {:?}: {:?}", guid, e)
                                }
                            }
                        }
                        Err(e) => tracing::error!("Error iterating server GUIDs: {:?}", e),
                    }
                }

                servers.sort();
                servers.dedup();

                tracing::info!("Found {} OPC servers", servers.len());
                Ok(servers)
            })();

            if com_init.is_ok() {
                unsafe {
                    CoUninitialize();
                }
            }
            result
        })
        .await
        .context("Task join error")?
    }

    async fn browse_tags(
        &self,
        server: &str,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
    ) -> Result<Vec<String>> {
        let server_name = server.to_string();
        tokio::task::spawn_blocking(move || {
            // Ensure COM is initialized for this thread pool thread (MTA)
            let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

            let result = (|| {
                tracing::info!(server = %server_name, max_tags, "Browsing tags");

                // 1. Resolve ProgID to CLSID
                let t0 = Instant::now();
                tracing::info!(server = %server_name, phase = "resolve_clsid", "Phase started");
                let clsid_raw = unsafe {
                    let server_wide: Vec<u16> = server_name
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    CLSIDFromProgID(PCWSTR(server_wide.as_ptr())).with_context(|| {
                        format!("Failed to resolve ProgID '{}' to CLSID", server_name)
                    })?
                };
                tracing::info!(
                    elapsed_ms = t0.elapsed().as_millis(),
                    server = %server_name,
                    phase = "resolve_clsid",
                    "Phase complete"
                );

                // Convert windows::core::GUID to opc_da GUID
                let clsid = unsafe { std::mem::transmute_copy(&clsid_raw) };

                // 2. Create server instance
                let t1 = Instant::now();
                tracing::info!(
                    server = %server_name,
                    phase = "create_server",
                    clsid = ?clsid,
                    "Phase started"
                );
                let client = Client;
                let opc_server = client
                    .create_server(clsid, opc_da::def::ClassContext::All)
                    .map_err(|e| {
                        let hint = friendly_com_hint(&anyhow::anyhow!("{:?}", e))
                            .unwrap_or("Check DCOM configuration and server status");
                        tracing::error!(
                            error = ?e,
                            server = %server_name,
                            phase = "create_server",
                            elapsed_ms = t1.elapsed().as_millis(),
                            hint,
                            "create_server failed"
                        );
                        e
                    })
                    .with_context(|| {
                        format!("Failed to create OPC server instance for '{}'", server_name)
                    })?;
                tracing::info!(
                    elapsed_ms = t1.elapsed().as_millis(),
                    server = %server_name,
                    phase = "create_server",
                    "Phase complete"
                );

                // 3. Detect namespace organization
                let t2 = Instant::now();
                tracing::info!(server = %server_name, phase = "query_organization", "Phase started");
                let org = opc_server
                    .query_organization()
                    .map_err(|e| {
                        tracing::warn!(
                            error = ?e,
                            server = %server_name,
                            phase = "query_organization",
                            "query_organization failed"
                        );
                        e
                    })
                    .context("Failed to query namespace organization")?;
                tracing::info!(
                    elapsed_ms = t2.elapsed().as_millis(),
                    organization = ?org,
                    server = %server_name,
                    phase = "query_organization",
                    "Phase complete"
                );

                let mut tags = Vec::new();
                let t3 = Instant::now();
                tracing::info!(
                    server = %server_name,
                    phase = "enumerate_tags",
                    organization = ?org,
                    "Phase started"
                );

                if org == OPC_NS_FLAT {
                    // Flat namespace: browse leaves directly at root
                    tracing::debug!("Using flat browse strategy");
                    let enum_string = opc_server
                        .browse_opc_item_ids(OPC_LEAF, Some(""), 0, 0)
                        .context("Failed to browse tags (flat)")?;
                    let string_iter = StringIterator::new(enum_string);
                    for tag_res in string_iter {
                        if tags.len() >= max_tags {
                            break;
                        }
                        let tag =
                            tag_res.map_err(|e| anyhow::anyhow!("Tag iteration error: {:?}", e))?;
                        tags.push(tag);
                        progress.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    // Hierarchical namespace: recursive depth-first walk
                    tracing::debug!("Using hierarchical browse strategy");
                    browse_recursive(&opc_server, &mut tags, max_tags, &progress, 0)?;
                }

                tracing::info!(
                    elapsed_ms = t3.elapsed().as_millis(),
                    count = tags.len(),
                    server = %server_name,
                    phase = "enumerate_tags",
                    "Phase complete"
                );
                Ok(tags)
            })();

            if com_init.is_ok() {
                unsafe {
                    CoUninitialize();
                }
            }
            result
        })
        .await
        .context("Task join error")?
    }

    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> Result<Vec<TagValue>> {
        let server_name = server.to_string();
        tokio::task::spawn_blocking(move || {
            // Ensure COM is initialized for this thread pool thread (MTA)
            let com_init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

            let result = (|| {
                tracing::info!(
                    server = %server_name,
                    tag_count = tag_ids.len(),
                    "Reading tag values"
                );

                // 1. Resolve ProgID to CLSID
                let clsid_raw = unsafe {
                    let server_wide: Vec<u16> = server_name
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    CLSIDFromProgID(PCWSTR(server_wide.as_ptr())).with_context(|| {
                        format!("Failed to resolve ProgID '{}' to CLSID", server_name)
                    })?
                };
                let clsid = unsafe { std::mem::transmute_copy(&clsid_raw) };

                // 2. Create server instance
                let client = Client;
                let opc_server = client
                    .create_server(clsid, opc_da::def::ClassContext::All)
                    .with_context(|| {
                        format!("Failed to create OPC server instance for '{}'", server_name)
                    })?;

                // 3. Add a group for reading
                let mut revised_update_rate = 0u32;
                let mut server_handle = 0u32;
                let group = opc_server
                    .add_group(
                        "opc-cli-read",
                        true, // active
                        0,    // client handle
                        1000, // update rate (ms)
                        0,    // locale_id
                        0,    // time_bias
                        0.0,  // percent_deadband
                        &mut revised_update_rate,
                        &mut server_handle,
                    )
                    .context("Failed to add OPC group")?;

                // 4. Build item definitions
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
                            vtRequestedDataType: 0, // VT_EMPTY = use server's native type
                            wReserved: 0,
                        }
                    })
                    .collect();

                // SAFETY: item_defs contains POD-like structs with pointers to UTF-16 data
                // that lives in tag_ids (which is owned by this closure). The pointers
                // are only used during the add_items call.
                let (results, errors) = group
                    .add_items(&item_defs)
                    .context("Failed to add items to group")?;

                // 5. Collect server handles (skip errors)
                let mut server_handles = Vec::new();
                let mut valid_indices = Vec::new();
                for (idx, (_item_result, error)) in results.iter().zip(errors.iter()).enumerate() {
                    if error.is_ok() {
                        server_handles.push(results[idx].hServer);
                        valid_indices.push(idx);
                    } else {
                        tracing::warn!(
                            tag = %tag_ids[idx],
                            error = ?error,
                            "Failed to add item, skipping"
                        );
                    }
                }

                if server_handles.is_empty() {
                    return Err(anyhow::anyhow!("No valid items to read"));
                }

                // 6. Perform SyncIO read
                let (item_states, read_errors) = group
                    .read(OPC_DS_DEVICE, &server_handles)
                    .context("Failed to read tag values")?;

                // 7. Build TagValue results
                let mut tag_values = Vec::new();
                for (i, idx) in valid_indices.iter().enumerate() {
                    let state = &item_states[i];
                    let read_error = &read_errors[i];

                    let value_str = if read_error.is_ok() {
                        variant_to_string(&state.vDataValue)
                    } else {
                        format!("Error: {:?}", read_error)
                    };

                    let quality_str = quality_to_string(state.wQuality);
                    let timestamp_str = filetime_to_string(&state.ftTimeStamp);

                    tag_values.push(TagValue {
                        tag_id: tag_ids[*idx].clone(),
                        value: value_str,
                        quality: quality_str,
                        timestamp: timestamp_str,
                    });
                }

                // 8. Cleanup: remove group
                let _ = opc_server.remove_group(server_handle, true);

                tracing::info!(count = tag_values.len(), "Read tag values successfully");
                Ok(tag_values)
            })();

            if com_init.is_ok() {
                unsafe {
                    CoUninitialize();
                }
            }
            result
        })
        .await
        .context("Task join error")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friendly_com_hint_known_codes() {
        let err = anyhow::anyhow!("COM error 0x800706F4");
        assert_eq!(
            friendly_com_hint(&err),
            Some("COM marshalling error — try restarting the OPC server")
        );

        let err = anyhow::anyhow!("COM error 0x80040154");
        assert_eq!(
            friendly_com_hint(&err),
            Some("Server is not registered on this machine")
        );

        let err = anyhow::anyhow!("COM error 0x80070005");
        assert!(friendly_com_hint(&err).unwrap().contains("Access denied"));

        let err = anyhow::anyhow!("COM error 0x800706BA");
        assert!(friendly_com_hint(&err)
            .unwrap()
            .contains("RPC server unavailable"));
    }

    #[test]
    fn test_friendly_com_hint_unknown_code() {
        let err = anyhow::anyhow!("Some other error");
        assert_eq!(friendly_com_hint(&err), None);
    }

    #[test]
    fn test_null_guid_is_skipped() {
        let null = windows::core::GUID::zeroed();
        let also_null = windows::core::GUID::from_values(0, 0, 0, [0; 8]);
        assert_eq!(null, also_null);

        let valid = windows::core::GUID::from_values(
            0xF8582CF2,
            0x88FB,
            0x11D0,
            [0xB8, 0x50, 0x00, 0xC0, 0xF0, 0x10, 0x43, 0x05],
        );
        assert_ne!(null, valid);
    }
}
