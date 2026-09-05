//! Dedicated background COM worker thread and request dispatch facade.

mod browse;
mod pool;
mod read;
mod write;

#[cfg(test)]
mod tests;

use crate::com::connector::{GroupConfig, ServerConnector};
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::provider::{OpcValue, TagCollector, TagValue, WriteResult};
use crate::types::{GroupHandle, OpcServerInfo, ServerIdentifier};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Calculates elapsed milliseconds from an [`std::time::Instant`].
#[inline]
pub(crate) fn elapsed_ms(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Constructs a standard ephemeral [`GroupConfig`] for short-lived worker operations.
pub(crate) fn ephemeral_group_config(name: &str) -> GroupConfig<'_> {
    GroupConfig {
        name,
        active: true,
        update_rate_ms: 1000,
        client_handle: GroupHandle::default(),
        time_bias: 0,
        percent_deadband: 0.0,
        locale_id: 0,
    }
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
        /// OPC server identifier.
        server: ServerIdentifier,
        /// List of fully qualified tag identifiers to read.
        tag_ids: Vec<String>,
        /// One-shot channel to send back the tag values result.
        reply: oneshot::Sender<OpcResult<Vec<TagValue>>>,
    },
    /// Request to write a typed value to a single tag.
    WriteTagValue {
        /// OPC server identifier.
        server: ServerIdentifier,
        /// Tag identifier to write.
        tag_id: String,
        /// Typed value to write.
        value: OpcValue,
        /// One-shot channel to send back the write operation result.
        reply: oneshot::Sender<OpcResult<WriteResult>>,
    },
    /// Request to recursively browse available tags on a server.
    BrowseTags {
        /// OPC server identifier.
        server: ServerIdentifier,
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

    let mut cache: HashMap<ServerIdentifier, C::Server> = HashMap::new();

    while let Some(req) = rx.blocking_recv() {
        handle_request(req, connector, &mut cache);
    }

    tracing::debug!("COM worker thread exiting cleanly");
}

/// Processes a single request dispatched to the COM worker thread.
fn handle_request<C: ServerConnector + 'static>(
    req: ComRequest,
    connector: &Arc<C>,
    cache: &mut HashMap<ServerIdentifier, C::Server>,
) {
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
            let _ = reply.send(servers);
        }

        ComRequest::ListServerDetails { host, reply } => {
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
            let _ = reply.send(servers);
        }

        ComRequest::ReadTagValues {
            server,
            tag_ids,
            reply,
        } => {
            let result = pool::dispatch_with_retry(cache, connector, &server, |opc_server| {
                read::handle_read(&server, &tag_ids, opc_server)
            });
            let _ = reply.send(result);
        }

        ComRequest::WriteTagValue {
            server,
            tag_id,
            value,
            reply,
        } => {
            let result = pool::dispatch_with_retry(cache, connector, &server, |opc_server| {
                write::handle_write(&server, &tag_id, &value, opc_server)
            });
            let _ = reply.send(result);
        }

        ComRequest::BrowseTags {
            server,
            collector,
            reply,
        } => {
            let result = pool::dispatch_with_retry(cache, connector, &server, |opc_server| {
                browse::handle_browse(&server, &collector, opc_server)
            });
            let _ = reply.send(result);
        }
    }
}
