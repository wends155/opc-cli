//! Isolated low-level COM and FFI subsystem.
//!
//! Contains raw Win32 COM interfaces (`bindings`), memory allocators and wrappers
//! (`memory`), and low-level FFI bridge types (`bridge`).

pub mod bindings;
pub mod bridge;
pub mod memory;
