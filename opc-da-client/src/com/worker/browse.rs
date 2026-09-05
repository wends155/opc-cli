//! Address space browsing engine with RAII position protection.

use crate::com::connector::ConnectedServer;
use crate::com::guard::BrowsePositionGuard;
use crate::errors::{OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::provider::TagCollector;
use crate::types::{BrowseType, NamespaceType, ServerIdentifier};

/// Maximum recursion depth allowed during depth-first namespace traversal.
pub const DEFAULT_MAX_BROWSE_DEPTH: usize = 50;

/// Browses available OPC DA item IDs on a server, attempting fast flat enumeration
/// first and falling back to recursive depth-first branch exploration.
#[tracing::instrument(
    name = "opc.browse_tags",
    level = "info",
    skip(collector, opc_server),
    fields(max_tags = collector.max_tags()),
    err
)]
pub fn handle_browse<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    collector: &TagCollector,
    opc_server: &S,
) -> OpcResult<Vec<String>> {
    #[cfg(feature = "dev-diagnostics")]
    tracing::trace!(
        server = %server_id,
        max_tags = collector.max_tags(),
        "browse_tags: starting operation"
    );
    let start = std::time::Instant::now();

    if collector.is_cancelled() || collector.is_full() {
        return Ok(collector.snapshot());
    }

    let org = opc_server.query_organization().inspect_err(|e| {
        log_opc_err!(
            e,
            OpcOperation::BrowseQueryOrganization,
            server = %server_id
        );
    })?;

    if org == NamespaceType::Flat as u32 {
        let string_iter = opc_server
            .browse_opc_item_ids(BrowseType::Leaf, Some(""), 0, 0)
            .inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::BrowseFlatLeaves,
                    server = %server_id
                );
            })?;
        for tag_res in string_iter {
            let tag = tag_res.inspect_err(|e| {
                log_opc_err!(
                    e,
                    OpcOperation::BrowseFlatLeafItem,
                    server = %server_id
                );
            })?;
            if !collector.push(tag) {
                break;
            }
        }
    } else {
        let use_flat = match opc_server.browse_opc_item_ids(BrowseType::Flat, Some(""), 0, 0) {
            Ok(mut flat_enum) => match flat_enum.next() {
                Some(Ok(first_tag)) => {
                    tracing::info!("OPC_FLAT browse supported — using fast flat enumeration");
                    if collector.push(first_tag) {
                        for tag_res in flat_enum {
                            match tag_res {
                                Ok(tag) => {
                                    if !collector.push(tag) {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    log_opc_err!(
                                        &e,
                                        OpcOperation::BrowseFlatEnumItem,
                                        server = %server_id
                                    );
                                }
                            }
                        }
                    }
                    true
                }
                Some(Err(e)) => {
                    tracing::debug!(error = ?e, "OPC_FLAT first item error, falling back to recursive");
                    false
                }
                None => {
                    tracing::debug!("OPC_FLAT returned no items, falling back to recursive");
                    false
                }
            },
            Err(e) => {
                tracing::debug!(error = ?e, "OPC_FLAT not supported, falling back to recursive");
                false
            }
        };

        if !use_flat {
            browse_recursive(opc_server, collector, 0)?;
        }
    }
    let result = collector.snapshot();
    tracing::info!(
        count = result.len(),
        elapsed_ms = super::elapsed_ms(start),
        "browse_tags completed"
    );
    Ok(result)
}

/// Recursively traverses server branch and leaf hierarchy with depth limit and cancellation guards.
#[tracing::instrument(level = "debug", skip(server, collector), err)]
fn browse_recursive<S: ConnectedServer>(
    server: &S,
    collector: &TagCollector,
    depth: usize,
) -> OpcResult<()> {
    if depth > DEFAULT_MAX_BROWSE_DEPTH || collector.is_cancelled() || collector.is_full() {
        if depth > DEFAULT_MAX_BROWSE_DEPTH {
            tracing::warn!(depth, "Max browse depth reached, truncating");
        }
        return Ok(());
    }

    let branch_enum = server
        .browse_opc_item_ids(BrowseType::Branch, Some(""), 0, 0)
        .inspect_err(|e| {
            log_opc_err!(e, OpcOperation::BrowseRecursiveBranches, depth = depth);
        })?;

    let branches: Vec<String> = branch_enum
        .filter_map(|r| match r {
            Ok(name) => Some(name),
            Err(e) => {
                log_opc_err!(&e, OpcOperation::BrowseRecursiveBranchItem, depth = depth);
                None
            }
        })
        .collect();

    let leaf_enum = server
        .browse_opc_item_ids(BrowseType::Leaf, Some(""), 0, 0)
        .inspect_err(|e| {
            log_opc_err!(e, OpcOperation::BrowseRecursiveLeaves, depth = depth);
        })?;
    for tag_res in leaf_enum {
        if collector.is_cancelled() || collector.is_full() {
            return Ok(());
        }
        let browse_name = tag_res.inspect_err(|e| {
            log_opc_err!(e, OpcOperation::BrowseRecursiveLeafItem, depth = depth);
        })?;
        let tag = match server.get_item_id(&browse_name) {
            Ok(id) => id,
            Err(e) => {
                log_opc_err!(
                    &e,
                    OpcOperation::BrowseRecursiveGetItemId,
                    browse_name = %browse_name,
                    depth = depth
                );
                browse_name
            }
        };
        if !collector.push(tag) {
            return Ok(());
        }
    }

    for branch in branches {
        if collector.is_cancelled() || collector.is_full() {
            return Ok(());
        }

        let guard = match BrowsePositionGuard::enter(server, &branch) {
            Ok(g) => g,
            Err(e) => {
                log_opc_err!(
                    &e,
                    OpcOperation::BrowseRecursiveChangePositionDown,
                    branch = %branch,
                    depth = depth
                );
                continue;
            }
        };

        if let Err(e) = browse_recursive(server, collector, depth + 1) {
            log_opc_err!(
                &e,
                OpcOperation::BrowseRecursiveChildBranch,
                depth = depth + 1
            );
        }

        drop(guard);
    }

    Ok(())
}
