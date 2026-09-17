//! Tag reading engine with in-place value population and active group caching.

use super::MAX_TAG_BATCH_SIZE;
use super::pool::{CachedGroup, PooledServer};
use crate::connector::{
    ConnectedGroup, ConnectedServer, DataSource, GroupItemResult, GroupItemState,
};
use crate::errors::{OpcError, OpcResult};
use crate::log_opc_err;
use crate::types::{
    OpcQuality, OpcServerEndpoint, ServerIdentifier, ServerItemHandle, TagBatch, TagValue,
    TagValues,
};

/// Executes synchronous device tag reading through the pooled server's active OPC group,
/// reusing cached group handles when tag batches match and populating values, qualities,
/// timestamps, and granular errors into a [`TagValues`] collection.
#[tracing::instrument(
    name = "opc.read_tag_values",
    level = "info",
    skip(tags, pooled),
    fields(tag_count = tags.len()),
    err
)]
#[allow(clippy::too_many_lines)]
pub fn handle_read<S: ConnectedServer>(
    endpoint: &OpcServerEndpoint,
    tags: &TagBatch,
    pooled: &mut PooledServer<S>,
) -> OpcResult<TagValues> {
    if tags.is_empty() {
        return Ok(TagValues::new(Vec::new()));
    }

    if tags.len() > MAX_TAG_BATCH_SIZE {
        return Err(OpcError::InvalidState(format!(
            "Tag batch size {} exceeds maximum allowed limit of {MAX_TAG_BATCH_SIZE}",
            tags.len()
        )));
    }

    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %endpoint,
        tag_count = tags.len(),
        sample_tags = ?tags.iter_str().take(5).collect::<Vec<_>>(),
        "read_tag_values: starting operation"
    );
    let start = std::time::Instant::now();

    // Check active group cache hit
    if let Some(idx) = pooled.find_active_group_idx(tags) {
        pooled.promote_group(idx);

        let read_res = if let Some(cached) = pooled.active_groups.front()
            && !cached.server_item_handles.is_empty()
        {
            Some(
                cached
                    .group
                    .read(DataSource::Device, &cached.server_item_handles),
            )
        } else {
            None
        };

        let states_opt = match read_res {
            Some(Ok(states)) => Some(Some(states)),
            None => Some(None),
            Some(Err(e)) if e.is_connection_error() => return Err(e),
            Some(Err(e)) => {
                log_opc_err!(
                    &e,
                    "read_tag_values:sync",
                    server = %endpoint.identifier,
                    "Cached active group read failed with non-connection error; invalidating group and retrying via fresh registration"
                );
                pooled.remove_active_group(0);
                None // Evicts group and falls through to cache-miss registration below
            }
        };

        if let Some(states) = states_opt {
            let (tag_values_res, should_clear) = if let Some(cached) = pooled.active_groups.front()
            {
                match assemble_tag_values(
                    tags.iter_str(),
                    &cached.valid_indices,
                    &cached.rejected_errors,
                    states,
                    &endpoint.identifier,
                ) {
                    Ok(values) => (Ok(values), false),
                    Err(e) => (Err(e), true),
                }
            } else {
                (
                    Err(OpcError::Internal(
                        "Active group unexpectedly missing".into(),
                    )),
                    false,
                )
            };

            if should_clear {
                if let Err(ref e) = tag_values_res {
                    log_opc_err!(
                        e,
                        "read_tag_values:sync",
                        server = %endpoint.identifier,
                        "Cached active group item state size mismatch; invalidating group"
                    );
                }
                pooled.remove_active_group(0);
            }

            let tag_values = tag_values_res?;

            tracing::info!(
                count = tag_values.len(),
                elapsed_ms = super::elapsed_ms(start),
                "read_tag_values (cache hit) completed"
            );
            return Ok(TagValues::new(tag_values));
        }
    }

    // Cache miss: multi-group LRU cache retains prior active groups up to MAX_ACTIVE_GROUPS
    let tag_ids: Vec<String> = tags.iter_str().map(ToString::to_string).collect();
    let super::RegisteredItemGroup {
        group,
        mut group_guard,
        item_results: results,
    } = super::register_item_group(&pooled.server, &endpoint.identifier, "opc-read", &tag_ids)?;

    let (server_handles, valid_indices, rejected_errors) =
        partition_item_results(&results, &tag_ids, &endpoint.identifier);

    let item_states = if !server_handles.is_empty() {
        let states = group
            .read(DataSource::Device, &server_handles)
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    "read_tag_values:sync",
                    server = %endpoint.identifier,
                    handle_count = server_handles.len()
                );
            })?;
        Some(states)
    } else {
        None
    };

    let tag_values = assemble_tag_values(
        tags.iter_str(),
        &valid_indices,
        &rejected_errors,
        item_states,
        &endpoint.identifier,
    )?;

    let server_handle = group_guard.disarm();
    drop(group_guard);

    pooled.insert_active_group(CachedGroup {
        tags: tag_ids,
        group,
        server_handle,
        server_item_handles: server_handles,
        valid_indices,
        rejected_errors,
    });

    tracing::info!(
        count = tag_values.len(),
        elapsed_ms = super::elapsed_ms(start),
        "read_tag_values (cache miss) completed"
    );
    Ok(TagValues::new(tag_values))
}

/// Separates valid item handles from rejected tags, recording configuration errors for rejected tags.
fn partition_item_results(
    results: &[GroupItemResult],
    tag_ids: &[String],
    server_id: &ServerIdentifier,
) -> (Vec<ServerItemHandle>, Vec<usize>, Vec<(usize, OpcError)>) {
    let mut server_handles = Vec::with_capacity(results.len());
    let mut valid_indices = Vec::with_capacity(results.len());
    let mut rejected_errors = Vec::new();

    for (idx, item_result) in results.iter().enumerate() {
        if let Some(ref err) = item_result.error {
            let err_msg = err.to_string();
            tracing::warn!(
                server = %server_id,
                tag = %tag_ids[idx],
                error = %err_msg,
                "read_tag_values: add_items rejected tag"
            );
            rejected_errors.push((idx, err.clone()));
        } else {
            server_handles.push(item_result.server_handle);
            valid_indices.push(idx);
        }
    }

    (server_handles, valid_indices, rejected_errors)
}

/// Assembles final [`TagValue`] instances directly from item states and rejected errors in a single pass.
///
/// Pre-allocates exact capacity and maps each tag by index without generating throwaway placeholder strings.
pub(crate) fn assemble_tag_values<'a>(
    tags: impl ExactSizeIterator<Item = &'a str>,
    valid_indices: &[usize],
    rejected_errors: &[(usize, OpcError)],
    item_states: Option<Vec<OpcResult<GroupItemState>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<TagValue>> {
    let tag_count = tags.len();
    let mut tag_values = Vec::with_capacity(tag_count);
    let states = item_states.unwrap_or_default();

    if states.len() != valid_indices.len() {
        let err = OpcError::Internal(format!(
            "Server {server_id} returned mismatched read result array size: expected {} items, got {}",
            valid_indices.len(),
            states.len()
        ));
        log_opc_err!(
            &err,
            "read_tag_values:mismatched",
            server = %server_id,
            expected = valid_indices.len(),
            actual = states.len()
        );
        return Err(err);
    }

    let mut state_iter = states.into_iter();
    let mut reject_iter = rejected_errors.iter().peekable();

    for (idx, tag_str) in tags.enumerate() {
        if let Some((rej_idx, err)) = reject_iter.peek()
            && *rej_idx == idx
        {
            let err = err.clone();
            reject_iter.next();
            tag_values.push(TagValue {
                tag_id: tag_str.to_string(),
                outcome: Err(err),
                quality: OpcQuality::BAD_CONFIG_ERROR,
                timestamp: None,
            });
        } else if let Some(state_res) = state_iter.next() {
            let (outcome, quality, timestamp) = match state_res {
                Ok(state) => (Ok(state.value), state.quality, Some(state.timestamp)),
                Err(e) => {
                    log_opc_err!(
                        &e,
                        "read_tag_values:per_item",
                        server = %server_id,
                        tag = %tag_str
                    );
                    (Err(e), OpcQuality::BAD_COMM_FAILURE, None)
                }
            };
            tag_values.push(TagValue {
                tag_id: tag_str.to_string(),
                outcome,
                quality,
                timestamp,
            });
        } else {
            return Err(OpcError::Internal(format!(
                "Unexpected state exhaustion at index {idx} on server {server_id}"
            )));
        }
    }

    Ok(tag_values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::worker::pool::PooledServer;
    use crate::connector::mock::MockConnectedServer;
    use crate::types::{OpcServerEndpoint, TagBatch};

    #[test]
    fn test_handle_read_empty_tags_short_circuits() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");
        let tags = TagBatch::from_static(&[]);
        let results = handle_read(&endpoint, &tags, &mut pooled).expect("empty tags must succeed");
        assert!(results.is_empty());
    }

    #[test]
    fn test_handle_read_with_mock_server() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");
        let tags = TagBatch::from_static(&["Random.Int4", "Random.Real8"]);
        let results =
            handle_read(&endpoint, &tags, &mut pooled).expect("reading tags must succeed");
        assert_eq!(results.len(), 2);
        assert_eq!(results.get("Random.Int4").unwrap().tag_id, "Random.Int4");
        assert_eq!(results.get("Random.Real8").unwrap().tag_id, "Random.Real8");
        assert!(results.get("Random.Int4").unwrap().value().is_some());
    }

    #[test]
    fn test_group_guard_disarm_on_read() {
        use std::sync::atomic::Ordering;

        let server = MockConnectedServer::default();
        let state = server.state.clone();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");
        let tags = TagBatch::from_static(&["Random.Int4"]);

        // First read (cache miss): group added, guard disarmed, cached in pooled
        let res = handle_read(&endpoint, &tags, &mut pooled);
        assert!(res.is_ok());
        assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
        assert_eq!(state.remove_group_count.load(Ordering::Relaxed), 0);
        assert!(pooled.has_active_groups());

        // Second read with same tags (cache hit): no new group created or removed
        let res2 = handle_read(&endpoint, &tags, &mut pooled);
        assert!(res2.is_ok());
        assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
        assert_eq!(state.remove_group_count.load(Ordering::Relaxed), 0);

        // Explicit clear active group triggers remove_group
        pooled.clear_active_group();
        assert_eq!(state.remove_group_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_handle_read_rejects_exceeding_max_batch_size() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");

        let tags_vec: Vec<String> = (0..=MAX_TAG_BATCH_SIZE)
            .map(|i| format!("Tag.{i}"))
            .collect();
        let tags = TagBatch::from(tags_vec);

        let err = handle_read(&endpoint, &tags, &mut pooled)
            .expect_err("batch exceeding limit must fail");
        assert!(
            matches!(err, OpcError::InvalidState(ref msg) if msg.contains("exceeds maximum allowed limit"))
        );
    }

    #[test]
    fn test_handle_read_cache_hit_zero_sentinel_errors() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");
        let tags = TagBatch::from_static(&["Random.Int4"]);

        // First read (miss)
        let _ = handle_read(&endpoint, &tags, &mut pooled).unwrap();

        // Second read (hit)
        let res = handle_read(&endpoint, &tags, &mut pooled).unwrap();
        let tag_val = res.get("Random.Int4").unwrap();
        assert!(tag_val.value().is_some());
        assert!(tag_val.error().is_none());
    }

    #[test]
    fn test_handle_read_cache_hit_preserves_caller_casing() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");

        // Miss: First read caches uppercase tag "TAG_A"
        let tags_upper = TagBatch::from_static(&["TAG_A"]);
        let res1 = handle_read(&endpoint, &tags_upper, &mut pooled).unwrap();
        assert_eq!(res1.get("TAG_A").unwrap().tag_id, "TAG_A");

        // Hit: Second read requests lowercase tag "tag_a" against cached "TAG_A"
        let tags_lower = TagBatch::from_static(&["tag_a"]);
        let res2 = handle_read(&endpoint, &tags_lower, &mut pooled).unwrap();
        let tag_val = res2
            .get("tag_a")
            .expect("TagValues must index using caller casing 'tag_a'");
        assert_eq!(
            tag_val.tag_id, "tag_a",
            "TagValue must preserve caller casing 'tag_a' rather than cached 'TAG_A'"
        );
    }

    #[test]
    fn test_assemble_tag_values_preserves_caller_casing() {
        use crate::connector::GroupItemState;
        use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let requested_tags = ["tag1", "mixedCase_Tag2"];
        let valid_indices = vec![0, 1];
        let rejected_errors = vec![];
        let states = Some(vec![
            Ok(GroupItemState {
                client_handle: ClientItemHandle::new(1),
                value: OpcValue::Int(42),
                quality: OpcQuality::GOOD,
                timestamp: std::time::SystemTime::UNIX_EPOCH,
            }),
            Ok(GroupItemState {
                client_handle: ClientItemHandle::new(2),
                value: OpcValue::String("val2".into()),
                quality: OpcQuality::GOOD,
                timestamp: std::time::SystemTime::UNIX_EPOCH,
            }),
        ]);

        let results = assemble_tag_values(
            requested_tags.into_iter(),
            &valid_indices,
            &rejected_errors,
            states,
            &server_id,
        )
        .expect("assemble_tag_values must succeed");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "tag1");
        assert_eq!(results[1].tag_id, "mixedCase_Tag2");
    }

    #[test]
    fn test_assemble_tag_values_array_length_parity_mismatch() {
        use crate::connector::GroupItemState;
        use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let requested_tags = ["tag1", "tag2"];
        let valid_indices = vec![0, 1]; // expects 2 states
        let rejected_errors = vec![];
        let states = Some(vec![Ok(GroupItemState {
            // server returns only 1 state
            client_handle: ClientItemHandle::new(1),
            value: OpcValue::Int(42),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        })]);

        let err = assemble_tag_values(
            requested_tags.into_iter(),
            &valid_indices,
            &rejected_errors,
            states,
            &server_id,
        )
        .expect_err("array length parity mismatch must return error");

        assert!(
            matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched read result array size")),
            "Expected OpcError::Internal with mismatch details, got: {err:?}"
        );
    }

    #[test]
    fn test_assemble_tag_values_mixed_valid_and_rejected() {
        use crate::connector::GroupItemState;
        use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let requested_tags = ["tag0", "tag1", "tag2"];
        let valid_indices = vec![0, 2];
        let rejected_errors = vec![(1, OpcError::InvalidState("Item rejected".into()))];
        let states = Some(vec![
            Ok(GroupItemState {
                client_handle: ClientItemHandle::new(0),
                value: OpcValue::Int(10),
                quality: OpcQuality::GOOD,
                timestamp: std::time::SystemTime::UNIX_EPOCH,
            }),
            Ok(GroupItemState {
                client_handle: ClientItemHandle::new(2),
                value: OpcValue::Int(20),
                quality: OpcQuality::GOOD,
                timestamp: std::time::SystemTime::UNIX_EPOCH,
            }),
        ]);

        let results = assemble_tag_values(
            requested_tags.into_iter(),
            &valid_indices,
            &rejected_errors,
            states,
            &server_id,
        )
        .expect("assembly of mixed valid and rejected tags must succeed");

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tag_id, "tag0");
        assert!(results[0].outcome.is_ok());
        assert_eq!(results[0].quality, OpcQuality::GOOD);

        assert_eq!(results[1].tag_id, "tag1");
        assert!(results[1].outcome.is_err());
        assert_eq!(results[1].quality, OpcQuality::BAD_CONFIG_ERROR);

        assert_eq!(results[2].tag_id, "tag2");
        assert!(results[2].outcome.is_ok());
        assert_eq!(results[2].quality, OpcQuality::GOOD);
    }
}
