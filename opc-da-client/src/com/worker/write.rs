//! Tag writing engine with validation and result mapping.

use crate::com::connector::{ConnectedGroup, ConnectedServer, GroupItemDef};
use crate::com::guard::GroupGuard;
use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::provider::{OpcValue, WriteResult};
use crate::types::{ItemHandle, ServerIdentifier};

/// Executes synchronous single-tag writing through a temporary OPC group, returning
/// a structured [`WriteResult`] capturing success or server-rejected error details.
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
        "write_tag_value: starting operation"
    );
    let start = std::time::Instant::now();

    let created = opc_server
        .add_group(&super::ephemeral_group_config("opc-da-client-write"))
        .inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::WriteAddGroup,
                server = %server_id,
                tag = %tag_id,
                value = ?value
            );
        })?;
    let group = created.group;
    let _group_guard = GroupGuard::new(opc_server, created.server_handle);

    let item_def = GroupItemDef {
        item_id: tag_id.to_string(),
        client_handle: ItemHandle::default(),
        active: true,
    };

    let results = group.add_items(&[item_def]).inspect_err(|e| {
        log_opc_err!(
            e,
            OpcOperation::WriteAddItems,
            server = %server_id,
            tag = %tag_id,
            value = ?value
        );
    })?;
    let item_res = results.first().ok_or_else(|| {
        let e = OpcError::Internal("Server returned empty item results".to_string());
        log_opc_err!(
            &e,
            OpcOperation::WriteEmptyItemResults,
            server = %server_id,
            tag = %tag_id,
            value = ?value
        );
        e
    })?;

    if let Some(e) = &item_res.error {
        log_opc_err!(
            e,
            OpcOperation::WriteAddItemsRejected,
            server = %server_id,
            tag = %tag_id,
            value = ?value
        );
        return Ok(WriteResult::failure(tag_id, e.clone()));
    }

    let item_handle = item_res.server_handle;
    let write_results = group
        .write(&[item_handle], std::slice::from_ref(value))
        .inspect_err(|e| {
            log_opc_err!(
                e,
                OpcOperation::WriteSync,
                server = %server_id,
                tag = %tag_id,
                value = ?value
            );
        })?;
    let write_res = write_results.first().ok_or_else(|| {
        let e = OpcError::Internal("Server returned empty write errors".to_string());
        log_opc_err!(
            &e,
            OpcOperation::WriteEmptyWriteErrors,
            server = %server_id,
            tag = %tag_id,
            value = ?value
        );
        e
    })?;

    let write_result = match write_res {
        Ok(()) => {
            tracing::info!(
                elapsed_ms = super::elapsed_ms(start),
                "write_tag_value completed"
            );
            WriteResult::success(tag_id)
        }
        Err(e) => {
            log_opc_err!(
                e,
                OpcOperation::WriteServerRejected,
                server = %server_id,
                tag = %tag_id,
                value = ?value,
                elapsed_ms = super::elapsed_ms(start)
            );
            WriteResult::failure(tag_id, e.clone())
        }
    };

    Ok(write_result)
}
