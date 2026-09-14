#![allow(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod connector;
pub mod errors;
mod provider;
pub mod types;

#[cfg(feature = "opc-da-backend")]
pub(crate) mod raw;

#[cfg(feature = "opc-da-backend")]
pub(crate) mod com;

// Stable public API
pub use errors::{ConversionError, OpcError, OpcResult, WorkerError};
pub use provider::{
    DisplayOptionOpcValue, DisplayOptionTimestamp, OpcProvider, OpcQuality, OpcValue,
    OpcValueOptionExt, QualityLimit, QualityMajor, QualitySubstatus, ServerDiscovery,
    SystemTimeOptionExt, TagBrowser, TagCollector, TagReader, TagValue, TagWriter, WriteResult,
};
pub use types::{
    BrowseDirection, BrowseType, ClientGroupHandle, ClientItemHandle, Clsid, IntoTags,
    IntoWriteBatch, OpcServerEndpoint, OpcServerInfo, ParseClsidError, ParseQualityError,
    ServerGroupHandle, ServerIdentifier, ServerItemHandle, TagBatch, TagBatchIter, TagExtractError,
    TagValues, WriteBatch, WriteBatchIntoIter, WriteBatchIter,
};

// Tier 2 Service Provider Interface (SPI)
pub use connector::{
    ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
    GroupItemResult, GroupItemState, GroupRemovalMode, ItemWrite, ServerBackend,
    ServerCatalogDiscovery, ServerConnector,
};

// Backend re-exports (conditional)
#[cfg(feature = "opc-da-backend")]
pub use com::{
    client::{Bound, OpcDaClient, OpcDaClientBuilder, Unbound},
    connector::ComConnector,
    discovery::{OpcServerRegistration, OpcServerType, inspect_local_registration},
};

// Test support re-export
#[cfg(feature = "test-support")]
pub use provider::{
    MockOpcProvider, MockServerDiscovery, MockTagBrowser, MockTagReader, MockTagWriter,
};

#[cfg(feature = "test-support")]
pub use connector::{MockConnectedGroup, MockConnectedServer, MockServerConnector, MockState};

/// Type alias for an [`OpcDaClient`] instantiated with [`MockServerConnector`].
#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
pub type MockOpcDaClient = com::client::OpcDaClient<connector::MockServerConnector>;

#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]
impl Default for com::client::OpcDaClient<connector::MockServerConnector> {
    fn default() -> Self {
        match Self::new(connector::MockServerConnector::default()) {
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
