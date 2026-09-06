#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

#[cfg(not(feature = "std"))]
use core::panic::PanicInfo;

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[link(name = "advapi32")]
extern "system" {
    /// RtlGenRandom (a.k.a. SystemFunction036) — cryptographically secure
    /// RNG available on all Windows versions since XP SP2.
    #[link_name = "SystemFunction036"]
    fn RtlGenRandom(pb_buffer: *mut u8, cb_buffer: u32) -> u8;
}

/// Polyfill for `ProcessPrng` (Windows 8+ / bcryptprimitives.dll).
///
/// Routes random byte generation to `RtlGenRandom` in `advapi32.dll`.
#[cfg(not(feature = "std"))]
#[no_mangle]
pub unsafe extern "system" fn ProcessPrng(pb_data: *mut u8, cb_data: usize) -> i32 {
    process_prng_impl(pb_data, cb_data)
}

/// Core implementation of `ProcessPrng`.
///
/// # Safety
/// Caller must ensure `pb_data` points to a writable buffer of at least `cb_data` bytes.
pub unsafe fn process_prng_impl(pb_data: *mut u8, cb_data: usize) -> i32 {
    if cb_data == 0 {
        return 1;
    }
    if pb_data.is_null() {
        return 0;
    }

    const CHUNK_SIZE: usize = 0x1000_0000; // 256 MiB chunks
    let mut offset = 0usize;
    while offset < cb_data {
        let chunk = core::cmp::min(cb_data - offset, CHUNK_SIZE);
        let res = RtlGenRandom(pb_data.add(offset), chunk as u32);
        if res == 0 {
            return 0;
        }
        offset += chunk;
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_prng_chunking_and_null_rejection() {
        let mut buf = [0u8; 64];
        unsafe {
            // Null pointer with non-zero length MUST return 0 (failure)
            assert_eq!(process_prng_impl(core::ptr::null_mut(), 10), 0, "Null pointer must return 0");
            // Zero length returns 1 (success)
            assert_eq!(process_prng_impl(buf.as_mut_ptr(), 0), 1, "Zero length must return 1");
            // Normal generation
            assert_eq!(process_prng_impl(buf.as_mut_ptr(), buf.len()), 1, "Buffer filled successfully");
            assert!(buf.iter().any(|&b| b != 0), "Buffer must contain non-zero entropy");
        }
    }
}
