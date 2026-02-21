use crate::bindings::da::{
    OPC_BRANCH, OPC_BROWSE_DOWN, OPC_BROWSE_UP, OPC_DS_DEVICE, OPC_LEAF, OPC_NS_FLAT, tagOPCITEMDEF,
};
use crate::helpers::{
    filetime_to_string, friendly_com_hint, guid_to_progid, is_known_iterator_bug,
    opc_value_to_variant, quality_to_string, variant_to_string,
};
use crate::opc_da::client::v2::Client;
use crate::opc_da::client::{ServerTrait, BrowseServerAddressSpaceTrait, ClientTrait};
use crate::provider::{OpcProvider, OpcValue, TagValue, WriteResult};
use crate::backend::connector::{ServerConnector, ConnectedServer, ConnectedGroup, ComConnector};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Concrete [`OpcProvider`] implementation for Windows OPC DA.
///
/// Heavy-weight implementation that uses the `opc_da` crate for
/// native COM interop.
pub struct OpcDaWrapper<C: ServerConnector = ComConnector> {
    connector: Arc<C>,
}

impl Default for OpcDaWrapper<ComConnector> {
    fn default() -> Self {
        Self::new(ComConnector)
    }
}

impl<C: ServerConnector> OpcDaWrapper<C> {
    /// Creates a new `OpcDaWrapper` with the given connector.
    pub fn new(connector: C) -> Self {
        Self { connector: Arc::new(connector) }
    }
}

fn browse_recursive<S: ConnectedServer>(
    server: &S,
    tags: &mut Vec<String>,
    max_tags: usize,
    progress: &Arc<AtomicUsize>,
    tags_sink: &Arc<std::sync::Mutex<Vec<String>>>,
    depth: usize,
) -> Result<()> {
    const MAX_DEPTH: usize = 50;
    if depth > MAX_DEPTH || tags.len() >= max_tags {
        if depth > MAX_DEPTH {
            tracing::warn!(depth, "Max browse depth reached, truncating");
        }
        return Ok(());
    }

    let branch_enum = server
        .browse_opc_item_ids(OPC_BRANCH.0 as u32, Some(""), 0, 0)
        .context("Failed to browse branches at current position")?;

    let branches: Vec<String> = branch_enum
        .filter_map(|r| match r {
            Ok(name) => Some(name),
            Err(e) => {
                if is_known_iterator_bug(&e) {
                    tracing::trace!(error = ?e, "Branch iteration: known crate bug, skipping");
                } else {
                    tracing::warn!(error = ?e, "Branch iteration error, skipping");
                }
                None
            }
        })
        .collect();

    for branch in branches {
        if tags.len() >= max_tags {
            break;
        }
        if let Err(e) = server.change_browse_position(OPC_BROWSE_DOWN.0 as u32, &branch) {
            tracing::warn!(branch = %branch, error = ?e, "Failed to enter branch, skipping");
            continue;
        }

        let recurse_result =
            browse_recursive(server, tags, max_tags, progress, tags_sink, depth + 1);

        if let Err(e) = server.change_browse_position(OPC_BROWSE_UP.0 as u32, "") {
            tracing::error!(branch = %branch, error = ?e, "Failed to navigate back up!");
            return Err(anyhow::anyhow!(
                "Browse position corrupted: failed to navigate up from branch '{branch}'"
            ));
        }
        recurse_result?;
    }

    if let Ok(leaf_enum) = server.browse_opc_item_ids(OPC_LEAF.0 as u32, Some(""), 0, 0) {
        for leaf_res in leaf_enum {
            if tags.len() >= max_tags {
                break;
            }
            let browse_name = match leaf_res {
                Ok(name) => name,
                Err(e) => {
                    if is_known_iterator_bug(&e) {
                        tracing::trace!(error = ?e, "Leaf iteration: known crate bug, skipping");
                    } else {
                        tracing::warn!(error = ?e, "Leaf iteration error, skipping");
                    }
                    continue;
                }
            };

            let tag = match server.get_item_id(&browse_name) {
                Ok(item_id) => {
                    tracing::debug!(
                        browse_name = %browse_name,
                        item_id = %item_id,
                        "get_item_id resolved"
                    );
                    item_id
                }
                Err(e) => {
                    tracing::warn!(
                        browse_name = %browse_name,
                        error = ?e,
                        "get_item_id failed, using browse name as fallback"
                    );
                    browse_name
                }
            };
            tags.push(tag.clone());
            if let Ok(mut sink) = tags_sink.lock() {
                sink.push(tag);
            }
            progress.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl<C: ServerConnector + 'static> OpcProvider for OpcDaWrapper<C> {
    async fn list_servers(&self, host: &str) -> Result<Vec<String>> {
        let host_owned = host.to_string();
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!("opc.list_servers", host = %host_owned);
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            connector.enumerate_servers()
        })
        .await?
    }

    async fn browse_tags(
        &self,
        server: &str,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
        tags_sink: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Result<Vec<String>> {
        let server_name = server.to_string();
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!("opc.browse_tags", server = %server_name, max_tags);
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let opc_server = connector.connect(&server_name)?;

            let org = opc_server
                .query_organization()
                .context("Failed to query namespace organization")?;
            let mut tags = Vec::new();

            if org == OPC_NS_FLAT.0 as u32 {
                let string_iter = opc_server.browse_opc_item_ids(OPC_LEAF.0 as u32, Some(""), 0, 0)?;
                for tag_res in string_iter {
                    if tags.len() >= max_tags {
                        break;
                    }
                    let tag = tag_res.map_err(|e| anyhow::anyhow!("Tag iteration error: {e:?}"))?;
                    tags.push(tag.clone());
                    if let Ok(mut sink) = tags_sink.lock() {
                        sink.push(tag);
                    }
                    progress.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                browse_recursive(&opc_server, &mut tags, max_tags, &progress, &tags_sink, 0)?;
            }
            tracing::debug!(count = tags.len(), "Browse complete");
            Ok(tags)
        })
        .await?
    }

    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> Result<Vec<TagValue>> {
        let server_name = server.to_string();
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!(
                "opc.read_tag_values",
                server = %server_name,
                tag_count = tag_ids.len()
            );
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let opc_server = connector.connect(&server_name)?;

            let mut revised_update_rate = 0u32;
            let mut server_handle = 0u32;
            let group = opc_server.add_group(
                "opc-da-client-read",
                true,
                1000, // update_rate
                0,    // client_handle
                0,    // time_bias
                0.0,  // percent_deadband
                0,    // locale_id
                &mut revised_update_rate,
                &mut server_handle,
            )?;

            // SAFETY: item_id_wides must outlive item_defs because
            // tagOPCITEMDEF.szItemID holds a raw pointer into each Vec.
            let item_id_wides: Vec<Vec<u16>> = tag_ids
                .iter()
                .map(|tag_id| tag_id.encode_utf16().chain(std::iter::once(0)).collect())
                .collect();

            let item_defs: Vec<tagOPCITEMDEF> = item_id_wides
                .iter()
                .enumerate()
                .map(|(idx, wide)| tagOPCITEMDEF {
                    szAccessPath: windows::core::PWSTR::null(),
                    szItemID: windows::core::PWSTR(wide.as_ptr().cast_mut()),
                    bActive: windows::Win32::Foundation::TRUE,
                    #[allow(clippy::cast_possible_truncation)]
                    hClient: idx as u32,
                    dwBlobSize: 0,
                    pBlob: std::ptr::null_mut(),
                    vtRequestedDataType: 0,
                    wReserved: 0,
                })
                .collect();

            let (results, errors) = group.add_items(&item_defs)?;
            let mut server_handles = Vec::new();
            let mut valid_indices = Vec::new();

            for (idx, (item_result, error)) in results
                .as_slice()
                .iter()
                .zip(errors.as_slice().iter())
                .enumerate()
            {
                if error.is_ok() {
                    server_handles.push(item_result.hServer);
                    valid_indices.push(idx);
                } else {
                    tracing::warn!(
                        tag = %tag_ids[idx],
                        error = ?error,
                        "read_tag_values: add_items rejected tag"
                    );
                }
            }

            if server_handles.is_empty() {
                return Err(anyhow::anyhow!("No valid items to read"));
            }

            let (item_states, read_errors) = group.read(OPC_DS_DEVICE, &server_handles)?;
            let mut tag_values = Vec::new();
            let item_states_slice = item_states.as_slice();
            let read_errors_slice = read_errors.as_slice();

            for (i, idx) in valid_indices.iter().enumerate() {
                let state = &item_states_slice[i];
                let read_error = &read_errors_slice[i];

                let (value_str, quality_str) = if read_error.is_ok() {
                    (
                        variant_to_string(&state.vDataValue),
                        quality_to_string(state.wQuality),
                    )
                } else {
                    let hint = friendly_com_hint(&anyhow::anyhow!("{read_error:?}"));
                    let full_msg = hint.unwrap_or("Unknown OPC error");

                    tracing::warn!(
                        tag = %tag_ids[*idx],
                        error = ?read_error,
                        hint = %full_msg,
                        "read_tag_values: per-item read error"
                    );

                    ("Error".to_string(), format!("Bad — {full_msg}"))
                };

                tag_values.push(TagValue {
                    tag_id: tag_ids[*idx].clone(),
                    value: value_str,
                    quality: quality_str,
                    timestamp: filetime_to_string(state.ftTimeStamp),
                });
            }

            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, "Failed to remove OPC group during cleanup");
            }
            Ok(tag_values)
        })
        .await?
    }

    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> Result<WriteResult> {
        let server_name = server.to_string();
        let tag = tag_id.to_string();
        let connector = Arc::clone(&self.connector);
        tokio::task::spawn_blocking(move || {
            let span = tracing::info_span!(
                "opc.write_tag_value",
                server = %server_name,
                tag = %tag
            );
            let _enter = span.enter();

            let _guard = crate::ComGuard::new()?;
            let opc_server = connector.connect(&server_name)?;

            let mut revised_update_rate = 0u32;
            let mut server_handle = 0u32;
            let group = opc_server.add_group(
                "opc-da-client-write",
                true,
                1000,
                0,
                0,
                0.0,
                0,
                &mut revised_update_rate,
                &mut server_handle,
            )?;

            // SAFETY: item_id_wide must outlive item_def because
            // tagOPCITEMDEF.szItemID holds a raw pointer into the Vec.
            let mut item_id_wide: Vec<u16> =
                tag.encode_utf16().chain(std::iter::once(0)).collect();
            let item_def = tagOPCITEMDEF {
                szAccessPath: windows::core::PWSTR::null(),
                szItemID: windows::core::PWSTR(item_id_wide.as_mut_ptr()),
                bActive: windows::Win32::Foundation::TRUE,
                hClient: 0,
                dwBlobSize: 0,
                pBlob: std::ptr::null_mut(),
                vtRequestedDataType: 0,
                wReserved: 0,
            };

            let (results, errors) = group.add_items(&[item_def])?;
            let item_res = results
                .as_slice()
                .first()
                .context("Server returned empty item results")?;
            let item_err = errors
                .as_slice()
                .first()
                .context("Server returned empty item errors")?;

            if let Err(e) = item_err.ok() {
                tracing::warn!(error = ?e, "write_tag_value: failed to add tag to group");
                return Ok(WriteResult {
                    tag_id: tag,
                    success: false,
                    error: Some(format!("Failed to add tag to group: {e:?}")),
                });
            }

            let variant = opc_value_to_variant(&value);
            let write_errors = group.write(&[item_res.hServer], &[variant])?;
            let write_error = write_errors
                .as_slice()
                .first()
                .context("Server returned empty write errors")?;

            let write_result = if write_error.is_ok() {
                tracing::info!("write_tag_value: write completed successfully");
                WriteResult {
                    tag_id: tag,
                    success: true,
                    error: None,
                }
            } else {
                let hint =
                    friendly_com_hint(&anyhow::anyhow!("{write_error:?}")).unwrap_or("");
                tracing::error!(error = ?write_error, hint = %hint, "write_tag_value: server rejected write");
                WriteResult {
                    tag_id: tag,
                    success: false,
                    error: Some(if hint.is_empty() {
                        format!("{write_error:?}")
                    } else {
                        format!("{write_error:?} ({hint})")
                    }),
                }
            };

            if let Err(e) = opc_server.remove_group(server_handle, true) {
                tracing::warn!(error = ?e, "Failed to remove OPC group during cleanup");
            }
            Ok(write_result)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::connector::{ConnectedGroup, ConnectedServer, ServerConnector};
    use crate::bindings::da::{tagOPCITEMDEF, tagOPCITEMRESULT, tagOPCITEMSTATE};
    use crate::opc_da::client::StringIterator;
    use crate::opc_da::utils::RemoteArray;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::System::Com::{IEnumString, IEnumString_Impl};
    use windows::core::{implement, PWSTR};

    #[implement(IEnumString)]
    struct MockEnumString {
        items: Vec<String>,
        index: std::sync::atomic::AtomicUsize,
    }

    impl IEnumString_Impl for MockEnumString_Impl {
        fn Next(
            &self,
            celt: u32,
            rgelt: *mut PWSTR,
            pceltfetched: *mut u32,
        ) -> windows::core::HRESULT {
            let mut fetched = 0;
            let index = self.index.load(std::sync::atomic::Ordering::Relaxed);
            let rgelt = unsafe { std::slice::from_raw_parts_mut(rgelt, celt as usize) };
            
            for i in 0..celt as usize {
                if index + i < self.items.len() {
                    let s = &self.items[index + i];
                    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                    let ptr = unsafe { windows::Win32::System::Com::CoTaskMemAlloc(w.len() * 2) };
                    unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), ptr as *mut u16, w.len()) };
                    rgelt[i] = PWSTR(ptr as *mut u16);
                    fetched += 1;
                } else {
                    break;
                }
            }
            
            self.index.store(index + fetched, std::sync::atomic::Ordering::Relaxed);
            
            if !pceltfetched.is_null() {
                unsafe { *pceltfetched = fetched as u32 };
            }
            
            if fetched == celt as usize {
                windows::Win32::Foundation::S_OK.into()
            } else {
                windows::Win32::Foundation::S_FALSE.into()
            }
        }
        fn Skip(&self, _celt: u32) -> windows::core::HRESULT {
            windows::Win32::Foundation::E_NOTIMPL.into()
        }
        fn Reset(&self) -> windows::core::Result<()> {
            self.index.store(0, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
        fn Clone(&self) -> windows::core::Result<IEnumString> {
            Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_NOTIMPL))
        }
    }

    struct MockGroup;

    impl ConnectedGroup for MockGroup {
        fn add_items(
            &self,
            _items: &[tagOPCITEMDEF],
        ) -> anyhow::Result<(
            RemoteArray<tagOPCITEMRESULT>,
            RemoteArray<windows::core::HRESULT>,
        )> {
            Ok((RemoteArray::empty(), RemoteArray::empty()))
        }

        fn read(
            &self,
            _source: crate::bindings::da::tagOPCDATASOURCE,
            _server_handles: &[u32],
        ) -> anyhow::Result<(
            RemoteArray<tagOPCITEMSTATE>,
            RemoteArray<windows::core::HRESULT>,
        )> {
            Ok((RemoteArray::empty(), RemoteArray::empty()))
        }

        fn write(
            &self,
            _server_handles: &[u32],
            _values: &[VARIANT],
        ) -> anyhow::Result<RemoteArray<windows::core::HRESULT>> {
            Ok(RemoteArray::empty())
        }
    }

    struct MockServer;

    impl ConnectedServer for MockServer {
        type Group = MockGroup;

        fn query_organization(&self) -> anyhow::Result<u32> {
            Ok(crate::bindings::da::OPC_NS_FLAT.0 as u32)
        }

        fn browse_opc_item_ids(
            &self,
            _browse_type: u32,
            _filter: Option<&str>,
            _data_type: u16,
            _access_rights: u32,
        ) -> anyhow::Result<StringIterator> {
            let mock_enum: IEnumString = MockEnumString {
                items: vec!["MockTag.1".to_string(), "MockTag.2".to_string()],
                index: std::sync::atomic::AtomicUsize::new(0),
            }
            .into();
            Ok(StringIterator::new(mock_enum))
        }

        fn change_browse_position(&self, _direction: u32, _name: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_item_id(&self, item_name: &str) -> anyhow::Result<String> {
            Ok(item_name.to_string())
        }

        fn add_group(
            &self,
            _name: &str,
            _active: bool,
            _update_rate: u32,
            _client_handle: u32,
            _time_bias: i32,
            _percent_deadband: f32,
            _locale_id: u32,
            revised_update_rate: &mut u32,
            server_handle: &mut u32,
        ) -> anyhow::Result<Self::Group> {
            *revised_update_rate = 1000;
            *server_handle = 1;
            Ok(MockGroup)
        }

        fn remove_group(&self, _server_group: u32, _force: bool) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct MockConnector;

    impl ServerConnector for MockConnector {
        type Server = MockServer;

        fn enumerate_servers(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["Mock.Server.1".to_string(), "Mock.Server.2".to_string()])
        }

        fn connect(&self, _server_name: &str) -> anyhow::Result<Self::Server> {
            Ok(MockServer)
        }
    }

    #[test]
    fn test_mock_list_servers() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };
            
            let servers = wrapper.list_servers("localhost").await.unwrap();
            assert_eq!(servers, vec!["Mock.Server.1", "Mock.Server.2"]);
        });
    }

    #[test]
    fn test_mock_browse_tags() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let wrapper = OpcDaWrapper {
                connector: std::sync::Arc::new(MockConnector),
            };
            
            let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let progress = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let tags = wrapper.browse_tags("Mock.Server.1", 1000, progress, sink).await.unwrap();
            
            assert_eq!(tags, vec!["MockTag.1", "MockTag.2"]);
        });
    }
}

