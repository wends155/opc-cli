//! Shared mock state and telemetry tracking for OPC DA SPI test doubles.

use crate::types::{BrowseDirection, OpcServerEndpoint};

/// Shared atomic state for mock failure injection and counters.
#[derive(Default, Debug)]
pub struct MockState {
    /// Number of successful connection invocations.
    pub connect_count: std::sync::atomic::AtomicUsize,
    /// Injects failure on server connection and server enumeration.
    pub should_fail_connect: std::sync::atomic::AtomicBool,
    /// Injects write errors on item write operations.
    pub should_fail_write: std::sync::atomic::AtomicBool,
    /// Simulates general connection drop errors.
    pub should_fail_connection: std::sync::atomic::AtomicBool,
    /// Simulates RPC server unavailable error (0x800706BA) triggering connection eviction.
    pub should_fail_with_connection_error: std::sync::atomic::AtomicBool,
    /// Injects failure on server ping.
    pub should_fail_ping: std::sync::atomic::AtomicBool,
    /// Injects ProgID resolution failure (CO_E_CLASSSTRING).
    pub should_fail_progid: std::sync::atomic::AtomicBool,
    /// Simulates worker thread panic on request handling.
    pub should_panic_on_request: std::sync::atomic::AtomicBool,
    /// Number of times remove_group has been invoked.
    pub remove_group_count: std::sync::atomic::AtomicUsize,
    /// Number of times add_group has been invoked.
    pub add_group_count: std::sync::atomic::AtomicUsize,
    /// Number of times add_items has been invoked.
    pub add_items_count: std::sync::atomic::AtomicUsize,
    /// Number of times read has been invoked.
    pub read_count: std::sync::atomic::AtomicUsize,
    /// Last group name passed to add_group.
    pub last_group_name: std::sync::Mutex<Option<String>>,
    /// Last endpoint passed to connect_identifier or connect_endpoint.
    pub last_connected_endpoint: std::sync::Mutex<Option<OpcServerEndpoint>>,
    /// Last host passed to enumerate_servers / enumerate_server_details.
    pub last_enumerated_host: std::sync::Mutex<Option<String>>,
    /// Number of times change_browse_position has been invoked.
    pub change_browse_position_count: std::sync::atomic::AtomicUsize,
    /// Last direction passed to change_browse_position.
    pub last_browse_direction: std::sync::Mutex<Option<BrowseDirection>>,
    /// Injects failure on change_browse_position.
    pub should_fail_browse_position: std::sync::atomic::AtomicBool,
}

impl MockState {
    /// Creates a new default mock state instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the last connected endpoint with lock-poison recovery.
    pub fn record_connection(&self, ep: &OpcServerEndpoint) {
        let mut lock = self
            .last_connected_endpoint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = Some(ep.clone());
    }

    /// Returns the last connected endpoint with lock-poison recovery.
    #[must_use]
    pub fn last_connected_endpoint(&self) -> Option<OpcServerEndpoint> {
        self.last_connected_endpoint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Records the last group name with lock-poison recovery.
    pub fn record_group_name(&self, name: &str) {
        let mut lock = self
            .last_group_name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = Some(name.to_string());
    }

    /// Returns the last group name with lock-poison recovery.
    #[must_use]
    pub fn last_group_name(&self) -> Option<String> {
        self.last_group_name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Records the last enumerated host with lock-poison recovery.
    pub fn record_enumerated_host(&self, host: &str) {
        let mut lock = self
            .last_enumerated_host
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = Some(host.to_string());
    }

    /// Returns the last enumerated host with lock-poison recovery.
    #[must_use]
    pub fn last_enumerated_host(&self) -> Option<String> {
        self.last_enumerated_host
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Records the last browse direction with lock-poison recovery.
    pub fn record_browse_direction(&self, dir: BrowseDirection) {
        let mut lock = self
            .last_browse_direction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = Some(dir);
    }

    /// Returns the last browse direction with lock-poison recovery.
    #[must_use]
    pub fn last_browse_direction(&self) -> Option<BrowseDirection> {
        *self
            .last_browse_direction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
