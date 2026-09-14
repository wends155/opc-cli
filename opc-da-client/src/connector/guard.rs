//! Pure-Rust RAII resource lifecycle guards for OPC DA client connections.

use crate::connector::traits::{ConnectedServer, GroupRemovalMode};
use crate::errors::OpcResult;
use crate::types::{BrowseDirection, ServerGroupHandle};

/// RAII drop guard for OPC DA group registration on a connected server.
///
/// Ensures `ConnectedServer::remove_group(handle, GroupRemovalMode::Force)` is called
/// when the guard is dropped, preventing group handle leaks on the OPC server across
/// early returns, error propagation with `?`, and thread panics per `spec.md § 678`.
#[must_use = "Dropping GroupGuard immediately removes the OPC group"]
pub struct GroupGuard<'a, S: ConnectedServer> {
    server: &'a S,
    handle: ServerGroupHandle,
    disarmed: bool,
}

impl<'a, S: ConnectedServer> GroupGuard<'a, S> {
    /// Creates a new active `GroupGuard` wrapping the specified server and group handle.
    pub fn new(server: &'a S, handle: ServerGroupHandle) -> Self {
        Self {
            server,
            handle,
            disarmed: false,
        }
    }

    /// Returns the managed server group handle.
    #[must_use]
    pub fn handle(&self) -> ServerGroupHandle {
        self.handle
    }

    /// Disarms the guard, suppressing removal of the group upon drop and returning the handle.
    pub fn disarm(&mut self) -> ServerGroupHandle {
        self.disarmed = true;
        self.handle
    }
}

impl<S: ConnectedServer> Drop for GroupGuard<'_, S> {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Err(e) = self
                .server
                .remove_group(self.handle, GroupRemovalMode::Force)
            {
                tracing::warn!(
                    error = ?e,
                    handle = self.handle.as_raw(),
                    "Failed to remove OPC group during RAII drop cleanup"
                );
            }
        }));
    }
}

/// RAII drop guard for OPC DA namespace browsing position.
///
/// Ensures `ConnectedServer::change_browse_position(BrowseDirection::Up, "")` is called
/// when the guard is dropped, restoring the browse cursor to the parent branch across
/// early returns, error propagation with `?`, and thread panics.
#[must_use = "Dropping BrowsePositionGuard immediately restores parent browse position"]
pub struct BrowsePositionGuard<'a, S: ConnectedServer> {
    server: &'a S,
    branch: String,
    active: bool,
}

impl<'a, S: ConnectedServer> BrowsePositionGuard<'a, S> {
    /// Changes the server browse position down into `branch` and returns an armed cursor guard.
    pub fn enter(server: &'a S, branch: &str) -> OpcResult<Self> {
        server.change_browse_position(BrowseDirection::Down, branch)?;
        Ok(Self {
            server,
            branch: branch.to_string(),
            active: true,
        })
    }

    /// Returns the target branch name that this guard entered.
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Disarms the guard, suppressing the restoration of the parent browse position on drop.
    pub fn disarm(&mut self) {
        self.active = false;
    }
}

impl<S: ConnectedServer> Drop for BrowsePositionGuard<'_, S> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Err(e) = self.server.change_browse_position(BrowseDirection::Up, "") {
                tracing::warn!(
                    error = ?e,
                    branch = %self.branch,
                    "Failed to restore OPC browse position during RAII drop cleanup"
                );
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::mock::MockConnectedServer;
    use crate::types::{BrowseDirection, ServerGroupHandle};
    use std::sync::atomic::Ordering;

    #[test]
    fn test_group_guard_cleanup_on_drop() {
        let server = MockConnectedServer::default();
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
        {
            let guard = GroupGuard::new(&server, ServerGroupHandle::new(42));
            assert_eq!(guard.handle(), ServerGroupHandle::new(42));
        }
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_group_guard_disarm_prevents_cleanup() {
        let server = MockConnectedServer::default();
        {
            let mut guard = GroupGuard::new(&server, ServerGroupHandle::new(42));
            let handle = guard.disarm();
            assert_eq!(handle, ServerGroupHandle::new(42));
        }
        assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_browse_position_guard_enter_and_drop() {
        let server = MockConnectedServer::default();
        assert_eq!(
            server
                .state
                .change_browse_position_count
                .load(Ordering::Relaxed),
            0
        );
        {
            let guard = BrowsePositionGuard::enter(&server, "Branch1").unwrap();
            assert_eq!(guard.branch(), "Branch1");
            assert_eq!(
                server
                    .state
                    .change_browse_position_count
                    .load(Ordering::Relaxed),
                1
            );
            assert_eq!(
                *server.state.last_browse_direction.lock().unwrap(),
                Some(BrowseDirection::Down)
            );
        }
        assert_eq!(
            server
                .state
                .change_browse_position_count
                .load(Ordering::Relaxed),
            2
        );
        assert_eq!(
            *server.state.last_browse_direction.lock().unwrap(),
            Some(BrowseDirection::Up)
        );
    }

    #[test]
    fn test_browse_position_guard_drop_error_handling() {
        let server = MockConnectedServer::default();
        let guard = BrowsePositionGuard::enter(&server, "Branch1").unwrap();
        server
            .state
            .should_fail_browse_position
            .store(true, Ordering::Relaxed);
        drop(guard); // must log warning without panic
    }

    #[test]
    fn test_browse_position_guard_disarm() {
        let server = MockConnectedServer::default();
        {
            let mut guard = BrowsePositionGuard::enter(&server, "Branch1").unwrap();
            guard.disarm();
        }
        assert_eq!(
            server
                .state
                .change_browse_position_count
                .load(Ordering::Relaxed),
            1
        );
    }
}
