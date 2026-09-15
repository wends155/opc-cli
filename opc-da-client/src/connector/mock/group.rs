use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use crate::connector::mock::state::MockState;
use crate::connector::traits::{
    ConnectedGroup, DataSource, GroupItemDef, GroupItemResult, GroupItemState, ItemWrite,
};
use crate::errors::hresult::{E_FAIL, RPC_S_SERVER_UNAVAILABLE};
use crate::errors::{OpcError, OpcResult};
use crate::types::handles::{ClientItemHandle, ServerItemHandle};
use crate::types::quality::OpcQuality;
use crate::types::value::OpcValue;
use crate::types::vartype::VarType;

/// Callback signature for mocking item addition to a group.
pub type MockAddItemsFn =
    Box<dyn Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync>;

/// Callback signature for mocking item reads from a group.
pub type MockReadFn = Box<
    dyn Fn(DataSource, &[ServerItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
        + Send
        + Sync,
>;

/// Callback signature for mocking item writes to a group.
pub type MockWriteFn =
    Box<dyn Fn(&[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync>;

/// Pure-Rust mock implementation of [`ConnectedGroup`] for testing.
pub struct MockConnectedGroup {
    /// Shared failure injection state.
    pub state: Arc<MockState>,
    /// Custom callback overriding item addition behavior.
    pub add_items_fn: Arc<Mutex<Option<MockAddItemsFn>>>,
    /// Custom callback overriding read behavior.
    pub read_fn: Arc<Mutex<Option<MockReadFn>>>,
    /// Custom callback overriding write behavior.
    pub write_fn: Arc<Mutex<Option<MockWriteFn>>>,
    /// Configured simulated tag values returned by default read implementation.
    pub tag_values: Arc<Mutex<Vec<OpcValue>>>,
}

impl std::fmt::Debug for MockConnectedGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockConnectedGroup").finish_non_exhaustive()
    }
}

impl Default for MockConnectedGroup {
    fn default() -> Self {
        Self {
            state: Arc::new(MockState::default()),
            add_items_fn: Arc::new(Mutex::new(None)),
            read_fn: Arc::new(Mutex::new(None)),
            write_fn: Arc::new(Mutex::new(None)),
            tag_values: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[allow(dead_code)]
impl MockConnectedGroup {
    /// Creates a new `MockConnectedGroup` with default simulation settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `MockConnectedGroup` with the provided shared mock state.
    #[must_use]
    pub fn with_state(state: Arc<MockState>) -> Self {
        Self {
            state,
            ..Default::default()
        }
    }

    /// Overrides handler for adding items to the mock group.
    #[must_use]
    pub fn with_add_items_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync + 'static,
    {
        *self
            .add_items_fn
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(f));
        self
    }

    /// Overrides handler for reading items from the mock group.
    #[must_use]
    pub fn with_read_fn<F>(self, f: F) -> Self
    where
        F: Fn(DataSource, &[ServerItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
            + Send
            + Sync
            + 'static,
    {
        *self.read_fn.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(f));
        self
    }

    /// Overrides handler for writing items to the mock group.
    #[must_use]
    pub fn with_write_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync + 'static,
    {
        *self.write_fn.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(f));
        self
    }

    /// Overrides configured simulated tag values.
    #[must_use]
    pub fn with_tag_values(self, values: Vec<OpcValue>) -> Self {
        *self
            .tag_values
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = values;
        self
    }
}

impl ConnectedGroup for MockConnectedGroup {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

        self.state.add_items_count.fetch_add(1, Ordering::Relaxed);

        let guard = self
            .add_items_fn
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(ref f) = *guard {
            return f(items);
        }
        drop(guard);

        Ok(items
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let handle_val = u32::try_from(i + 1).unwrap_or(u32::MAX);
                GroupItemResult {
                    server_handle: ServerItemHandle::new(handle_val),
                    canonical_type: VarType::BSTR,
                    error: None,
                }
            })
            .collect())
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ServerItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        if server_handles.is_empty() {
            return Err(OpcError::InvalidState(
                "server_handles cannot be empty".to_string(),
            ));
        }

        self.state.read_count.fetch_add(1, Ordering::Relaxed);

        let guard = self.read_fn.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(ref f) = *guard {
            return f(source, server_handles);
        }
        drop(guard);

        let configured = self
            .tag_values
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(server_handles
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let val = configured.get(i).cloned().unwrap_or(OpcValue::Int(42));
                Ok(GroupItemState {
                    client_handle: ClientItemHandle::new(h.as_raw()),
                    value: val,
                    quality: OpcQuality::GOOD,
                    timestamp: std::time::SystemTime::UNIX_EPOCH,
                })
            })
            .collect())
    }

    fn write(&self, items: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

        if self.state.should_fail_connection.load(Ordering::Relaxed)
            || self
                .state
                .should_fail_with_connection_error
                .load(Ordering::Relaxed)
        {
            return Err(OpcError::Com {
                source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            });
        }

        if self.state.should_fail_write.load(Ordering::Relaxed) {
            return Ok(items
                .iter()
                .map(|_| {
                    Err(OpcError::Com {
                        source: windows_core::Error::from_hresult(E_FAIL),
                    })
                })
                .collect());
        }

        let guard = self.write_fn.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(ref f) = *guard {
            return f(items);
        }
        drop(guard);

        Ok(items.iter().map(|_| Ok(())).collect())
    }
}

impl ConnectedGroup for Arc<MockConnectedGroup> {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        (**self).add_items(items)
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ServerItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        (**self).read(source, server_handles)
    }

    fn write(&self, items: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> {
        (**self).write(items)
    }
}
