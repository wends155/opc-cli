#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use core::ffi::c_void;

#[cfg(not(feature = "std"))]
use core::panic::PanicInfo;

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[link(name = "kernel32")]
extern "system" {
    #[link_name = "Sleep"]
    fn Kernel32Sleep(dw_milliseconds: u32);
    fn SetLastError(dw_err_code: u32);
}

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_TIMEOUT: u32 = 1460;

/// Re-export Sleep so the PE loader can resolve it from this API set DLL.
#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "system" fn Sleep(dw_milliseconds: u32) {
    Kernel32Sleep(dw_milliseconds);
}

/// Polyfill for `WaitOnAddress` (Windows 8+).
#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "system" fn WaitOnAddress(
    address: *const c_void,
    compare_address: *const c_void,
    address_size: usize,
    milliseconds: u32,
) -> i32 {
    wait_on_address_impl(address, compare_address, address_size, milliseconds)
}

/// Core implementation of `WaitOnAddress`.
///
/// # Safety
/// Caller must ensure `address` and `compare_address` point to readable memory of at least `address_size` bytes.
pub unsafe fn wait_on_address_impl(
    address: *const c_void,
    compare_address: *const c_void,
    address_size: usize,
    milliseconds: u32,
) -> i32 {
    if address.is_null() || compare_address.is_null() {
        SetLastError(ERROR_INVALID_PARAMETER);
        return 0;
    }

    if !matches!(address_size, 1 | 2 | 4 | 8) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return 0;
    }

    // Natural alignment requirement: address and compare_address must be naturally aligned to address_size
    if (address as usize) % address_size != 0 || (compare_address as usize) % address_size != 0 {
        SetLastError(ERROR_INVALID_PARAMETER);
        return 0;
    }

    let mut elapsed: u32 = 0;

    loop {
        // SAFETY: Pointer alignment and bounds are verified above; volatile read ensures LLVM does not hoist reads out of the spin loop
        let is_equal = match address_size {
            1 => core::ptr::read_volatile(address as *const u8) == core::ptr::read(compare_address as *const u8),
            2 => core::ptr::read_volatile(address as *const u16) == core::ptr::read(compare_address as *const u16),
            4 => core::ptr::read_volatile(address as *const u32) == core::ptr::read(compare_address as *const u32),
            8 => core::ptr::read_volatile(address as *const u64) == core::ptr::read(compare_address as *const u64),
            _ => unreachable!(),
        };

        if !is_equal {
            return 1;
        }

        if milliseconds != 0xFFFF_FFFF && elapsed >= milliseconds {
            SetLastError(ERROR_TIMEOUT);
            return 0;
        }

        Kernel32Sleep(1);
        if milliseconds != 0xFFFF_FFFF {
            elapsed = elapsed.saturating_add(1);
        }
    }
}

/// No-op polyfill — wakes one thread waiting on `WaitOnAddress`.
#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "system" fn WakeByAddressSingle(_address: *const c_void) {}

/// No-op polyfill — wakes all threads waiting on `WaitOnAddress`.
#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "system" fn WakeByAddressAll(_address: *const c_void) {}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_on_address_unaligned_and_timeout() {
        let mut buf = [0u8; 32];
        let mut cmp = [0u8; 32];

        // Odd offset (unaligned 32-bit pointer) - MUST return 0
        unsafe {
            let addr = buf.as_ptr().add(1) as *const core::ffi::c_void;
            let cmp_addr = cmp.as_ptr().add(1) as *const core::ffi::c_void;
            let ret = wait_on_address_impl(addr, cmp_addr, 4, 10);
            assert_eq!(ret, 0, "Unaligned address must return 0");
        }

        // Invalid size (3 bytes) - MUST return 0
        unsafe {
            let addr = buf.as_ptr() as *const core::ffi::c_void;
            let cmp_addr = cmp.as_ptr() as *const core::ffi::c_void;
            let ret = wait_on_address_impl(addr, cmp_addr, 3, 10);
            assert_eq!(ret, 0, "Invalid size must return 0");
        }

        // Different values (immediate return 1)
        buf[0] = 0xAA;
        cmp[0] = 0xBB;
        unsafe {
            let addr = buf.as_ptr() as *const core::ffi::c_void;
            let cmp_addr = cmp.as_ptr() as *const core::ffi::c_void;
            let ret = wait_on_address_impl(addr, cmp_addr, 1, 100);
            assert_eq!(ret, 1, "Different values must return 1 immediately");
        }
    }
}
