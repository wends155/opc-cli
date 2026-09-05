#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use crate::com::connector::{
    ConnectedGroup, ConnectedServer, DataSource, GroupConfig, GroupItemDef, ServerConnector,
};
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::provider::{OpcQuality, OpcValue, TagCollector, TagValue, WriteResult};
use crate::types::{BrowseDirection, BrowseType, GroupHandle, ItemHandle, NamespaceType};
use std::collections::HashMap;
use std::sync::Arc;
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
        /// Configured tag collector managing capacity, progress, and cancellation.
        collector: TagCollector,
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

/// RAII drop guard for OPC DA server groups.
///
/// Ensures `ConnectedServer::remove_group(server_handle, true)` is called
/// when the guard is dropped, preventing group handle leaks on the OPC server
/// across early returns, error propagation with `?`, and panics.
pub(crate) struct GroupGuard<'a, S: ConnectedServer> {
    server: &'a S,
    handle: GroupHandle,
    disarmed: bool,
}

impl<'a, S: ConnectedServer> GroupGuard<'a, S> {
    pub(crate) fn new(server: &'a S, handle: GroupHandle) -> Self {
        Self {
            server,
            handle,
            disarmed: false,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> GroupHandle {
        self.handle
    }

    #[allow(dead_code)]
    pub(crate) fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl<S: ConnectedServer> Drop for GroupGuard<'_, S> {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        if let Err(e) = self.server.remove_group(self.handle, true) {
            tracing::warn!(
                error = ?e,
                handle = self.handle.0,
                "Failed to remove OPC group during RAII drop cleanup"
            );
        }
    }
}

fn is_connection_error(err: &OpcError) -> bool {
    if let OpcError::Com { source } = err {
        crate::raw::hresult::is_connection_hresult(source.code())
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
                            log_opc_err!(
                                e,
                                OpcOperation::ListServers,
                                host = %host,
                                elapsed_ms =
                                    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
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
                        collector,
                        reply,
                    } => {
                        let result = Self::dispatch_with_retry(
                            &mut cache,
                            &connector,
                            &server,
                            |opc_server| Self::handle_browse(&server, &collector, opc_server),
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

    #[tracing::instrument(level = "debug", skip(cache, connector, operation))]
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
                log_opc_err!(
                    &e,
                    OpcOperation::DispatchConnectionError,
                    server = %server_name,
                    action = "evicting_stale_connection"
                );
                cache.remove(server_name);
                tracing::debug!(server = %server_name, "Reconnecting");
                let fresh_srv = connector.connect(server_name).inspect_err(|connect_e| {
                    log_opc_err!(
                        connect_e,
                        OpcOperation::DispatchReconnect,
                        server = %server_name
                    );
                })?;
                let fresh_ref = &fresh_srv;
                let result = operation(fresh_ref);
                if let Err(ref op_e) = result {
                    log_opc_err!(
                        op_e,
                        OpcOperation::DispatchRetriedOperation,
                        server = %server_name
                    );
                }
                tracing::info!(server = %server_name, "Reconnection successful, pool updated");
                cache.insert(server_name.to_string(), fresh_srv);
                result
            }
            Err(e) => {
                log_opc_err!(
                    &e,
                    OpcOperation::DispatchOperation,
                    server = %server_name
                );
                Err(e)
            }
            Ok(v) => Ok(v),
        }
    }

    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(
        name = "opc.read_tag_values",
        level = "info",
        skip(tag_ids, opc_server),
        fields(tag_count = tag_ids.len()),
        err
    )]
    fn handle_read(
        server_name: &str,
        tag_ids: &[String],
        opc_server: &C::Server,
    ) -> OpcResult<Vec<TagValue>> {
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_name,
            tag_count = tag_ids.len(),
            sample_tags = ?tag_ids.iter().take(5).collect::<Vec<_>>(),
            "read_tag_values: starting operation"
        );
        let start = std::time::Instant::now();

        let created = opc_server
            .add_group(&GroupConfig {
                name: "opc-da-client-read",
                active: true,
                update_rate_ms: 1000,
                client_handle: GroupHandle::default(),
                time_bias: 0,
                percent_deadband: 0.0,
                locale_id: 0,
            })
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::ReadAddGroup,
                    server = %server_name,
                    tag_count = tag_ids.len()
                );
            })?;
        let group = created.group;
        let _group_guard = GroupGuard::new(opc_server, created.server_handle);

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

        let results = group.add_items(&item_defs).inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::ReadAddItems,
                server = %server_name,
                tag_count = tag_ids.len()
            );
        })?;

        if results.len() != tag_ids.len() {
            let err =
                OpcError::Internal("OPC server returned mismatched result array sizes".into());
            log_opc_err!(
                &err,
                OpcOperation::ReadMismatchedResults,
                server = %server_name,
                expected = tag_ids.len(),
                actual = results.len()
            );
            return Err(err);
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
                    server = %server_name,
                    tag = %tag_ids[idx],
                    error = %err_msg,
                    "read_tag_values: add_items rejected tag"
                );
                tag_values[idx].quality = OpcQuality::BAD_CONFIG_ERROR;
            }
        }

        if server_handles.is_empty() {
            return Ok(tag_values);
        }

        let item_states = group
            .read(DataSource::Device, &server_handles)
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::ReadSync,
                    server = %server_name,
                    handle_count = server_handles.len()
                );
            })?;

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
                    log_opc_err!(
                        e,
                        OpcOperation::ReadPerItem,
                        server = %server_name,
                        tag = %tag_ids[*idx]
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
        Ok(tag_values)
    }

    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(
        name = "opc.write_tag_value",
        level = "info",
        skip(value, opc_server),
        fields(tag = %tag_id),
        err
    )]
    fn handle_write(
        server_name: &str,
        tag_id: &str,
        value: &OpcValue,
        opc_server: &C::Server,
    ) -> OpcResult<WriteResult> {
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_name,
            tag = %tag_id,
            value = ?value,
            "write_tag_value: starting operation"
        );
        let start = std::time::Instant::now();

        let created = opc_server
            .add_group(&GroupConfig {
                name: "opc-da-client-write",
                active: true,
                update_rate_ms: 1000,
                client_handle: GroupHandle(0),
                time_bias: 0,
                percent_deadband: 0.0,
                locale_id: 0,
            })
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::WriteAddGroup,
                    server = %server_name,
                    tag = %tag_id,
                    value = ?value
                );
            })?;
        let group = created.group;
        let _group_guard = GroupGuard::new(opc_server, created.server_handle);

        let item_def = GroupItemDef {
            item_id: tag_id.to_string(),
            client_handle: ItemHandle(0),
            active: true,
        };

        let results = group.add_items(&[item_def]).inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::WriteAddItems,
                server = %server_name,
                tag = %tag_id,
                value = ?value
            );
        })?;
        let item_res = results.first().ok_or_else(|| {
            let e = OpcError::Internal("Server returned empty item results".to_string());
            log_opc_err!(
                &e,
                OpcOperation::WriteEmptyItemResults,
                server = %server_name,
                tag = %tag_id,
                value = ?value
            );
            e
        })?;

        if let Some(e) = &item_res.error {
            log_opc_err!(
                e,
                OpcOperation::WriteAddItemsRejected,
                server = %server_name,
                tag = %tag_id,
                value = ?value
            );
            return Ok(WriteResult::failure(tag_id, e.clone()));
        }

        let item_handle = item_res.server_handle;
        let write_results = group
            .write(&[item_handle], std::slice::from_ref(value))
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::WriteSync,
                    server = %server_name,
                    tag = %tag_id,
                    value = ?value
                );
            })?;
        let write_res = write_results.first().ok_or_else(|| {
            let e = OpcError::Internal("Server returned empty write errors".to_string());
            log_opc_err!(
                &e,
                OpcOperation::WriteEmptyWriteErrors,
                server = %server_name,
                tag = %tag_id,
                value = ?value
            );
            e
        })?;

        let write_result = match write_res {
            Ok(()) => {
                tracing::info!(
                    elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "write_tag_value completed"
                );
                WriteResult::success(tag_id)
            }
            Err(e) => {
                log_opc_err!(
                    e,
                    OpcOperation::WriteServerRejected,
                    server = %server_name,
                    tag = %tag_id,
                    value = ?value,
                    elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
                );
                WriteResult::failure(tag_id, e.clone())
            }
        };

        Ok(write_result)
    }

    #[tracing::instrument(
        name = "opc.browse_tags",
        level = "info",
        skip(collector, opc_server),
        fields(max_tags = collector.max_tags()),
        err
    )]
    fn handle_browse(
        server_name: &str,
        collector: &TagCollector,
        opc_server: &C::Server,
    ) -> OpcResult<Vec<String>> {
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_name,
            max_tags = collector.max_tags(),
            "browse_tags: starting operation"
        );
        let start = std::time::Instant::now();

        if collector.is_cancelled() || collector.is_full() {
            return Ok(collector.snapshot());
        }

        let org = opc_server.query_organization().inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::BrowseQueryOrganization,
                server = %server_name
            );
        })?;

        if org == NamespaceType::Flat as u32 {
            let string_iter = opc_server
                .browse_opc_item_ids(BrowseType::Leaf, Some(""), 0, 0)
                .inspect_err(|e| {
                    log_opc_err!(
                        e,
                        OpcOperation::BrowseFlatLeaves,
                        server = %server_name
                    );
                })?;
            for tag_res in string_iter {
                let tag = tag_res.inspect_err(|e| {
                    log_opc_err!(
                        e,
                        OpcOperation::BrowseFlatLeafItem,
                        server = %server_name
                    );
                })?;
                if !collector.push(tag) {
                    break;
                }
            }
        } else {
            let use_flat = match opc_server.browse_opc_item_ids(BrowseType::Flat, Some(""), 0, 0) {
                Ok(mut flat_enum) => match flat_enum.next() {
                    Some(Ok(first_tag)) => {
                        tracing::info!("OPC_FLAT browse supported — using fast flat enumeration");
                        if collector.push(first_tag) {
                            for tag_res in flat_enum {
                                match tag_res {
                                    Ok(tag) => {
                                        if !collector.push(tag) {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        log_opc_err!(
                                            &e,
                                            OpcOperation::BrowseFlatEnumItem,
                                            server = %server_name
                                        );
                                    }
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
                Self::browse_recursive(opc_server, collector, 0)?;
            }
        }
        let result = collector.snapshot();
        tracing::info!(
            count = result.len(),
            elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            "browse_tags completed"
        );
        Ok(result)
    }

    #[tracing::instrument(level = "debug", skip(server, collector), err)]
    fn browse_recursive(
        server: &C::Server,
        collector: &TagCollector,
        depth: usize,
    ) -> OpcResult<()> {
        const MAX_DEPTH: usize = 50;
        if depth > MAX_DEPTH || collector.is_cancelled() || collector.is_full() {
            if depth > MAX_DEPTH {
                tracing::warn!(depth, "Max browse depth reached, truncating");
            }
            return Ok(());
        }

        let branch_enum = server
            .browse_opc_item_ids(BrowseType::Branch, Some(""), 0, 0)
            .inspect_err(|e| {
                log_opc_err!(e, OpcOperation::BrowseRecursiveBranches, depth = depth);
            })?;

        let branches: Vec<String> = branch_enum
            .filter_map(|r| match r {
                Ok(name) => Some(name),
                Err(e) => {
                    log_opc_err!(&e, OpcOperation::BrowseRecursiveBranchItem, depth = depth);
                    None
                }
            })
            .collect();

        let leaf_enum = server
            .browse_opc_item_ids(BrowseType::Leaf, Some(""), 0, 0)
            .inspect_err(|e| {
                log_opc_err!(e, OpcOperation::BrowseRecursiveLeaves, depth = depth);
            })?;
        for tag_res in leaf_enum {
            if collector.is_cancelled() || collector.is_full() {
                return Ok(());
            }
            let browse_name = tag_res.inspect_err(|e| {
                log_opc_err!(e, OpcOperation::BrowseRecursiveLeafItem, depth = depth);
            })?;
            let tag = match server.get_item_id(&browse_name) {
                Ok(id) => id,
                Err(e) => {
                    log_opc_err!(
                        &e,
                        OpcOperation::BrowseRecursiveGetItemId,
                        browse_name = %browse_name,
                        depth = depth
                    );
                    browse_name
                }
            };
            if !collector.push(tag) {
                return Ok(());
            }
        }

        for branch in branches {
            if collector.is_cancelled() || collector.is_full() {
                return Ok(());
            }
            if let Err(e) = server.change_browse_position(BrowseDirection::Down, &branch) {
                log_opc_err!(
                    &e,
                    OpcOperation::BrowseRecursiveChangePositionDown,
                    branch = %branch,
                    depth = depth
                );
                continue;
            }

            if let Err(e) = Self::browse_recursive(server, collector, depth + 1) {
                log_opc_err!(
                    &e,
                    OpcOperation::BrowseRecursiveChildBranch,
                    depth = depth + 1
                );
            }

            if let Err(e) = server.change_browse_position(BrowseDirection::Up, "") {
                log_opc_err!(
                    &e,
                    OpcOperation::BrowseRecursiveChangePositionUp,
                    depth = depth
                );
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
        GroupItemResult, GroupItemState, MockConnectedGroup, MockConnectedServer,
        MockServerConnector, MockState, ServerConnector, StringIterator,
    };

    use std::sync::atomic::Ordering;

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
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
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
        assert!(result.is_success(), "Write should be successful");
        assert!(result.error().is_none());
    }

    #[tokio::test]
    async fn test_worker_write_tag_value_failure() {
        let state = Arc::new(MockState::default());
        state.should_fail_write.store(true, Ordering::Relaxed);
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
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
            .expect("Request should complete");

        assert_eq!(result.tag_id, "Random.Int4");
        assert!(result.is_error(), "Write should fail");
        match result.status {
            Err(OpcError::Com { source }) => {
                assert_eq!(source.code(), windows::Win32::Foundation::E_FAIL);
            }
            other => panic!("Expected OpcError::Com, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_connection_cache_reuse() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
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
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
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
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
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
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
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
            type Server = std::sync::Arc<MockConnectedServer>;
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
        let connector = Arc::new(MockServerConnector::default());
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

    #[tokio::test]
    async fn test_worker_browse_tags_success() {
        let connector = Arc::new(MockServerConnector::default());
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let collector = TagCollector::new(100);
        let result = worker
            .send_request(|reply| ComRequest::BrowseTags {
                server: "Mock.Server.1".to_string(),
                collector: collector.clone(),
                reply,
            })
            .await
            .expect("BrowseTags request should succeed");

        assert_eq!(result.len(), 3);
        assert_eq!(result, vec!["Random.Int4", "Random.Real8", "Random.String"]);
        assert_eq!(collector.len(), 3);
    }

    #[tokio::test]
    async fn test_worker_browse_tags_cancelled() {
        let connector = Arc::new(MockServerConnector::default());
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let collector = TagCollector::new(100);
        collector.cancel();
        let result = worker
            .send_request(|reply| ComRequest::BrowseTags {
                server: "Mock.Server.1".to_string(),
                collector: collector.clone(),
                reply,
            })
            .await
            .expect("BrowseTags request should succeed when cancelled");

        assert_eq!(result.len(), 0);
        assert_eq!(collector.len(), 0);
    }

    #[tokio::test]
    async fn test_worker_browse_tags_capacity_cap() {
        let connector = Arc::new(MockServerConnector::default());
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let collector = TagCollector::new(2);
        let result = worker
            .send_request(|reply| ComRequest::BrowseTags {
                server: "Mock.Server.1".to_string(),
                collector: collector.clone(),
                reply,
            })
            .await
            .expect("BrowseTags request should succeed up to capacity");

        assert_eq!(result.len(), 2);
        assert_eq!(result, vec!["Random.Int4", "Random.Real8"]);
        assert!(collector.is_full());
    }

    #[tokio::test]
    async fn test_worker_browse_tags_flat_organization() {
        let connector = Arc::new(MockServerConnector::default());
        connector.server.organization.store(2, Ordering::Relaxed); // NamespaceType::Flat = 2
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();

        let collector = TagCollector::new(100);
        let result = worker
            .send_request(|reply| ComRequest::BrowseTags {
                server: "Mock.Server.1".to_string(),
                collector: collector.clone(),
                reply,
            })
            .await
            .expect("BrowseTags request should succeed on flat namespace");

        assert_eq!(result.len(), 3);
        assert_eq!(result, vec!["Random.Int4", "Random.Real8", "Random.String"]);
    }

    #[tokio::test]
    async fn test_worker_tracing_instrumentation_execution() {
        let connector = std::sync::Arc::new(MockServerConnector::default());
        let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
            .await
            .unwrap();
        let servers = worker
            .send_request(|reply| ComRequest::ListServers {
                host: "localhost".into(),
                reply,
            })
            .await
            .expect("list servers");
        assert_eq!(servers, vec!["Matrikon.OPC.Simulation.1".to_string()]);
    }

    #[test]
    fn test_group_guard_cleanup_on_drop() {
        let server = MockConnectedServer::default();
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
        {
            let guard = GroupGuard::new(&server, GroupHandle(42));
            assert_eq!(guard.handle(), GroupHandle(42));
        }
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_group_guard_disarm_prevents_cleanup() {
        let server = MockConnectedServer::default();
        {
            let mut guard = GroupGuard::new(&server, GroupHandle(42));
            guard.disarm();
        }
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_worker_handle_read_error_cleans_group() {
        let connector = Arc::new(MockServerConnector::default());
        let group = MockConnectedGroup {
            add_items_fn: Some(Box::new(|_| {
                Err(OpcError::Internal("Simulated add_items failure".into()))
            })),
            ..Default::default()
        };
        let server = Arc::new(MockConnectedServer {
            group: Arc::new(group),
            state: connector.state.clone(),
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec!["Test.Tag".to_string()])),
            organization: std::sync::atomic::AtomicU32::new(1),
        });
        let custom_connector = Arc::new(MockServerConnector {
            server: server.clone(),
            state: connector.state.clone(),
            servers: connector.servers.clone(),
        });
        let worker =
            tokio::task::spawn_blocking(move || ComWorker::start(custom_connector).unwrap())
                .await
                .unwrap();

        let result = worker
            .send_request(|reply| ComRequest::ReadTagValues {
                server: "Mock.Server.1".to_string(),
                tag_ids: vec!["Test.Tag".to_string()],
                reply,
            })
            .await;

        assert!(result.is_err());
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
    }
}
