//! Pure-Rust Tier 2 Service Provider Interface (SPI) connector traits and mock doubles.
//!
//! This module decouples the SPI traits ([`ServerConnector`], [`ConnectedServer`],
//! [`ConnectedGroup`]) and their test doubles from Windows COM interfaces, allowing
//! offline compilation and testing without `feature = "opc-da-backend"`.

pub mod traits;

#[cfg(any(test, feature = "test-support"))]
pub mod mock;

pub use traits::*;

#[cfg(any(test, feature = "test-support"))]
pub use mock::*;
