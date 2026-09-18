//! Batch abstraction for OPC DA tag write operations.

use crate::errors::OpcError;
use crate::types::OpcValue;
use std::sync::Arc;

/// Result of a single write operation.
///
/// # Examples
///
/// ```
/// use opc_da_client::{OpcError, WriteResult};
///
/// let ok_res = WriteResult::success("Tag1");
/// assert!(ok_res.is_success());
/// assert!(ok_res.status.is_ok());
/// assert!(ok_res.error().is_none());
///
/// let err_res = WriteResult::failure("Tag2", OpcError::Connection("Lost".into()));
/// assert!(err_res.is_error());
/// assert!(err_res.status.is_err());
/// assert_eq!(err_res.error(), Some(&OpcError::Connection("Lost".into())));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct WriteResult {
    /// The tag that was written to.
    pub tag_id: String,
    /// Outcome of the write operation: `Ok(())` on success, or `Err(OpcError)` on failure.
    pub status: Result<(), OpcError>,
}

impl WriteResult {
    /// Creates a successful write result.
    #[must_use]
    pub fn success(tag_id: impl Into<String>) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Ok(()),
        }
    }

    /// Creates a failed write result with a domain error.
    #[must_use]
    pub fn failure(tag_id: impl Into<String>, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Err(error),
        }
    }

    /// Returns `true` if the write succeeded.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status.is_ok()
    }

    /// Returns `true` if the write failed.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status.is_err()
    }

    /// Returns the error if the write failed, or `None` if it succeeded.
    #[must_use]
    pub fn error(&self) -> Option<&OpcError> {
        self.status.as_ref().err()
    }

    /// Returns `true` if the write operation failed with a transport or connection-level error.
    ///
    /// # Details
    ///
    /// Checks whether the contained error (if any) represents a connection-related failure,
    /// such as server disconnection, RPC failure, or dead connection handle. Returns `false`
    /// if the write succeeded or failed due to an item-level, configuration, or data validation error.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let conn_err = WriteResult::failure("Tag1", OpcError::Connection("Lost".into()));
    /// assert!(conn_err.is_connection_error());
    ///
    /// let state_err = WriteResult::failure("Tag2", OpcError::InvalidState("Bad tag".into()));
    /// assert!(!state_err.is_connection_error());
    ///
    /// let ok_res = WriteResult::success("Tag3");
    /// assert!(!ok_res.is_connection_error());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        self.error().is_some_and(OpcError::is_connection_error)
    }
}

impl std::fmt::Display for WriteResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.status {
            Ok(()) => write!(f, "Write '{}': succeeded", self.tag_id),
            Err(e) => write!(f, "Write '{}': failed ({e})", self.tag_id),
        }
    }
}

/// Represents a batch of OPC tag write requests (tag name and target value),
/// enabling zero-allocation conversion and cross-thread dispatch.
///
/// # Examples
///
/// ```
/// use opc_da_client::types::{IntoWriteBatch, OpcValue, WriteBatch};
///
/// let batch = ("Tag1", OpcValue::Int(42)).into_write_batch();
/// assert_eq!(batch.len(), 1);
/// assert!(!batch.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum WriteBatch {
    /// Single owned tag name and value.
    Single(String, OpcValue),
    /// Shared reference-counted slice of tag writes.
    Shared(Arc<[(String, OpcValue)]>),
    /// Owned vector of tag writes.
    Owned(Vec<(String, OpcValue)>),
}

impl WriteBatch {
    /// Creates an empty write batch.
    ///
    /// # Returns
    ///
    /// An empty `WriteBatch::Owned` variant.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let batch = WriteBatch::empty();
    /// assert!(batch.is_empty());
    /// ```
    #[must_use]
    pub const fn empty() -> Self {
        Self::Owned(Vec::new())
    }

    /// Returns the number of items in this batch.
    ///
    /// # Returns
    ///
    /// The number of tag write operations in this batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::{IntoWriteBatch, OpcValue};
    ///
    /// let batch = [("Tag1", OpcValue::Int(1)), ("Tag2", OpcValue::Int(2))].into_write_batch();
    /// assert_eq!(batch.len(), 2);
    /// ```
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Single(_, _) => 1,
            Self::Shared(slice) => slice.len(),
            Self::Owned(vec) => vec.len(),
        }
    }

    /// Returns `true` if this write batch contains no items.
    ///
    /// # Returns
    ///
    /// `true` if the batch length is 0; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// assert!(WriteBatch::empty().is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a borrowed slice of items if backed by a contiguous heap allocation (`Owned` or `Shared`).
    #[must_use]
    pub fn as_slice(&self) -> Option<&[(String, OpcValue)]> {
        match self {
            Self::Owned(vec) => Some(vec.as_slice()),
            Self::Shared(slice) => Some(slice),
            Self::Single(_, _) => None,
        }
    }

    /// Converts this batch into an efficiently cloneable representation.
    ///
    /// Ensures repeated clones (such as across thread or channel boundaries)
    /// avoid vector re-allocations by converting `Owned` into `Shared` (`Arc<[...]>`).
    ///
    /// # Returns
    ///
    /// A shareable `WriteBatch` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::{IntoWriteBatch, OpcValue};
    ///
    /// let batch = vec![("Tag1".to_string(), OpcValue::Int(10))].into_write_batch();
    /// let shareable = batch.into_shareable();
    /// assert_eq!(shareable.len(), 1);
    /// ```
    #[must_use]
    pub fn into_shareable(self) -> Self {
        match self {
            Self::Single(tag, val) => Self::Shared(Arc::from([(tag, val)])),
            Self::Shared(slice) => Self::Shared(slice),
            Self::Owned(vec) => Self::Shared(Arc::from(vec.into_boxed_slice())),
        }
    }

    /// Returns an iterator over borrowed string slices and values in this batch.
    ///
    /// # Returns
    ///
    /// A [`WriteBatchIter`] yielding `(&str, &OpcValue)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::{IntoWriteBatch, OpcValue};
    ///
    /// let batch = ("Tag1", OpcValue::Int(42)).into_write_batch();
    /// let mut iter = batch.iter();
    /// assert_eq!(iter.next(), Some(("Tag1", &OpcValue::Int(42))));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[must_use]
    pub fn iter(&self) -> WriteBatchIter<'_> {
        match self {
            Self::Single(tag, val) => WriteBatchIter::Single(Some((tag.as_str(), val))),
            Self::Shared(slice) => WriteBatchIter::Slice(slice.iter()),
            Self::Owned(vec) => WriteBatchIter::Slice(vec.iter()),
        }
    }
}

impl Default for WriteBatch {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

/// Zero-allocation borrowed iterator for [`WriteBatch`].
#[derive(Debug, Clone)]
pub enum WriteBatchIter<'a> {
    /// Single-item iterator.
    Single(Option<(&'a str, &'a OpcValue)>),
    /// Slice-based iterator over `(String, OpcValue)` pairs.
    Slice(std::slice::Iter<'a, (String, OpcValue)>),
}

impl<'a> Iterator for WriteBatchIter<'a> {
    type Item = (&'a str, &'a OpcValue);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(item) => item.take(),
            Self::Slice(iter) => iter.next().map(|(tag, val)| (tag.as_str(), val)),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Single(opt) => {
                let len = usize::from(opt.is_some());
                (len, Some(len))
            }
            Self::Slice(iter) => iter.size_hint(),
        }
    }
}

impl ExactSizeIterator for WriteBatchIter<'_> {}

impl<'a> IntoIterator for &'a WriteBatch {
    type Item = (&'a str, &'a OpcValue);
    type IntoIter = WriteBatchIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl IntoIterator for WriteBatch {
    type Item = (String, OpcValue);
    type IntoIter = WriteBatchIntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Single(tag, val) => WriteBatchIntoIter::Single(Some((tag, val))),
            Self::Shared(slice) => WriteBatchIntoIter::Shared(slice, 0),
            Self::Owned(vec) => WriteBatchIntoIter::Owned(vec.into_iter()),
        }
    }
}

/// Consuming iterator for [`WriteBatch`].
#[derive(Debug)]
pub enum WriteBatchIntoIter {
    /// Single item.
    Single(Option<(String, OpcValue)>),
    /// Shared slice item indexing.
    Shared(Arc<[(String, OpcValue)]>, usize),
    /// Owned vector iterator.
    Owned(std::vec::IntoIter<(String, OpcValue)>),
}

impl Iterator for WriteBatchIntoIter {
    type Item = (String, OpcValue);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(opt) => opt.take(),
            Self::Shared(slice, idx) => {
                if *idx < slice.len() {
                    let item = slice[*idx].clone();
                    *idx += 1;
                    Some(item)
                } else {
                    None
                }
            }
            Self::Owned(iter) => iter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Single(opt) => {
                let len = usize::from(opt.is_some());
                (len, Some(len))
            }
            Self::Shared(slice, idx) => {
                let len = slice.len().saturating_sub(*idx);
                (len, Some(len))
            }
            Self::Owned(iter) => iter.size_hint(),
        }
    }
}

impl ExactSizeIterator for WriteBatchIntoIter {}

impl<S: Into<String>, V: Into<OpcValue>> From<(S, V)> for WriteBatch {
    #[inline]
    fn from((tag, val): (S, V)) -> Self {
        Self::Single(tag.into(), val.into())
    }
}

impl<S: Into<String>, V: Into<OpcValue>, const N: usize> From<[(S, V); N]> for WriteBatch {
    #[inline]
    fn from(arr: [(S, V); N]) -> Self {
        Self::Owned(arr.into_iter().map(|(t, v)| (t.into(), v.into())).collect())
    }
}

impl<S: AsRef<str>, V: Clone + Into<OpcValue>, const N: usize> From<&[(S, V); N]> for WriteBatch {
    #[inline]
    fn from(arr: &[(S, V); N]) -> Self {
        Self::from(&arr[..])
    }
}

impl<S: Into<String>, V: Into<OpcValue>> From<Vec<(S, V)>> for WriteBatch {
    #[inline]
    fn from(vec: Vec<(S, V)>) -> Self {
        Self::Owned(vec.into_iter().map(|(t, v)| (t.into(), v.into())).collect())
    }
}

impl<S: AsRef<str>, V: Clone + Into<OpcValue>> From<&[(S, V)]> for WriteBatch {
    #[inline]
    fn from(slice: &[(S, V)]) -> Self {
        Self::Owned(
            slice
                .iter()
                .map(|(t, v)| (t.as_ref().to_owned(), v.clone().into()))
                .collect(),
        )
    }
}

/// Clones the write batch from a reference.
///
/// # Performance Note
///
/// * If `batch` is [`WriteBatch::Shared`], this is an O(1) reference-count increment (zero allocations).
/// * If `batch` is [`WriteBatch::Owned`], this performs an O(N) deep copy of all tag strings and values.
/// * If `batch` is [`WriteBatch::Single`], this clones the single tag String and OpcValue.
///
/// For repeated operations in loops, call [`.into_shareable()`](WriteBatch::into_shareable) once to ensure subsequent
/// reference conversions are O(1).
impl From<&Self> for WriteBatch {
    #[inline]
    fn from(batch: &Self) -> Self {
        batch.clone()
    }
}

impl From<Arc<[(String, OpcValue)]>> for WriteBatch {
    #[inline]
    fn from(arc: Arc<[(String, OpcValue)]>) -> Self {
        Self::Shared(arc)
    }
}

impl<S: Into<String>, V: Into<OpcValue>> FromIterator<(S, V)> for WriteBatch {
    #[inline]
    fn from_iter<T: IntoIterator<Item = (S, V)>>(iter: T) -> Self {
        Self::Owned(
            iter.into_iter()
                .map(|(tag, val)| (tag.into(), val.into()))
                .collect(),
        )
    }
}

/// Trait for types that can be converted into a [`WriteBatch`].
///
/// Enables flexible arguments for batch write methods, accepting single writes,
/// arrays, vectors, and shared slices.
///
/// # Examples
///
/// ```
/// use opc_da_client::types::{IntoWriteBatch, OpcValue};
///
/// let batch = ("Tag1", OpcValue::Int(42)).into_write_batch();
/// assert_eq!(batch.len(), 1);
/// ```
pub trait IntoWriteBatch {
    /// Converts this type into a [`WriteBatch`].
    ///
    /// # Returns
    ///
    /// A [`WriteBatch`] representing the write operations.
    fn into_write_batch(self) -> WriteBatch;
}

impl<T: Into<WriteBatch>> IntoWriteBatch for T {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        self.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_batch_single_and_iter() {
        let wb = ("Tag1", OpcValue::Int(10)).into_write_batch();
        assert_eq!(wb.len(), 1);
        assert!(!wb.is_empty());

        let items: Vec<(&str, &OpcValue)> = wb.iter().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "Tag1");
        assert_eq!(items[0].1, &OpcValue::Int(10));
    }

    #[test]
    fn test_write_batch_vec_and_into_iter() {
        let wb = vec![
            ("Tag1".to_string(), OpcValue::Int(1)),
            ("Tag2".to_string(), OpcValue::Int(2)),
        ]
        .into_write_batch();
        assert_eq!(wb.len(), 2);

        let items: Vec<(String, OpcValue)> = wb.into_iter().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "Tag1");
        assert_eq!(items[1].0, "Tag2");
    }

    #[test]
    fn test_write_batch_slice_and_from_iterator() {
        let items = vec![
            ("Tag1".to_string(), OpcValue::Int(1)),
            ("Tag2".to_string(), OpcValue::Int(2)),
        ];
        let batch: WriteBatch = items.clone().into_iter().collect();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.as_slice(), Some(items.as_slice()));

        let slices = [
            ("TagA", OpcValue::Float(1.5)),
            ("TagB", OpcValue::Float(2.5)),
        ];
        let batch_slices: WriteBatch = slices.into_iter().collect();
        assert_eq!(batch_slices.len(), 2);
        assert_eq!(batch_slices.as_slice().unwrap()[0].0, "TagA");
    }

    #[test]
    fn test_write_result_co_located() {
        use crate::errors::OpcError;

        let ok = WriteResult::success("Channel.Device.Tag1");
        assert!(ok.is_success());
        assert!(!ok.is_error());
        assert_eq!(ok.tag_id, "Channel.Device.Tag1");
        assert_eq!(ok.status, Ok(()));
        assert!(ok.error().is_none());

        let err = WriteResult::failure(
            "Channel.Device.Tag2",
            OpcError::Connection("Disconnected".into()),
        );
        assert!(err.is_error());
        assert!(!err.is_success());
        assert_eq!(err.tag_id, "Channel.Device.Tag2");
        assert_eq!(
            err.error(),
            Some(&OpcError::Connection("Disconnected".into()))
        );
    }

    #[test]
    fn test_generic_into_write_batch_conversions() {
        let arr_batch = [("Tag.1", 1.0f64), ("Tag.2", 2.0f64)].into_write_batch();
        assert_eq!(arr_batch.len(), 2);
        let items: Vec<(&str, &OpcValue)> = arr_batch.iter().collect();
        assert_eq!(items[0], ("Tag.1", &OpcValue::Float(1.0)));
        assert_eq!(items[1], ("Tag.2", &OpcValue::Float(2.0)));

        let ref_arr_batch = (&[("Tag.Ref1", 10i32), ("Tag.Ref2", 20i32)]).into_write_batch();
        assert_eq!(ref_arr_batch.len(), 2);

        let single_batch = ("Tag.Solo", 42i32).into_write_batch();
        assert_eq!(single_batch.len(), 1);
        let items: Vec<(&str, &OpcValue)> = single_batch.iter().collect();
        assert_eq!(items[0], ("Tag.Solo", &OpcValue::Int(42)));

        let vec_batch = vec![("Tag.Bool".to_string(), true)].into_write_batch();
        assert_eq!(vec_batch.len(), 1);
        let items: Vec<(&str, &OpcValue)> = vec_batch.iter().collect();
        assert_eq!(items[0], ("Tag.Bool", &OpcValue::Bool(true)));

        let slice_data = [("Tag.Str1", "running"), ("Tag.Str2", "stopped")];
        let slice_batch = (&slice_data[..]).into_write_batch();
        assert_eq!(slice_batch.len(), 2);
        let items: Vec<(&str, &OpcValue)> = slice_batch.iter().collect();
        assert_eq!(items[0], ("Tag.Str1", &OpcValue::String("running".into())));
        assert_eq!(items[1], ("Tag.Str2", &OpcValue::String("stopped".into())));

        let iter_batch: WriteBatch = [("Tag.Iter1", 999i32), ("Tag.Iter2", 1000i32)]
            .into_iter()
            .collect();
        assert_eq!(iter_batch.len(), 2);
    }

    #[test]
    fn test_into_write_batch_ref() {
        let single_orig = ("Motor.Speed", OpcValue::Int(1500)).into_write_batch();
        let single_ref_batch = (&single_orig).into_write_batch();
        assert_eq!(single_ref_batch, single_orig);
        let single_from_ref: WriteBatch = (&single_orig).into();
        assert_eq!(single_from_ref, single_orig);
        assert_eq!(single_orig.len(), 1);

        let owned_orig = vec![
            ("V1".to_string(), OpcValue::Bool(true)),
            ("V2".to_string(), OpcValue::Bool(false)),
        ]
        .into_write_batch();
        let owned_ref_batch = (&owned_orig).into_write_batch();
        assert_eq!(owned_ref_batch, owned_orig);
        assert_eq!(owned_orig.len(), 2);
    }

    #[test]
    fn test_into_write_batch_borrowed_value_tuples() {
        let val_int = OpcValue::Int(42);
        let val_float = OpcValue::Float(98.6);
        let val_str = OpcValue::String("Running".to_string());

        let tuple_slice: &[(&str, &OpcValue)] = &[
            ("Device.Motor.RPM", &val_int),
            ("Device.Motor.Temp", &val_float),
            ("Device.Motor.Status", &val_str),
        ];

        let batch = tuple_slice.into_write_batch();
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());

        let items: Vec<(&str, &OpcValue)> = batch.iter().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], ("Device.Motor.RPM", &val_int));
        assert_eq!(items[1], ("Device.Motor.Temp", &val_float));
        assert_eq!(items[2], ("Device.Motor.Status", &val_str));

        // Edge case: empty borrowed tuple slice
        let empty_tuples: &[(&str, &OpcValue)] = &[];
        let empty_batch = empty_tuples.into_write_batch();
        assert_eq!(empty_batch.len(), 0);
        assert!(empty_batch.is_empty());
    }

    #[test]
    fn test_write_batch_into_shareable_lifecycle() {
        // 1. Single variant → Shared with Arc lifecycle
        let single = WriteBatch::Single("Pump.Pressure".to_string(), OpcValue::Float(101.3));
        let shareable = single.into_shareable();

        match &shareable {
            WriteBatch::Shared(arc_slice) => {
                assert_eq!(arc_slice.len(), 1);
                assert_eq!(arc_slice[0].0, "Pump.Pressure");
                assert_eq!(arc_slice[0].1, OpcValue::Float(101.3));
                assert_eq!(Arc::strong_count(arc_slice), 1);

                let cloned = shareable.clone();
                assert_eq!(Arc::strong_count(arc_slice), 2);
                assert_eq!(cloned, shareable);

                drop(cloned);
                assert_eq!(Arc::strong_count(arc_slice), 1);
            }
            _ => panic!("Expected WriteBatch::Shared representation for shareable single"),
        }

        // 2. Owned variant → Shared
        let owned = vec![
            ("M1.Speed".to_string(), OpcValue::Int(1200)),
            ("M2.Speed".to_string(), OpcValue::Int(1500)),
        ]
        .into_write_batch();
        let shareable_owned = owned.into_shareable();
        match &shareable_owned {
            WriteBatch::Shared(arc_slice) => {
                assert_eq!(arc_slice.len(), 2);
                assert_eq!(Arc::strong_count(arc_slice), 1);
                let cloned = shareable_owned.clone();
                assert_eq!(Arc::strong_count(arc_slice), 2);
                drop(cloned);
            }
            _ => panic!("Expected WriteBatch::Shared for owned batch"),
        }

        // 3. Shared idempotency
        let already_shared = shareable_owned.clone().into_shareable();
        assert_eq!(already_shared, shareable_owned);

        // 4. Empty batch shareable
        let empty_shareable = WriteBatch::empty().into_shareable();
        assert_eq!(empty_shareable.len(), 0);
        assert!(empty_shareable.is_empty());
    }

    #[test]
    fn test_write_result_display_formatting() {
        let ok = WriteResult::success("Channel.Device.Tag1");
        assert_eq!(format!("{ok}"), "Write 'Channel.Device.Tag1': succeeded");

        let err = WriteResult::failure(
            "Channel.Device.Tag2",
            OpcError::InvalidState("Tag not found".into()),
        );
        assert_eq!(
            format!("{err}"),
            "Write 'Channel.Device.Tag2': failed (Invalid state: Tag not found)"
        );
    }

    #[test]
    fn test_write_result_is_connection_error() {
        let conn_err = WriteResult::failure(
            "Channel.Device.Tag1",
            OpcError::Connection("Server unreachable".into()),
        );
        assert!(conn_err.is_connection_error());

        let state_err = WriteResult::failure(
            "Channel.Device.Tag2",
            OpcError::InvalidState("Access denied".into()),
        );
        assert!(!state_err.is_connection_error());

        let ok_res = WriteResult::success("Channel.Device.Tag3");
        assert!(!ok_res.is_connection_error());
    }
}
