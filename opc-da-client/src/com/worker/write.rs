//! Tag writing engine with validation, native batching, and result mapping.

use super::MAX_TAG_BATCH_SIZE;
use crate::connector::{ConnectedGroup, ConnectedServer, GroupItemResult, ItemWrite};
use crate::errors::{OpcError, OpcResult};
use crate::log_opc_err;
use crate::types::{OpcValue, ServerIdentifier, WriteBatch, WriteResult};

/// Executes synchronous batch writing across multiple tags in a single atomic COM group, returning
/// a list of structured [`WriteResult`]s preserving the original index ordering.
///
/// # Parameters
///
/// - `server_id`: Identifier of the connected OPC DA server for contextual tracing and logging.
/// - `writes`: Batch of tag IDs and corresponding [`OpcValue`] payloads to write.
/// - `opc_server`: Connected OPC server instance implementing [`ConnectedServer`].
///
/// # Errors
///
/// - [`OpcError::InvalidState`]: Returned if the batch size exceeds [`MAX_TAG_BATCH_SIZE`].
/// - [`OpcError::Internal`]: Returned if internal buffer sizes, item registration counts, or
///   result array lengths exhibit invariant mismatches during input partitioning or result assembly.
/// - Transport / COM errors ([`OpcError::Com`]): Returned if COM group creation via
///   [`register_item_group`](super::register_item_group) or COM group write execution via
///   [`ConnectedGroup::write`] fails due to communication or server interface failure.
#[tracing::instrument(
    name = "opc.write_tag_values",
    level = "info",
    skip(writes, opc_server),
    fields(write_count = writes.len()),
    err
)]
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

    // Stage 1 & 2: Partition inputs and isolate interior null byte tags (CWE-626 defense)
    let (valid_tags, valid_orig_indices, mut write_results) =
        partition_write_inputs(&items, server_id);

    // Short-circuit if all tags were quarantined: zero COM group allocations
    if valid_tags.is_empty() {
        let final_results = assemble_write_results(&items, write_results, &[], None, server_id)?;
        tracing::info!(
            count = final_results.len(),
            elapsed_ms = super::elapsed_ms(start),
            "write_tag_values batch completed"
        );
        return Ok(final_results);
    }

    // Stage 3: Register COM item group for valid tags
    let crate::com::worker::RegisteredItemGroup {
        group,
        group_guard: _group_guard,
        item_results,
    } = crate::com::worker::register_item_group(opc_server, server_id, "opc-write", &valid_tags)?;

    // Stage 4: Partition item registration results
    let (valid_writes, valid_write_orig_indices) = partition_item_registration_results(
        item_results,
        &valid_orig_indices,
        &items,
        &mut write_results,
        server_id,
    )?;

    // Stage 5: Execute COM write (if any items registered successfully) and assemble final results
    let server_write_results = if !valid_writes.is_empty() {
        let results = group.write(&valid_writes).inspect_err(|e| {
            log_opc_err!(
                e,
                "write_tag_values:sync",
                server = %server_id,
                handle_count = valid_writes.len()
            );
        })?;
        Some(results)
    } else {
        None
    };

    let final_results = assemble_write_results(
        &items,
        write_results,
        &valid_write_orig_indices,
        server_write_results,
        server_id,
    )?;

    tracing::info!(
        count = final_results.len(),
        elapsed_ms = super::elapsed_ms(start),
        "write_tag_values batch completed"
    );
    Ok(final_results)
}

/// Executes synchronous single-tag writing, delegating to [`handle_write_batch`].
///
/// # Parameters
///
/// - `server_id`: Identifier of the connected OPC DA server for contextual tracing.
/// - `tag_id`: String identifier of the target OPC tag.
/// - `value`: [`OpcValue`] payload to write.
/// - `opc_server`: Connected OPC server instance implementing [`ConnectedServer`].
///
/// # Errors
///
/// - [`OpcError::Internal`]: Returned if internal result assembly fails to yield a result slot.
/// - Transport / COM errors ([`OpcError::Com`], [`OpcError::Connection`]): Returned if COM communication fails.
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
    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %server_id,
        tag = %tag_id,
        value = ?value,
        "write_tag_value: starting single write"
    );
    let batch = WriteBatch::from_str_lenient(tag_id, value.clone());
    let mut results = handle_write_batch(server_id, &batch, opc_server)?;
    results
        .pop()
        .ok_or_else(|| OpcError::Internal("No write result returned".into()))
}

/// Partitions incoming write requests, isolating tags with illegal interior null bytes (CWE-626).
///
/// Returns a tuple of:
/// 1. `valid_tags`: Borrowed tag ID slices suitable for Win32 COM registration.
/// 2. `valid_orig_indices`: Indices into the caller's original items array corresponding to valid tags.
/// 3. `write_results`: Pre-sized slot vector (`Vec<Option<WriteResult>>`) where quarantined null-byte tags
///    are pre-populated with [`WriteResult::failure`], and valid slots remain `None`.
///
/// # Errors
///
/// This function returns pure data collections and does not return [`OpcError`].
///
/// # Panics
///
/// This function does not panic.
#[must_use]
pub(crate) fn partition_write_inputs<'a>(
    items: &[(&'a str, &OpcValue)],
    server_id: &ServerIdentifier,
) -> (Vec<&'a str>, Vec<usize>, Vec<Option<WriteResult>>) {
    let mut valid_tags = Vec::with_capacity(items.len());
    let mut valid_orig_indices = Vec::with_capacity(items.len());
    let mut write_results: Vec<Option<WriteResult>> = vec![None; items.len()];

    for (orig_idx, &(tag_id, _val)) in items.iter().enumerate() {
        if tag_id.contains('\0') {
            tracing::warn!(
                server = %server_id,
                tag = %tag_id.escape_debug(),
                "write_tag_values: tag contains illegal interior null byte; quarantined"
            );
            if let Some(slot) = write_results.get_mut(orig_idx) {
                *slot = Some(WriteResult::failure(
                    tag_id,
                    OpcError::InvalidState(
                        "Tag identifier contains illegal interior null byte".into(),
                    ),
                ));
            } else {
                tracing::error!(
                    orig_idx,
                    len = write_results.len(),
                    "write_results slot out of bounds during input partitioning"
                );
            }
        } else {
            valid_tags.push(tag_id);
            valid_orig_indices.push(orig_idx);
        }
    }

    (valid_tags, valid_orig_indices, write_results)
}

/// Partitions COM item registration results into valid write requests and records registration failures.
///
/// # Errors
///
/// - [`OpcError::Internal`]: Returned if the lengths of `write_results` and `items` mismatch,
///   if `results.len()` does not equal `valid_orig_indices.len()`, or if an `orig_idx` is out of bounds for `items`.
///
/// # Panics
///
/// This function does not panic.
pub(crate) fn partition_item_registration_results(
    results: Vec<GroupItemResult>,
    valid_orig_indices: &[usize],
    items: &[(&str, &OpcValue)],
    write_results: &mut [Option<WriteResult>],
    server_id: &ServerIdentifier,
) -> OpcResult<(Vec<ItemWrite>, Vec<usize>)> {
    if write_results.len() != items.len() {
        return Err(OpcError::Internal(format!(
            "Buffer size mismatch in partition_item_registration_results: write_results={}, items={}",
            write_results.len(),
            items.len()
        )));
    }

    if results.len() != valid_orig_indices.len() {
        return Err(OpcError::Internal(format!(
            "Server {server_id} returned mismatched item registration count: expected {}, got {}",
            valid_orig_indices.len(),
            results.len()
        )));
    }

    let mut valid_writes = Vec::with_capacity(results.len());
    let mut valid_write_orig_indices = Vec::with_capacity(results.len());

    for (item_res, &orig_idx) in results.into_iter().zip(valid_orig_indices) {
        let &(tag_id, val) = items.get(orig_idx).ok_or_else(|| {
            OpcError::Internal(format!(
                "Original item index {orig_idx} out of bounds for items array (len {})",
                items.len()
            ))
        })?;

        if let Some(e) = item_res.error {
            let slot = write_results.get_mut(orig_idx).ok_or_else(|| {
                OpcError::Internal(format!(
                    "Invalid write results buffer index {orig_idx} during registration error assignment"
                ))
            })?;
            *slot = Some(WriteResult::failure(tag_id, e));
        } else {
            valid_writes.push(ItemWrite {
                handle: item_res.server_handle,
                value: (*val).clone(),
            });
            valid_write_orig_indices.push(orig_idx);
        }
    }

    Ok((valid_writes, valid_write_orig_indices))
}

/// Assembles final [`WriteResult`] outcomes from uncommitted slot buffers and optional server write responses.
///
/// # Errors
///
/// - [`OpcError::Internal`]: Returned if the lengths of `write_results` and `items` mismatch,
///   if the server write results count does not equal `valid_write_orig_indices.len()`,
///   or if an `orig_idx` is out of bounds.
///
/// # Panics
///
/// This function does not panic.
pub(crate) fn assemble_write_results(
    items: &[(&str, &OpcValue)],
    mut write_results: Vec<Option<WriteResult>>,
    valid_write_orig_indices: &[usize],
    server_write_results: Option<Vec<OpcResult<()>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<WriteResult>> {
    if write_results.len() != items.len() {
        return Err(OpcError::Internal(format!(
            "Buffer size mismatch in assemble_write_results: write_results={}, items={}",
            write_results.len(),
            items.len()
        )));
    }

    if let Some(server_res) = server_write_results {
        if server_res.len() != valid_write_orig_indices.len() {
            return Err(OpcError::Internal(format!(
                "Server {server_id} returned mismatched write result array size: expected {}, got {}",
                valid_write_orig_indices.len(),
                server_res.len()
            )));
        }

        for (res, &orig_idx) in server_res.into_iter().zip(valid_write_orig_indices) {
            let &(tag_id, _) = items.get(orig_idx).ok_or_else(|| {
                OpcError::Internal(format!(
                    "Original write index {orig_idx} out of bounds for items array (len {})",
                    items.len()
                ))
            })?;

            let slot = write_results.get_mut(orig_idx).ok_or_else(|| {
                OpcError::Internal(format!(
                    "Invalid write results buffer index {orig_idx} during write result assembly"
                ))
            })?;
            *slot = Some(match res {
                Ok(()) => WriteResult::success(tag_id),
                Err(e) => WriteResult::failure(tag_id, e),
            });
        }
    }

    let final_results: Vec<WriteResult> = write_results
        .into_iter()
        .enumerate()
        .map(|(orig_idx, opt)| {
            opt.unwrap_or_else(|| {
                let tag_id = items
                    .get(orig_idx)
                    .map_or("<unknown>", |(tag, _)| *tag);
                WriteResult::failure(
                    tag_id,
                    OpcError::Internal(format!(
                        "Write result slot for tag '{tag_id}' was not populated by server {server_id}"
                    )),
                )
            })
        })
        .collect();

    Ok(final_results)
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

    #[test]
    fn test_partition_write_inputs_clean_tags() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let items = [("Tag1", &val1), ("Tag2", &val2)];

        let (valid_tags, valid_orig_indices, write_results) =
            partition_write_inputs(&items, &server_id);

        assert_eq!(valid_tags, vec!["Tag1", "Tag2"]);
        assert_eq!(valid_orig_indices, vec![0, 1]);
        assert_eq!(write_results.len(), 2);
        assert!(write_results[0].is_none());
        assert!(write_results[1].is_none());
    }

    #[test]
    fn test_partition_write_inputs_contaminated_tags() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let val3 = OpcValue::Int(30);
        let items = [
            ("Clean.Tag1", &val1),
            ("Dirty\0.Tag2", &val2),
            ("Clean.Tag3", &val3),
        ];

        let (valid_tags, valid_orig_indices, write_results) =
            partition_write_inputs(&items, &server_id);

        assert_eq!(valid_tags, vec!["Clean.Tag1", "Clean.Tag3"]);
        assert_eq!(valid_orig_indices, vec![0, 2]);
        assert_eq!(write_results.len(), 3);
        assert!(write_results[0].is_none());
        assert!(write_results[2].is_none());

        let quarantined = write_results[1]
            .as_ref()
            .expect("quarantined result must be present");
        assert_eq!(quarantined.tag_id, "Dirty\0.Tag2");
        assert!(quarantined.is_error());
        assert!(
            quarantined
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
    }

    #[test]
    fn test_partition_write_inputs_all_null_tags() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let items = [("Bad\0One", &val1), ("Bad\0Two", &val2)];

        let (valid_tags, valid_orig_indices, write_results) =
            partition_write_inputs(&items, &server_id);

        assert!(valid_tags.is_empty());
        assert!(valid_orig_indices.is_empty());
        assert_eq!(write_results.len(), 2);
        assert_eq!(write_results[0].as_ref().unwrap().tag_id, "Bad\0One");
        assert!(write_results[0].as_ref().unwrap().is_error());
        assert!(
            write_results[0]
                .as_ref()
                .unwrap()
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(write_results[1].as_ref().unwrap().tag_id, "Bad\0Two");
        assert!(write_results[1].as_ref().unwrap().is_error());
        assert!(
            write_results[1]
                .as_ref()
                .unwrap()
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
    }

    #[test]
    fn test_partition_item_registration_results_partial() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val0 = OpcValue::Int(100);
        let val1 = OpcValue::Int(200);
        let items = [("Tag.A", &val0), ("Tag.B", &val1)];
        let valid_orig_indices = vec![0, 1];
        let mut write_results = vec![None, None];

        let registration_results = vec![
            GroupItemResult {
                server_handle: ServerItemHandle::new(42),
                canonical_type: VarType::I4,
                error: None,
            },
            GroupItemResult {
                server_handle: ServerItemHandle::new(0),
                canonical_type: VarType::EMPTY,
                error: Some(OpcError::InvalidState("Tag.B not found".into())),
            },
        ];

        let (valid_writes, valid_write_orig_indices) = partition_item_registration_results(
            registration_results,
            &valid_orig_indices,
            &items,
            &mut write_results,
            &server_id,
        )
        .expect("partitioning must succeed");

        assert_eq!(valid_writes.len(), 1);
        assert_eq!(valid_writes[0].handle.as_raw(), 42);
        assert_eq!(valid_writes[0].value, OpcValue::Int(100));
        assert_eq!(valid_write_orig_indices, vec![0]);

        assert!(write_results[0].is_none());
        let failed = write_results[1]
            .as_ref()
            .expect("Tag.B must have failed result");
        assert_eq!(failed.tag_id, "Tag.B");
        assert!(failed.is_error());
        assert!(
            failed
                .error()
                .unwrap()
                .to_string()
                .contains("Tag.B not found")
        );
    }

    #[test]
    fn test_partition_item_registration_results_count_mismatch() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val = OpcValue::Int(10);
        let items = [("Tag1", &val), ("Tag2", &val)];
        let valid_orig_indices = vec![0, 1];
        let mut write_results = vec![None, None];
        let registration_results = vec![GroupItemResult {
            server_handle: ServerItemHandle::new(1),
            canonical_type: VarType::I4,
            error: None,
        }]; // 1 result for 2 indices — mismatch!

        let err = partition_item_registration_results(
            registration_results,
            &valid_orig_indices,
            &items,
            &mut write_results,
            &server_id,
        )
        .expect_err("mismatched registration count must return Internal error");

        assert!(matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched")));
    }

    #[test]
    fn test_assemble_write_results_outcomes() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let val3 = OpcValue::Int(30);
        let items = [("Tag1", &val1), ("Tag2", &val2), ("Tag3", &val3)];
        let write_results = vec![
            None,
            Some(WriteResult::failure(
                "Tag2",
                OpcError::InvalidState("Failed in add_items".into()),
            )),
            None,
        ];
        let valid_write_orig_indices = vec![0, 2];
        let server_write_results = Some(vec![
            Ok(()),
            Err(OpcError::Com {
                source: windows_core::Error::from_hresult(crate::errors::hresult::E_FAIL),
            }),
        ]);

        let results = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            server_write_results,
            &server_id,
        )
        .expect("assembly must succeed");

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tag_id, "Tag1");
        assert!(results[0].is_success());

        assert_eq!(results[1].tag_id, "Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Failed in add_items")
        );

        assert_eq!(results[2].tag_id, "Tag3");
        assert!(results[2].is_error());
        assert!(matches!(results[2].error(), Some(OpcError::Com { .. })));
    }

    #[test]
    fn test_assemble_write_results_parity_mismatch() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val = OpcValue::Int(10);
        let items = [("Tag1", &val)];
        let write_results = vec![None];
        let valid_write_orig_indices = vec![0];
        let server_write_results = Some(vec![Ok(()), Ok(())]); // 2 results for 1 index!

        let err = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            server_write_results,
            &server_id,
        )
        .expect_err("parity mismatch must error");

        assert!(
            matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched write result array size"))
        );
    }

    #[test]
    fn test_assemble_write_results_unassigned_slots_fallback() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val = OpcValue::Int(10);
        let items = [("Tag1", &val)];
        let write_results = vec![None]; // Slot left None with no server write results
        let valid_write_orig_indices = vec![];

        let results = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            None,
            &server_id,
        )
        .expect("assembly should complete with fallback failure");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tag_id, "Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("was not populated")
        );
    }

    #[test]
    fn test_handle_write_batch_granular_null_byte_isolation() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = vec![
            ("Clean.Tag1".to_string(), OpcValue::Int(1)),
            ("Dirty\0.Tag2".to_string(), OpcValue::Int(2)),
            ("Clean.Tag3".to_string(), OpcValue::Int(3)),
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("batch write should succeed with granular isolation");

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tag_id, "Clean.Tag1");
        assert!(results[0].is_success());

        assert_eq!(results[1].tag_id, "Dirty\0.Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );

        assert_eq!(results[2].tag_id, "Clean.Tag3");
        assert!(results[2].is_success());

        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1,
            "Exactly 1 COM group should be registered for valid tags"
        );
    }

    #[test]
    fn test_handle_write_batch_all_null_short_circuits() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = vec![
            ("Bad\0Tag1".to_string(), OpcValue::Int(1)),
            ("Bad\0Tag2".to_string(), OpcValue::Int(2)),
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("all-null batch should short-circuit and return results");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "Bad\0Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(results[1].tag_id, "Bad\0Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0,
            "No COM groups should be allocated when all tags contain null bytes"
        );
    }

    #[test]
    fn test_handle_write_batch_all_rejected_registration_skips_write() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let write_invoked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let write_invoked_clone = write_invoked.clone();

        let group = crate::connector::mock::MockConnectedGroup {
            state: state.clone(),
            ..Default::default()
        }
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .map(|_| crate::connector::GroupItemResult {
                    server_handle: ServerItemHandle::new(0),
                    canonical_type: VarType::EMPTY,
                    error: Some(OpcError::InvalidState("Rejected in registration".into())),
                })
                .collect())
        })
        .with_write_fn(move |items| {
            write_invoked_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(items.iter().map(|_| Ok(())).collect())
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
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("all-rejected registration must return results without error");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("Rejected in registration")
        );
        assert_eq!(results[1].tag_id, "Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Rejected in registration")
        );
        assert!(
            !write_invoked.load(std::sync::atomic::Ordering::Relaxed),
            "group.write() must not be called when all items are rejected during registration"
        );
    }

    #[test]
    fn test_handle_write_scalar_null_byte_returns_failure_result() {
        let server = crate::connector::mock::MockConnectedServer::default();
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let value = OpcValue::Int(42);

        let result = handle_write(&server_id, "Bad\0Tag", &value, &server)
            .expect("scalar write must return Ok(WriteResult) even on item validation error");

        assert!(result.is_error());
        assert_eq!(result.tag_id, "Bad\0Tag");
        assert!(
            result
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
    }
}
