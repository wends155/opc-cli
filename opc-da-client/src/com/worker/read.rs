//! Tag reading engine with in-place value population and active group caching.

use super::pool::{CachedGroup, PooledServer};
use crate::com::connector::{
    ConnectedGroup, ConnectedServer, DataSource, GroupItemResult, GroupItemState,
};
use crate::errors::{OpcError, OpcOperation, OpcResult};
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

    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %endpoint,
        tag_count = tags.len(),
        sample_tags = ?tags.iter_str().take(5).collect::<Vec<_>>(),
        "read_tag_values: starting operation"
    );
    let start = std::time::Instant::now();

    // Check active group cache hit
    if let Some(cached) = pooled.active_group.as_ref()
        && cached.tags.len() == tags.len()
        && cached.tags.iter().zip(tags.iter_str()).all(|(a, b)| a == b)
    {
        let mut tag_values: Vec<TagValue> = cached
            .tags
            .iter()
            .map(|tag_id| TagValue {
                tag_id: tag_id.clone(),
                outcome: Err(OpcError::Internal("Not read".into())),
                quality: OpcQuality::BAD_CONFIG_ERROR,
                timestamp: None,
            })
            .collect();

        // Populate remembered errors for items that were rejected during add_items
        for &(idx, ref err) in &cached.rejected_errors {
            tag_values[idx].quality = OpcQuality::BAD_CONFIG_ERROR;
            tag_values[idx].outcome = Err(err.clone());
        }

        if !cached.server_item_handles.is_empty() {
            let item_states = cached
                .group
                .read(DataSource::Device, &cached.server_item_handles)
                .inspect_err(|e| {
                    log_opc_err!(
                        e,
                        OpcOperation::ReadSync,
                        server = %endpoint.identifier,
                        handle_count = cached.server_item_handles.len()
                    );
                })?;

            populate_item_states(
                item_states,
                &cached.valid_indices,
                &cached.tags,
                &endpoint.identifier,
                &mut tag_values,
            );
        }

        tracing::info!(
            count = tag_values.len(),
            elapsed_ms = super::elapsed_ms(start),
            "read_tag_values (cache hit) completed"
        );
        return Ok(TagValues::new(tag_values));
    }

    // Cache miss: remove previous active group
    pooled.clear_active_group();

    let tag_ids: Vec<String> = tags.iter_str().map(ToString::to_string).collect();
    let reg = super::register_item_group(
        &pooled.server,
        &endpoint.identifier,
        "opc-read",
        &tag_ids,
        OpcOperation::ReadAddGroup,
        OpcOperation::ReadAddItems,
    )?;
    let group = reg.group;
    let mut group_guard = reg.group_guard;
    let results = reg.item_results;

    let mut tag_values: Vec<TagValue> = tag_ids
        .iter()
        .map(|tag_id| TagValue {
            tag_id: tag_id.clone(),
            outcome: Err(OpcError::Internal("Not read".into())),
            quality: OpcQuality::BAD_CONFIG_ERROR,
            timestamp: None,
        })
        .collect();

    let (server_handles, valid_indices, rejected_errors) =
        partition_item_results(&results, &tag_ids, &endpoint.identifier, &mut tag_values);

    if !server_handles.is_empty() {
        let item_states = group
            .read(DataSource::Device, &server_handles)
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::ReadSync,
                    server = %endpoint.identifier,
                    handle_count = server_handles.len()
                );
            })?;

        populate_item_states(
            item_states,
            &valid_indices,
            &tag_ids,
            &endpoint.identifier,
            &mut tag_values,
        );
    }

    let server_handle = group_guard.disarm();

    pooled.active_group = Some(CachedGroup {
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
    tag_values: &mut [TagValue],
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
            tag_values[idx].quality = OpcQuality::BAD_CONFIG_ERROR;
            tag_values[idx].outcome = Err(err.clone());
            rejected_errors.push((idx, err.clone()));
        } else {
            server_handles.push(item_result.server_handle);
            valid_indices.push(idx);
        }
    }

    (server_handles, valid_indices, rejected_errors)
}

/// Writes device states into pre-allocated [`TagValue`] entries by original index.
fn populate_item_states(
    item_states: Vec<OpcResult<GroupItemState>>,
    valid_indices: &[usize],
    tag_ids: &[String],
    server_id: &ServerIdentifier,
    tag_values: &mut [TagValue],
) {
    for (state_res, &idx) in item_states.into_iter().zip(valid_indices) {
        match state_res {
            Ok(state) => {
                tag_values[idx].outcome = Ok(state.value);
                tag_values[idx].quality = state.quality;
                tag_values[idx].timestamp = Some(state.timestamp);
            }
            Err(e) => {
                log_opc_err!(
                    &e,
                    OpcOperation::ReadPerItem,
                    server = %server_id,
                    tag = %tag_ids[idx]
                );
                tag_values[idx].outcome = Err(e);
                tag_values[idx].quality = OpcQuality::BAD_COMM_FAILURE;
                tag_values[idx].timestamp = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::connector::mock::MockConnectedServer;
    use crate::com::worker::pool::PooledServer;
    use crate::types::{OpcServerEndpoint, TagBatch};

    #[test]
    fn test_handle_read_empty_tags_short_circuits() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::from("Test.Server");
        let tags = TagBatch::Static(&[]);
        let results = handle_read(&endpoint, &tags, &mut pooled).expect("empty tags must succeed");
        assert!(results.is_empty());
    }

    #[test]
    fn test_handle_read_with_mock_server() {
        let server = MockConnectedServer::default();
        let mut pooled = PooledServer::new(server);
        let endpoint = OpcServerEndpoint::from("Test.Server");
        let tags = TagBatch::Static(&["Random.Int4", "Random.Real8"]);
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
        let endpoint = OpcServerEndpoint::from("Test.Server");
        let tags = TagBatch::Static(&["Random.Int4"]);

        // First read (cache miss): group added, guard disarmed, cached in pooled
        let res = handle_read(&endpoint, &tags, &mut pooled);
        assert!(res.is_ok());
        assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
        assert_eq!(state.remove_group_count.load(Ordering::Relaxed), 0);
        assert!(pooled.active_group.is_some());

        // Second read with same tags (cache hit): no new group created or removed
        let res2 = handle_read(&endpoint, &tags, &mut pooled);
        assert!(res2.is_ok());
        assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
        assert_eq!(state.remove_group_count.load(Ordering::Relaxed), 0);

        // Explicit clear active group triggers remove_group
        pooled.clear_active_group();
        assert_eq!(state.remove_group_count.load(Ordering::Relaxed), 1);
    }
}
