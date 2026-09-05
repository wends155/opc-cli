//! Connection pool management and retry dispatch engine.

use crate::com::connector::ServerConnector;
use crate::errors::{OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::types::ServerIdentifier;
use std::collections::HashMap;
use std::sync::Arc;

/// Dispatches an operation against a pooled server connection, transparently evicting
/// and reconnecting if a stale proxy RPC error is detected.
#[tracing::instrument(level = "debug", skip(cache, connector, operation))]
pub fn dispatch_with_retry<C, F, R>(
    cache: &mut HashMap<ServerIdentifier, C::Server>,
    connector: &Arc<C>,
    identifier: &ServerIdentifier,
    operation: F,
) -> OpcResult<R>
where
    C: ServerConnector + 'static,
    F: Fn(&C::Server) -> OpcResult<R>,
{
    let server_ref = if let Some(srv) = cache.get_mut(identifier) {
        tracing::trace!(server = %identifier, "Cache hit");
        srv
    } else {
        tracing::debug!(server = %identifier, "Cache miss, connecting");
        let srv = connector.connect_identifier(identifier)?;
        tracing::info!(server = %identifier, "Connection established, added to pool");
        cache.entry(identifier.clone()).or_insert(srv)
    };

    match operation(server_ref) {
        Err(e) if e.is_connection_error() => {
            log_opc_err!(
                &e,
                OpcOperation::DispatchConnectionError,
                server = %identifier,
                action = "evicting_stale_connection"
            );
            cache.remove(identifier);
            tracing::debug!(server = %identifier, "Reconnecting");
            let fresh_srv = connector
                .connect_identifier(identifier)
                .inspect_err(|connect_e| {
                    log_opc_err!(
                        connect_e,
                        OpcOperation::DispatchReconnect,
                        server = %identifier
                    );
                })?;
            let fresh_ref = &fresh_srv;
            let result = operation(fresh_ref);
            if let Err(ref op_e) = result {
                log_opc_err!(
                    op_e,
                    OpcOperation::DispatchRetriedOperation,
                    server = %identifier
                );
            }
            tracing::info!(server = %identifier, "Reconnection successful, pool updated");
            cache.insert(identifier.clone(), fresh_srv);
            result
        }
        Err(e) => {
            log_opc_err!(
                &e,
                OpcOperation::DispatchOperation,
                server = %identifier
            );
            Err(e)
        }
        Ok(v) => Ok(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::connector::mock::{MockServerConnector, MockState};
    use crate::errors::OpcError;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_dispatch_cache_hit_avoids_reconnect() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut cache = HashMap::new();
        let id = ServerIdentifier::from("Matrikon.OPC.Simulation.1");

        // First call: cache miss, connect_count becomes 1
        let res1 = dispatch_with_retry(&mut cache, &connector, &id, |_| Ok(42));
        assert_eq!(res1.unwrap(), 42);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(cache.len(), 1);

        // Second call: cache hit, connect_count remains 1
        let res2 = dispatch_with_retry(&mut cache, &connector, &id, |_| Ok(84));
        assert_eq!(res2.unwrap(), 84);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dispatch_connection_error_evicts_and_reconnects() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut cache = HashMap::new();
        let id = ServerIdentifier::from("Matrikon.OPC.Simulation.1");

        // First connect
        let _ = dispatch_with_retry(&mut cache, &connector, &id, |_| Ok(()));
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);

        // Operation triggers connection error (RPC server unavailable)
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        let res = dispatch_with_retry(&mut cache, &connector, &id, |_| {
            let n = attempt.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(OpcError::Com {
                    source: windows::core::Error::from_hresult(windows::core::HRESULT(
                        i32::from_ne_bytes(0x8007_06BA_u32.to_ne_bytes()),
                    )),
                })
            } else {
                Ok("recovered")
            }
        });

        assert_eq!(res.unwrap(), "recovered");
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_dispatch_non_connection_error_does_not_evict() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut cache = HashMap::new();
        let id = ServerIdentifier::from("Matrikon.OPC.Simulation.1");

        let _ = dispatch_with_retry(&mut cache, &connector, &id, |_| Ok(()));
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);

        // Operation returns non-connection error (e.g. InvalidState)
        let res: OpcResult<()> = dispatch_with_retry(&mut cache, &connector, &id, |_| {
            Err(OpcError::InvalidState("item not found".into()))
        });
        assert!(res.is_err());
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(cache.len(), 1);
    }
}
