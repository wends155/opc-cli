//! Backend implementations for OPC DA communication.
//!
//! Each backend is gated behind a feature flag.

#[cfg(feature = "opc-da-backend")]
pub mod connector;

#[cfg(feature = "opc-da-backend")]
pub mod opc_da;
