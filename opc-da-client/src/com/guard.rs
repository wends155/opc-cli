//! RAII guard for COM initialization/teardown.
//!
//! Ensures `CoUninitialize` is called exactly once per successful
//! `CoInitializeEx`, even on early returns or panics.

use crate::errors::OpcResult;
#[cfg(feature = "opc-da-backend")]
use std::marker::PhantomData;
#[cfg(feature = "opc-da-backend")]
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
#[cfg(feature = "opc-da-backend")]
#[derive(Debug)]
#[must_use = "Dropping ComGuard immediately uninitializes COM on the current thread"]
pub struct ComGuard {
    /// Prevents `Send + Sync` auto-derivation. COM init is per-thread.
    _not_send: PhantomData<*mut ()>,
}

#[cfg(feature = "opc-da-backend")]
impl ComGuard {
    /// Initialize COM in Multi-Threaded Apartment (MTA) mode.
    /// # Returns
    ///
    /// `Ok(ComGuard)` on success (including `S_FALSE`, which means COM was already initialized on this thread).
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

#[cfg(feature = "opc-da-backend")]
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

/// Injectable COM initialization strategy.
///
/// Implementors control what "COM setup" means for a given connector:
/// - `DefaultComInit`: calls `CoInitializeEx(MTA)` on the worker thread (production).
/// - `NoOpComInit`: returns `Ok(())` immediately (offline / test connectors).
pub(crate) trait ComInitializer: Send + 'static {
    /// The guard type held alive for the duration of the worker thread.
    type Guard: 'static;
    fn init() -> OpcResult<Self::Guard>;
}

/// Production COM initializer — calls `ComGuard::new()`.
#[cfg(feature = "opc-da-backend")]
pub(crate) struct DefaultComInit;

#[cfg(feature = "opc-da-backend")]
impl ComInitializer for DefaultComInit {
    type Guard = ComGuard;
    fn init() -> OpcResult<ComGuard> {
        ComGuard::new()
    }
}

/// No-op COM initializer for offline/test connectors that do not require Windows COM.
/// Compiled unconditionally.
#[allow(dead_code)]
pub(crate) struct NoOpComInit;

impl ComInitializer for NoOpComInit {
    type Guard = ();
    fn init() -> OpcResult<()> {
        Ok(())
    }
}

/// Selects the production COM initializer when `opc-da-backend` is active,
/// falling back to the no-op initializer for offline / cross-platform builds.
#[cfg(feature = "opc-da-backend")]
pub(crate) type ActiveDefaultComInit = DefaultComInit;
#[cfg(not(feature = "opc-da-backend"))]
pub(crate) type ActiveDefaultComInit = NoOpComInit;

/// Test-only COM initializer that always fails with a synthetic HRESULT.
#[cfg(test)]
pub(crate) struct FailingComInit;

#[cfg(test)]
impl ComInitializer for FailingComInit {
    type Guard = ();
    fn init() -> OpcResult<()> {
        Err(crate::errors::OpcError::Internal(
            "Synthetic COM init failure (test)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify `NoOpComInit` compiles and returns `Ok(())` unconditionally.
    #[test]
    fn no_op_com_init_returns_ok() {
        assert!(NoOpComInit::init().is_ok());
    }

    #[cfg(feature = "opc-da-backend")]
    #[test]
    fn com_guard_new_returns_opc_result() {
        // Static compile test: ComGuard::new() must return OpcResult<ComGuard>.
        // This test will NOT compile if the return type remains an external Result.
        let _: OpcResult<ComGuard> = ComGuard::new();
    }

    #[cfg(feature = "opc-da-backend")]
    #[test]
    fn com_guard_constructs_and_drops() {
        // On Windows, CoInitializeEx(MTA) should succeed.
        // On non-Windows CI, this test is skipped by target gate.
        let guard = ComGuard::new();
        assert!(guard.is_ok(), "ComGuard::new() should succeed: {guard:?}");
        // Guard drops here — CoUninitialize runs.
    }
}
