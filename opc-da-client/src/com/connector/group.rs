//! Win32 COM group management, item registration, and synchronous I/O.
//!
//! Provides [`ComGroup`] implementing [`ConnectedGroup`] with leak-free
//! `ScopedVariant` and `ItemStatesGuard` resource management.

use crate::com::connector::traits::{
    ConnectedGroup, DataSource, GroupItemDef, GroupItemResult, GroupItemState,
};
use crate::com::variant::{ItemStatesGuard, ScopedVariant};
use crate::errors::{OpcError, OpcResult};
use crate::provider::{OpcQuality, OpcValue};
use crate::raw::memory::{LocalPointer, RemoteArray};
use crate::types::ItemHandle;
use windows::core::Interface;

/// COM-backed [`ConnectedGroup`].
#[allow(dead_code)]
pub struct ComGroup {
    pub(crate) item_mgt: crate::raw::bindings::da::IOPCItemMgt,
    pub(crate) group_state_mgt: crate::raw::bindings::da::IOPCGroupStateMgt,
    pub(crate) public_group_state_mgt: Option<crate::raw::bindings::da::IOPCPublicGroupStateMgt>,
    pub(crate) sync_io: crate::raw::bindings::da::IOPCSyncIO,
    pub(crate) async_io: Option<crate::raw::bindings::da::IOPCAsyncIO>,
    pub(crate) async_io2: crate::raw::bindings::da::IOPCAsyncIO2,
    pub(crate) connection_point_container: windows::Win32::System::Com::IConnectionPointContainer,
    pub(crate) data_object: Option<windows::Win32::System::Com::IDataObject>,
}

impl ConnectedGroup for ComGroup {
    #[tracing::instrument(level = "debug", skip(self, items), err)]
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

        let len = items.len().try_into()?;
        tracing::debug!(
            item_count = len,
            "Adding items to OPC group natively via IOPCItemMgt"
        );

        let wide_names: Vec<LocalPointer<Vec<u16>>> = items
            .iter()
            .map(|item| LocalPointer::from(&item.item_id))
            .collect();

        let mut item_defs: Vec<crate::raw::bindings::da::tagOPCITEMDEF> =
            Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            item_defs.push(crate::raw::bindings::da::tagOPCITEMDEF {
                szAccessPath: windows::core::PWSTR::null(),
                szItemID: wide_names[i].as_pwstr(),
                bActive: item.active.into(),
                hClient: item.client_handle.0,
                vtRequestedDataType: 0,
                dwBlobSize: 0,
                pBlob: std::ptr::null_mut(),
                wReserved: 0,
            });
        }

        let mut results = RemoteArray::new(len);
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method AddItems with valid item definition array and output buffers.
        unsafe {
            self.item_mgt.AddItems(
                len,
                item_defs.as_ptr(),
                results.as_mut_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        let results_slice = results.as_slice();
        let errors_slice = errors.as_slice();
        let mut group_results = Vec::with_capacity(items.len());

        for i in 0..items.len() {
            let err = if errors_slice[i].is_ok() {
                None
            } else {
                Some(OpcError::Com {
                    source: windows::core::Error::from_hresult(errors_slice[i]),
                })
            };
            group_results.push(GroupItemResult {
                server_handle: ItemHandle(results_slice[i].hServer),
                canonical_type: results_slice[i].vtCanonicalDataType,
                error: err,
            });
        }

        Ok(group_results)
    }

    #[tracing::instrument(level = "debug", skip(self, server_handles), err)]
    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        if server_handles.is_empty() {
            return Err(OpcError::InvalidState(
                "server_handles cannot be empty".to_string(),
            ));
        }

        let len = server_handles.len().try_into()?;
        let native_source = match source {
            DataSource::Cache => crate::raw::bindings::da::OPC_DS_CACHE,
            DataSource::Device => crate::raw::bindings::da::OPC_DS_DEVICE,
        };

        let mut item_values = RemoteArray::new(len);
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM interface method Read with valid server handle array and output buffers.
        unsafe {
            self.sync_io.Read(
                native_source,
                len,
                server_handles.as_ptr().cast(),
                item_values.as_mut_ptr(),
                errors.as_mut_ptr(),
            )?;
        }

        // RAII guard ensures VariantClear is invoked on all item states before RemoteArray frees memory.
        let guard = ItemStatesGuard(item_values.as_mut_slice());

        let errors_slice = errors.as_slice();
        let mut states = Vec::with_capacity(server_handles.len());

        for i in 0..server_handles.len() {
            let err = errors_slice[i];
            if err.is_ok() {
                let state = &guard[i];
                let value = crate::com::variant::variant_to_opc_value(&state.vDataValue);
                let quality = OpcQuality::from(state.wQuality);
                let timestamp =
                    crate::raw::memory::TryFromNative::try_from_native(&state.ftTimeStamp)
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                states.push(Ok(GroupItemState {
                    client_handle: ItemHandle(state.hClient),
                    value,
                    quality,
                    timestamp,
                }));
            } else {
                states.push(Err(OpcError::Com {
                    source: windows::core::Error::from_hresult(err),
                }));
            }
        }

        Ok(states)
    }

    #[tracing::instrument(level = "debug", skip(self, server_handles, values), err)]
    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        if server_handles.len() != values.len() {
            return Err(OpcError::InvalidState(
                "server_handles and values must have the same length".to_string(),
            ));
        }

        let len = server_handles.len().try_into()?;
        let variants: Vec<ScopedVariant> =
            values.iter().map(ScopedVariant::from_opc_value).collect();
        let mut errors = RemoteArray::new(len);

        // SAFETY: Calling COM Write with valid server handle and transparent ScopedVariant array (Drop calls VariantClear).
        unsafe {
            self.sync_io.Write(
                len,
                server_handles.as_ptr().cast(),
                variants.as_ptr().cast(),
                errors.as_mut_ptr(),
            )?;
        }

        let errors_slice = errors.as_slice();
        let results = errors_slice
            .iter()
            .map(|&hr| {
                if hr.is_ok() {
                    Ok(())
                } else {
                    Err(OpcError::Com {
                        source: windows::core::Error::from_hresult(hr),
                    })
                }
            })
            .collect();

        Ok(results)
    }
}

impl TryFrom<windows::core::IUnknown> for ComGroup {
    type Error = windows::core::Error;

    fn try_from(unknown: windows::core::IUnknown) -> Result<Self, Self::Error> {
        Ok(Self {
            item_mgt: unknown.cast()?,
            group_state_mgt: unknown.cast()?,
            public_group_state_mgt: unknown.cast().ok(),
            sync_io: unknown.cast()?,
            async_io: unknown.cast().ok(),
            async_io2: unknown.cast()?,
            connection_point_container: unknown.cast()?,
            data_object: unknown.cast().ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn dummy_iface<T: windows::core::Interface>() -> T {
        struct DummyObject {
            _vtable: &'static [usize; 32],
        }
        static DUMMY_VTABLE: [usize; 32] = [0; 32];
        static DUMMY_OBJ: DummyObject = DummyObject {
            _vtable: &DUMMY_VTABLE,
        };
        // SAFETY: Pointer is non-null and points to a static dummy object.
        unsafe { windows::core::Interface::from_raw((&raw const DUMMY_OBJ).cast_mut().cast()) }
    }

    #[test]
    fn test_com_group_preconditions() {
        // SAFETY: Dummy interface pointers wrapped in ManuallyDrop are never dropped,
        // avoiding calling Release on synthetic COM pointers while satisfying NonNull invariants.
        let group = std::mem::ManuallyDrop::new(unsafe {
            ComGroup {
                item_mgt: dummy_iface(),
                group_state_mgt: dummy_iface(),
                public_group_state_mgt: None,
                sync_io: dummy_iface(),
                async_io: None,
                async_io2: dummy_iface(),
                connection_point_container: dummy_iface(),
                data_object: None,
            }
        });

        // Test empty add_items returns InvalidState
        assert!(matches!(
            group.add_items(&[]),
            Err(OpcError::InvalidState(_))
        ));

        // Test empty read returns InvalidState
        assert!(matches!(
            group.read(DataSource::Device, &[]),
            Err(OpcError::InvalidState(_))
        ));

        // Test mismatched write lengths returns InvalidState
        assert!(matches!(
            group.write(&[ItemHandle(1)], &[]),
            Err(OpcError::InvalidState(_))
        ));
    }
}
