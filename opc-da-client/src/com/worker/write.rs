//! Tag writing engine with validation, native batching, and result mapping.

use super::MAX_TAG_BATCH_SIZE;
use crate::connector::{ConnectedGroup, ConnectedServer, ItemWrite};
use crate::errors::{OpcError, OpcResult};
use crate::log_opc_err;
use crate::types::{OpcValue, ServerIdentifier, WriteBatch, WriteResult};

/// Executes synchronous batch writing across multiple tags in a single atomic COM group, returning
/// a list of structured [`WriteResult`]s preserving the original index ordering.
#[tracing::instrument(
    name = "opc.write_tag_values",
    level = "info",
    skip(writes, opc_server),
    fields(write_count = writes.len()),
    err
)]
#[allow(clippy::too_many_lines)]
pub fn handle_write_batch<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    writes: &WriteBatch,
    opc_server: &S,
) -> OpcResult<Vec<WriteResult>> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }

    if writes.len() > MAX_TAG_BATCH_SIZE {
        return Err(OpcError::InvalidState(format!(
            "Write batch size {} exceeds maximum allowed limit of {MAX_TAG_BATCH_SIZE}",
            writes.len(),
        )));
    }

    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %server_id,
        write_count = writes.len(),
        sample_writes = ?writes.iter().take(5).collect::<Vec<_>>(),
        "write_tag_values: starting batch write"
    );
    let start = std::time::Instant::now();

    let items: Vec<(&str, &OpcValue)> = writes.iter().collect();
    let tag_names: Vec<&str> = items.iter().map(|(t, _)| *t).collect();
    let reg =
        crate::com::worker::register_item_group(opc_server, server_id, "opc-write", &tag_names)?;
    let group = reg.group;
    let _group_guard = reg.group_guard;
    let results = reg.item_results;

    let mut write_results: Vec<WriteResult> = items
        .iter()
        .map(|(tag_id, _)| {
            WriteResult::failure(
                *tag_id,
                OpcError::InvalidState("Item rejected during add_items".into()),
            )
        })
        .collect();

    let mut valid_writes = Vec::with_capacity(results.len());
    let mut valid_indices = Vec::with_capacity(results.len());

    for (idx, item_res) in results.iter().enumerate() {
        let (tag_id, val) = items[idx];
        if let Some(ref e) = item_res.error {
            log_opc_err!(
                e,
                "write_tag_values:items_rejected",
                server = %server_id,
                tag = %tag_id
            );
            write_results[idx] = WriteResult::failure(tag_id, e.clone());
        } else {
            valid_writes.push(ItemWrite::new(item_res.server_handle, val.clone()));
            valid_indices.push(idx);
        }
    }

    if !valid_writes.is_empty() {
        let server_write_results = group.write(&valid_writes).inspect_err(|e| {
            log_opc_err!(
                e,
                "write_tag_values:sync",
                server = %server_id,
                handle_count = valid_writes.len()
            );
        })?;

        if server_write_results.len() != valid_indices.len() {
            let err = OpcError::Internal(format!(
                "server returned mismatched write result array size: expected {}, got {}",
                valid_indices.len(),
                server_write_results.len()
            ));
            log_opc_err!(
                &err,
                "write_tag_values:mismatched",
                server = %server_id,
                expected = valid_indices.len(),
                actual = server_write_results.len()
            );
            return Err(err);
        }

        for (res, &orig_idx) in server_write_results.into_iter().zip(&valid_indices) {
            let (tag_id, _) = items[orig_idx];
            write_results[orig_idx] = match res {
                Ok(()) => WriteResult::success(tag_id),
                Err(e) => {
                    log_opc_err!(
                        &e,
                        "write_tag_values:server_rejected",
                        server = %server_id,
                        tag = %tag_id
                    );
                    WriteResult::failure(tag_id, e)
                }
            };
        }
    }

    tracing::info!(
        count = write_results.len(),
        elapsed_ms = super::elapsed_ms(start),
        "write_tag_values batch completed"
    );
    Ok(write_results)
}

/// Executes synchronous single-tag writing, delegating to [`handle_write_batch`].
#[tracing::instrument(
    name = "opc.write_tag_value",
    level = "info",
    skip(value, opc_server),
    fields(tag = %tag_id),
    err
)]
pub fn handle_write<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    tag_id: &str,
    value: &OpcValue,
    opc_server: &S,
) -> OpcResult<WriteResult> {
    let mut results = handle_write_batch(
        server_id,
        &WriteBatch::Single(tag_id.to_string(), value.clone()),
        opc_server,
    )?;
    results
        .pop()
        .ok_or_else(|| OpcError::Internal("No write result returned".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::GroupItemDef;
    use crate::connector::mock::MockConnectedServer;
    use crate::types::{IntoWriteBatch, ServerItemHandle, VarType};

    #[test]
    fn test_handle_write_success() {
        let server = MockConnectedServer::default();
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let value = OpcValue::Int(123);
        let result = handle_write(&server_id, "Random.Int4", &value, &server)
            .expect("write operation should succeed");
        assert!(result.is_success());
        assert_eq!(result.tag_id, "Random.Int4");
        assert!(result.error().is_none());
    }

    #[test]
    fn test_worker_native_write_batch_partial_failures_and_ordering() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let group = crate::connector::mock::MockConnectedGroup {
            state: state.clone(),
            ..Default::default()
        }
        .with_add_items_fn(|items: &[GroupItemDef]| {
            Ok(items
                .iter()
                .enumerate()
                .map(|(idx, _)| {
                    if idx == 1 {
                        crate::connector::GroupItemResult {
                            server_handle: ServerItemHandle::new(0),
                            canonical_type: VarType::EMPTY,
                            error: Some(OpcError::InvalidState(
                                "Tag2 rejected in add_items".into(),
                            )),
                        }
                    } else {
                        crate::connector::GroupItemResult {
                            #[allow(clippy::cast_possible_truncation)]
                            server_handle: ServerItemHandle::new((idx + 1) as u32),
                            canonical_type: VarType::EMPTY,
                            error: None,
                        }
                    }
                })
                .collect())
        })
        .with_write_fn(|items| {
            Ok(items
                .iter()
                .map(|item| {
                    if item.handle.as_raw() == 3 {
                        Err(OpcError::InvalidState("Tag3 rejected in write".into()))
                    } else {
                        Ok(())
                    }
                })
                .collect())
        });

        let server = crate::connector::mock::MockConnectedServer {
            group: std::sync::Arc::new(group),
            state,
            ..Default::default()
        };

        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = vec![
            ("Tag1".to_string(), OpcValue::Int(10)),
            ("Tag2".to_string(), OpcValue::Int(20)),
            ("Tag3".to_string(), OpcValue::Int(30)),
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("batch write must return results");
        assert_eq!(results.len(), 3);

        // Tag 1: success
        assert_eq!(results[0].tag_id, "Tag1");
        assert!(results[0].is_success());

        // Tag 2: failed in add_items
        assert_eq!(results[1].tag_id, "Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Tag2 rejected in add_items")
        );

        // Tag 3: failed in write
        assert_eq!(results[2].tag_id, "Tag3");
        assert!(results[2].is_error());
        assert!(
            results[2]
                .error()
                .unwrap()
                .to_string()
                .contains("Tag3 rejected in write")
        );
    }

    #[test]
    fn test_handle_write_batch_exceeds_max_limit() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();

        let writes_vec: Vec<(String, OpcValue)> = (0..=MAX_TAG_BATCH_SIZE)
            .map(|i| {
                #[allow(clippy::cast_possible_wrap)]
                (format!("Tag.{i}"), OpcValue::Int(i as i64))
            })
            .collect();
        let writes = writes_vec.into_write_batch();

        let err = handle_write_batch(&server_id, &writes, &server)
            .expect_err("batch exceeding limit must be rejected");
        assert!(
            matches!(err, OpcError::InvalidState(ref msg) if msg.contains("exceeds maximum allowed limit")),
            "Expected InvalidState error, got: {err:?}"
        );
        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0,
            "No COM group allocation should occur"
        );
    }

    #[test]
    fn test_handle_write_batch_empty() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = WriteBatch::empty();

        let results = handle_write_batch(&server_id, &writes, &server)
            .expect("empty write batch must succeed");
        assert!(results.is_empty());
        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0,
            "No COM groups must be allocated for empty batch"
        );
    }

    #[test]
    fn test_handle_write_batch_at_max_limit() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();

        let writes_vec: Vec<(String, OpcValue)> = (0..MAX_TAG_BATCH_SIZE)
            .map(|i| {
                #[allow(clippy::cast_possible_wrap)]
                (format!("Tag.{i}"), OpcValue::Int(i as i64))
            })
            .collect();
        let writes = writes_vec.into_write_batch();

        let results = handle_write_batch(&server_id, &writes, &server)
            .expect("batch at maximum limit must proceed");
        assert_eq!(results.len(), MAX_TAG_BATCH_SIZE);
        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1,
            "Exactly 1 COM group should be registered"
        );
    }
}
