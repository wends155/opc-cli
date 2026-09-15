//! Integration tests for OPC DA Tag I/O, typed getters, batch reads/writes, and caching.
//!
//! Exercises public `OpcDaClient` facade and service provider role traits
//! using `MockServerConnector` without Windows COM dependencies.

#![allow(clippy::float_cmp)]

use opc_da_client::connector::mock::{MockServerConnector, MockState};
use opc_da_client::connector::traits::{GroupItemResult, GroupItemState};
use opc_da_client::errors::hresult::{E_FAIL, OPC_E_BADTYPE, OPC_E_INVALIDITEMID};
use opc_da_client::errors::{OpcError, OpcResult};
use opc_da_client::provider::{TagReader, TagWriter};
use opc_da_client::types::{
    ClientItemHandle, OpcQuality, OpcValue, ServerItemHandle, TagExtractError, TagValue, VarType,
    WriteBatch, WriteResult,
};
use opc_da_client::{Bound, OpcDaClient, Unbound};
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn test_tag_io_mixed_type_batch_read_and_typed_getters() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone()).with_tag_values(vec![
        OpcValue::Int(123),
        OpcValue::UInt(456),
        OpcValue::Float(42.5),
        OpcValue::String("Status_OK".to_string()),
        OpcValue::Bool(true),
    ]);

    let client = OpcDaClient::builder()
        .server("Mock.MixedType.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let tags = client
        .read_tags(&[
            "Item.Int",
            "Item.Uint",
            "Item.Double",
            "Item.String",
            "Item.Bool",
        ])
        .await
        .expect("batch read must succeed");

    assert_eq!(tags.len(), 5);
    assert_eq!(tags.get_i32("Item.Int"), Ok(123));
    assert_eq!(tags.get_u32("Item.Uint"), Ok(456));
    assert_eq!(tags.get_f64("Item.Double"), Ok(42.5));
    assert_eq!(tags.get_str("Item.String"), Ok("Status_OK"));
    assert_eq!(tags.get_bool("Item.Bool"), Ok(true));

    // Case-insensitivity check
    assert_eq!(tags.get_i32("item.int"), Ok(123));
    assert_eq!(tags.get_str("ITEM.STRING"), Ok("Status_OK"));

    // Lossless typed coercion check via get_as
    assert_eq!(tags.get_as::<f64>("Item.Int"), Ok(123.0));
    assert_eq!(tags.get_as::<i64>("Item.Uint"), Ok(456_i64));

    // Type mismatch error check
    assert!(matches!(
        tags.get_f64("Item.String"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
}

#[tokio::test]
async fn test_tag_io_inherent_scalar_convenience_readers() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone())
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .map(|item| {
                    let handle = match item.item_id.as_str() {
                        "Tag.F64" => 1,
                        "Tag.I32" => 2,
                        "Tag.Bool" => 3,
                        "Tag.String" => 4,
                        "Tag.F32" => 5,
                        "Tag.I64" => 6,
                        "Tag.U32" => 7,
                        "Tag.U64" => 8,
                        "Tag.Single" => 9,
                        _ => 99,
                    };
                    GroupItemResult {
                        server_handle: ServerItemHandle::new(handle),
                        canonical_type: VarType::BSTR,
                        error: None,
                    }
                })
                .collect())
        })
        .with_read_fn(|_source, handles| {
            Ok(handles
                .iter()
                .map(|&h| {
                    let val = match h.as_raw() {
                        1 => OpcValue::Float(12.34),
                        2 => OpcValue::Int(42),
                        3 => OpcValue::Bool(true),
                        4 => OpcValue::String("Running".to_string()),
                        5 => OpcValue::Float(1.5),
                        6 => OpcValue::Int(999_999),
                        7 => OpcValue::UInt(100),
                        8 => OpcValue::UInt(888_888),
                        9 => OpcValue::Int(77),
                        _ => OpcValue::Int(0),
                    };
                    Ok(GroupItemState {
                        client_handle: ClientItemHandle::new(h.as_raw()),
                        value: val,
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });

    let client = OpcDaClient::builder()
        .server("Mock.Scalar.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    assert_eq!(client.read_f64("Tag.F64").await.expect("read f64"), 12.34);
    assert_eq!(client.read_i32("Tag.I32").await.expect("read i32"), 42);
    assert!(client.read_bool("Tag.Bool").await.expect("read bool"));
    assert_eq!(
        client.read_string("Tag.String").await.expect("read string"),
        "Running"
    );
    assert_eq!(client.read_f32("Tag.F32").await.expect("read f32"), 1.5);
    assert_eq!(client.read_i64("Tag.I64").await.expect("read i64"), 999_999);
    assert_eq!(client.read_u32("Tag.U32").await.expect("read u32"), 100);
    assert_eq!(client.read_u64("Tag.U64").await.expect("read u64"), 888_888);

    let single_tag = client
        .read_tag("Tag.Single")
        .await
        .expect("read single tag");
    assert_eq!(single_tag.tag_id, "Tag.Single");
    assert!(single_tag.is_good());
    assert_eq!(single_tag.outcome, Ok(OpcValue::Int(77)));
}

#[tokio::test]
async fn test_tag_io_partial_item_read_failure_handling() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone())
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    if i == 1 {
                        GroupItemResult {
                            server_handle: ServerItemHandle::new(2),
                            canonical_type: VarType::EMPTY,
                            error: Some(OpcError::from(OPC_E_INVALIDITEMID)),
                        }
                    } else {
                        let handle_val = u32::try_from(i + 1).unwrap_or(u32::MAX);
                        GroupItemResult {
                            server_handle: ServerItemHandle::new(handle_val),
                            canonical_type: VarType::I4,
                            error: None,
                        }
                    }
                })
                .collect())
        })
        .with_read_fn(|_source, handles| {
            Ok(handles
                .iter()
                .map(|&h| {
                    if h.as_raw() == 3 {
                        Err(OpcError::from(E_FAIL))
                    } else {
                        Ok(GroupItemState {
                            client_handle: ClientItemHandle::new(h.as_raw()),
                            value: OpcValue::Int(99),
                            quality: OpcQuality::GOOD,
                            timestamp: std::time::SystemTime::UNIX_EPOCH,
                        })
                    }
                })
                .collect())
        });

    let client = OpcDaClient::builder()
        .server("Mock.PartialRead.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let tags = client
        .read_tags(&["Tag.Good", "Tag.BadAdd", "Tag.BadRead"])
        .await
        .expect("batch read call itself must succeed even with partial item failures");

    assert_eq!(tags.len(), 3);

    // Item 0: Good
    let good_val = tags.get("Tag.Good").expect("Tag.Good must exist");
    assert!(good_val.is_good());
    assert_eq!(good_val.outcome, Ok(OpcValue::Int(99)));

    // Item 1: Add failed
    let bad_add = tags.get("Tag.BadAdd").expect("Tag.BadAdd must exist");
    assert!(!bad_add.is_good());
    assert!(bad_add.outcome.is_err());
    assert!(bad_add.quality.is_bad());

    // Item 2: Read failed
    let bad_read = tags.get("Tag.BadRead").expect("Tag.BadRead must exist");
    assert!(!bad_read.is_good());
    assert!(bad_read.outcome.is_err());
    assert!(bad_read.quality.is_bad());
}

#[tokio::test]
async fn test_tag_io_empty_batch_short_circuit() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());

    let client = OpcDaClient::builder()
        .server("Mock.EmptyBatch.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // 1. Empty read batch
    let empty_reads = client
        .read_tags(&[] as &[&str])
        .await
        .expect("empty read batch must succeed immediately");
    assert!(empty_reads.is_empty());
    assert_eq!(state.read_count.load(Ordering::Relaxed), 0);
    assert_eq!(state.add_group_count.load(Ordering::Relaxed), 0);

    // 2. Empty write batch
    let empty_writes: &[(&str, OpcValue)] = &[];
    let write_results = client
        .write_tags(empty_writes)
        .await
        .expect("empty write batch must succeed immediately");
    assert!(write_results.is_empty());
    assert_eq!(state.add_group_count.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_tag_io_batch_write_partial_failures_and_diagnostics() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone()).with_write_fn(|items| {
        Ok(items
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i == 0 {
                    Ok(())
                } else if i == 1 {
                    Err(OpcError::from(OPC_E_INVALIDITEMID))
                } else {
                    Err(OpcError::from(OPC_E_BADTYPE))
                }
            })
            .collect())
    });

    let client = OpcDaClient::builder()
        .server("Mock.WriteBatch.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let batch = [
        ("Tag.Valid", OpcValue::Int(100)),
        ("Tag.InvalidId", OpcValue::Int(200)),
        ("Tag.BadType", OpcValue::String("NotANumber".to_string())),
    ];

    let results = client
        .write_tags(batch)
        .await
        .expect("batch write dispatch must succeed");

    assert_eq!(results.len(), 3);

    // Item 0: Success
    assert!(results[0].is_success());
    assert_eq!(results[0].tag_id, "Tag.Valid");
    assert!(results[0].error().is_none());

    // Item 1: Invalid Item ID
    assert!(results[1].is_error());
    assert_eq!(results[1].tag_id, "Tag.InvalidId");
    let err1 = results[1].error().expect("must have error");
    let hint1 = err1.friendly_hint().expect("hint must exist");
    assert!(hint1.to_lowercase().contains("item id") || hint1.to_lowercase().contains("tag"));

    // Item 2: Bad Type
    assert!(results[2].is_error());
    assert_eq!(results[2].tag_id, "Tag.BadType");
    let err2 = results[2].error().expect("must have error");
    let hint2 = err2.friendly_hint().expect("hint must exist");
    assert!(hint2.to_lowercase().contains("type") || hint2.to_lowercase().contains("datatype"));
}

#[tokio::test]
async fn test_tag_io_role_trait_polymorphism() {
    let state = Arc::new(MockState::default());
    let connector =
        MockServerConnector::with_state(state.clone()).with_tag_values(vec![OpcValue::Int(77)]);

    let unbound: OpcDaClient<MockServerConnector, Unbound> = OpcDaClient::builder()
        .with_connector(connector.clone())
        .build()
        .expect("unbound client must succeed");

    let bound: OpcDaClient<MockServerConnector, Bound> = OpcDaClient::builder()
        .server("Mock.Poly.Server")
        .with_connector(connector)
        .build_bound()
        .expect("bound client must succeed");

    // Generic helper consuming TagReader
    async fn read_via_role<R: TagReader>(r: &R, server: &str, tag: &str) -> OpcResult<TagValue> {
        r.read_tag_value(server, tag).await
    }

    // Generic helper consuming TagWriter single write
    async fn write_via_role<W: TagWriter>(
        w: &W,
        server: &str,
        tag: &str,
        val: OpcValue,
    ) -> OpcResult<WriteResult> {
        w.write_tag_value(server, tag, val).await
    }

    // Generic helper consuming TagWriter batch write
    async fn write_batch_via_role<W: TagWriter>(
        w: &W,
        server: &str,
        writes: WriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        w.write_tag_batch(server, writes).await
    }

    // 1. Unbound instance polymorphism
    let res_unbound_read = read_via_role(&unbound, "Mock.Poly.Server", "Tag.A")
        .await
        .expect("unbound read via role must succeed");
    assert_eq!(res_unbound_read.outcome, Ok(OpcValue::Int(77)));

    let res_unbound_write =
        write_via_role(&unbound, "Mock.Poly.Server", "Tag.A", OpcValue::Int(10))
            .await
            .expect("unbound write via role must succeed");
    assert!(res_unbound_write.is_success());

    // 2. Bound instance polymorphism
    let res_bound_read = read_via_role(&bound, "Mock.Poly.Server", "Tag.B")
        .await
        .expect("bound read via role must succeed");
    assert_eq!(res_bound_read.outcome, Ok(OpcValue::Int(77)));

    let res_bound_write = write_via_role(&bound, "Mock.Poly.Server", "Tag.B", OpcValue::Int(20))
        .await
        .expect("bound write via role must succeed");
    assert!(res_bound_write.is_success());

    let res_batch_write = write_batch_via_role(
        &bound,
        "Mock.Poly.Server",
        vec![("Tag.C".to_string(), OpcValue::Int(30))].into(),
    )
    .await
    .expect("bound batch write via role must succeed");
    assert_eq!(res_batch_write.len(), 1);
    assert!(res_batch_write[0].is_success());
}

#[tokio::test]
async fn test_tag_io_active_group_cache_hit() {
    let state = Arc::new(MockState::default());
    let connector =
        MockServerConnector::with_state(state.clone()).with_tag_values(vec![OpcValue::Int(100)]);

    let client = OpcDaClient::builder()
        .server("Mock.Cache.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // Read 1: Initializes the active group
    let read1 = client.read_tags(&["Sensor.Temp"]).await.expect("read 1");
    assert_eq!(read1.get_i32("Sensor.Temp"), Ok(100));
    assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
    assert_eq!(state.read_count.load(Ordering::Relaxed), 1);

    // Read 2: Hits active group cache
    let read2 = client.read_tags(&["Sensor.Temp"]).await.expect("read 2");
    assert_eq!(read2.get_i32("Sensor.Temp"), Ok(100));
    assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
    assert_eq!(state.read_count.load(Ordering::Relaxed), 2);

    // Read 3: Hits active group cache again
    let read3 = client.read_tags(&["Sensor.Temp"]).await.expect("read 3");
    assert_eq!(read3.get_i32("Sensor.Temp"), Ok(100));
    assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
    assert_eq!(state.read_count.load(Ordering::Relaxed), 3);
}
