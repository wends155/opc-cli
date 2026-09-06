//! Dedicated background COM worker thread and request dispatch facade.

mod browse;
mod pool;
mod read;
mod write;

#[cfg(test)]
mod tests;

use crate::com::connector::ServerConnector;
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::provider::{TagCollector, WriteResult};
use crate::types::{OpcServerEndpoint, OpcServerInfo, OpcValue};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Calculates elapsed milliseconds from an [`std::time::Instant`].
#[inline]
pub(crate) fn elapsed_ms(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

static GROUP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Generates a collision-proof group name composed of a prefix, process ID, and atomic sequence.
pub(crate) fn generate_group_name(prefix: &str) -> String {
    let pid = std::process::id();
    let seq = GROUP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{prefix}-{pid:x}-{seq:x}")
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
        /// Tag identifier to write.
        tag_id: String,
        /// Typed value to write.
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
    #[tracing::instrument(skip(connector))]
    pub(crate) fn start_with_initializer<I: crate::com::guard::ComInitializer>(
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
    let mut low_priority_queue: std::collections::VecDeque<ComRequest> =
        std::collections::VecDeque::new();

    loop {
        let loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                // Determine next request with priority favoring Read/Write over Browse/List
                let next_req = if low_priority_queue.is_empty() {
                    rx.blocking_recv()
                } else {
                    match rx.try_recv() {
                        Ok(req) => Some(req),
                        Err(_) => low_priority_queue.pop_front(),
                    }
                };

                let Some(req) = next_req else {
                    break;
                };

                // If this is a low-priority request, check if any high-priority request is waiting in rx
                if !is_high_priority(&req) {
                    let mut high_prio = None;
                    while let Ok(candidate) = rx.try_recv() {
                        if is_high_priority(&candidate) && high_prio.is_none() {
                            high_prio = Some(candidate);
                        } else {
                            low_priority_queue.push_back(candidate);
                        }
                    }
                    if let Some(hp) = high_prio {
                        low_priority_queue.push_back(req);
                        handle_request(hp, connector, &mut pool);
                        continue;
                    }
                }

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

/// Processes a single request dispatched to the COM worker thread.
#[allow(clippy::too_many_lines)]
fn handle_request<C: ServerConnector + 'static>(
    req: ComRequest,
    connector: &Arc<C>,
    pool: &mut pool::ConnectionPool<C::Server>,
) {
    match req {
        ComRequest::ListServers { host, reply } => {
            let host_clone = host.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let span = tracing::info_span!("opc.list_servers", host = %host);
                let _enter = span.enter();
                #[cfg(feature = "dev-diagnostics")]
                tracing::trace!(host = %host, "list_servers: starting operation");
                let start = std::time::Instant::now();
                let servers = connector.enumerate_servers(&host);
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
                        host = %host,
                        elapsed_ms = elapsed_ms(start),
                    );
                }
                servers
            }));
            match result {
                Ok(servers) => {
                    let _ = reply.send(servers);
                }
                Err(payload) => {
                    let msg = extract_panic_message(&*payload);
                    log_opc_err!(
                        &OpcError::Internal(format!("COM worker panicked: {msg}")),
                        OpcOperation::ListServers,
                        host = %host_clone,
                    );
                    let _ = reply.send(Err(OpcError::Internal(format!(
                        "COM worker panicked: {msg}"
                    ))));
                }
            }
        }

        ComRequest::ListServerDetails { host, reply } => {
            let host_clone = host.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let span = tracing::info_span!("opc.list_server_details", host = %host);
                let _enter = span.enter();
                #[cfg(feature = "dev-diagnostics")]
                tracing::trace!(host = %host, "list_server_details: starting operation");
                let start = std::time::Instant::now();
                let servers = connector.enumerate_server_details(&host);
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
                        host = %host,
                        elapsed_ms = elapsed_ms(start),
                    );
                }
                servers
            }));
            match result {
                Ok(servers) => {
                    let _ = reply.send(servers);
                }
                Err(payload) => {
                    let msg = extract_panic_message(&*payload);
                    log_opc_err!(
                        &OpcError::Internal(format!("COM worker panicked: {msg}")),
                        OpcOperation::ListServerDetails,
                        host = %host_clone,
                    );
                    let _ = reply.send(Err(OpcError::Internal(format!(
                        "COM worker panicked: {msg}"
                    ))));
                }
            }
        }

        ComRequest::ReadTagValues {
            endpoint,
            tags,
            reply,
        } => {
            let endpoint_clone = endpoint.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool::dispatch_with_retry(pool, connector, &endpoint, |opc_server| {
                    read::handle_read(&endpoint, &tags, opc_server)
                })
            }));
            match result {
                Ok(res) => {
                    let _ = reply.send(res);
                }
                Err(payload) => {
                    pool.remove(&endpoint_clone);
                    let msg = extract_panic_message(&*payload);
                    log_opc_err!(
                        &OpcError::Internal(format!("COM worker panicked: {msg}")),
                        OpcOperation::ReadSync,
                        server = %endpoint_clone,
                    );
                    let _ = reply.send(Err(OpcError::Internal(format!(
                        "COM worker panicked: {msg}"
                    ))));
                }
            }
        }

        ComRequest::WriteTagValue {
            endpoint,
            tag_id,
            value,
            reply,
        } => {
            let endpoint_clone = endpoint.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool::dispatch_with_retry(pool, connector, &endpoint, |opc_server| {
                    write::handle_write(&endpoint.identifier, &tag_id, &value, opc_server)
                })
            }));
            match result {
                Ok(res) => {
                    let _ = reply.send(res);
                }
                Err(payload) => {
                    pool.remove(&endpoint_clone);
                    let msg = extract_panic_message(&*payload);
                    log_opc_err!(
                        &OpcError::Internal(format!("COM worker panicked: {msg}")),
                        OpcOperation::WriteSync,
                        server = %endpoint_clone,
                    );
                    let _ = reply.send(Err(OpcError::Internal(format!(
                        "COM worker panicked: {msg}"
                    ))));
                }
            }
        }

        ComRequest::WriteTagValues {
            endpoint,
            writes,
            reply,
        } => {
            let endpoint_clone = endpoint.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool::dispatch_with_retry(pool, connector, &endpoint, |opc_server| {
                    write::handle_write_batch(&endpoint.identifier, &writes, opc_server)
                })
            }));
            match result {
                Ok(res) => {
                    let _ = reply.send(res);
                }
                Err(payload) => {
                    pool.remove(&endpoint_clone);
                    let msg = extract_panic_message(&*payload);
                    log_opc_err!(
                        &OpcError::Internal(format!("COM worker panicked: {msg}")),
                        OpcOperation::WriteSync,
                        server = %endpoint_clone,
                    );
                    let _ = reply.send(Err(OpcError::Internal(format!(
                        "COM worker panicked: {msg}"
                    ))));
                }
            }
        }

        ComRequest::BrowseTags {
            endpoint,
            collector,
            reply,
        } => {
            let endpoint_clone = endpoint.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool::dispatch_with_retry(pool, connector, &endpoint, |opc_server| {
                    browse::handle_browse(&endpoint.identifier, &collector, opc_server)
                })
            }));
            match result {
                Ok(res) => {
                    let _ = reply.send(res);
                }
                Err(payload) => {
                    pool.remove(&endpoint_clone);
                    let msg = extract_panic_message(&*payload);
                    log_opc_err!(
                        &OpcError::Internal(format!("COM worker panicked: {msg}")),
                        OpcOperation::BrowseTags,
                        server = %endpoint_clone,
                    );
                    let _ = reply.send(Err(OpcError::Internal(format!(
                        "COM worker panicked: {msg}"
                    ))));
                }
            }
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
