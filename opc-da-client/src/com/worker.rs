#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use crate::com::connector::{
    ConnectedGroup, ConnectedServer, DataSource, GroupConfig, GroupItemDef, ServerConnector,
};
use crate::errors::{OpcError, OpcResult};
use crate::provider::{OpcQuality, OpcValue, TagValue, WriteResult};
use crate::types::{BrowseDirection, BrowseType, GroupHandle, ItemHandle, NamespaceType};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot};

/// Represents a asynchronous request dispatched to the COM worker thread.
pub enum ComRequest {
    /// Request to enumerate available OPC DA servers on a host.
    ListServers {
        /// Hostname or IP address to target.
        host: String,
        /// One-shot channel to send back the server enumeration result.
        reply: oneshot::Sender<OpcResult<Vec<String>>>,
    },
    /// Request to read current values, quality, and timestamps for tag IDs.
    ReadTagValues {
        /// OPC server ProgID.
        server: String,
        /// List of fully qualified tag identifiers to read.
        tag_ids: Vec<String>,
        /// One-shot channel to send back the tag values result.
        reply: oneshot::Sender<OpcResult<Vec<TagValue>>>,
    },
    /// Request to write a typed value to a single tag.
    WriteTagValue {
        /// OPC server ProgID.
        server: String,
        /// Tag identifier to write.
        tag_id: String,
        /// Typed value to write.
        value: OpcValue,
        /// One-shot channel to send back the write operation result.
        reply: oneshot::Sender<OpcResult<WriteResult>>,
    },
    /// Request to recursively browse available tags on a server.
    BrowseTags {
        /// OPC server ProgID.
        server: String,
        /// Maximum number of tags to discover before stopping.
        max_tags: usize,
        /// Atomic counter tracking total tags discovered.
        progress: Arc<AtomicUsize>,
        /// Shared mutex-protected vector storing discovered tag names incrementally.
        tags_sink: Arc<std::sync::Mutex<Vec<String>>>,
        /// One-shot channel to send back the complete tag discovery list.
        reply: oneshot::Sender<OpcResult<Vec<String>>>,
    },
}

/// Dedicated background worker thread manager handling COM MTA apartment thread affinity.
///
/// Dispatches requests received over an `mpsc` channel to Windows COM interfaces while maintaining
/// a persistent connection pool and transparently evicting stale connection handles on RPC errors.
pub struct ComWorker<C: ServerConnector + 'static> {
    /// Channel sender for dispatching requests to the worker loop.
    pub sender: mpsc::Sender<ComRequest>,
    /// Thread join handle for clean worker thread teardown.
    pub handle: Option<std::thread::JoinHandle<()>>,
    _phantom: std::marker::PhantomData<C>,
}

#[allow(clippy::cast_possible_wrap)]
fn is_connection_error(err: &OpcError) -> bool {
    if let OpcError::Com { source } = err {
        let code = source.code().0;
        code == windows::core::HRESULT(0x8007_06BA_u32 as i32).0
            || code == windows::core::HRESULT(0x8007_06BF_u32 as i32).0
            || code == windows::core::HRESULT(0x8007_06BE_u32 as i32).0
            || code == windows::core::HRESULT(0x8008_0005_u32 as i32).0
    } else {
        false
    }
}

impl<C: ServerConnector + 'static> ComWorker<C> {
    /// Creates a dummy/closed `ComWorker` handle used when background worker initialization fails.
    pub fn closed() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self {
            sender: tx,
            handle: None,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Starts the background COM worker thread with default MTA initialization.
    pub fn start(connector: Arc<C>) -> Result<Self, OpcError> {
        Self::start_with_initializer::<crate::com::guard::DefaultComInit>(connector)
    }

    /// Starts the background COM worker thread with a specified COM initialization strategy.
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(connector))]
    pub(crate) fn start_with_initializer<I: crate::com::guard::ComInitializer>(
        connector: Arc<C>,
    ) -> Result<Self, OpcError> {
        let (tx, mut rx) = mpsc::channel(32);
        let (init_tx, init_rx) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            tracing::debug!("COM worker thread spawned, initializing COM (MTA)");
            let _guard = match I::init() {
                Ok(g) => {
                    tracing::info!("COM MTA initialized successfully on worker thread");
                    let _ = init_tx.send(Ok(()));
                    g
                }
                Err(e) => {
                    tracing::error!(error = ?e, "COM worker failed to initialize MTA");
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };

            let mut cache: HashMap<String, C::Server> = HashMap::new();

            while let Some(req) = rx.blocking_recv() {
                match req {
                    ComRequest::ListServers { host, reply } => {
                        let span = tracing::info_span!("opc.list_servers", host = %host);
                        let _enter = span.enter();
                        #[cfg(feature = "dev-diagnostics")]
                        tracing::trace!(host = %host, "list_servers: starting operation");
                        let start = std::time::Instant::now();
                        let servers = connector.enumerate_servers();
                        if let Ok(s) = &servers {
                            tracing::info!(
                                count = s.len(),
                                elapsed_ms =
                                    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                                "list_servers completed"
                            );
                        } else if let Err(e) = &servers {
                            crate::errors::log_opc_error(e, "list_servers");
                            tracing::error!(
                                error = ?e,
                                elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                                "list_servers failed"
                            );
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
                                    &server, max_tags, &progress, &tags_sink, opc_server,
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
            .recv()
            .map_err(|_| OpcError::Internal("COM worker thread panicked during init".into()))??;

        tracing::debug!("COM worker thread started");

        Ok(Self {
            sender: tx,
            handle: Some(handle),
            _phantom: std::marker::PhantomData,
        })
    }

    #[tracing::instrument(skip(self, req_builder))]
    pub async fn send_request<F, R>(&self, req_builder: F) -> OpcResult<R>
    where
        F: FnOnce(oneshot::Sender<OpcResult<R>>) -> ComRequest,
    {
        if self
            .handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            tracing::error!("COM worker thread panicked or exited unexpectedly");
            return Err(OpcError::Internal("COM worker thread panicked".into()));
        }

        let (tx, rx) = oneshot::channel();
        let req = req_builder(tx);

        self.sender
            .send(req)
            .await
            .map_err(|_| OpcError::Internal("COM worker channel closed (worker stopped)".into()))?;

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
        let server_ref = match cache.entry(server_name.to_string()) {
            std::collections::hash_map::Entry::Occupied(e) => {
                tracing::trace!(server = %server_name, "Cache hit");
                e.into_mut()
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                tracing::debug!(server = %server_name, "Cache miss, connecting");
                let srv = connector.connect(server_name)?;
                tracing::info!(server = %server_name, "Connection established, added to pool");
                e.insert(srv)
            }
        };

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
                tracing::info!(server = %server_name, "Reconnection successful, pool updated");
                cache.insert(server_name.to_string(), fresh_srv);
                result
            }
            other => other,
        }
    }

    #[allow(clippy::too_many_lines)]
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
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_name,
            tag_count = tag_ids.len(),
            sample_tags = ?tag_ids.iter().take(5).collect::<Vec<_>>(),
            "read_tag_values: starting operation"
        );
        let start = std::time::Instant::now();

        let created = opc_server.add_group(&GroupConfig {
            name: "opc-da-client-read",
            active: true,
            update_rate_ms: 1000,
            client_handle: GroupHandle::default(),
            time_bias: 0,
            percent_deadband: 0.0,
            locale_id: 0,
        })?;
        let group = created.group;
        let server_handle = created.server_handle;

        let item_defs: Vec<GroupItemDef> = tag_ids
            .iter()
            .enumerate()
            .map(|(idx, tag_id)| GroupItemDef {
                item_id: tag_id.clone(),
                #[allow(clippy::cast_possible_truncation)]
                client_handle: ItemHandle(idx as u32),
                active: true,
            })
            .collect();

        let results = group.add_items(&item_defs)?;

        if results.len() != tag_ids.len() {
            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, operation = "read_tag_values", "Failed to remove OPC group during cleanup");
            }
            return Err(OpcError::Internal(
                "OPC server returned mismatched result array sizes".into(),
            ));
        }

        let mut tag_values: Vec<TagValue> = tag_ids
            .iter()
            .map(|tag_id| TagValue {
                tag_id: tag_id.clone(),
                value: None,
                quality: OpcQuality::BAD_CONFIG_ERROR,
                timestamp: None,
            })
            .collect();

        let mut server_handles: Vec<ItemHandle> = Vec::new();
        let mut valid_indices = Vec::new();

        for (idx, item_result) in results.iter().enumerate() {
            if item_result.error.is_none() {
                server_handles.push(item_result.server_handle);
                valid_indices.push(idx);
            } else {
                let err_msg = item_result
                    .error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                tracing::warn!(
                    tag = %tag_ids[idx],
                    error = %err_msg,
                    "read_tag_values: add_items rejected tag"
                );
                tag_values[idx].quality = OpcQuality::BAD_CONFIG_ERROR;
            }
        }

        if server_handles.is_empty() {
            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, operation = "read_tag_values", "Failed to remove OPC group during cleanup");
            }
            return Ok(tag_values);
        }

        let item_states = group.read(DataSource::Device, &server_handles)?;

        for (i, idx) in valid_indices.iter().enumerate() {
            match &item_states[i] {
                Ok(state) => {
                    tag_values[*idx] = TagValue {
                        tag_id: tag_ids[*idx].clone(),
                        value: Some(state.value.clone()),
                        quality: state.quality,
                        timestamp: Some(state.timestamp),
                    };
                }
                Err(e) => {
                    tracing::warn!(
                        tag = %tag_ids[*idx],
                        error = ?e,
                        "read_tag_values: per-item read error"
                    );
                    tag_values[*idx] = TagValue {
                        tag_id: tag_ids[*idx].clone(),
                        value: None,
                        quality: OpcQuality::BAD_COMM_FAILURE,
                        timestamp: None,
                    };
                }
            }
        }

        tracing::info!(
            count = tag_values.len(),
            elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            "read_tag_values completed"
        );
        if let Err(e) = opc_server.remove_group(server_handle, true) {
            tracing::warn!(error = ?e, operation = "read_tag_values", "Failed to remove OPC group during cleanup");
        }
        Ok(tag_values)
    }

    #[allow(clippy::too_many_lines)]
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
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_name,
            tag = %tag_id,
            value = ?value,
            "write_tag_value: starting operation"
        );
        let start = std::time::Instant::now();

        let created = opc_server.add_group(&GroupConfig {
            name: "opc-da-client-write",
            active: true,
            update_rate_ms: 1000,
            client_handle: GroupHandle(0),
            time_bias: 0,
            percent_deadband: 0.0,
            locale_id: 0,
        })?;
        let group = created.group;
        let server_handle = created.server_handle;

        let item_def = GroupItemDef {
            item_id: tag_id.to_string(),
            client_handle: ItemHandle(0),
            active: true,
        };

        let results = group.add_items(&[item_def])?;
        let item_res = results
            .first()
            .ok_or_else(|| OpcError::Internal("Server returned empty item results".to_string()))?;

        if let Some(e) = &item_res.error {
            tracing::warn!(error = ?e, "write_tag_value: failed to add tag to group");
            if let Err(err) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?err, operation = "write_tag_value", "Failed to remove OPC group during cleanup");
            }
            return Ok(WriteResult {
                tag_id: tag_id.to_string(),
                success: false,
                error: Some(format!("Failed to add tag: {e}")),
            });
        }

        let item_handle = item_res.server_handle;
        let write_results = group.write(&[item_handle], std::slice::from_ref(value))?;
        let write_res = write_results
            .first()
            .ok_or_else(|| OpcError::Internal("Server returned empty write errors".to_string()))?;

        let write_result = match write_res {
            Ok(()) => {
                tracing::info!(
                    elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "write_tag_value completed"
                );
                WriteResult {
                    tag_id: tag_id.to_string(),
                    success: true,
                    error: None,
                }
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(
                    error = %msg,
                    elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "write_tag_value: server rejected write"
                );
                WriteResult {
                    tag_id: tag_id.to_string(),
                    success: false,
                    error: Some(msg),
                }
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
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_name,
            max_tags,
            "browse_tags: starting operation"
        );
        let start = std::time::Instant::now();

        let org = opc_server.query_organization()?;
        let mut tags = Vec::new();

        if org == NamespaceType::Flat as u32 {
            let string_iter = opc_server.browse_opc_item_ids(BrowseType::Leaf, Some(""), 0, 0)?;
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
            let use_flat = match opc_server.browse_opc_item_ids(BrowseType::Flat, Some(""), 0, 0) {
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
        tracing::info!(
            count = tags.len(),
            elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            "browse_tags completed"
        );
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

        let branch_enum = server.browse_opc_item_ids(BrowseType::Branch, Some(""), 0, 0)?;

        let branches: Vec<String> = branch_enum
            .filter_map(|r| match r {
                Ok(name) => Some(name),
                Err(e) => {
                    tracing::warn!(error = ?e, "Branch iteration error, skipping");
                    None
                }
            })
            .collect();

        let leaf_enum = server.browse_opc_item_ids(BrowseType::Leaf, Some(""), 0, 0)?;
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
            if let Err(e) = server.change_browse_position(BrowseDirection::Down, &branch) {
                tracing::warn!(
                    branch = %branch,
                    error = ?e,
                    "Failed to browse down, skipping branch"
                );
                continue;
            }

            if let Err(e) =
                Self::browse_recursive(server, tags, max_tags, progress, tags_sink, depth + 1)
            {
                tracing::warn!(error = ?e, "browse_recursive error");
            }

            if let Err(e) = server.change_browse_position(BrowseDirection::Up, "") {
                tracing::warn!(error = ?e, "Failed to browse up, stopping recursion");
                break;
            }
        }

        Ok(())
    }
}

impl<C: ServerConnector + 'static> Drop for ComWorker<C> {
    fn drop(&mut self) {
        tracing::debug!("ComWorker dropping — channel closing, signaling thread shutdown");
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_char_pattern,
        clippy::cast_possible_wrap,
        clippy::ptr_as_ptr,
        clippy::borrow_as_ptr,
        clippy::mixed_attributes_style,
        clippy::unreadable_literal,
        clippy::undocumented_unsafe_blocks,
        clippy::manual_assert
    )]
    use super::*;
    use crate::com::connector::{
        ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
        GroupItemResult, GroupItemState, ServerConnector, StringIterator,
    };

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct MockState {
        connect_count: AtomicUsize,
        should_fail_connect: AtomicBool,
        should_fail_write: AtomicBool,
        should_fail_with_connection_error: AtomicBool,
        should_panic_on_request: AtomicBool,
    }

    struct ConfigurableMockConnector {
        state: Arc<MockState>,
    }

    struct ConfigurableMockServer {
        state: Arc<MockState>,
    }

    struct ConfigurableMockGroup {
        state: Arc<MockState>,
    }

    impl ConnectedGroup for ConfigurableMockGroup {
        fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
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

        fn read(
            &self,
            _source: DataSource,
            _server_handles: &[ItemHandle],
        ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
            Ok(vec![])
        }

        fn write(
            &self,
            server_handles: &[ItemHandle],
            _values: &[OpcValue],
        ) -> OpcResult<Vec<Result<(), OpcError>>> {
            if self
                .state
                .should_fail_with_connection_error
                .load(Ordering::Relaxed)
            {
                // RPC server unavailable (0x800706BA) triggers connection eviction
                return Err(OpcError::Com {
                    source: windows::core::Error::from_hresult(windows::core::HRESULT(
                        0x800706BA_u32 as i32,
                    )),
                });
            }

            if self.state.should_fail_write.load(Ordering::Relaxed) {
                Ok(server_handles
                    .iter()
                    .map(|_| {
                        Err(OpcError::Com {
                            source: windows::core::Error::from_hresult(
                                windows::Win32::Foundation::E_FAIL,
                            ),
                        })
                    })
                    .collect())
            } else {
                Ok(server_handles.iter().map(|_| Ok(())).collect())
            }
        }
    }

    impl ConnectedServer for ConfigurableMockServer {
        type Group = ConfigurableMockGroup;

        fn query_organization(&self) -> OpcResult<u32> {
            Ok(0)
        }

        fn browse_opc_item_ids(
            &self,
            _browse_type: BrowseType,
            _filter: Option<&str>,
            _data_type: u16,
            _access_rights: u32,
        ) -> OpcResult<StringIterator> {
            Err(OpcError::NotImplemented("mock".into()))
        }

        fn change_browse_position(
            &self,
            _direction: BrowseDirection,
            _name: &str,
        ) -> OpcResult<()> {
            Ok(())
        }

        fn get_item_id(&self, _item_name: &str) -> OpcResult<String> {
            Ok(String::new())
        }

        fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
            if self.state.should_panic_on_request.load(Ordering::Relaxed) {
                panic!("Simulated worker panic");
            }
            Ok(CreatedGroup {
                group: ConfigurableMockGroup {
                    state: self.state.clone(),
                },
                server_handle: GroupHandle(1),
                revised_update_rate_ms: config.update_rate_ms,
            })
        }

        fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
            Ok(())
        }
    }

    impl ServerConnector for ConfigurableMockConnector {
        type Server = ConfigurableMockServer;

        fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
            if self.state.should_fail_connect.load(Ordering::Relaxed) {
                Err(OpcError::Internal("Server enumeration failed".into()))
            } else {
                Ok(vec!["Mock.Server.1".into()])
            }
        }

        fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
            if self.state.should_fail_connect.load(Ordering::Relaxed) {
                Err(OpcError::Internal("Connection failed".into()))
            } else {
                self.state.connect_count.fetch_add(1, Ordering::Relaxed);
                Ok(ConfigurableMockServer {
                    state: self.state.clone(),
                })
            }
        }
    }

    struct WorkerMockConnector;
    struct WorkerMockServer;
    struct WorkerMockGroup;

    impl ConnectedGroup for WorkerMockGroup {
        fn add_items(&self, _items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn read(
            &self,
            _source: DataSource,
            _server_handles: &[ItemHandle],
        ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn write(
            &self,
            _server_handles: &[ItemHandle],
            _values: &[OpcValue],
        ) -> OpcResult<Vec<Result<(), OpcError>>> {
            Err(OpcError::NotImplemented("mock".into()))
        }
    }

    impl ConnectedServer for WorkerMockServer {
        type Group = WorkerMockGroup;
        fn query_organization(&self) -> OpcResult<u32> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn browse_opc_item_ids(
            &self,
            _browse_type: BrowseType,
            _filter: Option<&str>,
            _data_type: u16,
            _access_rights: u32,
        ) -> OpcResult<StringIterator> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn change_browse_position(
            &self,
            _direction: BrowseDirection,
            _name: &str,
        ) -> OpcResult<()> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn get_item_id(&self, _item_name: &str) -> OpcResult<String> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn add_group(&self, _config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
            Err(OpcError::NotImplemented("mock".into()))
        }
    }

    impl ServerConnector for WorkerMockConnector {
        type Server = WorkerMockServer;
        fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
            Ok(vec!["Mock.Server.1".into()])
        }
        fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
            Ok(WorkerMockServer)
        }
    }

    #[tokio::test]
    async fn test_worker_starts_and_stops() {
        let worker = tokio::task::spawn_blocking(|| {
            ComWorker::start(Arc::new(WorkerMockConnector)).unwrap()
        })
        .await
        .unwrap();
        drop(worker);
    }

    #[tokio::test]
    async fn test_worker_list_servers() {
        let worker = tokio::task::spawn_blocking(|| {
            ComWorker::start(Arc::new(WorkerMockConnector)).unwrap()
        })
        .await
        .unwrap();
        let (reply, _rx) = oneshot::channel();
        worker
            .sender
            .send(ComRequest::ListServers {
                host: "localhost".into(),
                reply,
            })
            .await
            .unwrap();
        // Wait for implementation
    }

    struct MismatchedConnector;
    struct MismatchedServer;
    struct MismatchedGroup;

    impl ConnectedGroup for MismatchedGroup {
        fn add_items(&self, _items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
            Ok(vec![])
        }
        fn read(
            &self,
            _source: DataSource,
            _server_handles: &[ItemHandle],
        ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
            Ok(vec![])
        }
        fn write(
            &self,
            _server_handles: &[ItemHandle],
            _values: &[OpcValue],
        ) -> OpcResult<Vec<Result<(), OpcError>>> {
            Ok(vec![])
        }
    }

    impl ConnectedServer for MismatchedServer {
        type Group = MismatchedGroup;
        fn query_organization(&self) -> OpcResult<u32> {
            Ok(0)
        }
        fn browse_opc_item_ids(
            &self,
            _b: BrowseType,
            _f: Option<&str>,
            _d: u16,
            _a: u32,
        ) -> OpcResult<StringIterator> {
            Err(OpcError::NotImplemented("mock".into()))
        }
        fn change_browse_position(
            &self,
            _direction: BrowseDirection,
            _name: &str,
        ) -> OpcResult<()> {
            Ok(())
        }
        fn get_item_id(&self, _item_name: &str) -> OpcResult<String> {
            Ok(String::new())
        }
        fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
            Ok(CreatedGroup {
                group: MismatchedGroup,
                server_handle: GroupHandle(1),
                revised_update_rate_ms: config.update_rate_ms,
            })
        }
        fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
            Ok(())
        }
    }

    impl ServerConnector for MismatchedConnector {
        type Server = MismatchedServer;
        fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
            Ok(vec![])
        }
        fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
            Ok(MismatchedServer)
        }
    }

    #[tokio::test]
    async fn test_worker_read_tag_values_mismatched_lengths() {
        let worker = tokio::task::spawn_blocking(|| {
            ComWorker::start(Arc::new(MismatchedConnector)).unwrap()
        })
        .await
        .unwrap();

        let result = worker
            .send_request(|reply| ComRequest::ReadTagValues {
                server: "MockServer".to_string(),
                tag_ids: vec!["Tag1".to_string(), "Tag2".to_string()],
                reply,
            })
            .await;

        assert!(
            result.is_err(),
            "Expected read to fail due to mismatched lengths"
        );
        if let Err(OpcError::Internal(msg)) = result {
            assert!(msg.contains("mismatched result array sizes"));
        } else {
            panic!("Expected OpcError::Internal, got {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_worker_write_tag_value() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(ConfigurableMockConnector {
            state: state.clone(),
        });
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let result = worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: "Mock.Server.1".to_string(),
                tag_id: "Random.Int4".to_string(),
                value: OpcValue::Int(42),
                reply,
            })
            .await
            .expect("Request should succeed");

        assert_eq!(result.tag_id, "Random.Int4");
        assert!(result.success, "Write should be successful");
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_connection_cache_reuse() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(ConfigurableMockConnector {
            state: state.clone(),
        });
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let _ = worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: "Mock.Server.1".to_string(),
                tag_id: "Tag1".to_string(),
                value: OpcValue::Int(1),
                reply,
            })
            .await
            .unwrap();

        let _ = worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: "Mock.Server.1".to_string(),
                tag_id: "Tag2".to_string(),
                value: OpcValue::Int(2),
                reply,
            })
            .await
            .unwrap();

        assert_eq!(
            state.connect_count.load(Ordering::Relaxed),
            1,
            "Server connection should be cached and reused"
        );
    }

    #[tokio::test]
    async fn test_stale_connection_eviction() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(ConfigurableMockConnector {
            state: state.clone(),
        });
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        // Initial connect
        let _ = worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: "Mock.Server.1".to_string(),
                tag_id: "Tag1".to_string(),
                value: OpcValue::Int(1),
                reply,
            })
            .await
            .unwrap();

        assert_eq!(state.connect_count.load(Ordering::Relaxed), 1);

        // Enable connection error flag to trigger eviction on next operation
        state
            .should_fail_with_connection_error
            .store(true, Ordering::Relaxed);

        // Next request triggers eviction and reconnect attempt
        let _ = worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: "Mock.Server.1".to_string(),
                tag_id: "Tag2".to_string(),
                value: OpcValue::Int(2),
                reply,
            })
            .await;

        assert_eq!(
            state.connect_count.load(Ordering::Relaxed),
            2,
            "Stale connection should be evicted and reconnected"
        );
    }

    #[tokio::test]
    async fn test_worker_panic_propagation() {
        let state = Arc::new(MockState::default());
        state.should_panic_on_request.store(true, Ordering::Relaxed);
        let connector = Arc::new(ConfigurableMockConnector {
            state: state.clone(),
        });
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let result = worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: "Mock.Server.1".to_string(),
                tag_id: "Tag1".to_string(),
                value: OpcValue::Int(1),
                reply,
            })
            .await;

        assert!(result.is_err());
        if let Err(OpcError::Internal(msg)) = result {
            assert!(
                msg.contains("shut down")
                    || msg.contains("channel closed")
                    || msg.contains("panicked"),
                "Expected worker termination message, got: {}",
                msg
            );
        } else {
            panic!("Expected OpcError::Internal, got {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_drop_during_active_request() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(ConfigurableMockConnector {
            state: state.clone(),
        });
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        // Dropping worker handle closes channel gracefully
        drop(worker);
    }

    #[tokio::test]
    async fn test_worker_init_failure() {
        struct FailingInitConnector;
        impl ServerConnector for FailingInitConnector {
            type Server = ConfigurableMockServer;
            fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
                Err(OpcError::Internal("COM subsystem failed".into()))
            }
            fn connect(&self, _name: &str) -> OpcResult<Self::Server> {
                Err(OpcError::Internal("COM subsystem failed".into()))
            }
        }

        let worker = tokio::task::spawn_blocking(|| {
            ComWorker::start(Arc::new(FailingInitConnector)).unwrap()
        })
        .await
        .unwrap();

        let result = worker
            .send_request(|reply| ComRequest::ListServers {
                host: "localhost".into(),
                reply,
            })
            .await;

        assert!(
            result.is_err(),
            "ListServers request should fail when connector enumeration fails"
        );
    }

    #[tokio::test]
    async fn test_worker_read_tag_values_quality_decoding() {
        use crate::types::{QualityLimit, QualityMajor, QualitySubstatus};
        struct QualityTestConnector;
        struct QualityTestServer;
        struct QualityTestGroup;

        impl ConnectedGroup for QualityTestGroup {
            fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
                Ok(items
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        if i == 4 {
                            GroupItemResult {
                                server_handle: ItemHandle(0),
                                canonical_type: 0,
                                error: Some(OpcError::Com {
                                    source: windows::core::Error::from_hresult(
                                        windows::Win32::Foundation::E_FAIL,
                                    ),
                                }),
                            }
                        } else {
                            GroupItemResult {
                                server_handle: ItemHandle((i + 1) as u32),
                                canonical_type: 8,
                                error: None,
                            }
                        }
                    })
                    .collect())
            }

            fn read(
                &self,
                _source: DataSource,
                server_handles: &[ItemHandle],
            ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
                let qualities: [u16; 4] = [0x00C0, 0x00D8, 0x0018, 0x0056];
                Ok(server_handles
                    .iter()
                    .enumerate()
                    .map(|(i, &h)| {
                        let val = if i != 2 {
                            OpcValue::Int(42)
                        } else {
                            OpcValue::String(String::new())
                        };
                        Ok(GroupItemState {
                            client_handle: h,
                            value: val,
                            quality: OpcQuality::from(qualities[i % qualities.len()]),
                            timestamp: std::time::SystemTime::UNIX_EPOCH,
                        })
                    })
                    .collect())
            }

            fn write(
                &self,
                _server_handles: &[ItemHandle],
                _values: &[OpcValue],
            ) -> OpcResult<Vec<Result<(), OpcError>>> {
                Ok(vec![])
            }
        }

        impl ConnectedServer for QualityTestServer {
            type Group = QualityTestGroup;
            fn query_organization(&self) -> OpcResult<u32> {
                Ok(0)
            }
            fn browse_opc_item_ids(
                &self,
                _b: BrowseType,
                _f: Option<&str>,
                _d: u16,
                _a: u32,
            ) -> OpcResult<StringIterator> {
                Err(OpcError::NotImplemented("mock".into()))
            }
            fn change_browse_position(&self, _d: BrowseDirection, _n: &str) -> OpcResult<()> {
                Ok(())
            }
            fn get_item_id(&self, _n: &str) -> OpcResult<String> {
                Ok(String::new())
            }
            fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
                Ok(CreatedGroup {
                    group: QualityTestGroup,
                    server_handle: GroupHandle(1),
                    revised_update_rate_ms: config.update_rate_ms,
                })
            }
            fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
                Ok(())
            }
        }

        impl ServerConnector for QualityTestConnector {
            type Server = QualityTestServer;
            fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
                Ok(vec!["Quality.Mock.Server".into()])
            }
            fn connect(&self, _name: &str) -> OpcResult<Self::Server> {
                Ok(QualityTestServer)
            }
        }

        let worker = tokio::task::spawn_blocking(|| {
            ComWorker::start(Arc::new(QualityTestConnector)).unwrap()
        })
        .await
        .unwrap();

        let tag_ids = vec![
            "Tag.Good".to_string(),
            "Tag.Override".to_string(),
            "Tag.Comm".to_string(),
            "Tag.Limit".to_string(),
            "Tag.Rejected".to_string(),
        ];

        let results = worker
            .send_request(|reply| ComRequest::ReadTagValues {
                server: "Quality.Mock.Server".to_string(),
                tag_ids,
                reply,
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 5);

        // Tag 0: Good standard (0x00C0)
        assert_eq!(results[0].tag_id, "Tag.Good");
        assert_eq!(results[0].value, Some(OpcValue::Int(42)));
        assert_eq!(results[0].display_value(), "42");
        assert_eq!(results[0].quality.major, QualityMajor::Good);
        assert_eq!(results[0].quality.substatus, QualitySubstatus::NonSpecific);
        assert_eq!(results[0].quality.limit, QualityLimit::NotLimited);
        assert_eq!(results[0].quality.to_string(), "Good");
        assert!(results[0].quality.is_good());
        assert!(!results[0].quality.is_bad());
        assert!(results[0].is_good());
        assert!(!results[0].is_error());

        // Tag 1: Good with Local Override (0x00D8)
        assert_eq!(results[1].tag_id, "Tag.Override");
        assert_eq!(results[1].value, Some(OpcValue::Int(42)));
        assert_eq!(results[1].quality.major, QualityMajor::Good);
        assert_eq!(
            results[1].quality.substatus,
            QualitySubstatus::LocalOverride
        );
        assert_eq!(results[1].quality.to_string(), "Good (Local Override)");

        // Tag 2: Bad with Comm Failure (0x0018)
        assert_eq!(results[2].tag_id, "Tag.Comm");
        assert_eq!(results[2].value, Some(OpcValue::String(String::new())));
        assert_eq!(results[2].quality.major, QualityMajor::Bad);
        assert_eq!(results[2].quality.substatus, QualitySubstatus::CommFailure);
        assert_eq!(results[2].quality.to_string(), "Bad (Comm Failure)");
        assert!(results[2].quality.is_bad());

        // Tag 3: Uncertain with EGU Exceeded and High Limited (0x0056)
        assert_eq!(results[3].tag_id, "Tag.Limit");
        assert_eq!(results[3].value, Some(OpcValue::Int(42)));
        assert_eq!(results[3].quality.major, QualityMajor::Uncertain);
        assert_eq!(results[3].quality.substatus, QualitySubstatus::EguExceeded);
        assert_eq!(results[3].quality.limit, QualityLimit::HighLimited);
        assert_eq!(
            results[3].quality.to_string(),
            "Uncertain (EGU Exceeded) [High Limited]"
        );
        assert!(results[3].quality.is_uncertain());
        assert!(results[3].quality.is_limited());

        // Tag 4: Rejected at add_items
        assert_eq!(results[4].tag_id, "Tag.Rejected");
        assert_eq!(results[4].value, None);
        assert_eq!(results[4].display_value(), "Error");
        assert_eq!(results[4].timestamp, None);
        assert_eq!(results[4].formatted_timestamp(), "N/A");
        assert!(results[4].is_error());
        assert_eq!(results[4].quality, OpcQuality::BAD_CONFIG_ERROR);
        assert_eq!(results[4].quality.to_string(), "Bad (Configuration Error)");
    }

    #[test]
    fn test_worker_com_init_failure_propagates_opc_error() {
        let connector = Arc::new(ConfigurableMockConnector {
            state: Arc::new(MockState::default()),
        });
        let result =
            ComWorker::start_with_initializer::<crate::com::guard::FailingComInit>(connector);
        assert!(result.is_err());
        let Err(err) = result else { unreachable!() };
        assert!(
            !err.to_string().contains("COM init failed on worker"),
            "Expected forwarded OpcError, got hardcoded string: {err}"
        );
        assert!(
            err.to_string().contains("Synthetic COM init failure"),
            "Expected synthetic failure message, got: {err}"
        );
    }
}
