use crate::backend::connector::{ComConnector, ConnectedGroup, ConnectedServer, ServerConnector};
use crate::bindings::da::{
    OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_UP, OPC_DS_DEVICE, OPC_FLAT, OPC_LEAF, OPC_NS_FLAT,
    tagOPCITEMDEF,
};
use crate::helpers::{
    filetime_to_string, format_hresult, opc_value_to_variant, quality_to_string, variant_to_string,
};
use crate::provider::{OpcProvider, OpcValue, TagValue, WriteResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Concrete [`OpcProvider`] implementation for Windows OPC DA.
///
/// Uses native `windows-rs` COM interop via the internal `opc_da` module.
pub struct OpcDaWrapper<C: ServerConnector = ComConnector> {
    connector: Arc<C>,
}

impl Default for OpcDaWrapper<ComConnector> {
    fn default() -> Self {
        Self::new(ComConnector)
    }
}

impl<C: ServerConnector> OpcDaWrapper<C> {
    /// Creates a new `OpcDaWrapper` with the given connector.
    pub fn new(connector: C) -> Self {
        Self {
            connector: Arc::new(connector),
        }
    }
}

fn browse_recursive<S: ConnectedServer>(
    server: &S,
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
        .browse_opc_item_ids(OPC_BRANCH.0 as u32, Some(""), 0, 0)
        .context("Failed to browse branches at current position")?;

    let branches: Vec<String> = branch_enum
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
        if let Err(e) = server.change_browse_position(OPC_BROWSE_DOWN.0 as u32, &branch) {
            tracing::warn!(branch = %branch, error = ?e, "Failed to enter branch, skipping");
            continue;
        }

        let recurse_result =
            browse_recursive(server, tags, max_tags, progress, tags_sink, depth + 1);

        if let Err(e) = server.change_browse_position(OPC_BROWSE_UP.0 as u32, "") {
            tracing::error!(branch = %branch, error = ?e, "Failed to navigate back up!");
            return Err(anyhow::anyhow!(
                "Browse position corrupted: failed to navigate up from branch '{branch}'"
            ));
        }
        recurse_result?;
    }

    if let Ok(leaf_enum) = server.browse_opc_item_ids(OPC_LEAF.0 as u32, Some(""), 0, 0) {
        for leaf_res in leaf_enum {
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

#[allow(clippy::too_many_lines)]
#[async_trait]
impl<C: ServerConnector + 'static> OpcProvider for OpcDaWrapper<C> {
    async fn list_servers(&self, host: &str) -> Result<Vec<String>> {
        let host_owned = host.to_string();
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!("opc.list_servers", host = %host_owned);
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let servers = connector.enumerate_servers()?;
            tracing::info!(count = servers.len(), "list_servers completed");
            Ok(servers)
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
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!("opc.browse_tags", server = %server_name, max_tags);
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let opc_server = connector.connect(&server_name)?;

            let org = opc_server
                .query_organization()
                .context("Failed to query namespace organization")?;
            let mut tags = Vec::new();

            if org == OPC_NS_FLAT.0 as u32 {
                let string_iter =
                    opc_server.browse_opc_item_ids(OPC_LEAF.0 as u32, Some(""), 0, 0)?;
                for tag_res in string_iter {
                    if tags.len() >= max_tags {
                        break;
                    }
                    let tag = tag_res.map_err(|e| anyhow::anyhow!("Tag iteration error: {e:?}"))?;
                    tags.push(tag.clone());
                    if let Ok(mut sink) = tags_sink.lock() {
                        sink.push(tag);
                    }
                    progress.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                // Try OPC_FLAT — returns fully-qualified item IDs directly,
                // eliminating recursive traversal and per-leaf get_item_id().
                let use_flat = match opc_server.browse_opc_item_ids(OPC_FLAT.0 as u32, Some(""), 0, 0) {
                    Ok(mut flat_enum) => match flat_enum.next() {
                        Some(Ok(first_tag)) => {
                            tracing::info!("OPC_FLAT browse supported — using fast flat enumeration");
                            tags.push(first_tag.clone());
                            if let Ok(mut sink) = tags_sink.lock() {
                                sink.push(first_tag);
                            }
                            progress.fetch_add(1, Ordering::Relaxed);

                            for tag_res in flat_enum {
                                if tags.len() >= max_tags { break; }
                                match tag_res {
                                    Ok(tag) => {
                                        tags.push(tag.clone());
                                        if let Ok(mut sink) = tags_sink.lock() {
                                            sink.push(tag);
                                        }
                                        progress.fetch_add(1, Ordering::Relaxed);
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = ?e, "OPC_FLAT tag iteration error, skipping");
                                    }
                                }
                            }
                            true
                        }
                        Some(Err(e)) => {
                            tracing::debug!(error = ?e, "OPC_FLAT first item error, falling back to recursive");
                            false
                        }
                        None => {
                            tracing::debug!("OPC_FLAT returned no items, falling back to recursive");
                            false
                        }
                    },
                    Err(e) => {
                        tracing::debug!(error = ?e, "OPC_FLAT not supported, falling back to recursive");
                        false
                    }
                };

                if !use_flat {
                    browse_recursive(&opc_server, &mut tags, max_tags, &progress, &tags_sink, 0)?;
                }
            }
            tracing::info!(count = tags.len(), "browse_tags completed");
            Ok(tags)
        })
        .await?
    }

    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> Result<Vec<TagValue>> {
        let server_name = server.to_string();
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!(
                "opc.read_tag_values",
                server = %server_name,
                tag_count = tag_ids.len()
            );
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let opc_server = connector.connect(&server_name)?;

            let mut revised_update_rate = 0u32;
            let mut server_handle = 0u32;
            let group = opc_server.add_group(
                "opc-da-client-read",
                true,
                1000, // update_rate
                0,    // client_handle
                0,    // time_bias
                0.0,  // percent_deadband
                0,    // locale_id
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

            // Pre-fill ALL tags with an error placeholder.
            // We will only overwrite the ones that are successfully added and read.
            let mut tag_values: Vec<TagValue> = tag_ids
                .iter()
                .map(|tag_id| TagValue {
                    tag_id: tag_id.clone(),
                    value: "Error".to_string(),
                    quality: "Bad — not added to group".to_string(),
                    timestamp: String::new(),
                })
                .collect();

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
                    let hint = format_hresult(*error);
                    tracing::warn!(
                        tag = %tag_ids[idx],
                        error = %hint,
                        "read_tag_values: add_items rejected tag"
                    );
                    tag_values[idx].quality = format!("Bad — {hint}");
                }
            }

            if server_handles.is_empty() {
                if let Err(e) = opc_server.remove_group(server_handle, true) {
                    tracing::warn!(error = ?e, operation = "read_tag_values", "Failed to remove OPC group during cleanup");
                }
                return Ok(tag_values);
            }

            let (item_states, read_errors) = group.read(OPC_DS_DEVICE, &server_handles)?;
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
                    let full_msg = format_hresult(*read_error);
                    tracing::warn!(
                        tag = %tag_ids[*idx],
                        error = ?read_error,
                        hint = %full_msg,
                        "read_tag_values: per-item read error"
                    );
                    ("Error".to_string(), format!("Bad — {full_msg}"))
                };

                tag_values[*idx] = TagValue {
                    tag_id: tag_ids[*idx].clone(),
                    value: value_str,
                    quality: quality_str,
                    timestamp: filetime_to_string(state.ftTimeStamp),
                };
            }

            tracing::info!(count = tag_values.len(), "read_tag_values completed");
            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, operation = "read_tag_values", "Failed to remove OPC group during cleanup");
            }
            Ok(tag_values)
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
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!(
                "opc.write_tag_value",
                server = %server_name,
                tag = %tag
            );
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let opc_server = connector.connect(&server_name)?;

            let mut revised_update_rate = 0u32;
            let mut server_handle = 0u32;
            let group = opc_server.add_group(
                "opc-da-client-write",
                true,
                1000,
                0,
                0,
                0.0,
                0,
                &mut revised_update_rate,
                &mut server_handle,
            )?;

            // SAFETY: item_id_wide must outlive item_def because
            // tagOPCITEMDEF.szItemID holds a raw pointer into the Vec.
            let mut item_id_wide: Vec<u16> =
                tag.encode_utf16().chain(std::iter::once(0)).collect();
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
            let item_res = results
                .as_slice()
                .first()
                .context("Server returned empty item results")?;
            let item_err = errors
                .as_slice()
                .first()
                .context("Server returned empty item errors")?;

            if let Err(e) = item_err.ok() {
                tracing::warn!(error = ?e, "write_tag_value: failed to add tag to group");
                if let Err(e) = opc_server.remove_group(server_handle, true) {
                    tracing::warn!(error = ?e, operation = "write_tag_value", "Failed to remove OPC group during cleanup");
                }
                return Ok(WriteResult {
                    tag_id: tag,
                    success: false,
                    error: Some(format!("Failed to add tag to group: {:?}", item_err)),
                });
            }

            let variant = opc_value_to_variant(&value);
            let write_errors = group.write(&[item_res.hServer], &[variant])?;
            let write_error = write_errors
                .as_slice()
                .first()
                .context("Server returned empty write errors")?;

            let write_result = if write_error.is_ok() {
                tracing::info!("write_tag_value: write completed successfully");
                WriteResult {
                    tag_id: tag,
                    success: true,
                    error: None,
                }
            } else {
                let hint = format_hresult(*write_error);
                tracing::warn!(error = ?write_error, hint = %hint, "write_tag_value: server rejected write");
                WriteResult {
                    tag_id: tag,
                    success: false,
                    error: Some(hint),
                }
            };

            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, operation = "write_tag_value", "Failed to remove OPC group during cleanup");
            }
            Ok(write_result)
        })
        .await?
    }
}

#[cfg(test)]
#[allow(
    clippy::undocumented_unsafe_blocks,
    clippy::ptr_as_ptr,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::ref_as_ptr,
    clippy::inline_always
)]
mod tests {
    use super::*;
    use crate::backend::connector::{ConnectedGroup, ConnectedServer, ServerConnector};
    use crate::bindings::da::{
        OPC_NS_HIERARCHIAL, tagOPCITEMDEF, tagOPCITEMRESULT, tagOPCITEMSTATE,
    };
    use crate::opc_da::client::StringIterator;
    use crate::opc_da::utils::RemoteArray;
    use windows::Win32::System::Com::{IEnumString, IEnumString_Impl};
    use windows::Win32::System::Variant::VARIANT;
    use windows::core::{PWSTR, implement};

    #[allow(clippy::ref_as_ptr, clippy::inline_always)]
    #[implement(IEnumString)]
    struct MockEnumString {
        items: Vec<String>,
        index: std::sync::atomic::AtomicUsize,
    }

    impl IEnumString_Impl for MockEnumString_Impl {
        fn Next(
            &self,
            celt: u32,
            rgelt: *mut PWSTR,
            pceltfetched: *mut u32,
        ) -> windows::core::HRESULT {
            let mut fetched = 0;
            let index = self.index.load(std::sync::atomic::Ordering::Relaxed);
            let rgelt = unsafe { std::slice::from_raw_parts_mut(rgelt, celt as usize) };

            for (i, elem) in rgelt.iter_mut().enumerate().take(celt as usize) {
                if index + i < self.items.len() {
                    let s = &self.items[index + i];
                    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                    let ptr = unsafe { windows::Win32::System::Com::CoTaskMemAlloc(w.len() * 2) };
                    unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), ptr as *mut u16, w.len()) };
                    *elem = PWSTR(ptr as *mut u16);
                    fetched += 1;
                } else {
                    break;
                }
            }

            self.index
                .store(index + fetched, std::sync::atomic::Ordering::Relaxed);

            if !pceltfetched.is_null() {
                unsafe { *pceltfetched = fetched as u32 };
            }

            if fetched == celt as usize {
                windows::Win32::Foundation::S_OK
            } else {
                windows::Win32::Foundation::S_FALSE
            }
        }
        fn Skip(&self, _celt: u32) -> windows::core::HRESULT {
            windows::Win32::Foundation::E_NOTIMPL
        }
        fn Reset(&self) -> windows::core::Result<()> {
            self.index.store(0, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn Clone(&self) -> windows::core::Result<IEnumString> {
            Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_NOTIMPL,
            ))
        }
    }

    struct MockGroup;

    impl ConnectedGroup for MockGroup {
        fn add_items(
            &self,
            items: &[tagOPCITEMDEF],
        ) -> anyhow::Result<(
            RemoteArray<tagOPCITEMRESULT>,
            RemoteArray<windows::core::HRESULT>,
        )> {
            let mut results = Vec::new();
            let mut errors = Vec::new();

            for (i, item) in items.iter().enumerate() {
                // Read the wide string
                let mut name_w = Vec::new();
                let mut ptr = item.szItemID.0;
                unsafe {
                    while !ptr.is_null() && *ptr != 0 {
                        name_w.push(*ptr);
                        ptr = ptr.add(1);
                    }
                }
                let name = String::from_utf16_lossy(&name_w);

                let res = tagOPCITEMRESULT {
                    hServer: (i + 1) as u32,
                    ..Default::default()
                };

                if name == "RejectMe" {
                    errors.push(windows::core::HRESULT(0xC004_0007_u32 as i32)); // OPC_E_UNKNOWNITEMID
                } else if name == "RejectAll" {
                    return Err(anyhow::anyhow!("Total failure"));
                } else {
                    errors.push(windows::core::HRESULT(0)); // S_OK
                }
                results.push(res);
            }

            unsafe {
                let p_res = windows::Win32::System::Com::CoTaskMemAlloc(
                    results.len() * std::mem::size_of::<tagOPCITEMRESULT>(),
                ) as *mut tagOPCITEMRESULT;
                std::ptr::copy_nonoverlapping(results.as_ptr(), p_res, results.len());
                let p_err = windows::Win32::System::Com::CoTaskMemAlloc(
                    errors.len() * std::mem::size_of::<windows::core::HRESULT>(),
                ) as *mut windows::core::HRESULT;
                std::ptr::copy_nonoverlapping(errors.as_ptr(), p_err, errors.len());

                Ok((
                    RemoteArray::from_mut_ptr(p_res, results.len() as u32),
                    RemoteArray::from_mut_ptr(p_err, errors.len() as u32),
                ))
            }
        }

        fn read(
            &self,
            _source: crate::bindings::da::tagOPCDATASOURCE,
            server_handles: &[u32],
        ) -> anyhow::Result<(
            RemoteArray<tagOPCITEMSTATE>,
            RemoteArray<windows::core::HRESULT>,
        )> {
            let mut states = Vec::new();
            let mut errors = Vec::new();

            for &handle in server_handles {
                let mut state = tagOPCITEMSTATE {
                    hClient: handle, // mock echoing the handle as client handle for verification
                    wQuality: crate::bindings::da::OPC_QUALITY_GOOD,
                    ..Default::default()
                };
                // mock value VT_I4 = 42
                use windows::Win32::System::Variant::{
                    VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0,
                };
                let variant = VARIANT_0_0 {
                    vt: VARENUM(3), // VT_I4
                    Anonymous: VARIANT_0_0_0 { lVal: 42 },
                    ..Default::default()
                };
                state.vDataValue = VARIANT {
                    Anonymous: VARIANT_0 {
                        Anonymous: std::mem::ManuallyDrop::new(variant),
                    },
                };

                states.push(state);
                errors.push(windows::core::HRESULT(0)); // S_OK
            }

            unsafe {
                let p_states = windows::Win32::System::Com::CoTaskMemAlloc(
                    states.len() * std::mem::size_of::<tagOPCITEMSTATE>(),
                ) as *mut tagOPCITEMSTATE;
                std::ptr::copy_nonoverlapping(states.as_ptr(), p_states, states.len());
                let p_err = windows::Win32::System::Com::CoTaskMemAlloc(
                    errors.len() * std::mem::size_of::<windows::core::HRESULT>(),
                ) as *mut windows::core::HRESULT;
                std::ptr::copy_nonoverlapping(errors.as_ptr(), p_err, errors.len());

                Ok((
                    RemoteArray::from_mut_ptr(p_states, states.len() as u32),
                    RemoteArray::from_mut_ptr(p_err, errors.len() as u32),
                ))
            }
        }

        fn write(
            &self,
            server_handles: &[u32],
            _values: &[VARIANT],
        ) -> anyhow::Result<RemoteArray<windows::core::HRESULT>> {
            let mut errors = Vec::new();
            for _ in server_handles {
                errors.push(windows::core::HRESULT(0)); // S_OK
            }

            unsafe {
                let p_err = windows::Win32::System::Com::CoTaskMemAlloc(
                    errors.len() * std::mem::size_of::<windows::core::HRESULT>(),
                ) as *mut windows::core::HRESULT;
                std::ptr::copy_nonoverlapping(errors.as_ptr(), p_err, errors.len());
                Ok(RemoteArray::from_mut_ptr(p_err, errors.len() as u32))
            }
        }
    }

    struct MockServer;

    impl ConnectedServer for MockServer {
        type Group = MockGroup;

        fn query_organization(&self) -> anyhow::Result<u32> {
            Ok(crate::bindings::da::OPC_NS_FLAT.0 as u32)
        }

        fn browse_opc_item_ids(
            &self,
            _browse_type: u32,
            _filter: Option<&str>,
            _data_type: u16,
            _access_rights: u32,
        ) -> anyhow::Result<StringIterator> {
            let mock_enum: IEnumString = MockEnumString {
                items: vec!["MockTag.1".to_string(), "MockTag.2".to_string()],
                index: std::sync::atomic::AtomicUsize::new(0),
            }
            .into();
            Ok(StringIterator::new(mock_enum))
        }

        fn change_browse_position(&self, _direction: u32, _name: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_item_id(&self, item_name: &str) -> anyhow::Result<String> {
            Ok(item_name.to_string())
        }

        fn add_group(
            &self,
            _name: &str,
            _active: bool,
            _update_rate: u32,
            _client_handle: u32,
            _time_bias: i32,
            _percent_deadband: f32,
            _locale_id: u32,
            revised_update_rate: &mut u32,
            server_handle: &mut u32,
        ) -> anyhow::Result<Self::Group> {
            *revised_update_rate = 1000;
            *server_handle = 1;
            Ok(MockGroup)
        }

        fn remove_group(&self, _server_group: u32, _force: bool) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct MockConnector;

    impl ServerConnector for MockConnector {
        type Server = MockServer;

        fn enumerate_servers(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec![
                "Mock.Server.1".to_string(),
                "Mock.Server.2".to_string(),
            ])
        }

        fn connect(&self, _server_name: &str) -> anyhow::Result<Self::Server> {
            Ok(MockServer)
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    enum OpcFlatBehavior {
        Success(Vec<String>),
        ReturnsError,
        ReturnsEmpty,
    }

    struct MockHierarchicalServer {
        opc_flat_behavior: OpcFlatBehavior,
        position: std::cell::RefCell<Vec<String>>,
    }

    impl ConnectedServer for MockHierarchicalServer {
        type Group = MockGroup;

        fn query_organization(&self) -> anyhow::Result<u32> {
            Ok(OPC_NS_HIERARCHIAL.0 as u32)
        }

        fn browse_opc_item_ids(
            &self,
            browse_type: u32,
            _filter: Option<&str>,
            _data_type: u16,
            _access_rights: u32,
        ) -> anyhow::Result<StringIterator> {
            let pos = self.position.borrow();
            let mut results = Vec::new();

            if browse_type == OPC_FLAT.0 as u32 {
                match &self.opc_flat_behavior {
                    OpcFlatBehavior::Success(items) => {
                        results = items.clone();
                    }
                    OpcFlatBehavior::ReturnsError => {
                        return Err(anyhow::anyhow!("OPC_FLAT not supported mock error"));
                    }
                    OpcFlatBehavior::ReturnsEmpty => {
                        // Return empty iterator
                    }
                }
            } else if browse_type == OPC_BRANCH.0 as u32 {
                if pos.is_empty() {
                    results.push("Branch1".to_string());
                    results.push("Branch2".to_string());
                }
            } else if browse_type == OPC_LEAF.0 as u32 {
                if pos.len() == 1 && pos[0] == "Branch1" {
                    results.push("Leaf1".to_string());
                    results.push("Leaf2".to_string());
                } else if pos.len() == 1 && pos[0] == "Branch2" {
                    results.push("Leaf3".to_string());
                }
            }

            let mock_enum: IEnumString = MockEnumString {
                items: results,
                index: std::sync::atomic::AtomicUsize::new(0),
            }
            .into();
            Ok(StringIterator::new(mock_enum))
        }

        fn change_browse_position(&self, direction: u32, name: &str) -> anyhow::Result<()> {
            let mut pos = self.position.borrow_mut();
            if direction == OPC_BROWSE_DOWN.0 as u32 {
                pos.push(name.to_string());
            } else if direction == OPC_BROWSE_UP.0 as u32 {
                pos.pop();
            }
            Ok(())
        }

        fn get_item_id(&self, item_name: &str) -> anyhow::Result<String> {
            let pos = self.position.borrow();
            if pos.is_empty() {
                Ok(item_name.to_string())
            } else {
                Ok(format!("{}.{}", pos.join("."), item_name))
            }
        }

        fn add_group(
            &self,
            _name: &str,
            _active: bool,
            _update_rate: u32,
            _client_handle: u32,
            _time_bias: i32,
            _percent_deadband: f32,
            _locale_id: u32,
            _revised_update_rate: &mut u32,
            _server_handle: &mut u32,
        ) -> anyhow::Result<Self::Group> {
            Ok(MockGroup)
        }

        fn remove_group(&self, _server_group: u32, _force: bool) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockHierarchicalConnector {
        opc_flat_behavior: OpcFlatBehavior,
    }

    impl ServerConnector for MockHierarchicalConnector {
        type Server = MockHierarchicalServer;

        fn enumerate_servers(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["Mock.Hierarchical.1".to_string()])
        }

        fn connect(&self, _server_name: &str) -> anyhow::Result<Self::Server> {
            Ok(MockHierarchicalServer {
                opc_flat_behavior: self.opc_flat_behavior.clone(),
                position: std::cell::RefCell::new(Vec::new()),
            })
        }
    }

    #[test]
    fn test_browse_tags_flat_server() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            let progress = Arc::new(AtomicUsize::new(0));
            let sink = Arc::new(std::sync::Mutex::new(Vec::new()));

            let tags = wrapper
                .browse_tags("Mock.Server", 100, progress.clone(), sink.clone())
                .await
                .unwrap();

            assert_eq!(tags.len(), 2);
            assert_eq!(tags[0], "MockTag.1");
            assert_eq!(tags[1], "MockTag.2");

            assert_eq!(progress.load(Ordering::Relaxed), 2);
            let sink_tags = sink.lock().unwrap().clone();
            assert_eq!(sink_tags, tags);
        });
    }

    #[test]
    fn test_browse_tags_hierarchical_recursive() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let connector = MockHierarchicalConnector {
                opc_flat_behavior: OpcFlatBehavior::ReturnsError,
            };
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(connector),
            };

            let progress = Arc::new(AtomicUsize::new(0));
            let sink = Arc::new(std::sync::Mutex::new(Vec::new()));

            let tags = wrapper
                .browse_tags("Mock.Hierarchical", 100, progress.clone(), sink.clone())
                .await
                .unwrap();

            assert_eq!(tags.len(), 3);
            assert_eq!(tags[0], "Branch1.Leaf1");
            assert_eq!(tags[1], "Branch1.Leaf2");
            assert_eq!(tags[2], "Branch2.Leaf3");

            assert_eq!(progress.load(Ordering::Relaxed), 3);
            let sink_tags = sink.lock().unwrap().clone();
            assert_eq!(sink_tags, tags);
        });
    }

    #[test]
    fn test_browse_tags_opc_flat_success() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let connector = MockHierarchicalConnector {
                opc_flat_behavior: OpcFlatBehavior::Success(vec![
                    "FQ.Branch1.Leaf1".to_string(),
                    "FQ.Branch1.Leaf2".to_string(),
                    "FQ.Branch2.Leaf3".to_string(),
                ]),
            };
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(connector),
            };

            let progress = Arc::new(AtomicUsize::new(0));
            let sink = Arc::new(std::sync::Mutex::new(Vec::new()));

            let tags = wrapper
                .browse_tags("Mock.Hierarchical", 100, progress.clone(), sink.clone())
                .await
                .unwrap();

            assert_eq!(tags.len(), 3);
            assert_eq!(tags[0], "FQ.Branch1.Leaf1");
            assert_eq!(tags[1], "FQ.Branch1.Leaf2");
            assert_eq!(tags[2], "FQ.Branch2.Leaf3");

            assert_eq!(progress.load(Ordering::Relaxed), 3);
            let sink_tags = sink.lock().unwrap().clone();
            assert_eq!(sink_tags, tags);
        });
    }

    #[test]
    fn test_browse_tags_opc_flat_error_fallback() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let connector = MockHierarchicalConnector {
                opc_flat_behavior: OpcFlatBehavior::ReturnsError,
            };
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(connector),
            };

            let progress = Arc::new(AtomicUsize::new(0));
            let sink = Arc::new(std::sync::Mutex::new(Vec::new()));

            let tags = wrapper
                .browse_tags("Mock.Hierarchical", 100, progress.clone(), sink.clone())
                .await
                .unwrap();

            assert_eq!(tags.len(), 3);
            assert_eq!(tags[0], "Branch1.Leaf1");
            assert_eq!(tags[1], "Branch1.Leaf2");
            assert_eq!(tags[2], "Branch2.Leaf3");
        });
    }

    #[test]
    fn test_browse_tags_opc_flat_empty_fallback() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let connector = MockHierarchicalConnector {
                opc_flat_behavior: OpcFlatBehavior::ReturnsEmpty,
            };
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(connector),
            };

            let progress = Arc::new(AtomicUsize::new(0));
            let sink = Arc::new(std::sync::Mutex::new(Vec::new()));

            let tags = wrapper
                .browse_tags("Mock.Hierarchical", 100, progress.clone(), sink.clone())
                .await
                .unwrap();

            assert_eq!(tags.len(), 3);
            assert_eq!(tags[0], "Branch1.Leaf1");
            assert_eq!(tags[1], "Branch1.Leaf2");
            assert_eq!(tags[2], "Branch2.Leaf3");
        });
    }

    #[test]
    fn test_browse_tags_max_tags_limit() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            let progress = Arc::new(AtomicUsize::new(0));
            let sink = Arc::new(std::sync::Mutex::new(Vec::new()));

            // Limit to 2 tags
            let tags = wrapper
                .browse_tags("Mock.Server", 2, progress.clone(), sink.clone())
                .await
                .unwrap();

            assert_eq!(tags.len(), 2);
            assert_eq!(progress.load(Ordering::Relaxed), 2);
        });
    }

    #[test]
    fn test_mock_list_servers() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            let servers = wrapper.list_servers("localhost").await.unwrap();
            assert_eq!(servers, vec!["Mock.Server.1", "Mock.Server.2"]);
        });
    }

    #[test]
    fn test_mock_read_tags_happy() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            let tags = vec!["Tag1".to_string(), "Tag2".to_string()];
            let results = wrapper
                .read_tag_values("Mock.Server.1", tags)
                .await
                .unwrap();
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].tag_id, "Tag1");
            assert_eq!(results[0].value, "42");
            assert_eq!(results[1].tag_id, "Tag2");
            assert_eq!(results[1].value, "42");
        });
    }

    #[test]
    fn test_mock_read_tags_partial_reject() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            let tags = vec![
                "Tag1".to_string(),
                "RejectMe".to_string(),
                "Tag3".to_string(),
            ];
            let results = wrapper
                .read_tag_values("Mock.Server.1", tags)
                .await
                .unwrap();
            assert_eq!(results.len(), 3);

            assert_eq!(results[0].value, "42");
            assert_eq!(results[1].value, "Error");
            assert!(results[1].quality.starts_with("Bad"));
            assert_eq!(results[2].value, "42");
        });
    }

    #[test]
    fn test_mock_read_tags_all_reject() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            // Before our fixes, this returns Err("No valid items to read").
            // After our fixes, it should return Ok(Vec) where all are Errors.
            let tags = vec!["RejectAll".to_string()];
            let res = wrapper.read_tag_values("Mock.Server.1", tags).await;
            assert!(res.is_err()); // The mock `if name == "RejectAll"` returns `return Err(anyhow::anyhow!("Total failure"));`

            let tags2 = vec!["RejectMe".to_string(), "RejectMe".to_string()];
            let results2 = wrapper
                .read_tag_values("Mock.Server.1", tags2)
                .await
                .unwrap();
            assert_eq!(results2.len(), 2);
            assert_eq!(results2[0].value, "Error");
            assert_eq!(results2[1].value, "Error");
        });
    }

    #[test]
    fn test_mock_write_tag_happy() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            use crate::provider::OpcValue;
            let res = wrapper
                .write_tag_value("Mock.Server.1", "Tag1", OpcValue::Int(42))
                .await
                .unwrap();
            assert!(res.success);
            assert!(res.error.is_none());
        });
    }

    #[test]
    fn test_mock_write_tag_add_fail() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };

            use crate::provider::OpcValue;
            let res = wrapper
                .write_tag_value("Mock.Server.1", "RejectMe", OpcValue::Int(42))
                .await
                .unwrap();
            assert!(!res.success);
            assert!(res.error.is_some());
        });
    }
}
