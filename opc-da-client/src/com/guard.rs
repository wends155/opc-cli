//! RAII guard for COM initialization/teardown.
//!
//! Ensures `CoUninitialize` is called exactly once per successful
//! `CoInitializeEx`, even on early returns or panics.

use crate::com::connector::ConnectedServer;
use crate::errors::OpcResult;
use crate::types::{BrowseDirection, GroupHandle};
use std::marker::PhantomData;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// Drop guard for COM thread initialization.
///
/// Calling [`ComGuard::new`] initializes COM in Multi-Threaded Apartment
/// (MTA) mode. When the guard is dropped, `CoUninitialize` is called
/// automatically.
///
/// # Thread Safety
///
/// `ComGuard` is intentionally `!Send` and `!Sync`. COM initialization
/// is per-thread — the guard **must** be created and dropped on the same
/// OS thread. This is enforced at compile time.
///
/// # Examples
///
/// ```ignore
/// use opc_da_client::com::guard::ComGuard;
/// use opc_da_client::errors::OpcResult;
/// fn initialize_com() -> OpcResult<()> {
///     let _guard = ComGuard::new()?;
///     // ... COM operations ...
///     // CoUninitialize called automatically on drop
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct ComGuard {
    /// Prevents `Send + Sync` auto-derivation. COM init is per-thread.
    _not_send: PhantomData<*mut ()>,
}

impl ComGuard {
    /// Initialize COM in Multi-Threaded Apartment (MTA) mode.
    ///
    /// Returns `Ok(ComGuard)` on success (including `S_FALSE`, which
    /// means COM was already initialized on this thread).
    ///
    /// # Errors
    ///
    /// Returns `Err(OpcError::Com)` if `CoInitializeEx` fails with a fatal HRESULT.
    #[tracing::instrument(level = "info", err)]
    pub fn new() -> OpcResult<Self> {
        // SAFETY: CoInitializeEx is a standard Win32 FFI call passing COINIT_MULTITHREADED to join MTA.
        // SAFETY: Result is checked below, and CoUninitialize is guaranteed via Drop.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

        hr.ok()
            .inspect_err(|e| tracing::error!(error = ?e, "COM MTA initialization failed"))?;

        tracing::debug!("COM MTA initialized");

        Ok(Self {
            _not_send: PhantomData,
        })
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        tracing::debug!("COM MTA teardown");
        // SAFETY: Paired with the successful CoInitializeEx in new().
        // SAFETY: Construction guarantees COM was initialized, so this call is always balanced. Only runs on the creating thread (!Send).
        unsafe {
            CoUninitialize();
        }
    }
}

/// Injectable COM initialization strategy — enables testing the failure path.
///
/// This is an internal trait. External consumers use [`ComGuard::new`] directly.
pub(crate) trait ComInitializer: Send + 'static {
    fn init() -> OpcResult<ComGuard>;
}

/// Production COM initializer — calls `ComGuard::new()`.
pub(crate) struct DefaultComInit;

impl ComInitializer for DefaultComInit {
    fn init() -> OpcResult<ComGuard> {
        ComGuard::new()
    }
}

/// Test-only COM initializer that always fails with a synthetic HRESULT.
#[cfg(test)]
pub(crate) struct FailingComInit;

#[cfg(test)]
impl ComInitializer for FailingComInit {
    fn init() -> OpcResult<ComGuard> {
        Err(crate::errors::OpcError::Internal(
            "Synthetic COM init failure (test)".into(),
        ))
    }
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

/// RAII drop guard for OPC DA namespace browsing position.
///
/// Ensures `ConnectedServer::change_browse_position(BrowseDirection::Up, "")` is called
/// when the guard is dropped, restoring the browse cursor to the parent branch
/// across early returns, error propagation with `?`, and panics.
pub(crate) struct BrowsePositionGuard<'a, S: ConnectedServer> {
    server: &'a S,
    active: bool,
}

impl<'a, S: ConnectedServer> BrowsePositionGuard<'a, S> {
    /// Changes the server browse position down to `branch` and arms the guard.
    pub(crate) fn enter(server: &'a S, branch: &str) -> OpcResult<Self> {
        server.change_browse_position(BrowseDirection::Down, branch)?;
        Ok(Self {
            server,
            active: true,
        })
    }

    /// Disarms the guard so `BrowseDirection::Up` will not be invoked on drop.
    #[allow(dead_code)]
    pub(crate) fn disarm(&mut self) {
        self.active = false;
    }
}

impl<S: ConnectedServer> Drop for BrowsePositionGuard<'_, S> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Err(e) = self.server.change_browse_position(BrowseDirection::Up, "") {
            tracing::warn!(
                error = ?e,
                "Failed to restore OPC browse position during RAII drop cleanup"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::connector::MockConnectedServer;
    use crate::errors::OpcResult;
    use std::sync::atomic::Ordering;

    #[test]
    fn com_guard_new_returns_opc_result() {
        // Static compile test: ComGuard::new() must return OpcResult<ComGuard>.
        // This test will NOT compile if the return type remains an external Result.
        let _: OpcResult<ComGuard> = ComGuard::new();
    }

    #[test]
    fn com_guard_constructs_and_drops() {
        // On Windows, CoInitializeEx(MTA) should succeed.
        // On non-Windows CI, this test is skipped by target gate.
        let guard = ComGuard::new();
        assert!(guard.is_ok(), "ComGuard::new() should succeed: {guard:?}");
        // Guard drops here — CoUninitialize runs.
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

    #[test]
    fn test_browse_position_guard_enter_and_drop() {
        let server = MockConnectedServer::default();
        {
            let guard = BrowsePositionGuard::enter(&server, "Branch1");
            assert!(guard.is_ok());
        }
    }

    #[test]
    fn test_browse_position_guard_disarm() {
        let server = MockConnectedServer::default();
        {
            let mut guard = BrowsePositionGuard::enter(&server, "Branch1").unwrap();
            guard.disarm();
        }
    }
}
