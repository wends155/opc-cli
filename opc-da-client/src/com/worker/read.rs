//! Tag reading engine with in-place value population.

use crate::com::connector::{
    ConnectedGroup, ConnectedServer, DataSource, GroupItemDef, GroupItemResult, GroupItemState,
};
use crate::com::guard::GroupGuard;
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::provider::{OpcQuality, TagValue};
use crate::types::{ItemHandle, ServerIdentifier};

/// Executes synchronous device tag reading through a temporary OPC group, populating
/// values, qualities, and timestamps into pre-allocated [`TagValue`] slots.
#[tracing::instrument(
    name = "opc.read_tag_values",
    level = "info",
    skip(tag_ids, opc_server),
    fields(tag_count = tag_ids.len()),
    err
)]
pub fn handle_read<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    tag_ids: &[String],
    opc_server: &S,
) -> OpcResult<Vec<TagValue>> {
    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %server_id,
        tag_count = tag_ids.len(),
        sample_tags = ?tag_ids.iter().take(5).collect::<Vec<_>>(),
        "read_tag_values: starting operation"
    );
    let start = std::time::Instant::now();

    let created = opc_server
        .add_group(&super::ephemeral_group_config("opc-da-client-read"))
        .inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::ReadAddGroup,
                server = %server_id,
                tag_count = tag_ids.len()
            );
        })?;
    let group = created.group;
    let _group_guard = GroupGuard::new(opc_server, created.server_handle);

    let item_defs: Vec<GroupItemDef> = tag_ids
        .iter()
        .enumerate()
        .map(|(idx, tag_id)| GroupItemDef {
            item_id: tag_id.clone(),
            #[allow(clippy::cast_possible_truncation)]
            client_handle: ItemHandle(idx as u32),
            active: true,
        })
        .collect();

    let results = group.add_items(&item_defs).inspect_err(|e| {
        log_opc_err!(
            e,
            OpcOperation::ReadAddItems,
            server = %server_id,
            tag_count = tag_ids.len()
        );
    })?;

    if results.len() != tag_ids.len() {
        let err = OpcError::Internal("OPC server returned mismatched result array sizes".into());
        log_opc_err!(
            &err,
            OpcOperation::ReadMismatchedResults,
            server = %server_id,
            expected = tag_ids.len(),
            actual = results.len()
        );
        return Err(err);
    }

    let mut tag_values: Vec<TagValue> = tag_ids
        .iter()
        .map(|tag_id| TagValue {
            tag_id: tag_id.clone(),
            value: None,
            quality: OpcQuality::BAD_CONFIG_ERROR,
            timestamp: None,
        })
        .collect();

    let (server_handles, valid_indices) =
        partition_item_results(&results, tag_ids, server_id, &mut tag_values);

    if server_handles.is_empty() {
        return Ok(tag_values);
    }

    let item_states = group
        .read(DataSource::Device, &server_handles)
        .inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::ReadSync,
                server = %server_id,
                handle_count = server_handles.len()
            );
        })?;

    populate_item_states(
        &item_states,
        &valid_indices,
        tag_ids,
        server_id,
        &mut tag_values,
    );

    tracing::info!(
        count = tag_values.len(),
        elapsed_ms = super::elapsed_ms(start),
        "read_tag_values completed"
    );
    Ok(tag_values)
}

/// Separates valid item handles from rejected tags, recording configuration errors for rejected tags.
fn partition_item_results(
    results: &[GroupItemResult],
    tag_ids: &[String],
    server_id: &ServerIdentifier,
    tag_values: &mut [TagValue],
) -> (Vec<ItemHandle>, Vec<usize>) {
    let mut server_handles = Vec::new();
    let mut valid_indices = Vec::new();

    for (idx, item_result) in results.iter().enumerate() {
        if item_result.error.is_none() {
            server_handles.push(item_result.server_handle);
            valid_indices.push(idx);
        } else {
            let err_msg = item_result
                .error
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            tracing::warn!(
                server = %server_id,
                tag = %tag_ids[idx],
                error = %err_msg,
                "read_tag_values: add_items rejected tag"
            );
            tag_values[idx].quality = OpcQuality::BAD_CONFIG_ERROR;
        }
    }

    (server_handles, valid_indices)
}

/// Writes device states into pre-allocated [`TagValue`] entries by original index.
fn populate_item_states(
    item_states: &[OpcResult<GroupItemState>],
    valid_indices: &[usize],
    tag_ids: &[String],
    server_id: &ServerIdentifier,
    tag_values: &mut [TagValue],
) {
    for (i, idx) in valid_indices.iter().enumerate() {
        match &item_states[i] {
            Ok(state) => {
                tag_values[*idx].value = Some(state.value.clone());
                tag_values[*idx].quality = state.quality;
                tag_values[*idx].timestamp = Some(state.timestamp);
            }
            Err(e) => {
                log_opc_err!(
                    e,
                    OpcOperation::ReadPerItem,
                    server = %server_id,
                    tag = %tag_ids[*idx]
                );
                tag_values[*idx].value = None;
                tag_values[*idx].quality = OpcQuality::BAD_COMM_FAILURE;
                tag_values[*idx].timestamp = None;
            }
        }
    }
}
