import re

content = """use crate::backend::connector::{ConnectedGroup, ConnectedServer, ServerConnector};
use crate::bindings::da::{
    tagOPCDATASOURCE, tagOPCITEMDEF, tagOPCITEMRESULT, tagOPCITEMSTATE, OPC_BRANCH, OPC_DS_DEVICE,
    OPC_FLAT, OPC_LEAF, OPC_NS_FLAT, OPC_BROWSE_DOWN, OPC_BROWSE_UP
};
use crate::helpers::{
    filetime_to_string, format_hresult, opc_value_to_variant, quality_to_string, variant_to_string,
};
use crate::opc_da::errors::{OpcError, OpcResult};
use crate::opc_da::typedefs::{GroupHandle, ItemHandle};
use crate::provider::{OpcValue, TagValue, WriteResult};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub enum ComRequest {
    ListServers {
        host: String,
        reply: oneshot::Sender<OpcResult<Vec<String>>>,
    },
    ReadTagValues {
        server: String,
        tag_ids: Vec<String>,
        reply: oneshot::Sender<OpcResult<Vec<TagValue>>>,
    },
    WriteTagValue {
        server: String,
        tag_id: String,
        value: OpcValue,
        reply: oneshot::Sender<OpcResult<WriteResult>>,
    },
    BrowseTags {
        server: String,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
        tags_sink: Arc<std::sync::Mutex<Vec<String>>>,
        reply: oneshot::Sender<OpcResult<Vec<String>>>,
    },
}

pub struct ComWorker<C: ServerConnector + 'static> {
    pub sender: mpsc::Sender<ComRequest>,
    pub handle: Option<std::thread::JoinHandle<()>>,
    _phantom: std::marker::PhantomData<C>,
}

fn is_connection_error(err: &OpcError) -> bool {
    if let OpcError::Com(hres) = err {
        let code = hres.0;
        code == windows::core::HRESULT(0x8007_06BA_u32 as i32).0
            || code == windows::core::HRESULT(0x8007_06BF_u32 as i32).0
            || code == windows::core::HRESULT(0x8007_06BE_u32 as i32).0
            || code == windows::core::HRESULT(0x8008_0005_u32 as i32).0
    } else {
        false
    }
}

impl<C: ServerConnector + 'static> ComWorker<C> {
    pub fn start(connector: Arc<C>) -> Result<Self, OpcError> {
        let (tx, mut rx) = mpsc::channel(32);
        let (init_tx, init_rx) = oneshot::channel();

        let handle = std::thread::spawn(move || {
            let _guard = match crate::ComGuard::new() {
                Ok(g) => {
                    let _ = init_tx.send(Ok(()));
                    g
                }
                Err(e) => {
                    tracing::error!(error = ?e, "COM worker failed to initialize MTA");
                    let _ = init_tx.send(Err(OpcError::Internal("COM init failed on worker".into())));
                    return;
                }
            };

            let mut cache: HashMap<String, C::Server> = HashMap::new();

            while let Some(req) = rx.blocking_recv() {
                match req {
                    ComRequest::ListServers { host, reply } => {
                        let span = tracing::info_span!("opc.list_servers", host = %host);
                        let _enter = span.enter();
                        let servers = connector.enumerate_servers();
                        if let Ok(s) = &servers {
                            tracing::info!(count = s.len(), "list_servers completed");
                        }
                        let _ = reply.send(servers);
                    }
                    ComRequest::ReadTagValues {
                        server,
                        tag_ids,
                        reply,
                    } => {
                        let result = Self::dispatch_with_retry(
                            &mut cache,
                            &connector,
                            &server,
                            |opc_server| Self::handle_read(&server, &tag_ids, opc_server),
                        );
                        let _ = reply.send(result);
                    }
                    ComRequest::WriteTagValue {
                        server,
                        tag_id,
                        value,
                        reply,
                    } => {
                        let result = Self::dispatch_with_retry(
                            &mut cache,
                            &connector,
                            &server,
                            |opc_server| Self::handle_write(&server, &tag_id, &value, opc_server),
                        );
                        let _ = reply.send(result);
                    }
                    ComRequest::BrowseTags {
                        server,
                        max_tags,
                        progress,
                        tags_sink,
                        reply,
                    } => {
                        let result = Self::dispatch_with_retry(
                            &mut cache,
                            &connector,
                            &server,
                            |opc_server| {
                                Self::handle_browse(
                                    &server,
                                    max_tags,
                                    &progress,
                                    &tags_sink,
                                    opc_server,
                                )
                            },
                        );
                        let _ = reply.send(result);
                    }
                }
            }

            tracing::debug!("COM worker thread exiting cleanly");
        });

        init_rx
            .blocking_recv()
            .map_err(|_| OpcError::Internal("COM worker thread panicked during init".into()))??;

        tracing::debug!("COM worker thread started");

        Ok(Self {
            sender: tx,
            handle: Some(handle),
            _phantom: std::marker::PhantomData,
        })
    }

    pub async fn send_request<F, R>(&self, req_builder: F) -> OpcResult<R>
    where
        F: FnOnce(oneshot::Sender<OpcResult<R>>) -> ComRequest,
    {
        if let Some(h) = &self.handle {
            if h.is_finished() {
                tracing::error!("COM worker thread panicked or exited unexpectedly");
                return Err(OpcError::Internal("COM worker thread panicked".into()));
            }
        }

        let (tx, rx) = oneshot::channel();
        let req = req_builder(tx);

        self.sender.send(req).await.map_err(|_| {
            OpcError::Internal("COM worker channel closed (worker stopped)".into())
        })?;

        rx.await
            .map_err(|_| OpcError::Internal("COM worker shut down during request".into()))?
    }

    fn dispatch_with_retry<F, R>(
        cache: &mut HashMap<String, C::Server>,
        connector: &Arc<C>,
        server_name: &str,
        operation: F,
    ) -> OpcResult<R>
    where
        F: Fn(&C::Server) -> OpcResult<R>,
    {
        let cached_server = if let Some(srv) = cache.get(server_name) {
            tracing::trace!(server = %server_name, "Cache hit");
            Some(srv)
        } else {
            tracing::debug!(server = %server_name, "Cache miss, connecting");
            let srv = connector.connect(server_name)?;
            cache.insert(server_name.to_string(), srv);
            cache.get(server_name)
        };

        let server_ref = cached_server.unwrap();
        match operation(server_ref) {
            Err(e) if is_connection_error(&e) => {
                tracing::warn!(server = %server_name, error = ?e, "Evicting stale connection");
                cache.remove(server_name);
                tracing::debug!(server = %server_name, "Reconnecting");
                let fresh_srv = connector.connect(server_name).map_err(|connect_e| {
                    tracing::error!(error = ?connect_e, "Reconnect failed");
                    connect_e
                })?;
                let fresh_ref = &fresh_srv;
                let result = operation(fresh_ref);
                cache.insert(server_name.to_string(), fresh_srv);
                result
            }
            other => other,
        }
    }

    fn handle_read(
        server_name: &str,
        tag_ids: &[String],
        opc_server: &C::Server,
    ) -> OpcResult<Vec<TagValue>> {
        let span = tracing::info_span!(
            "opc.read_tag_values",
            server = %server_name,
            tag_count = tag_ids.len()
        );
        let _enter = span.enter();

        let mut revised_update_rate = 0u32;
        let mut server_handle = GroupHandle::default();
        let group = opc_server.add_group(
            "opc-da-client-read",
            true,
            1000,
            server_handle,
            0,
            0.0,
            0,
            &mut revised_update_rate,
            &mut server_handle,
        )?;

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

        let mut tag_values: Vec<TagValue> = tag_ids
            .iter()
            .map(|tag_id| TagValue {
                tag_id: tag_id.clone(),
                value: "Error".to_string(),
                quality: "Bad — not added to group".to_string(),
                timestamp: String::new(),
            })
            .collect();

        let mut server_handles: Vec<ItemHandle> = Vec::new();
        let mut valid_indices = Vec::new();

        for (idx, (item_result, error)) in results
            .as_slice()
            .iter()
            .zip(errors.as_slice().iter())
            .enumerate()
        {
            if error.is_ok() {
                server_handles.push(ItemHandle(item_result.hServer));
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
    }

    fn handle_write(
        server_name: &str,
        tag_id: &str,
        value: &OpcValue,
        opc_server: &C::Server,
    ) -> OpcResult<WriteResult> {
        let span = tracing::info_span!(
            "opc.write_tag_value",
            server = %server_name,
            tag = %tag_id
        );
        let _enter = span.enter();

        let mut revised_update_rate = 0u32;
        let mut server_handle = GroupHandle::default();
        let group = opc_server.add_group(
            "opc-da-client-write",
            true,
            1000,
            GroupHandle(0),
            0,
            0.0,
            0,
            &mut revised_update_rate,
            &mut server_handle,
        )?;

        let mut item_id_wide: Vec<u16> =
            tag_id.encode_utf16().chain(std::iter::once(0)).collect();
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
            .ok_or_else(|| OpcError::Internal("Server returned empty item results".to_string()))?;
        let item_err = errors
            .as_slice()
            .first()
            .ok_or_else(|| OpcError::Internal("Server returned empty item errors".to_string()))?;

        if let Err(e) = item_err.ok() {
            tracing::warn!(error = ?e, "write_tag_value: failed to add tag to group");
            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, operation = "write_tag_value", "Failed to remove OPC group during cleanup");
            }
            return Ok(WriteResult {
                success: false,
                error: Some(format!("Failed to add tag: {}", format_hresult(*item_err))),
            });
        }

        let item_handle = ItemHandle(item_res.hServer);
        let variant = opc_value_to_variant(value)?;

        let write_errors = group.write(&[item_handle], &[variant])?;
        let write_err = write_errors
            .as_slice()
            .first()
            .ok_or_else(|| OpcError::Internal("Server returned empty write errors".to_string()))?;

        let write_result = if write_err.is_ok() {
            tracing::info!("write_tag_value completed");
            WriteResult {
                success: true,
                error: None,
            }
        } else {
            let msg = format_hresult(*write_err);
            tracing::warn!(error = %msg, "write_tag_value: server rejected write");
            WriteResult {
                success: false,
                error: Some(msg),
            }
        };

        if let Err(e) = opc_server.remove_group(server_handle, true) {
            tracing::warn!(error = ?e, operation = "write_tag_value", "Failed to remove OPC group during cleanup");
        }
        Ok(write_result)
    }

    fn handle_browse(
        server_name: &str,
        max_tags: usize,
        progress: &Arc<AtomicUsize>,
        tags_sink: &Arc<std::sync::Mutex<Vec<String>>>,
        opc_server: &C::Server,
    ) -> OpcResult<Vec<String>> {
        let span = tracing::info_span!("opc.browse_tags", server = %server_name, max_tags);
        let _enter = span.enter();

        let org = opc_server.query_organization()?;
        let mut tags = Vec::new();

        if org == OPC_NS_FLAT.0 as u32 {
            let string_iter =
                opc_server.browse_opc_item_ids(OPC_LEAF.0 as u32, Some(""), 0, 0)?;
            for tag_res in string_iter {
                if tags.len() >= max_tags {
                    break;
                }
                let tag = tag_res?;
                tags.push(tag.clone());
                if let Ok(mut sink) = tags_sink.lock() {
                    sink.push(tag);
                }
                progress.fetch_add(1, Ordering::Relaxed);
            }
        } else {
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
                            if tags.len() >= max_tags {
                                break;
                            }
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
                Self::browse_recursive(opc_server, &mut tags, max_tags, progress, tags_sink, 0)?;
            }
        }
        tracing::info!(count = tags.len(), "browse_tags completed");
        Ok(tags)
    }

    fn browse_recursive(
        server: &C::Server,
        tags: &mut Vec<String>,
        max_tags: usize,
        progress: &Arc<AtomicUsize>,
        tags_sink: &Arc<std::sync::Mutex<Vec<String>>>,
        depth: usize,
    ) -> OpcResult<()> {
        const MAX_DEPTH: usize = 50;
        if depth > MAX_DEPTH || tags.len() >= max_tags {
            if depth > MAX_DEPTH {
                tracing::warn!(depth, "Max browse depth reached, truncating");
            }
            return Ok(());
        }

        let branch_enum = server.browse_opc_item_ids(OPC_BRANCH.0 as u32, Some(""), 0, 0)?;

        let branches: Vec<String> = branch_enum
            .filter_map(|r| match r {
                Ok(name) => Some(name),
                Err(e) => {
                    tracing::warn!(error = ?e, "Branch iteration error, skipping");
                    None
                }
            })
            .collect();

        let leaf_enum = server.browse_opc_item_ids(OPC_LEAF.0 as u32, Some(""), 0, 0)?;
        for tag_res in leaf_enum {
            if tags.len() >= max_tags {
                return Ok(());
            }
            let browse_name = tag_res?;
            let tag = match server.get_item_id(&browse_name) {
                Ok(id) => id,
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

        for branch in branches {
            if tags.len() >= max_tags {
                return Ok(());
            }
            if let Err(e) = server.change_browse_position(OPC_BROWSE_DOWN.0 as u32, &branch) {
                tracing::warn!(
                    branch = %branch,
                    error = ?e,
                    "Failed to browse down, skipping branch"
                );
                continue;
            }

            if let Err(e) = Self::browse_recursive(
                server, tags, max_tags, progress, tags_sink, depth + 1,
            ) {
                tracing::warn!(error = ?e, "browse_recursive error");
            }

            if let Err(e) = server.change_browse_position(OPC_BROWSE_UP.0 as u32, "") {
                tracing::warn!(error = ?e, "Failed to browse up, stopping recursion");
                break;
            }
        }

        Ok(())
    }
}
"""

with open("src/com_worker.rs", "r", encoding="cp1252") as f:
    existing = f.read()

import re
match = re.search(r'#\[cfg\(test\)\]\s*mod tests \{.*$', existing, re.DOTALL)
tests_block = match.group(0) if match else ''

with open("src/com_worker.rs", "w", encoding="utf-8") as f:
    f.write(content + "\n" + tests_block)

print("done")
