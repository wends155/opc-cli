//! Isolated low-level COM and FFI subsystem.
//!
//! Contains raw Win32 COM interfaces (`bindings`), memory allocators and wrappers
//! (`memory`), and COM HRESULT error definitions (`hresult`).

pub mod bindings;
pub mod hresult;
pub mod memory;
