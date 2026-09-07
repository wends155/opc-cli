//! Connection pool management, active group caching, and retry dispatch engine.

use crate::com::connector::traits::{ConnectedServer, GroupRemovalMode, ServerConnector};
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::types::{NamespaceType, OpcServerEndpoint, ServerGroupHandle, ServerItemHandle};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Circuit breaker cooldown period for unreachable endpoints (5 seconds).
pub const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(5);

/// Cached active group holding server handle, group proxy, item handles, and tag IDs.
#[allow(dead_code)]
pub struct CachedGroup<G> {
    /// Tag IDs associated with this cached group in registration order.
    pub tags: Vec<String>,
    /// Underlying connected group facade instance.
    pub group: G,
    /// Server-assigned handle for the group.
    pub server_handle: ServerGroupHandle,
    /// Server-assigned handles for items corresponding to `tags`.
    pub server_item_handles: Vec<ServerItemHandle>,
    /// Indices of items that were successfully registered on the server.
    pub valid_indices: Vec<usize>,
    /// Rejected item indices and their errors.
    pub rejected_errors: Vec<(usize, OpcError)>,
}

/// A connected server instance held in the connection pool with an optional cached active group.
pub struct PooledServer<S: ConnectedServer> {
    /// Connected server facade instance.
    pub server: S,
    /// Optional cached active group reused across consecutive reads of identical tag batches.
    pub active_group: Option<CachedGroup<S::Group>>,
}

impl<S: ConnectedServer> PooledServer<S> {
    /// Creates a new pooled server wrapper without an active group.
    #[must_use]
    pub fn new(server: S) -> Self {
        Self {
            server,
            active_group: None,
        }
    }

    /// Explicitly removes and clears the cached active group from the server if one exists.
    pub fn clear_active_group(&mut self) {
        if let Some(cached) = self.active_group.take() {
            let _ = self
                .server
                .remove_group(cached.server_handle, GroupRemovalMode::Force);
        }
    }
}

impl<S: ConnectedServer> Drop for PooledServer<S> {
    fn drop(&mut self) {
        self.clear_active_group();
    }
}

impl<S: ConnectedServer> std::ops::Deref for PooledServer<S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.server
    }
}

impl<S: ConnectedServer> std::ops::DerefMut for PooledServer<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.server
    }
}

impl<S: ConnectedServer> ConnectedServer for PooledServer<S> {
    type Group = S::Group;

    fn query_organization(&self) -> OpcResult<NamespaceType> {
        self.server.query_organization()
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: crate::types::BrowseType,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> OpcResult<crate::com::iterator::StringIterator> {
        self.server
            .browse_opc_item_ids(browse_type, filter, data_type, access_rights)
    }

    fn change_browse_position(
        &self,
        direction: crate::types::BrowseDirection,
        name: &str,
    ) -> OpcResult<()> {
        self.server.change_browse_position(direction, name)
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        self.server.get_item_id(item_name)
    }

    fn add_group(
        &self,
        config: &crate::com::connector::GroupConfig<'_>,
    ) -> OpcResult<crate::com::connector::CreatedGroup<Self::Group>> {
        self.server.add_group(config)
    }

    fn remove_group(
        &self,
        server_group: ServerGroupHandle,
        mode: GroupRemovalMode,
    ) -> OpcResult<()> {
        self.server.remove_group(server_group, mode)
    }
}

/// Connection pool managing active server instances and failure cooldowns keyed by [`OpcServerEndpoint`].
pub struct ConnectionPool<S: ConnectedServer> {
    /// Map of active pooled server connections.
    pub connections: HashMap<OpcServerEndpoint, PooledServer<S>>,
    /// Map of endpoint failure timestamps for circuit breaker cooldowns.
    pub failure_cooldowns: HashMap<OpcServerEndpoint, Instant>,
}

impl<S: ConnectedServer> Default for ConnectionPool<S> {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            failure_cooldowns: HashMap::new(),
        }
    }
}

/// Maximum number of tracked endpoint failure cooldowns before LRU eviction.
pub const MAX_COOLDOWNS: usize = 256;

impl<S: ConnectedServer> ConnectionPool<S> {
    /// Creates a new empty connection pool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an endpoint failure timestamp, pruning expired circuit breaker cooldowns
    /// and capping memory growth at [`MAX_COOLDOWNS`] via LRU eviction.
    pub fn record_failure(&mut self, endpoint: OpcServerEndpoint) {
        self.failure_cooldowns
            .retain(|_, failed_at| failed_at.elapsed() < CIRCUIT_BREAKER_COOLDOWN);

        if self.failure_cooldowns.len() >= MAX_COOLDOWNS {
            if let Some(oldest) = self
                .failure_cooldowns
                .iter()
                .min_by_key(|(_, t)| **t)
                .map(|(k, _)| k.clone())
            {
                self.failure_cooldowns.remove(&oldest);
            }
        }

        self.failure_cooldowns.insert(endpoint, Instant::now());
    }

    /// Number of active connections currently maintained in the pool.
    #[must_use]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Returns `true` if the connection pool holds no active connections.
    #[must_use]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Clears all active connections (triggering group cleanups via `Drop`) and resets failure cooldowns.
    pub fn clear(&mut self) {
        self.connections.clear();
        self.failure_cooldowns.clear();
    }

    /// Removes an endpoint from the connection pool, returning its `PooledServer` if present.
    pub fn remove(&mut self, endpoint: &OpcServerEndpoint) -> Option<PooledServer<S>> {
        self.connections.remove(endpoint)
    }

    /// Evicts an endpoint from the connection pool, synchronously clearing any active group proxy before removal.
    ///
    /// Returns `true` if an active connection was present and evicted.
    pub fn evict(&mut self, endpoint: &OpcServerEndpoint) -> bool {
        if let Some(mut pooled) = self.connections.remove(endpoint) {
            pooled.clear_active_group();
            true
        } else {
            false
        }
    }
}

/// Dispatches an operation against a pooled server connection, transparently evicting
/// and reconnecting if a stale proxy RPC error is detected, while enforcing circuit breaker cooldowns.
#[tracing::instrument(level = "debug", skip(pool, connector, operation))]
pub fn dispatch_with_retry<C, F, R>(
    pool: &mut ConnectionPool<C::Server>,
    connector: &Arc<C>,
    endpoint: &OpcServerEndpoint,
    mut operation: F,
) -> OpcResult<R>
where
    C: ServerConnector + 'static,
    F: FnMut(&mut PooledServer<C::Server>) -> OpcResult<R>,
{
    // Check circuit breaker failure cooldown
    if let Some(failed_at) = pool.failure_cooldowns.get(endpoint) {
        if failed_at.elapsed() < CIRCUIT_BREAKER_COOLDOWN {
            let remaining = CIRCUIT_BREAKER_COOLDOWN.saturating_sub(failed_at.elapsed());
            tracing::warn!(
                endpoint = %endpoint,
                remaining_ms = remaining.as_millis(),
                "Endpoint in failure cooldown, short-circuiting connection"
            );
            return Err(OpcError::Connection(format!(
                "Endpoint '{endpoint}' is in circuit breaker cooldown (retry in {}ms)",
                remaining.as_millis()
            )));
        }
        pool.failure_cooldowns.remove(endpoint);
    }

    let server_ref = if let Some(srv) = pool.connections.get_mut(endpoint) {
        tracing::trace!(server = %endpoint, "Cache hit");
        srv
    } else {
        tracing::debug!(server = %endpoint, "Cache miss, connecting");
        let srv = match connector.connect_endpoint(endpoint) {
            Ok(s) => s,
            Err(e) => {
                if e.is_connection_error() {
                    pool.record_failure(endpoint.clone());
                }
                return Err(e);
            }
        };
        tracing::info!(server = %endpoint, "Connection established, added to pool");
        pool.connections
            .entry(endpoint.clone())
            .or_insert_with(|| PooledServer::new(srv))
    };

    match operation(server_ref) {
        Err(e) if e.is_connection_error() => {
            log_opc_err!(
                &e,
                OpcOperation::DispatchConnectionError,
                server = %endpoint,
                action = "evicting_stale_connection"
            );
            pool.evict(endpoint);
            tracing::debug!(server = %endpoint, "Reconnecting");
            let fresh_srv = match connector.connect_endpoint(endpoint) {
                Ok(s) => s,
                Err(connect_e) => {
                    log_opc_err!(
                        &connect_e,
                        OpcOperation::DispatchReconnect,
                        server = %endpoint
                    );
                    if connect_e.is_connection_error() {
                        pool.record_failure(endpoint.clone());
                    }
                    return Err(connect_e);
                }
            };
            let mut fresh_pooled = PooledServer::new(fresh_srv);
            let result = operation(&mut fresh_pooled);
            if let Err(ref op_e) = result {
                log_opc_err!(
                    op_e,
                    OpcOperation::DispatchRetriedOperation,
                    server = %endpoint
                );
                if op_e.is_connection_error() {
                    pool.record_failure(endpoint.clone());
                    return result;
                }
            }
            tracing::info!(server = %endpoint, "Reconnection successful, pool updated");
            pool.connections.insert(endpoint.clone(), fresh_pooled);
            result
        }
        Err(e) => {
            log_opc_err!(
                &e,
                OpcOperation::DispatchOperation,
                server = %endpoint
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
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::from("Matrikon.OPC.Simulation.1");

        // First call: cache miss, connect_count becomes 1
        let res1 = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| Ok(42));
        assert_eq!(res1.unwrap(), 42);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 1);

        // Second call: cache hit, connect_count remains 1
        let res2 = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| Ok(84));
        assert_eq!(res2.unwrap(), 84);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dispatch_connection_error_evicts_and_reconnects() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::from("Matrikon.OPC.Simulation.1");

        // First connect
        let _ = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| Ok(()));
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);

        // Operation triggers connection error (RPC server unavailable)
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        let res = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| {
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
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_dispatch_non_connection_error_does_not_evict() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::from("Matrikon.OPC.Simulation.1");

        let _ = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| Ok(()));
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);

        // Operation returns non-connection error (e.g. InvalidState)
        let res: OpcResult<()> = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| {
            Err(OpcError::InvalidState("item not found".into()))
        });
        assert!(res.is_err());
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_worker_active_group_caching_hit_miss_and_invalidation() {
        use crate::com::connector::{ConnectedGroup, ConnectedServer, GroupConfig, GroupItemDef};
        use crate::types::{ClientItemHandle, OpcServerEndpoint, ServerItemHandle};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::from("Matrikon.OPC.Simulation.1");

        let tags_a = vec!["Tag1".to_string(), "Tag2".to_string()];
        let tags_b = vec!["Tag3".to_string()];

        // 1. First read with tags_a: cache miss, creates group & adds items
        let res1 = dispatch_with_retry(&mut pool, &connector, &endpoint, |pooled| {
            if let Some(cached) = &pooled.active_group
                && cached.tags == tags_a
            {
                return Ok("hit");
            }
            let group_config = GroupConfig::ephemeral("opc-group-1");
            let created = pooled.server.add_group(&group_config)?;
            let item_defs: Vec<GroupItemDef> = tags_a
                .iter()
                .map(|t| GroupItemDef {
                    item_id: t.clone(),
                    client_handle: ClientItemHandle::new(1),
                    active: true,
                })
                .collect();
            let _ = created.group.add_items(&item_defs)?;
            pooled.active_group = Some(CachedGroup {
                tags: tags_a.clone(),
                group: created.group,
                server_handle: created.server_handle,
                server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
                valid_indices: vec![0, 1],
                rejected_errors: Vec::new(),
            });
            Ok("miss")
        });
        assert_eq!(res1.unwrap(), "miss");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.add_items_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // 2. Second read with EXACT SAME tags_a: cache hit! No add_group or add_items
        let res2 = dispatch_with_retry(&mut pool, &connector, &endpoint, |pooled| {
            if let Some(cached) = &pooled.active_group
                && cached.tags == tags_a
            {
                return Ok("hit");
            }
            Ok("miss")
        });
        assert_eq!(res2.unwrap(), "hit");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.add_items_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // 3. Third read with DIFFERENT tags_b: cache miss, removes old group, creates new
        let res3 = dispatch_with_retry(&mut pool, &connector, &endpoint, |pooled| {
            if let Some(cached) = &pooled.active_group
                && cached.tags == tags_b
            {
                return Ok("hit");
            }
            pooled.clear_active_group();
            let group_config = GroupConfig::ephemeral("opc-group-2");
            let created = pooled.server.add_group(&group_config)?;
            let item_defs: Vec<GroupItemDef> = tags_b
                .iter()
                .map(|t| GroupItemDef {
                    item_id: t.clone(),
                    client_handle: ClientItemHandle::new(1),
                    active: true,
                })
                .collect();
            let _ = created.group.add_items(&item_defs)?;
            pooled.active_group = Some(CachedGroup {
                tags: tags_b.clone(),
                group: created.group,
                server_handle: created.server_handle,
                server_item_handles: vec![ServerItemHandle::new(1)],
                valid_indices: vec![0],
                rejected_errors: Vec::new(),
            });
            Ok("miss_switched")
        });
        assert_eq!(res3.unwrap(), "miss_switched");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.add_items_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 1);

        // 4. Invalidation on connection error: evicts connection and drops cached group
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        let res4: OpcResult<()> = dispatch_with_retry(&mut pool, &connector, &endpoint, |_| {
            if attempt.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(OpcError::Com {
                    source: windows::core::Error::from_hresult(windows::core::HRESULT(
                        i32::from_ne_bytes(0x8007_06BA_u32.to_ne_bytes()),
                    )),
                })
            } else {
                Ok(())
            }
        });
        assert!(res4.is_ok());
        let srv = pool.connections.get(&endpoint).unwrap();
        assert!(srv.active_group.is_none());
    }

    #[test]
    fn test_circuit_breaker_dual_phase_and_endpoint_isolation() {
        use crate::types::OpcServerEndpoint;
        use std::time::{Duration, Instant};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let dead_endpoint = OpcServerEndpoint::from("Dead.Server.1");
        let live_endpoint = OpcServerEndpoint::from("Live.Server.1");

        // Phase 1: Initial connection failure enters 5s cooldown
        state.should_fail_connect.store(true, Ordering::SeqCst);
        let res1: OpcResult<()> =
            dispatch_with_retry(&mut pool, &connector, &dead_endpoint, |_| Ok(()));
        assert!(res1.is_err());
        assert!(res1.unwrap_err().is_connection_error());

        // Fast-fail: Next call within 5s immediately fails without calling connector
        let start = Instant::now();
        let connect_count_before = state.connect_count.load(Ordering::SeqCst);
        let res2: OpcResult<()> =
            dispatch_with_retry(&mut pool, &connector, &dead_endpoint, |_| Ok(()));
        let elapsed = start.elapsed();
        assert!(res2.is_err());
        assert!(elapsed < Duration::from_millis(100));
        assert_eq!(
            state.connect_count.load(Ordering::SeqCst),
            connect_count_before
        );

        // Phase 2: Endpoint isolation — live_endpoint operates normally
        state.should_fail_connect.store(false, Ordering::SeqCst);
        let res_live: OpcResult<i32> =
            dispatch_with_retry(&mut pool, &connector, &live_endpoint, |_| Ok(123));
        assert_eq!(res_live.unwrap(), 123);
        assert_eq!(pool.len(), 1);

        // dead_endpoint is STILL in cooldown
        let res3: OpcResult<()> =
            dispatch_with_retry(&mut pool, &connector, &dead_endpoint, |_| Ok(()));
        assert!(res3.is_err());
        assert_eq!(
            state.connect_count.load(Ordering::SeqCst),
            connect_count_before + 1
        );
    }

    #[test]
    fn test_pool_evict_synchronously_clears_active_group() {
        use crate::com::connector::{ConnectedServer, GroupConfig};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::from("Matrikon.OPC.Simulation.1");

        // Connect and create active group
        let _ = dispatch_with_retry(&mut pool, &connector, &endpoint, |pooled| {
            let group_config = GroupConfig::ephemeral("test-group");
            let created = pooled.server.add_group(&group_config)?;
            pooled.active_group = Some(CachedGroup {
                tags: vec!["Tag1".to_string()],
                group: created.group,
                server_handle: created.server_handle,
                server_item_handles: vec![ServerItemHandle::new(1)],
                valid_indices: vec![0],
                rejected_errors: Vec::new(),
            });
            Ok(())
        });

        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);
        assert_eq!(pool.len(), 1);

        // Evict endpoint
        let evicted = pool.evict(&endpoint);
        assert!(evicted);
        assert_eq!(pool.len(), 0);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 1);

        // Second evict should return false
        assert!(!pool.evict(&endpoint));
    }

    #[test]
    fn test_failure_cooldowns_pruning_and_capacity_cap() {
        use crate::com::connector::ServerConnector;
        let mut pool: ConnectionPool<<MockServerConnector as ServerConnector>::Server> =
            ConnectionPool::new();
        for i in 0..=MAX_COOLDOWNS + 5 {
            let ep = OpcServerEndpoint::from(format!("Server.{i}").as_str());
            pool.record_failure(ep);
        }
        assert!(pool.failure_cooldowns.len() <= MAX_COOLDOWNS);
    }
}
