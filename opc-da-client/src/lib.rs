#![allow(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod errors;
mod provider;
pub mod types;

#[cfg(feature = "opc-da-backend")]
pub(crate) mod raw;

#[cfg(feature = "opc-da-backend")]
pub mod com;

// Stable public API
pub use errors::{OpcError, OpcResult};
pub use provider::{
    DisplayOptionOpcValue, DisplayOptionTimestamp, OpcProvider, OpcQuality, OpcValue,
    OpcValueOptionExt, QualityLimit, QualityMajor, QualitySubstatus, ServerDiscovery,
    SystemTimeOptionExt, TagBrowser, TagCollector, TagReader, TagValue, TagWriter, WriteResult,
};
#[allow(deprecated)]
pub use types::{
    BrowseDirection, BrowseType, ClientGroupHandle, ClientItemHandle, GroupHandle, IntoTags,
    ItemHandle, OpcServerEndpoint, OpcServerInfo, ParseQualityError, ServerGroupHandle,
    ServerIdentifier, ServerItemHandle, TagBatch, TagBatchIter, TagExtractError, TagFailure,
    TagResult, TagSuccess, TagValues,
};

// Backend re-exports (conditional)
#[cfg(feature = "opc-da-backend")]
pub use com::{
    client::{OpcDaClient, OpcDaClientBuilder},
    connector::{
        ComConnector, ConnectedGroup, ConnectedServer, GroupRemovalMode, ItemWrite, ServerConnector,
    },
    discovery::{OpcServerRegistration, OpcServerType, inspect_local_registration},
};

// Test support re-export
#[cfg(feature = "test-support")]
pub use provider::MockOpcProvider;

#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
pub use com::connector::{MockConnectedGroup, MockConnectedServer, MockServerConnector, MockState};

/// Type alias for an [`OpcDaClient`] instantiated with [`MockServerConnector`].
#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
pub type MockOpcDaClient = com::client::OpcDaClient<com::connector::MockServerConnector>;

#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
impl Default for com::client::OpcDaClient<com::connector::MockServerConnector> {
    fn default() -> Self {
        match Self::new(com::connector::MockServerConnector::default()) {
            Ok(client) => client,
            Err(e) => unreachable!("mock client initializes successfully: {e:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
    #[test]
    fn test_mock_opc_da_client_default() {
        use super::MockOpcDaClient;
        let _client = MockOpcDaClient::default();
    }

    #[cfg(feature = "opc-da-backend")]
    #[test]
    fn test_opc_da_client_builder_reexport() {
        use super::OpcDaClientBuilder;
        let builder = OpcDaClientBuilder::new()
            .host("localhost")
            .server("Matrikon.OPC.Simulation.1");
        let _ = format!("{builder:?}");
    }

    #[test]
    fn test_parse_quality_error_reexport() {
        use super::ParseQualityError;
        let err: ParseQualityError = "INVALID".parse::<super::OpcQuality>().unwrap_err();
        let _: &dyn std::error::Error = &err;
        assert!(err.to_string().contains("Invalid OPC quality string"));
    }
}
