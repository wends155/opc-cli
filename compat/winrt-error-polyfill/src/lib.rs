//! # WinRT Error API Polyfill (`api-ms-win-core-winrt-error-l1-1-0.dll`)
//!
//! Provides Windows 7 SP1 and Windows Server 2008 R2 SP1 (NT 6.1) backward compatibility
//! stubs for WinRT error APIs introduced in Windows 8, preventing entry-point DLL
//! load failures in legacy and air-gapped industrial environments.

#![no_std]
#![allow(non_snake_case)]

use core::ffi::c_void;
use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

type HRESULT = i32;

const S_OK: HRESULT = 0;
const S_FALSE: HRESULT = 1;

/// Reports an error and provides an optional descriptive string.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
/// Pointer arguments may be null or point to unmanaged memory.
///
/// # Arguments
///
/// * `_error` - The error code associated with the error condition.
/// * `_message` - Optional pointer to an `HSTRING` error message.
///
/// # Returns
///
/// Returns 0 (success) as a no-op polyfill stub.
#[no_mangle]
pub unsafe extern "system" fn RoOriginateError(_error: HRESULT, _message: *const c_void) -> i32 {
    0
}

/// Reports an error and provides an optional UTF-16 wide string description.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
/// Pointer arguments may be null or point to valid wide string buffers.
///
/// # Arguments
///
/// * `_error` - The error code associated with the error condition.
/// * `_cch_max` - The maximum number of characters in the message string.
/// * `_message` - Optional pointer to a null-terminated UTF-16 message string.
///
/// # Returns
///
/// Returns 0 (success) as a no-op polyfill stub.
#[no_mangle]
pub unsafe extern "system" fn RoOriginateErrorW(
    _error: HRESULT,
    _cch_max: u32,
    _message: *const u16,
) -> i32 {
    0
}

/// Reports a transformed error and an existing error string to an attached debugger.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
/// Pointer arguments may be null or point to unmanaged memory.
///
/// # Arguments
///
/// * `_old_error` - The original error code associated with the error condition.
/// * `_new_error` - A different error code associated with the error condition.
/// * `_message` - Optional pointer to an `HSTRING` error message.
///
/// # Returns
///
/// Returns 0 (success) as a no-op polyfill stub.
#[no_mangle]
pub unsafe extern "system" fn RoTransformError(
    _old_error: HRESULT,
    _new_error: HRESULT,
    _message: *const c_void,
) -> i32 {
    0
}

/// Sets the restricted error information object for the current logical thread.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
/// Pointer argument may be null or point to an `IRestrictedErrorInfo` instance.
///
/// # Arguments
///
/// * `_restricted_error_info` - Optional pointer to an `IRestrictedErrorInfo` interface.
///
/// # Returns
///
/// Returns `S_OK` (0).
#[no_mangle]
pub unsafe extern "system" fn SetRestrictedErrorInfo(
    _restricted_error_info: *const c_void,
) -> HRESULT {
    S_OK
}

/// Retrieves the restricted error information object set by a previous call to `SetRestrictedErrorInfo`.
///
/// # Safety
///
/// If `restricted_error_info` is non-null, it must point to valid writable memory capable of
/// storing a pointer.
///
/// # Arguments
///
/// * `restricted_error_info` - Out-pointer to receive the restricted error info interface pointer.
///
/// # Returns
///
/// Returns `S_FALSE` (1), indicating no restricted error info is present.
#[no_mangle]
pub unsafe extern "system" fn GetRestrictedErrorInfo(
    restricted_error_info: *mut *mut c_void,
) -> HRESULT {
    if !restricted_error_info.is_null() {
        *restricted_error_info = core::ptr::null_mut();
    }
    S_FALSE
}

/// Removes the current restricted error information object from the calling thread.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
#[no_mangle]
pub unsafe extern "system" fn RoClearError() {}

/// Associates an error context with a thread.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
///
/// # Arguments
///
/// * `_hr` - The `HRESULT` associated with the error condition.
///
/// # Returns
///
/// Returns `S_OK` (0).
#[no_mangle]
pub unsafe extern "system" fn RoCaptureErrorContext(_hr: HRESULT) -> HRESULT {
    S_OK
}

/// Raises a fail-fast exception in the current process with error context.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
///
/// # Arguments
///
/// * `_hr` - The `HRESULT` associated with the error condition.
#[no_mangle]
pub unsafe extern "system" fn RoFailFastWithErrorContext(_hr: HRESULT) {}

/// Notifies registered callbacks that an unhandled error has occurred.
///
/// # Safety
///
/// Must be invoked according to standard Win32 `extern "system"` calling conventions.
/// Pointer argument may be null or point to an `IRestrictedErrorInfo` interface.
///
/// # Arguments
///
/// * `_error_info` - Optional pointer to an `IRestrictedErrorInfo` interface.
///
/// # Returns
///
/// Returns `S_OK` (0).
#[no_mangle]
pub unsafe extern "system" fn RoReportUnhandledError(_error_info: *const c_void) -> HRESULT {
    S_OK
}

/// Retrieves restricted error information matching the specified `HRESULT`.
///
/// # Safety
///
/// If out-pointers are non-null, they must point to valid writable pointer memory.
///
/// # Arguments
///
/// * `_hr` - The `HRESULT` to match.
/// * `_restricted_error_string` - Out-pointer to receive the restricted error string.
/// * `_error_info` - Out-pointer to receive the restricted error info interface pointer.
///
/// # Returns
///
/// Returns `S_FALSE` (1).
#[no_mangle]
pub unsafe extern "system" fn RoGetMatchingErrorRestricted(
    _hr: HRESULT,
    _restricted_error_string: *mut *mut c_void,
    _error_info: *mut *mut c_void,
) -> HRESULT {
    S_FALSE
}
