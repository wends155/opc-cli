//! Dedicated background COM worker thread and request dispatch facade.

mod browse;
mod pool;
mod read;
mod write;

#[cfg(test)]
mod tests;

use crate::com::connector::{
    ConnectedGroup, ConnectedServer, GroupConfig, GroupItemDef, ServerConnector,
};
use crate::com::guard::GroupGuard;
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::types::{
    ClientItemHandle, OpcServerEndpoint, OpcServerInfo, OpcValue, ServerIdentifier, TagCollector,
    WriteResult,
};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Calculates elapsed milliseconds from an [`std::time::Instant`].
#[inline]
pub fn elapsed_ms(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

static GROUP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Generates a collision-proof group name composed of a prefix, process ID, and atomic sequence.
pub fn generate_group_name(prefix: &str) -> String {
    let pid = std::process::id();
    let seq = GROUP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{prefix}-{pid:x}-{seq:x}")
}

/// An ephemeral group created on an OPC server with items registered and validated.
pub struct RegisteredItemGroup<'a, S: ConnectedServer> {
    /// Connected group proxy.
    pub group: S::Group,
    /// Guard that deletes the group on drop unless disarmed.
    pub group_guard: GroupGuard<'a, S>,
    /// Results of the `add_items` call, matching the input tag order.
    pub item_results: Vec<crate::com::connector::GroupItemResult>,
}

/// Helper to create an ephemeral group and register items on a connected OPC server.
///
/// # Arguments
/// * `server` - Reference to the connected OPC server.
/// * `server_id` - Server identifier for structured logging.
/// * `prefix` - Prefix for the ephemeral group name (e.g. `"opc-read"` or `"opc-write"`).
/// * `tags` - Slice of tag names to register in the group.
/// * `add_group_op` - Operation name for group creation logging.
/// * `add_items_op` - Operation name for item addition logging.
pub fn register_item_group<'a, S: ConnectedServer>(
    server: &'a S,
    server_id: &ServerIdentifier,
    prefix: &str,
    tags: &[impl AsRef<str>],
    add_group_op: OpcOperation,
    add_items_op: OpcOperation,
) -> OpcResult<RegisteredItemGroup<'a, S>> {
    let group_name = generate_group_name(prefix);
    let created = server
        .add_group(&GroupConfig::ephemeral(&group_name))
        .inspect_err(|e| {
            log_opc_err!(
                e,
                add_group_op,
                server = %server_id,
                tag_count = tags.len()
            );
        })?;

    let group = created.group;
    let group_guard = GroupGuard::new(server, created.server_handle);

    let item_defs: Vec<GroupItemDef> = tags
        .iter()
        .enumerate()
        .map(|(idx, tag)| GroupItemDef {
            item_id: tag.as_ref().to_string(),
            #[allow(clippy::cast_possible_truncation)]
            client_handle: ClientItemHandle::new(idx as u32),
            active: true,
        })
        .collect();

    let results = group.add_items(&item_defs).inspect_err(|e| {
        log_opc_err!(
            e,
            add_items_op,
            server = %server_id,
            tag_count = tags.len()
        );
    })?;

    if results.len() != tags.len() {
        let err = OpcError::Internal("OPC server returned mismatched result array sizes".into());
        log_opc_err!(
            &err,
            OpcOperation::ReadMismatchedResults,
            server = %server_id,
            expected = tags.len(),
            actual = results.len()
        );
        return Err(err);
    }

    Ok(RegisteredItemGroup {
        group,
        group_guard,
        item_results: results,
    })
}

/// Represents an asynchronous request dispatched to the COM worker thread.
pub enum ComRequest {
    /// Request to enumerate available OPC DA servers on a host.
    ListServers {
        /// Hostname or IP address to target.
        host: String,
        /// One-shot channel to send back the server enumeration result.
        reply: oneshot::Sender<OpcResult<Vec<String>>>,
    },
    /// Request to enumerate available OPC DA servers with rich metadata on a host.
    ListServerDetails {
        /// Hostname or IP address to target.
        host: String,
        /// One-shot channel to send back the structured server details result.
        reply: oneshot::Sender<OpcResult<Vec<OpcServerInfo>>>,
    },
    /// Request to read current values, quality, and timestamps for tag IDs.
    ReadTagValues {
        /// Target OPC server endpoint.
        endpoint: OpcServerEndpoint,
        /// Batch of tag identifiers to read.
        tags: crate::types::TagBatch,
        /// One-shot channel to send back the tag values result.
        reply: oneshot::Sender<OpcResult<crate::types::TagValues>>,
    },
    /// Request to write a typed value to a single tag.
    WriteTagValue {
        /// Target OPC server endpoint.
        endpoint: OpcServerEndpoint,
        /// Tag identifier to write to.
        tag_id: String,
        /// Value to write.
        value: OpcValue,
        /// One-shot channel to send back the write operation result.
        reply: oneshot::Sender<OpcResult<WriteResult>>,
    },
    /// Request to write a batch of typed values.
    WriteTagValues {
        /// Target OPC server endpoint.
        endpoint: OpcServerEndpoint,
        /// List of tag ID and typed value pairs to write.
        writes: Vec<(String, OpcValue)>,
        /// One-shot channel to send back write operation results.
        reply: oneshot::Sender<OpcResult<Vec<WriteResult>>>,
    },
    /// Request to recursively browse available tags on a server.
    BrowseTags {
        /// Target OPC server endpoint.
        endpoint: OpcServerEndpoint,
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

impl<C: ServerConnector + 'static> ComWorker<C> {
    /// Starts the background COM worker thread with default MTA initialization.
    pub fn start(connector: Arc<C>) -> Result<Self, OpcError> {
        Self::start_with_initializer::<crate::com::guard::DefaultComInit>(connector)
    }

    /// Starts the background COM worker thread with a specified COM initialization strategy.
    #[tracing::instrument(skip(connector))]
    pub fn start_with_initializer<I: crate::com::guard::ComInitializer>(
        connector: Arc<C>,
    ) -> Result<Self, OpcError> {
        let (tx, rx) = mpsc::channel(32);
        let (init_tx, init_rx) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            run_worker_thread::<C, I>(rx, &connector, &init_tx);
        });

        init_rx.recv().inspect_err(
            |e| tracing::error!(error = ?e, "COM worker thread disconnected during init"),
        )??;

        tracing::debug!("COM worker thread started");

        Ok(Self {
            sender: tx,
            handle: Some(handle),
            _phantom: std::marker::PhantomData,
        })
    }

    /// Dispatches a request to the background COM worker thread and awaits the one-shot reply.
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

        self.sender.send(req).await.inspect_err(
            |e| tracing::error!(error = ?e, "COM worker channel closed (worker stopped)"),
        )?;

        rx.await
            .inspect_err(|e| tracing::error!(error = ?e, "COM worker shut down during request"))?
    }
}

impl<C: ServerConnector + 'static> Drop for ComWorker<C> {
    fn drop(&mut self) {
        tracing::debug!("ComWorker dropping — channel closing, signaling thread shutdown");
    }
}

/// Helper to determine if a request has high processing priority (I/O over discovery).
fn is_high_priority(req: &ComRequest) -> bool {
    matches!(
        req,
        ComRequest::ReadTagValues { .. }
            | ComRequest::WriteTagValue { .. }
            | ComRequest::WriteTagValues { .. }
    )
}

/// Two-tier priority request queue for the dedicated COM worker thread.
/// High priority (Read/Write I/O) requests are always dispatched before low priority (Browse/List) requests.
pub struct PriorityRequestQueue {
    high: std::collections::VecDeque<ComRequest>,
    low: std::collections::VecDeque<ComRequest>,
}

impl PriorityRequestQueue {
    pub fn new() -> Self {
        Self {
            high: std::collections::VecDeque::new(),
            low: std::collections::VecDeque::new(),
        }
    }

    pub fn push(&mut self, req: ComRequest) {
        if is_high_priority(&req) {
            self.high.push_back(req);
        } else {
            self.low.push_back(req);
        }
    }

    pub fn pop_next(&mut self) -> Option<ComRequest> {
        self.high.pop_front().or_else(|| self.low.pop_front())
    }

    pub fn is_empty(&self) -> bool {
        self.high.is_empty() && self.low.is_empty()
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.high.clear();
        self.low.clear();
    }
}

/// Helper to extract panic message string from catch_unwind payload.
fn extract_panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Executes the main event loop on the dedicated COM STA/MTA worker thread.
fn run_worker_thread<C, I>(
    mut rx: mpsc::Receiver<ComRequest>,
    connector: &Arc<C>,
    init_tx: &std::sync::mpsc::Sender<Result<(), OpcError>>,
) where
    C: ServerConnector + 'static,
    I: crate::com::guard::ComInitializer,
{
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

    let mut pool: pool::ConnectionPool<C::Server> = pool::ConnectionPool::new();
    let mut queue = PriorityRequestQueue::new();

    loop {
        let loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                // If queue is empty, block waiting for the next incoming request
                if queue.is_empty() {
                    let Some(first) = rx.blocking_recv() else {
                        break;
                    };
                    queue.push(first);
                }

                // Opportunistically drain any pending channel items into priority-classified queues
                while let Ok(pending) = rx.try_recv() {
                    queue.push(pending);
                }

                // Always serve high-priority before low-priority
                let Some(req) = queue.pop_next() else {
                    break;
                };

                handle_request(req, connector, &mut pool);
            }
        }));

        match loop_result {
            Ok(()) => break,
            Err(payload) => {
                let msg = extract_panic_message(&*payload);
                tracing::error!(
                    panic = %msg,
                    "Unhandled panic in COM worker loop; resetting pool and continuing"
                );
                pool.clear();
            }
        }
    }

    tracing::debug!("COM worker thread exiting cleanly");
}

/// Dispatches a discovery operation wrapped in exception safety handling.
fn dispatch_discovery_request<R, F>(
    host: &str,
    op: OpcOperation,
    reply: oneshot::Sender<OpcResult<R>>,
    f: F,
) where
    F: FnOnce(&str) -> OpcResult<R>,
{
    let host_str = host.to_string();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(host)));
    match result {
        Ok(res) => {
            let _ = reply.send(res);
        }
        Err(payload) => {
            let msg = extract_panic_message(&*payload);
            log_opc_err!(
                &OpcError::Internal(format!("COM worker panicked: {msg}")),
                op,
                host = %host_str,
            );
            let _ = reply.send(Err(OpcError::Internal(format!(
                "COM worker panicked: {msg}"
            ))));
        }
    }
}

/// Dispatches an operation against a pooled server connection with full panic containment,
/// circuit breaker cooldown activation, and proxy eviction.
fn dispatch_pooled_request<C, R, F>(
    pool: &mut pool::ConnectionPool<C::Server>,
    connector: &Arc<C>,
    endpoint: &OpcServerEndpoint,
    op: OpcOperation,
    reply: oneshot::Sender<OpcResult<R>>,
    mut f: F,
) where
    C: ServerConnector + 'static,
    F: FnMut(&mut pool::PooledServer<C::Server>) -> OpcResult<R>,
{
    let endpoint_clone = endpoint.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pool::dispatch_with_retry(pool, connector, endpoint, &mut f)
    }));

    match result {
        Ok(res) => {
            let _ = reply.send(res);
        }
        Err(payload) => {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool.remove(&endpoint_clone);
            }));
            let msg = extract_panic_message(&*payload);
            log_opc_err!(
                &OpcError::Internal(format!("COM worker panicked: {msg}")),
                op,
                server = %endpoint_clone,
            );
            let _ = reply.send(Err(OpcError::Internal(format!(
                "COM worker panicked: {msg}"
            ))));
        }
    }
}

/// Processes a single request dispatched to the COM worker thread.
#[allow(clippy::too_many_lines)]
fn handle_request<C: ServerConnector + 'static>(
    req: ComRequest,
    connector: &Arc<C>,
    pool: &mut pool::ConnectionPool<C::Server>,
) {
    match req {
        ComRequest::ListServers { host, reply } => {
            dispatch_discovery_request(&host, OpcOperation::ListServers, reply, |h| {
                let span = tracing::info_span!("opc.list_servers", host = %h);
                let _enter = span.enter();
                #[cfg(feature = "dev-diagnostics")]
                tracing::trace!(host = %h, "list_servers: starting operation");
                let start = std::time::Instant::now();
                let servers = connector.enumerate_servers(h);
                if let Ok(s) = &servers {
                    tracing::info!(
                        count = s.len(),
                        elapsed_ms = elapsed_ms(start),
                        "list_servers completed"
                    );
                } else if let Err(e) = &servers {
                    log_opc_err!(
                        e,
                        OpcOperation::ListServers,
                        host = %h,
                        elapsed_ms = elapsed_ms(start),
                    );
                }
                servers
            });
        }

        ComRequest::ListServerDetails { host, reply } => {
            dispatch_discovery_request(&host, OpcOperation::ListServerDetails, reply, |h| {
                let span = tracing::info_span!("opc.list_server_details", host = %h);
                let _enter = span.enter();
                #[cfg(feature = "dev-diagnostics")]
                tracing::trace!(host = %h, "list_server_details: starting operation");
                let start = std::time::Instant::now();
                let servers = connector.enumerate_server_details(h);
                if let Ok(s) = &servers {
                    tracing::info!(
                        count = s.len(),
                        elapsed_ms = elapsed_ms(start),
                        "list_server_details completed"
                    );
                } else if let Err(e) = &servers {
                    log_opc_err!(
                        e,
                        OpcOperation::ListServerDetails,
                        host = %h,
                        elapsed_ms = elapsed_ms(start),
                    );
                }
                servers
            });
        }

        ComRequest::ReadTagValues {
            endpoint,
            tags,
            reply,
        } => {
            dispatch_pooled_request(
                pool,
                connector,
                &endpoint,
                OpcOperation::ReadSync,
                reply,
                |opc_server| read::handle_read(&endpoint, &tags, opc_server),
            );
        }

        ComRequest::WriteTagValue {
            endpoint,
            tag_id,
            value,
            reply,
        } => {
            dispatch_pooled_request(
                pool,
                connector,
                &endpoint,
                OpcOperation::WriteSync,
                reply,
                |opc_server| write::handle_write(&endpoint.identifier, &tag_id, &value, opc_server),
            );
        }

        ComRequest::WriteTagValues {
            endpoint,
            writes,
            reply,
        } => {
            dispatch_pooled_request(
                pool,
                connector,
                &endpoint,
                OpcOperation::WriteSync,
                reply,
                |opc_server| write::handle_write_batch(&endpoint.identifier, &writes, opc_server),
            );
        }

        ComRequest::BrowseTags {
            endpoint,
            collector,
            reply,
        } => {
            dispatch_pooled_request(
                pool,
                connector,
                &endpoint,
                OpcOperation::BrowseTags,
                reply,
                |opc_server| browse::handle_browse(&endpoint.identifier, &collector, opc_server),
            );
        }
    }
}

#[cfg(test)]
mod group_name_tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn test_collision_proof_group_name_concurrency() {
        let names = Arc::new(Mutex::new(HashSet::new()));
        let mut handles = Vec::new();

        for _ in 0..8 {
            let names_clone = Arc::clone(&names);
            handles.push(thread::spawn(move || {
                let mut local = Vec::with_capacity(1000);
                for _ in 0..1000 {
                    let name = generate_group_name("opc-test");
                    assert!(name.starts_with("opc-test-"));
                    local.push(name);
                }
                let mut guard = names_clone.lock().unwrap();
                for n in local {
                    assert!(guard.insert(n), "Collision detected in group name");
                }
                drop(guard);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(names.lock().unwrap().len(), 8000);
    }
}
