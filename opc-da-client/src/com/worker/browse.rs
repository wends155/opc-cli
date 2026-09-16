//! Address space browsing engine with RAII position protection.

use crate::connector::{BrowsePositionGuard, ConnectedServer};
use crate::errors::OpcResult;
use crate::log_opc_err;
use crate::types::{BrowseType, NamespaceType, ServerIdentifier, TagCollector, VarType};

/// Maximum recursion depth allowed during depth-first namespace traversal.
pub const DEFAULT_MAX_BROWSE_DEPTH: usize = 50;

/// Chunk size for batching leaf tag ingestion to minimize collector mutex contention.
pub const BROWSE_CHUNK_SIZE: usize = 256;

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
            "browse:query_organization",
            server = %server_id
        );
    })?;

    if org == NamespaceType::Flat {
        browse_flat_namespace(server_id, collector, opc_server)?;
    } else {
        let use_flat = try_fast_flat_browse(server_id, collector, opc_server);
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

fn browse_flat_namespace<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    collector: &TagCollector,
    opc_server: &S,
) -> OpcResult<()> {
    let string_iter = opc_server
        .browse_opc_item_ids(BrowseType::Leaf, Some(""), VarType::EMPTY, 0)
        .inspect_err(|e| {
            log_opc_err!(
                e,
                "browse_flat:leaves",
                server = %server_id
            );
        })?;
    let mut chunk = Vec::with_capacity(BROWSE_CHUNK_SIZE);
    for tag_res in string_iter {
        if collector.is_cancelled() || collector.is_full() {
            break;
        }
        let tag = match tag_res {
            Ok(tag) => tag,
            Err(e) => {
                log_opc_err!(
                    &e,
                    "browse_flat:leaf_item",
                    server = %server_id
                );
                continue;
            }
        };
        chunk.push(tag);
        if chunk.len() >= BROWSE_CHUNK_SIZE {
            let _ = collector.push_batch(std::mem::replace(
                &mut chunk,
                Vec::with_capacity(BROWSE_CHUNK_SIZE),
            ));
            if collector.is_cancelled() || collector.is_full() {
                break;
            }
        }
    }
    if !chunk.is_empty() {
        let _ = collector.push_batch(chunk);
    }
    Ok(())
}

fn try_fast_flat_browse<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    collector: &TagCollector,
    opc_server: &S,
) -> bool {
    match opc_server.browse_opc_item_ids(BrowseType::Flat, Some(""), VarType::EMPTY, 0) {
        Ok(mut flat_enum) => match flat_enum.next() {
            Some(Ok(first_tag)) => {
                tracing::info!("OPC_FLAT browse supported — using fast flat enumeration");
                let mut chunk = Vec::with_capacity(BROWSE_CHUNK_SIZE);
                chunk.push(first_tag);
                for tag_res in flat_enum {
                    if collector.is_cancelled() || collector.is_full() {
                        break;
                    }
                    match tag_res {
                        Ok(tag) => {
                            chunk.push(tag);
                            if chunk.len() >= BROWSE_CHUNK_SIZE {
                                let _ = collector.push_batch(std::mem::replace(
                                    &mut chunk,
                                    Vec::with_capacity(BROWSE_CHUNK_SIZE),
                                ));
                                if collector.is_cancelled() || collector.is_full() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            log_opc_err!(
                                &e,
                                "browse_flat:enum_item",
                                server = %server_id
                            );
                        }
                    }
                }
                if !chunk.is_empty() {
                    let _ = collector.push_batch(chunk);
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
    }
}

/// Recursively traverses OPC branches and accumulates leaf item IDs into the collector.
fn browse_recursive<S: ConnectedServer>(
    server: &S,
    collector: &TagCollector,
    depth: usize,
) -> OpcResult<()> {
    if depth >= DEFAULT_MAX_BROWSE_DEPTH || collector.is_cancelled() || collector.is_full() {
        return Ok(());
    }

    let leaf_iter = server
        .browse_opc_item_ids(BrowseType::Leaf, Some(""), VarType::EMPTY, 0)
        .inspect_err(|e| {
            log_opc_err!(e, "browse_recursive:leaves", depth = depth);
        })?;

    let mut leaf_ids = Vec::new();
    for leaf_res in leaf_iter {
        if collector.is_cancelled() || collector.is_full() {
            break;
        }
        let leaf_name = match leaf_res {
            Ok(name) => name,
            Err(err) => {
                log_opc_err!(&err, "browse_recursive:leaf_item", depth = depth);
                continue;
            }
        };
        let item_id = match server.get_item_id(&leaf_name) {
            Ok(id) => id,
            Err(err) => {
                log_opc_err!(
                    &err,
                    "browse_recursive:get_item_id",
                    depth = depth,
                    leaf = %leaf_name
                );
                continue;
            }
        };
        leaf_ids.push(item_id);
    }

    if !leaf_ids.is_empty() {
        let _ = collector.push_batch(leaf_ids);
        if collector.is_cancelled() || collector.is_full() {
            return Ok(());
        }
    }

    let branch_iter = server
        .browse_opc_item_ids(BrowseType::Branch, Some(""), VarType::EMPTY, 0)
        .inspect_err(|e| {
            log_opc_err!(e, "browse_recursive:branches", depth = depth);
        })?;

    let branches: Vec<String> = branch_iter
        .filter_map(|b_res| {
            b_res
                .inspect_err(|e| {
                    log_opc_err!(e, "browse_recursive:branch_item", depth = depth);
                })
                .ok()
        })
        .collect();

    for branch in branches {
        if collector.is_cancelled() || collector.is_full() {
            return Ok(());
        }

        let _position_guard = match BrowsePositionGuard::enter(server, &branch) {
            Ok(guard) => guard,
            Err(e) => {
                log_opc_err!(
                    &e,
                    "browse_recursive:change_position_down",
                    depth = depth,
                    branch = %branch
                );
                continue;
            }
        };

        if let Err(e) = browse_recursive(server, collector, depth + 1) {
            log_opc_err!(
                &e,
                "browse_recursive:child_branch",
                depth = depth,
                branch = %branch
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::mock::MockConnectedServer;

    #[test]
    fn test_handle_browse_preserves_collector() {
        let server = MockConnectedServer::default();
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(100);

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("browse operation should succeed");
        assert!(!tags.is_empty());
        // Collector snapshot preserves accumulator contents
        assert_eq!(collector.len(), tags.len());
        assert_eq!(collector.snapshot(), tags);
    }

    #[test]
    fn test_handle_browse_cancelled_returns_snapshot() {
        let server = MockConnectedServer::default();
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(100);
        collector.cancel();

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("cancelled browse should return empty ok");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_handle_browse_flat_chunked_batch_push() {
        let server = MockConnectedServer::default();
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(1000);

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("browse flat operation should succeed");
        assert_eq!(tags.len(), collector.len());
        assert_eq!(collector.snapshot(), tags);
    }
}
