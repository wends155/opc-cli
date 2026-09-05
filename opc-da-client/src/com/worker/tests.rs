#![allow(
    clippy::single_char_pattern,
    clippy::cast_possible_wrap,
    clippy::ptr_as_ptr,
    clippy::borrow_as_ptr,
    clippy::mixed_attributes_style,
    clippy::unreadable_literal,
    clippy::undocumented_unsafe_blocks,
    clippy::manual_assert
)]

use super::*;
use crate::com::connector::{
    ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
    GroupItemResult, GroupItemState, MockConnectedGroup, MockConnectedServer, MockServerConnector,
    MockState, ServerConnector, StringIterator,
};
use crate::com::guard::GroupGuard;
use crate::errors::{OpcError, OpcResult};
use crate::provider::{OpcQuality, OpcValue, TagCollector};
use crate::types::{
    BrowseDirection, BrowseType, GroupHandle, ItemHandle, OpcServerInfo, ServerIdentifier,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::oneshot;

struct WorkerMockConnector;
struct WorkerMockServer;
struct WorkerMockGroup;

impl ConnectedGroup for WorkerMockGroup {
    fn add_items(&self, _items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn read(
        &self,
        _source: DataSource,
        _server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn write(
        &self,
        _server_handles: &[ItemHandle],
        _values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        Err(OpcError::NotImplemented("mock".into()))
    }
}

impl ConnectedServer for WorkerMockServer {
    type Group = WorkerMockGroup;
    fn query_organization(&self) -> OpcResult<u32> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn browse_opc_item_ids(
        &self,
        _browse_type: BrowseType,
        _filter: Option<&str>,
        _data_type: u16,
        _access_rights: u32,
    ) -> OpcResult<StringIterator> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn change_browse_position(&self, _direction: BrowseDirection, _name: &str) -> OpcResult<()> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn get_item_id(&self, _item_name: &str) -> OpcResult<String> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn add_group(&self, _config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
        Err(OpcError::NotImplemented("mock".into()))
    }
}

impl ServerConnector for WorkerMockConnector {
    type Server = WorkerMockServer;
    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        Ok(vec!["Mock.Server.1".into()])
    }
    fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        Ok(vec![OpcServerInfo {
            prog_id: "Mock.Server.1".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Mock Server 1".into()),
            host: None,
        }])
    }
    fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
        Ok(WorkerMockServer)
    }
}

#[tokio::test]
async fn test_worker_starts_and_stops() {
    let worker =
        tokio::task::spawn_blocking(|| ComWorker::start(Arc::new(WorkerMockConnector)).unwrap())
            .await
            .unwrap();
    drop(worker);
}

#[tokio::test]
async fn test_worker_list_servers() {
    let worker =
        tokio::task::spawn_blocking(|| ComWorker::start(Arc::new(WorkerMockConnector)).unwrap())
            .await
            .unwrap();
    let (reply, _rx) = oneshot::channel();
    worker
        .sender
        .send(ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_worker_list_server_details() {
    let worker =
        tokio::task::spawn_blocking(|| ComWorker::start(Arc::new(WorkerMockConnector)).unwrap())
            .await
            .unwrap();
    let (reply, rx) = oneshot::channel();
    worker
        .sender
        .send(ComRequest::ListServerDetails {
            host: "localhost".into(),
            reply,
        })
        .await
        .unwrap();
    let details = rx.await.unwrap().unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].prog_id, "Mock.Server.1");
}

struct MismatchedConnector;
struct MismatchedServer;
struct MismatchedGroup;

impl ConnectedGroup for MismatchedGroup {
    fn add_items(&self, _items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        Ok(vec![])
    }
    fn read(
        &self,
        _source: DataSource,
        _server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        Ok(vec![])
    }
    fn write(
        &self,
        _server_handles: &[ItemHandle],
        _values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        Ok(vec![])
    }
}

impl ConnectedServer for MismatchedServer {
    type Group = MismatchedGroup;
    fn query_organization(&self) -> OpcResult<u32> {
        Ok(0)
    }
    fn browse_opc_item_ids(
        &self,
        _b: BrowseType,
        _f: Option<&str>,
        _d: u16,
        _a: u32,
    ) -> OpcResult<StringIterator> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn change_browse_position(&self, _direction: BrowseDirection, _name: &str) -> OpcResult<()> {
        Ok(())
    }
    fn get_item_id(&self, _item_name: &str) -> OpcResult<String> {
        Ok(String::new())
    }
    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        Ok(CreatedGroup {
            group: MismatchedGroup,
            server_handle: GroupHandle(1),
            revised_update_rate_ms: config.update_rate_ms,
        })
    }
    fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
        Ok(())
    }
}

impl ServerConnector for MismatchedConnector {
    type Server = MismatchedServer;
    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        Ok(vec![])
    }
    fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        Ok(vec![])
    }
    fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
        Ok(MismatchedServer)
    }
}

#[tokio::test]
async fn test_worker_read_tag_values_mismatched_lengths() {
    let worker =
        tokio::task::spawn_blocking(|| ComWorker::start(Arc::new(MismatchedConnector)).unwrap())
            .await
            .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            server: ServerIdentifier::from("MockServer"),
            tag_ids: vec!["Tag1".to_string(), "Tag2".to_string()],
            reply,
        })
        .await;

    assert!(
        result.is_err(),
        "Expected read to fail due to mismatched lengths"
    );
    if let Err(OpcError::Internal(msg)) = result {
        assert!(msg.contains("mismatched result array sizes"));
    } else {
        panic!("Expected OpcError::Internal, got {:?}", result);
    }
}

#[tokio::test]
async fn test_worker_write_tag_value() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Random.Int4".to_string(),
            value: OpcValue::Int(42),
            reply,
        })
        .await
        .expect("Request should succeed");

    assert_eq!(result.tag_id, "Random.Int4");
    assert!(result.is_success(), "Write should be successful");
    assert!(result.error().is_none());
}

#[tokio::test]
async fn test_worker_write_tag_value_failure() {
    let state = Arc::new(MockState::default());
    state.should_fail_write.store(true, Ordering::Relaxed);
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Random.Int4".to_string(),
            value: OpcValue::Int(42),
            reply,
        })
        .await
        .expect("Request should complete");

    assert_eq!(result.tag_id, "Random.Int4");
    assert!(result.is_error(), "Write should fail");
    match result.status {
        Err(OpcError::Com { source }) => {
            assert_eq!(source.code(), windows::Win32::Foundation::E_FAIL);
        }
        other => panic!("Expected OpcError::Com, got {:?}", other),
    }
}

#[tokio::test]
async fn test_connection_cache_reuse() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await
        .unwrap();

    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Tag2".to_string(),
            value: OpcValue::Int(2),
            reply,
        })
        .await
        .unwrap();

    assert_eq!(
        state.connect_count.load(Ordering::Relaxed),
        1,
        "Server connection should be cached and reused"
    );
}

#[tokio::test]
async fn test_stale_connection_eviction() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    // Initial connect
    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await
        .unwrap();

    assert_eq!(state.connect_count.load(Ordering::Relaxed), 1);

    // Enable connection error flag to trigger eviction on next operation
    state
        .should_fail_with_connection_error
        .store(true, Ordering::Relaxed);

    // Next request triggers eviction and reconnect attempt
    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Tag2".to_string(),
            value: OpcValue::Int(2),
            reply,
        })
        .await;

    assert_eq!(
        state.connect_count.load(Ordering::Relaxed),
        2,
        "Stale connection should be evicted and reconnected"
    );
}

#[tokio::test]
async fn test_worker_panic_propagation() {
    let state = Arc::new(MockState::default());
    state.should_panic_on_request.store(true, Ordering::Relaxed);
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await;

    assert!(result.is_err());
    if let Err(OpcError::Internal(msg)) = result {
        assert!(
            msg.contains("shut down") || msg.contains("channel closed") || msg.contains("panicked"),
            "Expected worker termination message, got: {}",
            msg
        );
    } else {
        panic!("Expected OpcError::Internal, got {:?}", result);
    }
}

#[tokio::test]
async fn test_drop_during_active_request() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    // Dropping worker handle closes channel gracefully
    drop(worker);
}

#[tokio::test]
async fn test_worker_init_failure() {
    struct FailingInitConnector;
    impl ServerConnector for FailingInitConnector {
        type Server = std::sync::Arc<MockConnectedServer>;
        fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
            Err(OpcError::Internal("COM subsystem failed".into()))
        }
        fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
            Err(OpcError::Internal("COM subsystem failed".into()))
        }
        fn connect(&self, _name: &str) -> OpcResult<Self::Server> {
            Err(OpcError::Internal("COM subsystem failed".into()))
        }
    }

    let worker =
        tokio::task::spawn_blocking(|| ComWorker::start(Arc::new(FailingInitConnector)).unwrap())
            .await
            .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await;

    assert!(
        result.is_err(),
        "ListServers request should fail when connector enumeration fails"
    );
}

struct QualityTestConnector;
struct QualityTestServer;
struct QualityTestGroup;

impl ConnectedGroup for QualityTestGroup {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        Ok(items
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i == 4 {
                    GroupItemResult {
                        server_handle: ItemHandle(0),
                        canonical_type: 0,
                        error: Some(OpcError::Com {
                            source: windows::core::Error::from_hresult(
                                windows::Win32::Foundation::E_FAIL,
                            ),
                        }),
                    }
                } else {
                    GroupItemResult {
                        #[allow(clippy::cast_possible_truncation)]
                        server_handle: ItemHandle((i + 1) as u32),
                        canonical_type: 8,
                        error: None,
                    }
                }
            })
            .collect())
    }

    fn read(
        &self,
        _source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        let qualities: [u16; 4] = [0x00C0, 0x00D8, 0x0018, 0x0056];
        Ok(server_handles
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let val = if i != 2 {
                    OpcValue::Int(42)
                } else {
                    OpcValue::String(String::new())
                };
                Ok(GroupItemState {
                    client_handle: h,
                    value: val,
                    quality: OpcQuality::from(qualities[i % qualities.len()]),
                    timestamp: std::time::SystemTime::UNIX_EPOCH,
                })
            })
            .collect())
    }

    fn write(
        &self,
        _server_handles: &[ItemHandle],
        _values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        Ok(vec![])
    }
}

impl ConnectedServer for QualityTestServer {
    type Group = QualityTestGroup;
    fn query_organization(&self) -> OpcResult<u32> {
        Ok(0)
    }
    fn browse_opc_item_ids(
        &self,
        _b: BrowseType,
        _f: Option<&str>,
        _d: u16,
        _a: u32,
    ) -> OpcResult<StringIterator> {
        Err(OpcError::NotImplemented("mock".into()))
    }
    fn change_browse_position(&self, _d: BrowseDirection, _n: &str) -> OpcResult<()> {
        Ok(())
    }
    fn get_item_id(&self, _n: &str) -> OpcResult<String> {
        Ok(String::new())
    }
    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        Ok(CreatedGroup {
            group: QualityTestGroup,
            server_handle: GroupHandle(1),
            revised_update_rate_ms: config.update_rate_ms,
        })
    }
    fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
        Ok(())
    }
}

impl ServerConnector for QualityTestConnector {
    type Server = QualityTestServer;
    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        Ok(vec!["Quality.Mock.Server".into()])
    }
    fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        Ok(vec![OpcServerInfo {
            prog_id: "Quality.Mock.Server".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Quality Mock Server".into()),
            host: None,
        }])
    }
    fn connect(&self, _name: &str) -> OpcResult<Self::Server> {
        Ok(QualityTestServer)
    }
}

#[tokio::test]
async fn test_worker_read_tag_values_quality_decoding() {
    use crate::types::{QualityLimit, QualityMajor, QualitySubstatus};

    let worker =
        tokio::task::spawn_blocking(|| ComWorker::start(Arc::new(QualityTestConnector)).unwrap())
            .await
            .unwrap();

    let tag_ids = vec![
        "Tag.Good".to_string(),
        "Tag.Override".to_string(),
        "Tag.Comm".to_string(),
        "Tag.Limit".to_string(),
        "Tag.Rejected".to_string(),
    ];

    let results = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            server: ServerIdentifier::from("Quality.Mock.Server"),
            tag_ids,
            reply,
        })
        .await
        .unwrap();

    assert_eq!(results.len(), 5);

    // Tag 0: Good standard (0x00C0)
    assert_eq!(results[0].tag_id, "Tag.Good");
    assert_eq!(results[0].value, Some(OpcValue::Int(42)));
    assert_eq!(results[0].display_value(), "42");
    assert_eq!(results[0].quality.major, QualityMajor::Good);
    assert_eq!(results[0].quality.substatus, QualitySubstatus::NonSpecific);
    assert_eq!(results[0].quality.limit, QualityLimit::NotLimited);
    assert_eq!(results[0].quality.to_string(), "Good");
    assert!(results[0].quality.is_good());
    assert!(!results[0].quality.is_bad());
    assert!(results[0].is_good());
    assert!(!results[0].is_error());

    // Tag 1: Good with Local Override (0x00D8)
    assert_eq!(results[1].tag_id, "Tag.Override");
    assert_eq!(results[1].value, Some(OpcValue::Int(42)));
    assert_eq!(results[1].quality.major, QualityMajor::Good);
    assert_eq!(
        results[1].quality.substatus,
        QualitySubstatus::LocalOverride
    );
    assert_eq!(results[1].quality.to_string(), "Good (Local Override)");

    // Tag 2: Bad with Comm Failure (0x0018)
    assert_eq!(results[2].tag_id, "Tag.Comm");
    assert_eq!(results[2].value, Some(OpcValue::String(String::new())));
    assert_eq!(results[2].quality.major, QualityMajor::Bad);
    assert_eq!(results[2].quality.substatus, QualitySubstatus::CommFailure);
    assert_eq!(results[2].quality.to_string(), "Bad (Comm Failure)");
    assert!(results[2].quality.is_bad());

    // Tag 3: Uncertain with EGU Exceeded and High Limited (0x0056)
    assert_eq!(results[3].tag_id, "Tag.Limit");
    assert_eq!(results[3].value, Some(OpcValue::Int(42)));
    assert_eq!(results[3].quality.major, QualityMajor::Uncertain);
    assert_eq!(results[3].quality.substatus, QualitySubstatus::EguExceeded);
    assert_eq!(results[3].quality.limit, QualityLimit::HighLimited);
    assert_eq!(
        results[3].quality.to_string(),
        "Uncertain (EGU Exceeded) [High Limited]"
    );
    assert!(results[3].quality.is_uncertain());
    assert!(results[3].quality.is_limited());

    // Tag 4: Rejected at add_items
    assert_eq!(results[4].tag_id, "Tag.Rejected");
    assert_eq!(results[4].value, None);
    assert_eq!(results[4].display_value(), "Error");
    assert_eq!(results[4].timestamp, None);
    assert_eq!(results[4].formatted_timestamp(), "N/A");
    assert!(results[4].is_error());
    assert_eq!(results[4].quality, OpcQuality::BAD_CONFIG_ERROR);
    assert_eq!(results[4].quality.to_string(), "Bad (Configuration Error)");
}

#[test]
fn test_worker_com_init_failure_propagates_opc_error() {
    let connector = Arc::new(MockServerConnector::default());
    let result = ComWorker::start_with_initializer::<crate::com::guard::FailingComInit>(connector);
    assert!(result.is_err());
    let Err(err) = result else { unreachable!() };
    assert!(
        !err.to_string().contains("COM init failed on worker"),
        "Expected forwarded OpcError, got hardcoded string: {err}"
    );
    assert!(
        err.to_string().contains("Synthetic COM init failure"),
        "Expected synthetic failure message, got: {err}"
    );
}

#[tokio::test]
async fn test_worker_browse_tags_success() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(100);
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            server: ServerIdentifier::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed");

    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["Random.Int4", "Random.Real8", "Random.String"]);
    assert_eq!(collector.len(), 3);
}

#[tokio::test]
async fn test_worker_browse_tags_cancelled() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(100);
    collector.cancel();
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            server: ServerIdentifier::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed when cancelled");

    assert_eq!(result.len(), 0);
    assert_eq!(collector.len(), 0);
}

#[tokio::test]
async fn test_worker_browse_tags_capacity_cap() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(2);
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            server: ServerIdentifier::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed up to capacity");

    assert_eq!(result.len(), 2);
    assert_eq!(result, vec!["Random.Int4", "Random.Real8"]);
    assert!(collector.is_full());
}

#[tokio::test]
async fn test_worker_browse_tags_flat_organization() {
    let connector = Arc::new(MockServerConnector::default());
    connector.server.organization.store(2, Ordering::Relaxed); // NamespaceType::Flat = 2
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(100);
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            server: ServerIdentifier::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed on flat namespace");

    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["Random.Int4", "Random.Real8", "Random.String"]);
}

#[tokio::test]
async fn test_worker_tracing_instrumentation_execution() {
    let connector = std::sync::Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();
    let servers = worker
        .send_request(|reply| ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await
        .expect("list servers");
    assert_eq!(servers, vec!["Matrikon.OPC.Simulation.1".to_string()]);
}

#[test]
fn test_group_guard_cleanup_on_drop() {
    let server = MockConnectedServer::default();
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
    {
        let guard = GroupGuard::new(&server, GroupHandle(42));
        assert_eq!(guard.handle(), GroupHandle(42));
    }
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_group_guard_disarm_prevents_cleanup() {
    let server = MockConnectedServer::default();
    {
        let mut guard = GroupGuard::new(&server, GroupHandle(42));
        guard.disarm();
    }
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_worker_handle_read_error_cleans_group() {
    let connector = Arc::new(MockServerConnector::default());
    let group = MockConnectedGroup {
        add_items_fn: Some(Box::new(|_| {
            Err(OpcError::Internal("Simulated add_items failure".into()))
        })),
        ..Default::default()
    };
    let server = Arc::new(MockConnectedServer {
        group: Arc::new(group),
        state: connector.state.clone(),
        should_fail_connection: std::sync::atomic::AtomicBool::new(false),
        tags: std::sync::Arc::new(std::sync::Mutex::new(vec!["Test.Tag".to_string()])),
        organization: std::sync::atomic::AtomicU32::new(1),
    });
    let custom_connector = Arc::new(MockServerConnector {
        server: server.clone(),
        state: connector.state.clone(),
        servers: connector.servers.clone(),
        server_details: connector.server_details.clone(),
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(custom_connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            server: ServerIdentifier::from("Mock.Server.1"),
            tag_ids: vec!["Test.Tag".to_string()],
            reply,
        })
        .await;

    assert!(result.is_err());
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_worker_channel_drop_error_propagation() {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx); // Drop receiver to simulate closed worker channel
    let worker: ComWorker<MockServerConnector> = ComWorker {
        sender: tx,
        handle: None,
        _phantom: std::marker::PhantomData,
    };
    let err = worker
        .send_request(|reply| ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, OpcError::Internal(msg) if msg.contains("channel closed")));
}
