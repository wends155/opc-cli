//! Connection pool management and retry dispatch engine.

use crate::com::connector::ServerConnector;
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::types::ServerIdentifier;
use std::collections::HashMap;
use std::sync::Arc;

/// Determines whether an error indicates a broken or stale COM/RPC connection.
#[must_use]
pub fn is_connection_error(err: &OpcError) -> bool {
    if let OpcError::Com { source } = err {
        crate::raw::hresult::is_connection_hresult(source.code())
    } else {
        false
    }
}

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
    let server_ref = match cache.entry(identifier.clone()) {
        std::collections::hash_map::Entry::Occupied(e) => {
            tracing::trace!(server = %identifier, "Cache hit");
            e.into_mut()
        }
        std::collections::hash_map::Entry::Vacant(e) => {
            tracing::debug!(server = %identifier, "Cache miss, connecting");
            let srv = connector.connect_identifier(identifier)?;
            tracing::info!(server = %identifier, "Connection established, added to pool");
            e.insert(srv)
        }
    };

    match operation(server_ref) {
        Err(e) if is_connection_error(&e) => {
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
