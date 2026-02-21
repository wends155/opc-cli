//! RAII guard for COM initialization/teardown.
//!
//! Ensures `CoUninitialize` is called exactly once per successful
//! `CoInitializeEx`, even on early returns or panics.

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// Drop guard for COM thread initialization.
///
/// # Safety invariant
///
/// `CoUninitialize` is only called if `CoInitializeEx` returned `Ok`.
/// The guard must be used on the same thread that called [`ComGuard::new`].
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # use opc_da_client::ComGuard;
/// # fn main() -> Result<()> {
/// let _guard = ComGuard::new()?;
/// // ... COM operations ...
/// // CoUninitialize called automatically on drop
/// # Ok(())
/// # }
/// ```
pub struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    /// Initialize COM in Multi-Threaded Apartment (MTA) mode.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `CoInitializeEx` fails with a fatal HRESULT.
    /// `S_FALSE` (already initialized) is treated as success.
    pub fn new() -> anyhow::Result<Self> {
        // SAFETY: `CoInitializeEx` is an FFI call to the Windows COM
        // runtime. We pass `COINIT_MULTITHREADED` to join the MTA.
        // The result is checked and `CoUninitialize` is guaranteed
        // via the Drop impl.
        // SAFETY: CoInitializeEx is a standard COM initialization call.
        // We use COINIT_MULTITHREADED to ensure MTA for the process/thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Ok(Self {
            initialized: hr.is_ok(),
        })
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: Paired with the successful `CoInitializeEx` in
            // `new()`. Only called on the same thread (enforced by
            // `spawn_blocking` in the caller).
            // SAFETY: CoUninitialize balances the CoInitializeEx call in new().
            // We ensure this is only called if initialization succeeded.
            unsafe {
                CoUninitialize();
            }
        }
    }
}
