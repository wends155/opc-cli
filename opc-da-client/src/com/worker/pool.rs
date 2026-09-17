//! Connection pool management, active group caching, and retry dispatch engine.

use crate::connector::traits::{
    ConnectedServer, CreatedGroup, GroupConfig, GroupRemovalMode, ServerConnector,
};
use crate::errors::{OpcError, OpcResult};
use crate::log_opc_err;
use crate::types::TagBatch;
use crate::types::{
    NamespaceType, OpcServerEndpoint, ServerGroupHandle, ServerItemHandle, VarType,
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Circuit breaker cooldown period for unreachable endpoints (5 seconds).
pub(crate) const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(5);

/// Maximum number of cached active groups retained per pooled server.
pub(crate) const MAX_ACTIVE_GROUPS: usize = 4;

/// Maximum number of active server connections retained in the connection pool.
pub(crate) const MAX_ACTIVE_CONNECTIONS: usize = 32;

/// Cached active group holding server handle, group proxy, item handles, and tag IDs.
#[allow(dead_code)]
pub(crate) struct CachedGroup<G> {
    /// Tag IDs associated with this cached group in registration order.
    pub(crate) tags: Vec<String>,
    /// Underlying connected group facade instance.
    pub(crate) group: G,
    /// Server-assigned handle for the group.
    pub(crate) server_handle: ServerGroupHandle,
    /// Server-assigned handles for items corresponding to `tags`.
    pub(crate) server_item_handles: Vec<ServerItemHandle>,
    /// Indices of items that were successfully registered on the server.
    pub(crate) valid_indices: Vec<usize>,
    /// Rejected item indices and their errors.
    pub(crate) rejected_errors: Vec<(usize, OpcError)>,
}

/// A connected server instance held in the connection pool with bounded LRU active groups.
pub(crate) struct PooledServer<S: ConnectedServer> {
    /// Connected server facade instance.
    pub(crate) server: S,
    /// Bounded LRU active groups reused across consecutive or interleaved reads.
    pub(crate) active_groups: VecDeque<CachedGroup<S::Group>>,
    /// Timestamp of the last access for LRU connection pool eviction.
    pub(crate) last_used: Instant,
}

impl<S: ConnectedServer> PooledServer<S> {
    /// Creates a new pooled server wrapper without active groups.
    #[must_use]
    pub(crate) fn new(server: S) -> Self {
        Self {
            server,
            active_groups: VecDeque::new(),
            last_used: Instant::now(),
        }
    }

    /// Searches for a cached active group whose tag list matches `tags`.
    pub(crate) fn find_active_group_idx(&self, tags: &TagBatch) -> Option<usize> {
        self.active_groups.iter().position(|g| {
            g.tags.len() == tags.len()
                && g.tags
                    .iter()
                    .zip(tags.iter_str())
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    }

    /// Promotes a cached group to the most-recently-used (MRU) position (front).
    pub(crate) fn promote_group(&mut self, idx: usize) {
        if idx > 0
            && idx < self.active_groups.len()
            && let Some(group) = self.active_groups.remove(idx)
        {
            self.active_groups.push_front(group);
        }
    }

    /// Inserts a newly registered active group at the MRU position (front),
    /// evicting the least-recently-used group if capacity exceeds [`MAX_ACTIVE_GROUPS`].
    pub(crate) fn insert_active_group(&mut self, group: CachedGroup<S::Group>) {
        if self.active_groups.len() >= MAX_ACTIVE_GROUPS
            && let Some(lru) = self.active_groups.pop_back()
        {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = self
                    .server
                    .remove_group(lru.server_handle, GroupRemovalMode::Force);
            }));
        }
        self.active_groups.push_front(group);
    }

    /// Removes and disarms a specific active group at index `idx` (e.g. on length mismatch).
    pub(crate) fn remove_active_group(&mut self, idx: usize) -> Option<CachedGroup<S::Group>> {
        if idx < self.active_groups.len() {
            let group = self.active_groups.remove(idx)?;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = self
                    .server
                    .remove_group(group.server_handle, GroupRemovalMode::Force);
            }));
            Some(group)
        } else {
            None
        }
    }

    /// Explicitly removes and clears all cached active groups from the server.
    pub(crate) fn clear_active_groups(&mut self) {
        for group in self.active_groups.drain(..) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = self
                    .server
                    .remove_group(group.server_handle, GroupRemovalMode::Force);
            }));
        }
    }

    /// Backward-compatible alias for existing callers/tests.
    pub(crate) fn clear_active_group(&mut self) {
        self.clear_active_groups();
    }

    /// Returns `true` if this pooled server maintains at least one active group.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn has_active_groups(&self) -> bool {
        !self.active_groups.is_empty()
    }
}

impl<S: ConnectedServer> Drop for PooledServer<S> {
    fn drop(&mut self) {
        self.clear_active_groups();
    }
}

impl<S: ConnectedServer> ConnectedServer for PooledServer<S> {
    type Group = S::Group;
    type ItemIterator = S::ItemIterator;

    fn ping(&self) -> OpcResult<()> {
        self.server.ping()
    }

    fn query_organization(&self) -> OpcResult<NamespaceType> {
        self.server.query_organization()
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: crate::types::BrowseType,
        filter: Option<&str>,
        data_type: VarType,
        access_rights: u32,
    ) -> OpcResult<Self::ItemIterator> {
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

    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
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
pub(crate) struct ConnectionPool<S: ConnectedServer> {
    /// Map of active pooled server connections.
    pub(crate) connections: HashMap<OpcServerEndpoint, PooledServer<S>>,
    /// Map of endpoint failure timestamps for circuit breaker cooldowns.
    pub(crate) failure_cooldowns: HashMap<OpcServerEndpoint, Instant>,
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
pub(crate) const MAX_COOLDOWNS: usize = 256;

impl<S: ConnectedServer> ConnectionPool<S> {
    /// Creates a new empty connection pool.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records an endpoint failure timestamp, pruning expired circuit breaker cooldowns
    /// and capping memory growth at [`MAX_COOLDOWNS`] via LRU eviction.
    pub(crate) fn record_failure(&mut self, endpoint: OpcServerEndpoint) {
        self.failure_cooldowns
            .retain(|_, failed_at| failed_at.elapsed() < CIRCUIT_BREAKER_COOLDOWN);

        if self.failure_cooldowns.len() >= MAX_COOLDOWNS
            && let Some(oldest) = self
                .failure_cooldowns
                .iter()
                .min_by_key(|(_, t)| **t)
                .map(|(k, _)| k.clone())
        {
            self.failure_cooldowns.remove(&oldest);
        }

        self.failure_cooldowns.insert(endpoint, Instant::now());
    }

    /// Number of active connections currently maintained in the pool.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.connections.len()
    }

    /// Returns `true` if the connection pool holds no active connections.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Clears all active connections (triggering group cleanups via `Drop`) and resets failure cooldowns.
    pub(crate) fn clear(&mut self) {
        self.connections.clear();
        self.failure_cooldowns.clear();
    }

    /// Removes an endpoint from the connection pool, returning its `PooledServer` if present.
    pub(crate) fn remove(&mut self, endpoint: &OpcServerEndpoint) -> Option<PooledServer<S>> {
        self.connections.remove(endpoint)
    }

    /// Evicts an endpoint from the connection pool, synchronously clearing any active group proxy before removal.
    ///
    /// Returns `true` if an active connection was present and evicted.
    pub(crate) fn evict(&mut self, endpoint: &OpcServerEndpoint) -> bool {
        if let Some(mut pooled) = self.connections.remove(endpoint) {
            pooled.clear_active_group();
            true
        } else {
            false
        }
    }

    /// Evicts the least-recently-used connection if capacity is full, excluding the given target endpoint.
    pub(crate) fn evict_lru_connection(&mut self, exclude: &OpcServerEndpoint) {
        if self.connections.len() >= MAX_ACTIVE_CONNECTIONS {
            let oldest_key = self
                .connections
                .iter()
                .filter(|(ep, _)| *ep != exclude)
                .min_by_key(|(_, srv)| srv.last_used)
                .map(|(ep, _)| ep.clone());

            if let Some(oldest) = oldest_key {
                self.evict(&oldest);
            }
        }
    }
}

/// Declares whether a pooled operation may be transparently retried on connection failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryPolicy {
    /// Operation is safe to retry on fresh connection (e.g. ReadTagValues, BrowseTags, Ping).
    Idempotent,
    /// Operation mutates PLC state and must NOT be automatically retried (e.g. WriteTagValue).
    NonIdempotent,
}

/// Dispatches an operation against a pooled server connection, transparently evicting
/// and reconnecting if a stale proxy RPC error is detected, while enforcing circuit breaker cooldowns.
#[tracing::instrument(level = "debug", skip(pool, connector, operation))]
pub(crate) fn dispatch_with_retry<C, F, R>(
    pool: &mut ConnectionPool<C::Server>,
    connector: &Arc<C>,
    endpoint: &OpcServerEndpoint,
    retry_policy: RetryPolicy,
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
        srv.last_used = Instant::now();
        srv
    } else {
        tracing::debug!(server = %endpoint, "Cache miss, connecting");
        pool.evict_lru_connection(endpoint);
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
                "dispatch:connection_error",
                server = %endpoint,
                action = "evicting_stale_connection"
            );
            pool.evict(endpoint);
            if retry_policy == RetryPolicy::NonIdempotent {
                tracing::warn!(
                    server = %endpoint,
                    "Non-idempotent operation failed with connection error; skipping automatic retry"
                );
                return Err(e);
            }
            tracing::debug!(server = %endpoint, "Reconnecting");
            let fresh_srv = match connector.connect_endpoint(endpoint) {
                Ok(s) => s,
                Err(connect_e) => {
                    log_opc_err!(
                        &connect_e,
                        "dispatch:reconnect",
                        server = %endpoint
                    );
                    if connect_e.is_connection_error() {
                        pool.record_failure(endpoint.clone());
                    }
                    return Err(connect_e);
                }
            };
            let mut fresh_pooled = PooledServer::new(fresh_srv);
            fresh_pooled.last_used = Instant::now();
            let result = operation(&mut fresh_pooled);
            if let Err(ref op_e) = result {
                log_opc_err!(
                    op_e,
                    "dispatch:retried",
                    server = %endpoint
                );
                if op_e.is_connection_error() {
                    pool.record_failure(endpoint.clone());
                    return result;
                }
            }
            tracing::info!(server = %endpoint, "Reconnection successful, pool updated");
            pool.evict_lru_connection(endpoint);
            pool.connections.insert(endpoint.clone(), fresh_pooled);
            result
        }
        Err(e) => {
            log_opc_err!(
                &e,
                "dispatch:operation",
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
    use crate::connector::mock::{MockServerConnector, MockState};
    use crate::errors::OpcError;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_dispatch_cache_hit_avoids_reconnect() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        // First call: cache miss, connect_count becomes 1
        let res1 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(42),
        );
        assert_eq!(res1.unwrap(), 42);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 1);

        // Second call: cache hit, connect_count remains 1
        let res2 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(84),
        );
        assert_eq!(res2.unwrap(), 84);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_dispatch_connection_error_evicts_and_reconnects() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        // First connect
        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(()),
        );
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);

        // Operation triggers connection error (RPC server unavailable)
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        let res = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| {
                let n = attempt.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(OpcError::Com {
                        source: windows_core::Error::from_hresult(windows_core::HRESULT(
                            i32::from_ne_bytes(0x8007_06BA_u32.to_ne_bytes()),
                        )),
                    })
                } else {
                    Ok("recovered")
                }
            },
        );

        assert_eq!(res.unwrap(), "recovered");
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 2);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_dispatch_non_connection_error_does_not_evict() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(()),
        );
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);

        // Operation returns non-connection error (e.g. InvalidState)
        let res: OpcResult<()> = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| Err(OpcError::InvalidState("item not found".into())),
        );
        assert!(res.is_err());
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_worker_active_group_caching_hit_miss_and_invalidation() {
        use crate::connector::{ConnectedGroup, ConnectedServer, GroupConfig, GroupItemDef};
        use crate::types::{ClientItemHandle, OpcServerEndpoint, ServerItemHandle};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        let tags_a = vec!["Tag1".to_string(), "Tag2".to_string()];
        let tags_b = vec!["Tag3".to_string()];

        // 1. First read with tags_a: cache miss, creates group & adds items
        let res1 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if pooled.active_groups.iter().any(|g| g.tags == tags_a) {
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
                pooled.insert_active_group(CachedGroup {
                    tags: tags_a.clone(),
                    group: created.group,
                    server_handle: created.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
                    valid_indices: vec![0, 1],
                    rejected_errors: Vec::new(),
                });
                Ok("miss")
            },
        );
        assert_eq!(res1.unwrap(), "miss");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.add_items_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // 2. Second read with EXACT SAME tags_a: cache hit! No add_group or add_items
        let res2 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if pooled.active_groups.iter().any(|g| g.tags == tags_a) {
                    return Ok("hit");
                }
                Ok("miss")
            },
        );
        assert_eq!(res2.unwrap(), "hit");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.add_items_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // 3. Third read with DIFFERENT tags_b: cache miss, removes old group, creates new
        let res3 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if pooled.active_groups.iter().any(|g| g.tags == tags_b) {
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
                pooled.insert_active_group(CachedGroup {
                    tags: tags_b.clone(),
                    group: created.group,
                    server_handle: created.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(1)],
                    valid_indices: vec![0],
                    rejected_errors: Vec::new(),
                });
                Ok("miss_switched")
            },
        );
        assert_eq!(res3.unwrap(), "miss_switched");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.add_items_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 1);

        // 4. Invalidation on connection error: evicts connection and drops cached group
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        let res4: OpcResult<()> = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |_| {
                if attempt.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(OpcError::Com {
                        source: windows_core::Error::from_hresult(windows_core::HRESULT(
                            i32::from_ne_bytes(0x8007_06BA_u32.to_ne_bytes()),
                        )),
                    })
                } else {
                    Ok(())
                }
            },
        );
        assert!(res4.is_ok());
        let srv = pool.connections.get(&endpoint).unwrap();
        assert!(!srv.has_active_groups());
    }

    #[test]
    fn test_circuit_breaker_dual_phase_and_endpoint_isolation() {
        use crate::types::OpcServerEndpoint;
        use std::time::{Duration, Instant};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let dead_endpoint = OpcServerEndpoint::local_prog_id("Dead.Server.1");
        let live_endpoint = OpcServerEndpoint::local_prog_id("Live.Server.1");

        // Phase 1: Initial connection failure enters 5s cooldown
        state.should_fail_connect.store(true, Ordering::SeqCst);
        let res1: OpcResult<()> = dispatch_with_retry(
            &mut pool,
            &connector,
            &dead_endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(()),
        );
        assert!(res1.is_err());
        assert!(res1.unwrap_err().is_connection_error());

        // Fast-fail: Next call within 5s immediately fails without calling connector
        let start = Instant::now();
        let connect_count_before = state.connect_count.load(Ordering::SeqCst);
        let res2: OpcResult<()> = dispatch_with_retry(
            &mut pool,
            &connector,
            &dead_endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(()),
        );
        let elapsed = start.elapsed();
        assert!(res2.is_err());
        assert!(elapsed < Duration::from_millis(100));
        assert_eq!(
            state.connect_count.load(Ordering::SeqCst),
            connect_count_before
        );

        // Phase 2: Endpoint isolation — live_endpoint operates normally
        state.should_fail_connect.store(false, Ordering::SeqCst);
        let res_live: OpcResult<i32> = dispatch_with_retry(
            &mut pool,
            &connector,
            &live_endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(123),
        );
        assert_eq!(res_live.unwrap(), 123);
        assert_eq!(pool.len(), 1);

        // dead_endpoint is STILL in cooldown
        let res3: OpcResult<()> = dispatch_with_retry(
            &mut pool,
            &connector,
            &dead_endpoint,
            RetryPolicy::Idempotent,
            |_| Ok(()),
        );
        assert!(res3.is_err());
        assert_eq!(
            state.connect_count.load(Ordering::SeqCst),
            connect_count_before + 1
        );
    }

    #[test]
    fn test_pool_evict_synchronously_clears_active_group() {
        use crate::connector::{ConnectedServer, GroupConfig};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        // Connect and create active group
        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                let group_config = GroupConfig::ephemeral("test-group");
                let created = pooled.server.add_group(&group_config)?;
                pooled.insert_active_group(CachedGroup {
                    tags: vec!["Tag1".to_string()],
                    group: created.group,
                    server_handle: created.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(1)],
                    valid_indices: vec![0],
                    rejected_errors: Vec::new(),
                });
                Ok(())
            },
        );

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
    fn test_dispatch_non_idempotent_skips_retry_on_connection_error() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        // First connect
        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::NonIdempotent,
            |_| Ok(()),
        );
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 1);

        // Operation triggers connection error (RPC server unavailable)
        let attempt = std::sync::atomic::AtomicUsize::new(0);
        let res: OpcResult<()> = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::NonIdempotent,
            |_| {
                attempt.fetch_add(1, Ordering::SeqCst);
                Err(OpcError::Com {
                    source: windows_core::Error::from_hresult(windows_core::HRESULT(
                        i32::from_ne_bytes(0x8007_06BA_u32.to_ne_bytes()),
                    )),
                })
            },
        );

        assert!(res.is_err(), "Non-idempotent operation should return error");
        assert_eq!(
            attempt.load(Ordering::SeqCst),
            1,
            "Operation should only be attempted once"
        );
        assert_eq!(
            state.connect_count.load(Ordering::SeqCst),
            1,
            "Should not have reconnected for non-idempotent operation"
        );
        assert_eq!(pool.len(), 0, "Stale connection should be evicted");
    }

    #[test]
    fn test_failure_cooldowns_pruning_and_capacity_cap() {
        use crate::connector::ServerConnector;
        let mut pool: ConnectionPool<<MockServerConnector as ServerConnector>::Server> =
            ConnectionPool::new();
        for i in 0..=MAX_COOLDOWNS + 5 {
            let ep = OpcServerEndpoint::local_prog_id(format!("Server.{i}"));
            pool.record_failure(ep);
        }
        assert!(pool.failure_cooldowns.len() <= MAX_COOLDOWNS);
    }

    #[test]
    fn test_active_groups_multi_slot_caching_hit_both() {
        use crate::connector::{ConnectedServer, GroupConfig};
        use crate::types::{IntoTags, ServerItemHandle};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        let batch_a = ["Tag1", "Tag2"].into_tag_batch();
        let batch_b = ["Tag3", "Tag4"].into_tag_batch();

        // 1. Read Batch A -> cache miss, adds group
        let res1 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if let Some(idx) = pooled.find_active_group_idx(&batch_a) {
                    pooled.promote_group(idx);
                    return Ok("hit_a");
                }
                let created = pooled.server.add_group(&GroupConfig::ephemeral("g-a"))?;
                pooled.insert_active_group(CachedGroup {
                    tags: batch_a.iter_str().map(String::from).collect(),
                    group: created.group,
                    server_handle: created.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
                    valid_indices: vec![0, 1],
                    rejected_errors: Vec::new(),
                });
                Ok("miss_a")
            },
        );
        assert_eq!(res1.unwrap(), "miss_a");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 1);

        // 2. Read Batch B -> cache miss, adds second group (retains Batch A)
        let res2 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if let Some(idx) = pooled.find_active_group_idx(&batch_b) {
                    pooled.promote_group(idx);
                    return Ok("hit_b");
                }
                let created = pooled.server.add_group(&GroupConfig::ephemeral("g-b"))?;
                pooled.insert_active_group(CachedGroup {
                    tags: batch_b.iter_str().map(String::from).collect(),
                    group: created.group,
                    server_handle: created.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(3), ServerItemHandle::new(4)],
                    valid_indices: vec![0, 1],
                    rejected_errors: Vec::new(),
                });
                Ok("miss_b")
            },
        );
        assert_eq!(res2.unwrap(), "miss_b");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // 3. Read Batch A again -> cache hit! No recreation
        let res3 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if let Some(idx) = pooled.find_active_group_idx(&batch_a) {
                    pooled.promote_group(idx);
                    return Ok("hit_a");
                }
                Ok("miss_unexpected")
            },
        );
        assert_eq!(res3.unwrap(), "hit_a");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // 4. Read Batch B again -> cache hit! No recreation
        let res4 = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                if let Some(idx) = pooled.find_active_group_idx(&batch_b) {
                    pooled.promote_group(idx);
                    return Ok("hit_b");
                }
                Ok("miss_unexpected")
            },
        );
        assert_eq!(res4.unwrap(), "hit_b");
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 2);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_active_groups_lru_eviction_at_capacity() {
        use crate::connector::{ConnectedServer, GroupConfig};
        use crate::types::{IntoTags, ServerItemHandle};

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        // Insert 4 distinct batches (Batch 1..=4)
        for i in 1..=4 {
            let tags = vec![format!("Tag{i}")];
            let _ = dispatch_with_retry(
                &mut pool,
                &connector,
                &endpoint,
                RetryPolicy::Idempotent,
                |pooled| {
                    let created = pooled
                        .server
                        .add_group(&GroupConfig::ephemeral(&format!("g-{i}")))?;
                    pooled.insert_active_group(CachedGroup {
                        tags: tags.clone(),
                        group: created.group,
                        server_handle: created.server_handle,
                        server_item_handles: vec![ServerItemHandle::new(1)],
                        valid_indices: vec![0],
                        rejected_errors: Vec::new(),
                    });
                    Ok(())
                },
            );
        }

        let srv = pool.connections.get(&endpoint).unwrap();
        assert_eq!(srv.active_groups.len(), 4);
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 4);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 0);

        // Insert Batch 5 -> evicts Batch 1 (LRU at back)
        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                let created = pooled.server.add_group(&GroupConfig::ephemeral("g-5"))?;
                pooled.insert_active_group(CachedGroup {
                    tags: vec!["Tag5".to_string()],
                    group: created.group,
                    server_handle: created.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(1)],
                    valid_indices: vec![0],
                    rejected_errors: Vec::new(),
                });
                Ok(())
            },
        );

        let srv = pool.connections.get(&endpoint).unwrap();
        assert_eq!(srv.active_groups.len(), 4);
        assert_eq!(state.add_group_count.load(Ordering::SeqCst), 5);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 1);
        // Batch 1 is gone
        assert!(
            srv.find_active_group_idx(&["Tag1"].into_tag_batch())
                .is_none()
        );
        // Batch 5 is MRU at front
        assert_eq!(
            srv.find_active_group_idx(&["Tag5"].into_tag_batch()),
            Some(0)
        );
    }

    #[test]
    fn test_active_groups_single_group_invalidation() {
        use crate::connector::{ConnectedServer, GroupConfig};
        use crate::types::ServerItemHandle;

        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();
        let endpoint = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");

        // Insert Group A and Group B
        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &endpoint,
            RetryPolicy::Idempotent,
            |pooled| {
                let g1 = pooled.server.add_group(&GroupConfig::ephemeral("g1"))?;
                pooled.insert_active_group(CachedGroup {
                    tags: vec!["TagA".to_string()],
                    group: g1.group,
                    server_handle: g1.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(1)],
                    valid_indices: vec![0],
                    rejected_errors: Vec::new(),
                });
                let g2 = pooled.server.add_group(&GroupConfig::ephemeral("g2"))?;
                pooled.insert_active_group(CachedGroup {
                    tags: vec!["TagB".to_string()],
                    group: g2.group,
                    server_handle: g2.server_handle,
                    server_item_handles: vec![ServerItemHandle::new(2)],
                    valid_indices: vec![0],
                    rejected_errors: Vec::new(),
                });
                Ok(())
            },
        );

        let srv = pool.connections.get_mut(&endpoint).unwrap();
        assert_eq!(srv.active_groups.len(), 2);

        // Invalidate MRU group at index 0 (TagB)
        let removed = srv.remove_active_group(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().tags, vec!["TagB".to_string()]);
        assert_eq!(srv.active_groups.len(), 1);
        assert_eq!(state.remove_group_count.load(Ordering::SeqCst), 1);

        // Group A remains intact
        assert_eq!(srv.active_groups[0].tags, vec!["TagA".to_string()]);
    }

    #[test]
    fn test_connection_pool_bounded_lru_eviction() {
        let state = Arc::new(MockState::default());
        let connector = Arc::new(MockServerConnector::with_state(state.clone()));
        let mut pool = ConnectionPool::new();

        // Populate 32 connections (MAX_ACTIVE_CONNECTIONS)
        for i in 1..=32 {
            let ep = OpcServerEndpoint::local_prog_id(format!("Server.{i}"));
            let _ =
                dispatch_with_retry(&mut pool, &connector, &ep, RetryPolicy::Idempotent, |_| {
                    Ok(())
                });
        }
        assert_eq!(pool.len(), 32);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 32);

        // Access Server.1 to refresh its last_used timestamp
        let ep1 = OpcServerEndpoint::local_prog_id("Server.1");
        let _ = dispatch_with_retry(&mut pool, &connector, &ep1, RetryPolicy::Idempotent, |_| {
            Ok(())
        });
        assert_eq!(pool.len(), 32);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 32);

        // Connect to 33rd endpoint -> evicts LRU (Server.2)
        let ep33 = OpcServerEndpoint::local_prog_id("Server.33");
        let _ = dispatch_with_retry(
            &mut pool,
            &connector,
            &ep33,
            RetryPolicy::Idempotent,
            |_| Ok(()),
        );

        assert_eq!(pool.len(), 32);
        assert_eq!(state.connect_count.load(Ordering::SeqCst), 33);
        // Server.1 was refreshed, so it is still in the pool
        assert!(pool.connections.contains_key(&ep1));
        // Server.33 is in the pool
        assert!(pool.connections.contains_key(&ep33));
        // Server.2 was the oldest unrefreshed, so it was evicted
        let ep2 = OpcServerEndpoint::local_prog_id("Server.2");
        assert!(!pool.connections.contains_key(&ep2));
    }

    #[test]
    fn test_active_group_cache_hit_case_insensitive() {
        use crate::connector::mock::MockConnectedServer;
        use crate::types::TagBatch;
        use crate::types::handles::{ServerGroupHandle, ServerItemHandle};

        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);

        pooled.insert_active_group(CachedGroup {
            tags: vec!["TEMP1".to_string(), "DEVICE.SPEED".to_string()],
            group: pooled.server.group.clone(),
            server_handle: ServerGroupHandle::new(1),
            server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
            valid_indices: vec![0, 1],
            rejected_errors: Vec::new(),
        });

        let search_batch = TagBatch::from_static(&["temp1", "device.speed"]);
        assert_eq!(
            pooled.find_active_group_idx(&search_batch),
            Some(0),
            "find_active_group_idx must match cached group when tag names differ only in ASCII casing"
        );
    }

    #[test]
    fn test_active_group_cache_order_sensitive() {
        use crate::connector::mock::MockConnectedServer;
        use crate::types::TagBatch;
        use crate::types::handles::{ServerGroupHandle, ServerItemHandle};

        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);

        pooled.insert_active_group(CachedGroup {
            tags: vec!["TAG_A".to_string(), "TAG_B".to_string()],
            group: pooled.server.group.clone(),
            server_handle: ServerGroupHandle::new(1),
            server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
            valid_indices: vec![0, 1],
            rejected_errors: Vec::new(),
        });

        let permuted_batch = TagBatch::from_static(&["TAG_B", "TAG_A"]);
        assert_eq!(
            pooled.find_active_group_idx(&permuted_batch),
            None,
            "Permuted tag batch must not match cached active group (matching must be strictly positional)"
        );
    }

    #[test]
    fn test_active_group_cache_length_mismatch() {
        use crate::connector::mock::MockConnectedServer;
        use crate::types::TagBatch;
        use crate::types::handles::{ServerGroupHandle, ServerItemHandle};

        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);

        pooled.insert_active_group(CachedGroup {
            tags: vec!["TAG_A".to_string(), "TAG_B".to_string()],
            group: pooled.server.group.clone(),
            server_handle: ServerGroupHandle::new(1),
            server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
            valid_indices: vec![0, 1],
            rejected_errors: Vec::new(),
        });

        let shorter = TagBatch::from_static(&["TAG_A"]);
        assert_eq!(
            pooled.find_active_group_idx(&shorter),
            None,
            "Shorter batch must not match longer cached group"
        );
        let longer = TagBatch::from_static(&["TAG_A", "TAG_B", "TAG_C"]);
        assert_eq!(
            pooled.find_active_group_idx(&longer),
            None,
            "Longer batch must not match shorter cached group"
        );
    }

    #[test]
    fn test_active_group_cache_empty_batch() {
        use crate::connector::mock::MockConnectedServer;
        use crate::types::TagBatch;
        use crate::types::handles::{ServerGroupHandle, ServerItemHandle};

        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);

        pooled.insert_active_group(CachedGroup {
            tags: vec!["TAG_A".to_string()],
            group: pooled.server.group.clone(),
            server_handle: ServerGroupHandle::new(1),
            server_item_handles: vec![ServerItemHandle::new(1)],
            valid_indices: vec![0],
            rejected_errors: Vec::new(),
        });

        let empty = TagBatch::from_static(&[]);
        assert_eq!(
            pooled.find_active_group_idx(&empty),
            None,
            "Empty batch must not match non-empty cached active group"
        );
    }
}
