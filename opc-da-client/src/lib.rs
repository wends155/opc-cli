#![allow(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod client;
pub mod connector;
pub mod errors;
pub mod provider;
pub mod types;

#[cfg(feature = "opc-da-backend")]
pub(crate) mod raw;

pub(crate) mod com;

// =========================================================================
// Tier 1: Public Facade (Root Exports)
// =========================================================================

pub use errors::{ConversionError, OpcError, OpcResult, WorkerError};

pub use provider::{OpcProvider, ServerDiscovery, TagBrowser, TagReader, TagWriter};

pub use types::{
    BaseVarType, BrowseDirection, BrowseType, ClientGroupHandle, ClientItemHandle, Clsid,
    DisplayOptionOpcValue, DisplayOptionTimestamp, IntoTags, IntoWriteBatch, OpcQuality,
    OpcServerEndpoint, OpcServerInfo, OpcValue, OpcValueOptionExt, ParseClsidError,
    ParseEndpointError, ParseQualityError, ParseServerIdError, QualityLimit, QualityMajor,
    QualitySubstatus, ServerGroupHandle, ServerIdentifier, ServerItemHandle, SystemTimeOptionExt,
    TagBatch, TagBatchIter, TagCollector, TagExtractError, TagValue, TagValues, VarType,
    WriteBatch, WriteBatchIntoIter, WriteBatchIter, WriteResult,
};

pub use client::{Bound, DefaultOpcDaClient, OpcDaClient, OpcDaClientBuilder, Unbound};

#[cfg(feature = "opc-da-backend")]
pub use com::{
    connector::ComConnector,
    discovery::{OpcServerRegistration, OpcServerType, inspect_local_registration},
};

// =========================================================================
// Test Support Exports
// =========================================================================

#[cfg(feature = "test-support")]
pub use provider::{
    MockOpcProvider, MockServerDiscovery, MockTagBrowser, MockTagReader, MockTagWriter,
};

#[cfg(feature = "test-support")]
pub use client::MockOpcDaClient;

// =========================================================================
// Sanity Test Suite
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_independence_types() {
        let endpoint = OpcServerEndpoint::local("Test.ProgId");
        assert_eq!(endpoint.host.as_deref(), None);
        assert_eq!(endpoint.identifier.to_string(), "Test.ProgId");
    }

    #[cfg(feature = "test-support")]
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

    #[test]
    fn test_parse_endpoint_error_reexport() {
        use super::ParseEndpointError;
        let err: ParseEndpointError = "".parse::<super::OpcServerEndpoint>().unwrap_err();
        let _: &dyn std::error::Error = &err;
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_default_opc_da_client_reexport() {
        use super::DefaultOpcDaClient;
        fn assert_send<T: Send>() {}
        assert_send::<DefaultOpcDaClient>();
    }
}
