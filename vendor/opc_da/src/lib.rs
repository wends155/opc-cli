#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::cargo_common_metadata,
    clippy::needless_pass_by_value,
    clippy::unreadable_literal
)]
pub mod def;
pub mod utils;

#[cfg(feature = "unstable_client")]
pub mod client;
#[cfg(feature = "unstable_server")]
pub mod server;
