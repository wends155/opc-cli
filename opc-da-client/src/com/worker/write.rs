//! Tag writing engine with validation, native batching, and result mapping.

use crate::com::connector::{ConnectedGroup, ConnectedServer, GroupConfig, GroupItemDef};
use crate::com::guard::GroupGuard;
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::types::{ItemHandle, OpcValue, ServerIdentifier, WriteResult};

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
    writes: &[(String, OpcValue)],
    opc_server: &S,
) -> OpcResult<Vec<WriteResult>> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }

    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %server_id,
        write_count = writes.len(),
        sample_writes = ?writes.iter().take(5).collect::<Vec<_>>(),
        "write_tag_values: starting batch write"
    );
    let start = std::time::Instant::now();

    let group_name = crate::com::worker::generate_group_name("opc-write");
    let created = opc_server
        .add_group(&GroupConfig::ephemeral(&group_name))
        .inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::WriteAddGroup,
                server = %server_id,
                write_count = writes.len()
            );
        })?;
    let group = created.group;
    let _group_guard = GroupGuard::new(opc_server, created.server_handle);

    let item_defs: Vec<GroupItemDef> = writes
        .iter()
        .enumerate()
        .map(|(idx, (tag_id, _))| GroupItemDef {
            item_id: tag_id.clone(),
            #[allow(clippy::cast_possible_truncation)]
            client_handle: ItemHandle::new(idx as u32),
            active: true,
        })
        .collect();

    let results = group.add_items(&item_defs).inspect_err(|e| {
        log_opc_err!(
            e,
            OpcOperation::WriteAddItems,
            server = %server_id,
            write_count = writes.len()
        );
    })?;

    if results.len() != writes.len() {
        let err = OpcError::Internal("Server returned mismatched item results length".to_string());
        log_opc_err!(
            &err,
            OpcOperation::WriteEmptyItemResults,
            server = %server_id,
            expected = writes.len(),
            actual = results.len()
        );
        return Err(err);
    }

    let mut write_results: Vec<WriteResult> = writes
        .iter()
        .map(|(tag_id, _)| {
            WriteResult::failure(
                tag_id,
                OpcError::InvalidState("Item rejected during add_items".into()),
            )
        })
        .collect();

    let mut valid_server_handles = Vec::with_capacity(results.len());
    let mut valid_values = Vec::with_capacity(results.len());
    let mut valid_indices = Vec::with_capacity(results.len());

    for (idx, item_res) in results.iter().enumerate() {
        let tag_id = &writes[idx].0;
        if let Some(ref e) = item_res.error {
            log_opc_err!(
                e,
                OpcOperation::WriteAddItemsRejected,
                server = %server_id,
                tag = %tag_id
            );
            write_results[idx] = WriteResult::failure(tag_id, e.clone());
        } else {
            valid_server_handles.push(item_res.server_handle);
            valid_values.push(writes[idx].1.clone());
            valid_indices.push(idx);
        }
    }

    if !valid_server_handles.is_empty() {
        let server_write_results = group
            .write(&valid_server_handles, &valid_values)
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::WriteSync,
                    server = %server_id,
                    handle_count = valid_server_handles.len()
                );
            })?;

        for (res, &orig_idx) in server_write_results.into_iter().zip(&valid_indices) {
            let tag_id = &writes[orig_idx].0;
            write_results[orig_idx] = match res {
                Ok(()) => WriteResult::success(tag_id),
                Err(e) => {
                    log_opc_err!(
                        &e,
                        OpcOperation::WriteServerRejected,
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
        &[(tag_id.to_string(), value.clone())],
        opc_server,
    )?;
    results
        .pop()
        .ok_or_else(|| OpcError::Internal("No write result returned".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::connector::mock::MockConnectedServer;

    #[test]
    fn test_handle_write_success() {
        let server = MockConnectedServer::default();
        let server_id = ServerIdentifier::from("Test.Server");
        let value = OpcValue::Int(123);
        let result = handle_write(&server_id, "Random.Int4", &value, &server)
            .expect("write operation should succeed");
        assert!(result.is_success());
        assert_eq!(result.tag_id, "Random.Int4");
        assert!(result.error().is_none());
    }

    #[test]
    fn test_worker_native_write_batch_partial_failures_and_ordering() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let mut group = crate::com::connector::mock::MockConnectedGroup {
            state: state.clone(),
            ..Default::default()
        };
        // Simulate item 2 (index 1) rejected during add_items
        group.add_items_fn = Some(Box::new(|items: &[GroupItemDef]| {
            Ok(items
                .iter()
                .enumerate()
                .map(|(idx, _)| {
                    if idx == 1 {
                        crate::com::connector::GroupItemResult {
                            server_handle: ItemHandle::new(0),
                            canonical_type: 0,
                            error: Some(OpcError::InvalidState(
                                "Tag2 rejected in add_items".into(),
                            )),
                        }
                    } else {
                        crate::com::connector::GroupItemResult {
                            #[allow(clippy::cast_possible_truncation)]
                            server_handle: ItemHandle::new(idx as u32 + 1),
                            canonical_type: 8,
                            error: None,
                        }
                    }
                })
                .collect())
        }));
        // Simulate item 3 (index 2) failing during write
        group.write_fn = Some(Box::new(|handles, _| {
            Ok(handles
                .iter()
                .map(|h| {
                    if h.as_raw() == 3 {
                        Err(OpcError::InvalidState("Tag3 rejected in write".into()))
                    } else {
                        Ok(())
                    }
                })
                .collect())
        }));

        let server = crate::com::connector::mock::MockConnectedServer {
            group: std::sync::Arc::new(group),
            state,
            ..Default::default()
        };

        let server_id = ServerIdentifier::from("Test.Server");
        let writes = vec![
            ("Tag1".to_string(), OpcValue::Int(10)),
            ("Tag2".to_string(), OpcValue::Int(20)),
            ("Tag3".to_string(), OpcValue::Int(30)),
        ];

        let results = handle_write_batch(&server_id, &writes, &server)
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
}
