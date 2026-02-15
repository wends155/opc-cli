use crate::traits::OpcProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use opc_da::client::v2::Client;
use opc_da::client::{BrowseServerAddressSpaceTrait, ClientTrait, StringIterator};
use opc_da_bindings::{OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_UP, OPC_LEAF, OPC_NS_FLAT};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoInitializeEx, CoTaskMemFree, CoUninitialize, ProgIDFromCLSID,
    COINIT_MULTITHREADED,
};

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
    } else if msg.contains("0x800706F4") {
        Some("COM marshalling error — try restarting the OPC server")
    } else if msg.contains("0x80040154") {
        Some("Server is not registered on this machine")
    } else {
        None
    }
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
                tracing::warn!(error = ?e, "Branch iteration error, skipping");
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
                        tracing::warn!(error = ?e, "Leaf iteration error, skipping");
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
                tracing::debug!("Resolving ProgID '{}' to CLSID...", server_name);
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
                    "CLSIDFromProgID complete"
                );

                // Convert windows::core::GUID to opc_da GUID
                let clsid = unsafe { std::mem::transmute_copy(&clsid_raw) };

                // 2. Create server instance
                let t1 = Instant::now();
                tracing::debug!(
                    "Creating OPC server instance (LocalServer) for GUID {:?}...",
                    clsid
                );
                let client = Client;
                let opc_server = client
                    .create_server(clsid, opc_da::def::ClassContext::All)
                    .map_err(|e| {
                        tracing::error!(error = ?e, server = %server_name, "create_server failed");
                        e
                    })
                    .with_context(|| {
                        format!("Failed to create OPC server instance for '{}'", server_name)
                    })?;
                tracing::info!(
                    elapsed_ms = t1.elapsed().as_millis(),
                    server = %server_name,
                    "create_server complete"
                );

                // 3. Detect namespace organization
                let t2 = Instant::now();
                let org = opc_server
                    .query_organization()
                    .map_err(|e| {
                        tracing::warn!(error = ?e, server = %server_name, "query_organization failed");
                        e
                    })
                    .context("Failed to query namespace organization")?;
                tracing::info!(
                    elapsed_ms = t2.elapsed().as_millis(),
                    organization = ?org,
                    server = %server_name,
                    "query_organization complete"
                );

                let mut tags = Vec::new();
                let t3 = Instant::now();

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
                    "Tag enumeration complete"
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
