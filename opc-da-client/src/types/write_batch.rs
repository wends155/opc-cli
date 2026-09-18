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
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::WriteResult;
    ///
    /// let res = WriteResult::success("Tag1");
    /// assert!(res.is_success());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn success(tag_id: impl Into<String>) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Ok(()),
        }
    }

    /// Creates a failed write result with a domain error.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let res = WriteResult::failure("Tag1", OpcError::Internal("Disk full".into()));
    /// assert!(res.is_error());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn failure(tag_id: impl Into<String>, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Err(error),
        }
    }

    /// Returns `true` if the write succeeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::WriteResult;
    ///
    /// let res = WriteResult::success("Tag1");
    /// assert!(res.is_success());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status.is_ok()
    }

    /// Returns `true` if the write failed.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let res = WriteResult::failure("Tag1", OpcError::Internal("Timeout".into()));
    /// assert!(res.is_error());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status.is_err()
    }

    /// Returns the error if the write failed, or `None` if it succeeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let ok_res = WriteResult::success("Tag1");
    /// assert_eq!(ok_res.error(), None);
    ///
    /// let err_res = WriteResult::failure("Tag2", OpcError::Internal("Timeout".into()));
    /// assert_eq!(err_res.error(), Some(&OpcError::Internal("Timeout".into())));
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
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
/// # Layout & Performance
///
/// `WriteBatch` is an opaque type wrapping an internal 5-variant representation:
/// - `StaticSingle`: Zero-allocation compile-time string literal and target value.
/// - `InlineSingle`: Stack Small String Optimization (SSO) for tags up to 31 bytes.
/// - `OwnedSingle`: Heap-allocated single tag for tags exceeding 31 bytes.
/// - `Shared`: Reference-counted slice `Arc<[(String, OpcValue)]>` for shared dispatches.
/// - `Owned`: Move-only `Vec<(String, OpcValue)>` for dynamic collections.
///
/// Total size on x86_64 is exactly 72 bytes (align 8, 0 internal padding bytes).
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
#[derive(Debug, Clone)]
pub struct WriteBatch {
    pub(crate) repr: WriteBatchRepr,
}

#[derive(Debug, Clone)]
pub(crate) enum WriteBatchRepr {
    StaticSingle(&'static str, OpcValue),
    InlineSingle([u8; 31], u8, OpcValue),
    OwnedSingle(String, OpcValue),
    Shared(Arc<[(String, OpcValue)]>),
    Owned(Vec<(String, OpcValue)>),
}

impl WriteBatch {
    /// Creates an empty write batch.
    ///
    /// # Returns
    ///
    /// An empty `WriteBatch` backed by `WriteBatchRepr::Owned`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let batch = WriteBatch::empty();
    /// assert!(batch.is_empty());
    /// assert_eq!(batch.len(), 0);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            repr: WriteBatchRepr::Owned(Vec::new()),
        }
    }

    /// Creates a single-tag write batch from a compile-time static string literal.
    ///
    /// # Parameters
    ///
    /// - `tag`: A `'static` string slice representing the tag name.
    /// - `val`: The target value, convertible into [`OpcValue`].
    ///
    /// # Performance
    ///
    /// Performs zero heap allocations.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let batch = WriteBatch::from_static("Channel1.Device1.StaticTag", 100);
    /// assert_eq!(batch.len(), 1);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn from_static(tag: &'static str, val: impl Into<OpcValue>) -> Self {
        Self {
            repr: WriteBatchRepr::StaticSingle(tag, val.into()),
        }
    }

    /// Creates a single-tag write batch from an arbitrary string slice, applying
    /// stack Small String Optimization (SSO) for tags up to 31 bytes.
    ///
    /// If `tag.len() <= 31`, the tag bytes are copied into an inline stack buffer
    /// without heap allocation (`InlineSingle`). If `tag.len() > 31`, it spills over
    /// cleanly into an owned heap string (`OwnedSingle`) without truncation or codepoint tearing.
    ///
    /// # Security (CWE-20 / CWE-787)
    ///
    /// Never tears multibyte UTF-8 codepoints because tags exceeding 31 bytes are not sliced;
    /// they are allocated as full strings.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let sso_batch = WriteBatch::from_str_lenient("ShortTag", 42);
    /// assert_eq!(sso_batch.len(), 1);
    ///
    /// let long_name = "VeryLongTagNameToExceedTheThirtyOneByteStackLimitHere";
    /// let heap_batch = WriteBatch::from_str_lenient(long_name, 42);
    /// assert_eq!(heap_batch.len(), 1);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn from_str_lenient(tag: &str, val: impl Into<OpcValue>) -> Self {
        let opc_val = val.into();
        if tag.len() <= 31 {
            let mut buf = [0u8; 31];
            buf[..tag.len()].copy_from_slice(tag.as_bytes());
            #[allow(clippy::cast_possible_truncation)]
            Self {
                repr: WriteBatchRepr::InlineSingle(buf, tag.len() as u8, opc_val),
            }
        } else {
            Self {
                repr: WriteBatchRepr::OwnedSingle(tag.to_string(), opc_val),
            }
        }
    }

    /// Recovers a borrowed `&str` from an inline buffer with boundary clamping
    /// and UTF-8 recovery.
    #[inline]
    pub(crate) fn inline_as_str(buf: &[u8; 31], len: u8) -> &str {
        let valid_len = (len as usize).min(31);
        let slice = &buf[..valid_len];
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(e) => core::str::from_utf8(&slice[..e.valid_up_to()]).unwrap_or(""),
        }
    }

    /// Returns the number of write requests in the batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let empty = WriteBatch::empty();
    /// assert_eq!(empty.len(), 0);
    ///
    /// let single = WriteBatch::from_static("Tag1", 10);
    /// assert_eq!(single.len(), 1);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.repr {
            WriteBatchRepr::StaticSingle(_, _)
            | WriteBatchRepr::InlineSingle(_, _, _)
            | WriteBatchRepr::OwnedSingle(_, _) => 1,
            WriteBatchRepr::Shared(slice) => slice.len(),
            WriteBatchRepr::Owned(vec) => vec.len(),
        }
    }

    /// Returns `true` if the batch contains no write requests.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// assert!(WriteBatch::empty().is_empty());
    /// assert!(!WriteBatch::from_static("Tag1", 10).is_empty());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a slice of write requests if the batch is backed by a contiguous heap allocation.
    ///
    /// Returns `Some(&[(String, OpcValue)])` for `Shared` and `Owned` variants,
    /// or `None` for single-tag scalar variants (`StaticSingle`, `InlineSingle`, `OwnedSingle`).
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::{OpcValue, WriteBatch};
    ///
    /// let single = WriteBatch::from_static("Tag1", 10);
    /// assert_eq!(single.as_slice(), None);
    ///
    /// let vec_batch: WriteBatch = vec![("Tag1".to_string(), OpcValue::Int(10))].into_iter().collect();
    /// assert!(vec_batch.as_slice().is_some());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn as_slice(&self) -> Option<&[(String, OpcValue)]> {
        match &self.repr {
            WriteBatchRepr::StaticSingle(_, _)
            | WriteBatchRepr::InlineSingle(_, _, _)
            | WriteBatchRepr::OwnedSingle(_, _) => None,
            WriteBatchRepr::Shared(slice) => Some(slice),
            WriteBatchRepr::Owned(vec) => Some(vec.as_slice()),
        }
    }

    /// Promotes the batch into a shareable representation.
    ///
    /// Retains `StaticSingle` and `InlineSingle` on the stack (zero heap allocation, zero atomic refcount increments).
    /// Promotes `OwnedSingle` and `Owned` to `Shared(Arc<[(String, OpcValue)]>)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let batch = WriteBatch::from_static("Tag1", 10).into_shareable();
    /// assert_eq!(batch.len(), 1);
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn into_shareable(self) -> Self {
        match self.repr {
            WriteBatchRepr::StaticSingle(tag, val) => Self {
                repr: WriteBatchRepr::StaticSingle(tag, val),
            },
            WriteBatchRepr::InlineSingle(buf, len, val) => Self {
                repr: WriteBatchRepr::InlineSingle(buf, len, val),
            },
            WriteBatchRepr::Shared(slice) => Self {
                repr: WriteBatchRepr::Shared(slice),
            },
            WriteBatchRepr::OwnedSingle(tag, val) => Self {
                repr: WriteBatchRepr::Shared(Arc::from([(tag, val)])),
            },
            WriteBatchRepr::Owned(vec) => Self {
                repr: WriteBatchRepr::Shared(Arc::from(vec.into_boxed_slice())),
            },
        }
    }

    /// Consumes the batch and returns a vector of tag-value pairs.
    ///
    /// For `Owned`, this is a zero-reallocation move.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::WriteBatch;
    ///
    /// let batch = WriteBatch::from_static("Tag1", 10);
    /// let vec = batch.into_vec();
    /// assert_eq!(vec.len(), 1);
    /// assert_eq!(vec[0].0, "Tag1");
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn into_vec(self) -> Vec<(String, OpcValue)> {
        match self.repr {
            WriteBatchRepr::StaticSingle(tag, val) => vec![(tag.to_string(), val)],
            WriteBatchRepr::InlineSingle(buf, len, val) => {
                vec![(Self::inline_as_str(&buf, len).to_string(), val)]
            }
            WriteBatchRepr::OwnedSingle(tag, val) => vec![(tag, val)],
            WriteBatchRepr::Shared(slice) => slice.to_vec(),
            WriteBatchRepr::Owned(vec) => vec,
        }
    }

    /// Returns an iterator over borrowed tag-value pairs `(&str, &OpcValue)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::{OpcValue, WriteBatch};
    ///
    /// let batch = WriteBatch::from_static("Tag1", 10);
    /// for (tag, val) in batch.iter() {
    ///     assert_eq!(tag, "Tag1");
    ///     assert_eq!(val, &OpcValue::Int(10));
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn iter(&self) -> WriteBatchIter<'_> {
        let inner = match &self.repr {
            WriteBatchRepr::StaticSingle(tag, val) => {
                WriteBatchIterInner::Single(Some((*tag, val)))
            }
            WriteBatchRepr::InlineSingle(buf, len, val) => {
                let tag = Self::inline_as_str(buf, *len);
                WriteBatchIterInner::Single(Some((tag, val)))
            }
            WriteBatchRepr::OwnedSingle(tag, val) => {
                WriteBatchIterInner::Single(Some((tag.as_str(), val)))
            }
            WriteBatchRepr::Shared(slice) => WriteBatchIterInner::Slice(slice.iter()),
            WriteBatchRepr::Owned(vec) => WriteBatchIterInner::Slice(vec.as_slice().iter()),
        };
        WriteBatchIter { inner }
    }
}

impl Default for WriteBatch {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

/// Borrowed iterator over `(&'a str, &'a OpcValue)` pairs in a [`WriteBatch`].
#[derive(Debug, Clone)]
pub struct WriteBatchIter<'a> {
    inner: WriteBatchIterInner<'a>,
}

#[derive(Debug, Clone)]
enum WriteBatchIterInner<'a> {
    Single(Option<(&'a str, &'a OpcValue)>),
    Slice(std::slice::Iter<'a, (String, OpcValue)>),
}

impl<'a> Iterator for WriteBatchIter<'a> {
    type Item = (&'a str, &'a OpcValue);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            WriteBatchIterInner::Single(opt) => opt.take(),
            WriteBatchIterInner::Slice(iter) => iter.next().map(|(s, v)| (s.as_str(), v)),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for WriteBatchIter<'_> {
    #[inline]
    fn len(&self) -> usize {
        match &self.inner {
            WriteBatchIterInner::Single(opt) => usize::from(opt.is_some()),
            WriteBatchIterInner::Slice(iter) => iter.len(),
        }
    }
}

impl std::iter::FusedIterator for WriteBatchIter<'_> {}

impl<'a> IntoIterator for &'a WriteBatch {
    type Item = (&'a str, &'a OpcValue);
    type IntoIter = WriteBatchIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Consuming iterator over owned `(String, OpcValue)` pairs from a [`WriteBatch`].
#[derive(Debug)]
pub struct WriteBatchIntoIter {
    inner: WriteBatchIntoIterInner,
}

#[derive(Debug)]
enum WriteBatchIntoIterInner {
    Single(Option<(String, OpcValue)>),
    Shared {
        arc: Arc<[(String, OpcValue)]>,
        idx: usize,
    },
    Owned(std::vec::IntoIter<(String, OpcValue)>),
}

impl Iterator for WriteBatchIntoIter {
    type Item = (String, OpcValue);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            WriteBatchIntoIterInner::Single(opt) => opt.take(),
            WriteBatchIntoIterInner::Shared { arc, idx } => {
                if *idx < arc.len() {
                    let item = arc[*idx].clone();
                    *idx += 1;
                    Some(item)
                } else {
                    None
                }
            }
            WriteBatchIntoIterInner::Owned(iter) => iter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for WriteBatchIntoIter {
    #[inline]
    fn len(&self) -> usize {
        match &self.inner {
            WriteBatchIntoIterInner::Single(opt) => usize::from(opt.is_some()),
            WriteBatchIntoIterInner::Shared { arc, idx } => arc.len().saturating_sub(*idx),
            WriteBatchIntoIterInner::Owned(iter) => iter.len(),
        }
    }
}

impl std::iter::FusedIterator for WriteBatchIntoIter {}

impl IntoIterator for WriteBatch {
    type Item = (String, OpcValue);
    type IntoIter = WriteBatchIntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let inner = match self.repr {
            WriteBatchRepr::StaticSingle(tag, val) => {
                WriteBatchIntoIterInner::Single(Some((tag.to_string(), val)))
            }
            WriteBatchRepr::InlineSingle(buf, len, val) => {
                let tag = Self::inline_as_str(&buf, len).to_string();
                WriteBatchIntoIterInner::Single(Some((tag, val)))
            }
            WriteBatchRepr::OwnedSingle(tag, val) => {
                WriteBatchIntoIterInner::Single(Some((tag, val)))
            }
            WriteBatchRepr::Shared(arc) => WriteBatchIntoIterInner::Shared { arc, idx: 0 },
            WriteBatchRepr::Owned(vec) => WriteBatchIntoIterInner::Owned(vec.into_iter()),
        };
        WriteBatchIntoIter { inner }
    }
}

impl PartialEq for WriteBatch {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<V: Into<OpcValue>> From<(&'static str, V)> for WriteBatch {
    #[inline]
    fn from((tag, val): (&'static str, V)) -> Self {
        Self::from_static(tag, val)
    }
}

impl<V: Into<OpcValue>> From<(String, V)> for WriteBatch {
    #[inline]
    fn from((tag, val): (String, V)) -> Self {
        Self {
            repr: WriteBatchRepr::OwnedSingle(tag, val.into()),
        }
    }
}

impl From<Vec<(String, OpcValue)>> for WriteBatch {
    #[inline]
    fn from(vec: Vec<(String, OpcValue)>) -> Self {
        Self {
            repr: WriteBatchRepr::Owned(vec),
        }
    }
}

impl From<Arc<[(String, OpcValue)]>> for WriteBatch {
    #[inline]
    fn from(arc: Arc<[(String, OpcValue)]>) -> Self {
        Self {
            repr: WriteBatchRepr::Shared(arc),
        }
    }
}

impl From<&Self> for WriteBatch {
    #[inline]
    fn from(batch: &Self) -> Self {
        batch.clone()
    }
}

impl<S: Into<String>, V: Into<OpcValue>> FromIterator<(S, V)> for WriteBatch {
    #[inline]
    fn from_iter<I: IntoIterator<Item = (S, V)>>(iter: I) -> Self {
        let vec: Vec<(String, OpcValue)> = iter
            .into_iter()
            .map(|(s, v)| (s.into(), v.into()))
            .collect();
        Self {
            repr: WriteBatchRepr::Owned(vec),
        }
    }
}

/// Trait for types that can be converted into a [`WriteBatch`].
///
/// Implemented for `WriteBatch`, `&WriteBatch`, `Arc<[(String, OpcValue)]>`,
/// tuples `(&'a str, V)` and `(String, V)`, vectors, and fixed-size/borrowed
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
pub trait IntoWriteBatch: Send {
    /// Converts this type into a [`WriteBatch`].
    ///
    /// # Returns
    ///
    /// A [`WriteBatch`] representing the write operations.
    fn into_write_batch(self) -> WriteBatch;
}

impl IntoWriteBatch for WriteBatch {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        self
    }
}

impl IntoWriteBatch for &WriteBatch {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        self.clone()
    }
}

impl IntoWriteBatch for Arc<[(String, OpcValue)]> {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        WriteBatch::from(self)
    }
}

impl<V: Into<OpcValue> + Send> IntoWriteBatch for (&str, V) {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        WriteBatch::from_str_lenient(self.0, self.1)
    }
}

impl<V: Into<OpcValue> + Send> IntoWriteBatch for (String, V) {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        WriteBatch::from((self.0, self.1))
    }
}

impl<S: Into<String> + Send, V: Into<OpcValue> + Send> IntoWriteBatch for Vec<(S, V)> {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        self.into_iter().collect()
    }
}

impl<S: Into<String> + Send, V: Into<OpcValue> + Send, const N: usize> IntoWriteBatch
    for [(S, V); N]
{
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        self.into_iter().collect()
    }
}

impl<S: AsRef<str> + Sync, V: Clone + Into<OpcValue> + Send + Sync, const N: usize> IntoWriteBatch
    for &[(S, V); N]
{
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        self.as_slice().into_write_batch()
    }
}

impl<S: AsRef<str> + Sync, V: Clone + Into<OpcValue> + Send + Sync> IntoWriteBatch for &[(S, V)] {
    #[inline]
    fn into_write_batch(self) -> WriteBatch {
        let vec: Vec<(String, OpcValue)> = self
            .iter()
            .map(|(s, v)| (s.as_ref().to_string(), v.clone().into()))
            .collect();
        WriteBatch {
            repr: WriteBatchRepr::Owned(vec),
        }
    }
}

#[cfg(test)]
#[allow(clippy::similar_names)]
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
    fn test_write_batch_sso_boundary_ascii() {
        // 1. 0-byte empty string -> InlineSingle
        let b_empty = WriteBatch::from_str_lenient("", OpcValue::Int(0));
        assert_eq!(b_empty.len(), 1);
        assert!(!b_empty.is_empty());
        assert!(matches!(
            b_empty.repr,
            WriteBatchRepr::InlineSingle(_, 0, _)
        ));
        let items: Vec<(&str, &OpcValue)> = b_empty.iter().collect();
        assert_eq!(items, vec![("", &OpcValue::Int(0))]);
        let owned: Vec<(String, OpcValue)> = b_empty.into_iter().collect();
        assert_eq!(owned, vec![(String::new(), OpcValue::Int(0))]);

        // 2. 1-byte string -> InlineSingle
        let b_1 = WriteBatch::from_str_lenient("A", OpcValue::Int(1));
        assert_eq!(b_1.len(), 1);
        assert!(matches!(b_1.repr, WriteBatchRepr::InlineSingle(_, 1, _)));
        let items: Vec<(&str, &OpcValue)> = b_1.iter().collect();
        assert_eq!(items, vec![("A", &OpcValue::Int(1))]);

        // 3. Exactly 31-byte ASCII string -> InlineSingle boundary upper bound
        let s31 = "1234567890123456789012345678901";
        assert_eq!(s31.len(), 31);
        let b_thirty_one = WriteBatch::from_str_lenient(s31, OpcValue::Int(31));
        assert_eq!(b_thirty_one.len(), 1);
        assert!(matches!(
            b_thirty_one.repr,
            WriteBatchRepr::InlineSingle(_, 31, _)
        ));
        let items: Vec<(&str, &OpcValue)> = b_thirty_one.iter().collect();
        assert_eq!(items, vec![(s31, &OpcValue::Int(31))]);

        // 4. Exactly 32-byte ASCII string -> OwnedSingle spillover
        let s32 = "12345678901234567890123456789012";
        assert_eq!(s32.len(), 32);
        let b_32 = WriteBatch::from_str_lenient(s32, OpcValue::Int(32));
        assert_eq!(b_32.len(), 1);
        assert!(matches!(b_32.repr, WriteBatchRepr::OwnedSingle(ref s, _) if s == s32));
        let items: Vec<(&str, &OpcValue)> = b_32.iter().collect();
        assert_eq!(items, vec![(s32, &OpcValue::Int(32))]);
    }

    #[test]
    fn test_write_batch_sso_multibyte_utf8_safety() {
        // 1. 2-byte Cyrillic within 31-byte limit: "Датчик1" = 6 chars * 2 bytes + 1 ASCII byte = 13 bytes
        let cyr_short = "Датчик1";
        assert_eq!(cyr_short.len(), 13);
        let b_cyr = WriteBatch::from_str_lenient(cyr_short, OpcValue::Int(10));
        assert_eq!(b_cyr.len(), 1);
        assert!(matches!(b_cyr.repr, WriteBatchRepr::InlineSingle(_, 13, _)));
        assert_eq!(b_cyr.iter().next(), Some((cyr_short, &OpcValue::Int(10))));

        // 2. 2-byte Cyrillic boundary transition: 15 chars (30 bytes) vs 16 chars (32 bytes)
        let cyr_30 = "Д".repeat(15);
        assert_eq!(cyr_30.len(), 30);
        let b_cyr_30 = WriteBatch::from_str_lenient(&cyr_30, OpcValue::Int(30));
        assert!(matches!(
            b_cyr_30.repr,
            WriteBatchRepr::InlineSingle(_, 30, _)
        ));
        assert_eq!(
            b_cyr_30.iter().next(),
            Some((cyr_30.as_str(), &OpcValue::Int(30)))
        );

        let cyr_32 = "Д".repeat(16);
        assert_eq!(cyr_32.len(), 32);
        let b_cyr_32 = WriteBatch::from_str_lenient(&cyr_32, OpcValue::Int(32));
        assert!(matches!(b_cyr_32.repr, WriteBatchRepr::OwnedSingle(ref s, _) if s == &cyr_32));
        assert_eq!(
            b_cyr_32.iter().next(),
            Some((cyr_32.as_str(), &OpcValue::Int(32)))
        );

        // 3. 3-byte CJK within boundary: "タグ１２３" = 5 chars * 3 bytes = 15 bytes
        let cjk_short = "タグ１２３";
        assert_eq!(cjk_short.len(), 15);
        let b_cjk = WriteBatch::from_str_lenient(cjk_short, OpcValue::Int(20));
        assert_eq!(b_cjk.len(), 1);
        assert!(matches!(b_cjk.repr, WriteBatchRepr::InlineSingle(_, 15, _)));
        assert_eq!(b_cjk.iter().next(), Some((cjk_short, &OpcValue::Int(20))));

        // 4. 3-byte CJK boundary transition: 10 chars (30 bytes) vs 11 chars (33 bytes)
        let cjk_30 = "タ".repeat(10);
        assert_eq!(cjk_30.len(), 30);
        let b_cjk_30 = WriteBatch::from_str_lenient(&cjk_30, OpcValue::Int(30));
        assert!(matches!(
            b_cjk_30.repr,
            WriteBatchRepr::InlineSingle(_, 30, _)
        ));

        let cjk_33 = "タ".repeat(11);
        assert_eq!(cjk_33.len(), 33);
        let b_cjk_33 = WriteBatch::from_str_lenient(&cjk_33, OpcValue::Int(33));
        assert!(matches!(b_cjk_33.repr, WriteBatchRepr::OwnedSingle(ref s, _) if s == &cjk_33));
        assert_eq!(
            b_cjk_33.iter().next(),
            Some((cjk_33.as_str(), &OpcValue::Int(33)))
        );

        // 5. 4-byte Emoji within and crossing boundary: 2 emojis (8 bytes) vs 8 emojis (32 bytes)
        let emoji_short = "🚀🏭⚡";
        assert_eq!(emoji_short.len(), 11);
        let b_emoji = WriteBatch::from_str_lenient(emoji_short, OpcValue::Int(40));
        assert!(matches!(
            b_emoji.repr,
            WriteBatchRepr::InlineSingle(_, 11, _)
        ));
        assert_eq!(
            b_emoji.iter().next(),
            Some((emoji_short, &OpcValue::Int(40)))
        );

        let emoji_32 = "🚀".repeat(8);
        assert_eq!(emoji_32.len(), 32);
        let b_emoji_32 = WriteBatch::from_str_lenient(&emoji_32, OpcValue::Int(32));
        assert!(matches!(b_emoji_32.repr, WriteBatchRepr::OwnedSingle(ref s, _) if s == &emoji_32));

        // 6. Defensive UTF-8 truncation in inline_as_str: corrupted buffer fallback
        let mut bad_buf = [0u8; 31];
        bad_buf[0] = 0x41; // 'A'
        bad_buf[1] = 0xE3; // First byte of 3-byte UTF-8 without continuation
        let decoded = WriteBatch::inline_as_str(&bad_buf, 2);
        assert_eq!(
            decoded, "A",
            "Must recover valid UTF-8 prefix without panic"
        );
    }

    #[test]
    fn test_write_batch_interior_null_bytes_preserved() {
        // 1. InlineSingle with interior null byte (CWE-626 preservation)
        let null_tag = "Tag\0Motor";
        assert_eq!(null_tag.len(), 9);
        let b_inline = WriteBatch::from_str_lenient(null_tag, OpcValue::Int(100));
        assert_eq!(b_inline.len(), 1);
        assert!(matches!(
            b_inline.repr,
            WriteBatchRepr::InlineSingle(_, 9, _)
        ));
        let items: Vec<(&str, &OpcValue)> = b_inline.iter().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "Tag\0Motor");
        assert_eq!(items[0].1, &OpcValue::Int(100));
        let owned: Vec<(String, OpcValue)> = b_inline.into_iter().collect();
        assert_eq!(owned[0].0, "Tag\0Motor");

        // 2. StaticSingle with interior null byte
        let b_static = WriteBatch::from(("Static\0Tag", OpcValue::Int(200)));
        assert_eq!(b_static.len(), 1);
        assert_eq!(b_static.iter().next().unwrap().0, "Static\0Tag");

        // 3. OwnedSingle with interior null byte (> 31 bytes)
        let long_null = format!("LongPrefix_{}\0_LongSuffix", "A".repeat(25));
        assert!(long_null.len() > 31);
        let b_owned = WriteBatch::from((long_null.clone(), OpcValue::Int(300)));
        assert_eq!(b_owned.len(), 1);
        assert_eq!(b_owned.iter().next().unwrap().0, long_null.as_str());
    }

    #[test]
    fn test_write_batch_len_invariants_cwe682() {
        // Scalar variants MUST return 1, NOT the byte length of the tag identifier
        let b_static = WriteBatch::from(("Device.Motor.Speed.RPM", OpcValue::Int(1)));
        assert_eq!(b_static.len(), 1, "StaticSingle must return 1");
        assert!(!b_static.is_empty());

        let b_inline_0 = WriteBatch::from_str_lenient("", OpcValue::Int(2));
        assert_eq!(
            b_inline_0.len(),
            1,
            "InlineSingle with empty tag must return 1"
        );

        let b_inline_15 = WriteBatch::from_str_lenient("Tag.Motor.Speed", OpcValue::Int(3));
        assert_eq!(
            b_inline_15.len(),
            1,
            "InlineSingle with 15-byte tag must return 1, NOT 15"
        );

        let b_inline_31 =
            WriteBatch::from_str_lenient("1234567890123456789012345678901", OpcValue::Int(4));
        assert_eq!(
            b_inline_31.len(),
            1,
            "InlineSingle with 31-byte tag must return 1, NOT 31"
        );

        let b_owned_single = WriteBatch::from((
            "Very.Long.Tag.Name.Exceeding.Thirty.One.Bytes".to_string(),
            OpcValue::Int(5),
        ));
        assert_eq!(b_owned_single.len(), 1, "OwnedSingle must return 1");

        // Multi-item variants return slice/vec length
        let b_shared = WriteBatch::from(Arc::from([
            ("T1".to_string(), OpcValue::Int(1)),
            ("T2".to_string(), OpcValue::Int(2)),
        ]));
        assert_eq!(b_shared.len(), 2);

        let b_owned = WriteBatch::from(vec![
            ("T1".to_string(), OpcValue::Int(1)),
            ("T2".to_string(), OpcValue::Int(2)),
            ("T3".to_string(), OpcValue::Int(3)),
        ]);
        assert_eq!(b_owned.len(), 3);

        // Empty batch returns 0
        let b_empty = WriteBatch::empty();
        assert_eq!(b_empty.len(), 0);
        assert!(b_empty.is_empty());
    }

    #[test]
    fn test_write_batch_cross_variant_partial_eq_permutations() {
        let tag = "Device.Motor.RPM";
        let val = OpcValue::Int(1500);

        // Construct identical (tag, val) across all 5 representations
        let v_static = WriteBatch::from((tag, val.clone()));
        let v_inline = WriteBatch::from_str_lenient(tag, val.clone());
        let v_owned_single = WriteBatch::from((tag.to_string(), val.clone()));
        let v_shared = WriteBatch::from(Arc::from([(tag.to_string(), val.clone())]));
        let v_owned = WriteBatch::from(vec![(tag.to_string(), val.clone())]);

        let variants = [
            ("StaticSingle", &v_static),
            ("InlineSingle", &v_inline),
            ("OwnedSingle", &v_owned_single),
            ("Shared", &v_shared),
            ("Owned", &v_owned),
        ];

        // All 5 x 5 = 25 pairwise permutations must equal true
        for (name1, b1) in &variants {
            for (name2, b2) in &variants {
                assert_eq!(
                    *b1, *b2,
                    "Equality failed for permutation: {name1} == {name2}"
                );
            }
        }

        // Negative assertions: differing tag name
        let diff_tag = WriteBatch::from_str_lenient("Device.Motor.Temp", val.clone());
        assert_ne!(v_static, diff_tag);
        assert_ne!(v_inline, diff_tag);
        assert_ne!(v_owned_single, diff_tag);
        assert_ne!(v_shared, diff_tag);
        assert_ne!(v_owned, diff_tag);

        // Negative assertions: differing value
        let diff_val = WriteBatch::from_str_lenient(tag, OpcValue::Int(1501));
        assert_ne!(v_inline, diff_val);
        assert_ne!(v_static, diff_val);

        // Negative assertions: differing length
        let multi_batch = WriteBatch::from(vec![
            (tag.to_string(), val),
            ("Device.Motor.Temp".to_string(), OpcValue::Float(45.2)),
        ]);
        assert_ne!(v_inline, multi_batch);
        assert_ne!(v_owned, multi_batch);
    }

    #[test]
    fn test_write_batch_into_shareable_stack_preservation() {
        // 1. StaticSingle is preserved on the stack without Arc allocation
        let b_static = WriteBatch::from(("Sensor.Static", OpcValue::Int(10)));
        let shareable_static = b_static.clone().into_shareable();
        assert!(
            matches!(
                shareable_static.repr,
                WriteBatchRepr::StaticSingle("Sensor.Static", _)
            ),
            "StaticSingle must remain StaticSingle in into_shareable"
        );
        assert_eq!(shareable_static, b_static);

        // 2. InlineSingle is preserved on the stack without Arc allocation
        let b_inline = WriteBatch::from_str_lenient("Sensor.Inline", OpcValue::Int(20));
        let shareable_inline = b_inline.clone().into_shareable();
        assert!(
            matches!(
                shareable_inline.repr,
                WriteBatchRepr::InlineSingle(_, 13, _)
            ),
            "InlineSingle must remain InlineSingle in into_shareable"
        );
        assert_eq!(shareable_inline, b_inline);

        // 3. OwnedSingle is promoted to Shared with Direct Array Sharing (Arc<[(String, OpcValue)]>)
        let b_owned_single =
            WriteBatch::from(("Sensor.OwnedSingle".to_string(), OpcValue::Int(30)));
        let shareable_owned_single = b_owned_single.clone().into_shareable();
        match &shareable_owned_single.repr {
            WriteBatchRepr::Shared(arc) => {
                assert_eq!(arc.len(), 1);
                assert_eq!(arc[0].0, "Sensor.OwnedSingle");
                assert_eq!(arc[0].1, OpcValue::Int(30));
                assert_eq!(Arc::strong_count(arc), 1);
            }
            _ => panic!("OwnedSingle must be promoted to Shared in into_shareable"),
        }
        assert_eq!(shareable_owned_single, b_owned_single);

        // 4. Owned vector is promoted to Shared
        let b_owned = WriteBatch::from(vec![
            ("T1".to_string(), OpcValue::Int(1)),
            ("T2".to_string(), OpcValue::Int(2)),
        ]);
        let shareable_owned = b_owned.clone().into_shareable();
        assert!(matches!(shareable_owned.repr, WriteBatchRepr::Shared(_)));
        assert_eq!(shareable_owned, b_owned);

        // 5. Shared is idempotent
        let shareable_idempotent = shareable_owned.clone().into_shareable();
        assert_eq!(shareable_idempotent, shareable_owned);

        // 6. Empty batch shareable
        let empty_shareable = WriteBatch::empty().into_shareable();
        assert_eq!(empty_shareable.len(), 0);
        assert!(empty_shareable.is_empty());
    }

    #[test]
    fn test_write_batch_iterators_all_variants() {
        type VariantTestCase<'a> = (&'static str, WriteBatch, Vec<(&'a str, OpcValue)>);
        let variants: Vec<VariantTestCase<'_>> = vec![
            (
                "StaticSingle",
                WriteBatch::from(("StaticTag", OpcValue::Int(1))),
                vec![("StaticTag", OpcValue::Int(1))],
            ),
            (
                "InlineSingle",
                WriteBatch::from_str_lenient("InlineTag", OpcValue::Int(2)),
                vec![("InlineTag", OpcValue::Int(2))],
            ),
            (
                "OwnedSingle",
                WriteBatch::from(("OwnedSingleTag".to_string(), OpcValue::Int(3))),
                vec![("OwnedSingleTag", OpcValue::Int(3))],
            ),
            (
                "Shared",
                WriteBatch::from(Arc::from([
                    ("Shared1".to_string(), OpcValue::Int(4)),
                    ("Shared2".to_string(), OpcValue::Int(5)),
                ])),
                vec![("Shared1", OpcValue::Int(4)), ("Shared2", OpcValue::Int(5))],
            ),
            (
                "Owned",
                WriteBatch::from(vec![
                    ("Owned1".to_string(), OpcValue::Int(6)),
                    ("Owned2".to_string(), OpcValue::Int(7)),
                ]),
                vec![("Owned1", OpcValue::Int(6)), ("Owned2", OpcValue::Int(7))],
            ),
        ];

        for (name, batch, expected) in variants {
            // 1. Borrowed iter() & size_hint()
            let mut iter = batch.iter();
            let expected_len = expected.len();
            assert_eq!(iter.len(), expected_len, "iter.len() mismatch for {name}");
            assert_eq!(
                iter.size_hint(),
                (expected_len, Some(expected_len)),
                "size_hint mismatch for {name}"
            );

            let mut collected = Vec::new();
            for (tag, val) in iter.by_ref() {
                collected.push((tag, val.clone()));
            }
            assert_eq!(iter.len(), 0, "iter must be exhausted for {name}");
            assert_eq!(iter.size_hint(), (0, Some(0)));
            assert_eq!(
                collected, expected,
                "iter yielded unexpected items for {name}"
            );

            // 2. Consuming into_iter() & size_hint()
            let mut into_iter = batch.into_iter();
            assert_eq!(
                into_iter.len(),
                expected_len,
                "into_iter.len() mismatch for {name}"
            );
            assert_eq!(
                into_iter.size_hint(),
                (expected_len, Some(expected_len)),
                "into_iter size_hint mismatch for {name}"
            );

            let mut consumed = Vec::new();
            for (tag, val) in into_iter.by_ref() {
                consumed.push((tag, val));
            }
            assert_eq!(into_iter.len(), 0, "into_iter must be exhausted for {name}");
            assert_eq!(into_iter.size_hint(), (0, Some(0)));
            let expected_owned: Vec<(String, OpcValue)> = expected
                .into_iter()
                .map(|(t, v)| (t.to_string(), v))
                .collect();
            assert_eq!(
                consumed, expected_owned,
                "into_iter yielded unexpected items for {name}"
            );
        }
    }

    #[test]
    fn test_write_batch_from_conversions_coherence() {
        // 1. From<(&'static str, V)> -> StaticSingle
        let b_static: WriteBatch = ("Static.Tag", 42i32).into();
        assert!(
            matches!(b_static.repr, WriteBatchRepr::StaticSingle("Static.Tag", _)),
            "(&'static str, V) must route to StaticSingle"
        );

        // 2. From<(String, V)> -> OwnedSingle
        let b_owned_single: WriteBatch = ("Dynamic.Tag".to_string(), 42i32).into();
        assert!(
            matches!(b_owned_single.repr, WriteBatchRepr::OwnedSingle(ref s, _) if s == "Dynamic.Tag"),
            "(String, V) must route to OwnedSingle"
        );

        // 3. IntoWriteBatch for (&str, V) -> from_str_lenient
        let short_str: &str = "Short.Tag";
        let b_dyn_short = (short_str, 10i32).into_write_batch();
        assert!(
            matches!(b_dyn_short.repr, WriteBatchRepr::InlineSingle(_, 9, _)),
            "(&str, V) <= 31 bytes must route to InlineSingle"
        );

        let long_str: String = "Long.Tag.That.Exceeds.Thirty.One.Bytes.Identifier".to_string();
        let b_dyn_long = (long_str.as_str(), 20i32).into_write_batch();
        assert!(
            matches!(b_dyn_long.repr, WriteBatchRepr::OwnedSingle(ref s, _) if s == &long_str),
            "(&str, V) > 31 bytes must route to OwnedSingle"
        );

        // 4. Collections
        let arr_batch = [("Tag1", 1), ("Tag2", 2)].into_write_batch();
        assert_eq!(arr_batch.len(), 2);

        let slice_batch = (&[("Tag1", 1), ("Tag2", 2)][..]).into_write_batch();
        assert_eq!(slice_batch.len(), 2);

        let vec_batch = vec![("Tag1".to_string(), OpcValue::Int(1))].into_write_batch();
        assert_eq!(vec_batch.len(), 1);

        // 5. Arc<[(String, OpcValue)]>
        let arc_slice: Arc<[(String, OpcValue)]> =
            Arc::from([("Tag1".to_string(), OpcValue::Int(1))]);
        let arc_into_batch = arc_slice.into_write_batch();
        assert!(matches!(arc_into_batch.repr, WriteBatchRepr::Shared(_)));
    }

    #[test]
    fn test_into_write_batch_send_bound() {
        fn assert_send<T: IntoWriteBatch + Send>() {}
        assert_send::<WriteBatch>();
        assert_send::<(&'static str, i32)>();
        assert_send::<(String, i32)>();
        assert_send::<[(&'static str, i32); 2]>();
        assert_send::<Vec<(String, OpcValue)>>();
        assert_send::<Arc<[(String, OpcValue)]>>();
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
