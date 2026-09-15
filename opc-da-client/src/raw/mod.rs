//! Isolated low-level COM and FFI subsystem.
//!
//! Contains raw Win32 COM interfaces (`bindings`) and memory allocators and wrappers
//! (`memory`).

pub mod bindings;
pub mod memory;
