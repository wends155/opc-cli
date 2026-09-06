//! Zero-allocation tag batching and collection conversions.

use std::sync::Arc;

/// Represents a batch of OPC tag names, enabling zero-allocation conversion
/// across static literals, fixed-size arrays, borrowed string slices, and owned collections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagBatch {
    /// Borrowed slice of static string slices (e.g. `&["Random.Int4", "Random.Real8"]`).
    Static(&'static [&'static str]),
    /// Single static string slice literal (e.g. `"Random.Int4"`).
    StaticSingle(&'static str),
    /// Single inline tag string up to 31 bytes (zero-allocation dynamic single-tag reads).
    InlineSingle([u8; 31], u8),
    /// Shared reference-counted slice of strings.
    Shared(Arc<[String]>),
    /// Owned vector of heap strings (e.g. from `browse_tags` or dynamic configuration).
    Owned(Vec<String>),
    /// Single owned heap string.
    OwnedSingle(String),
}

impl TagBatch {
    /// Creates a single-tag batch from a string slice without heap allocation
    /// if the string is 31 bytes or shorter.
    #[must_use]
    pub fn from_str_lenient(s: &str) -> Self {
        if s.len() <= 31 {
            let mut buf = [0u8; 31];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            #[allow(clippy::cast_possible_truncation)]
            Self::InlineSingle(buf, s.len() as u8)
        } else {
            Self::OwnedSingle(s.to_string())
        }
    }

    /// Creates an empty static tag batch.
    #[must_use]
    pub const fn empty() -> Self {
        Self::Static(&[])
    }
}

impl Default for TagBatch {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl TagBatch {
    /// Returns the number of tags in this batch.
    ///
    /// # Returns
    ///
    /// The number of tag identifiers contained in this batch.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::IntoTags;
    ///
    /// let batch = ["Tag1", "Tag2"].into_tag_batch();
    /// assert_eq!(batch.len(), 2);
    /// ```
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Static(slice) => slice.len(),
            Self::StaticSingle(_) | Self::InlineSingle(_, _) | Self::OwnedSingle(_) => 1,
            Self::Shared(slice) => slice.len(),
            Self::Owned(vec) => vec.len(),
        }
    }

    /// Converts this batch into an efficiently cloneable representation, ensuring
    /// repeated clones (such as in subscription polling loops) avoid heap re-allocations.
    #[must_use]
    pub fn into_shareable(self) -> Self {
        match self {
            Self::Static(slice) => Self::Static(slice),
            Self::StaticSingle(s) => Self::StaticSingle(s),
            Self::InlineSingle(buf, len) => {
                let s = std::str::from_utf8(&buf[..len as usize]).unwrap_or_default();
                Self::Shared(Arc::from(vec![s.to_string()].into_boxed_slice()))
            }
            Self::Shared(slice) => Self::Shared(slice),
            Self::Owned(vec) => Self::Shared(Arc::from(vec.into_boxed_slice())),
            Self::OwnedSingle(s) => Self::Shared(Arc::from(vec![s].into_boxed_slice())),
        }
    }

    /// Returns `true` if this tag batch contains no tags.
    ///
    /// # Returns
    ///
    /// `true` if the batch has a length of 0; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::IntoTags;
    ///
    /// let batch = ["Tag1"].into_tag_batch();
    /// assert!(!batch.is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator yielding string slices (`&str`) for each tag in this batch.
    ///
    /// # Returns
    ///
    /// A zero-allocation [`TagBatchIter`] yielding borrowed `&str` references for each tag.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::IntoTags;
    ///
    /// let batch = ["Tag1", "Tag2"].into_tag_batch();
    /// let collected: Vec<&str> = batch.iter_str().collect();
    /// assert_eq!(collected, vec!["Tag1", "Tag2"]);
    /// ```
    #[inline]
    pub fn iter_str(&self) -> TagBatchIter<'_> {
        match self {
            Self::Static(slice) => TagBatchIter::Static(slice.iter()),
            Self::StaticSingle(s) => TagBatchIter::Single(Some(*s)),
            Self::InlineSingle(buf, len) => {
                let s = std::str::from_utf8(&buf[..*len as usize]).unwrap_or_default();
                TagBatchIter::Single(Some(s))
            }
            Self::Shared(slice) => TagBatchIter::Ref(slice.iter()),
            Self::Owned(vec) => TagBatchIter::Ref(vec.iter()),
            Self::OwnedSingle(s) => TagBatchIter::Single(Some(s.as_str())),
        }
    }

    /// Returns an iterator yielding string slices (`&str`) for each tag in this batch.
    #[inline]
    #[must_use]
    pub fn iter(&self) -> TagBatchIter<'_> {
        self.iter_str()
    }

    /// Consumes the batch and converts it into a `Vec<String>`.
    ///
    /// Reuses existing allocation when the batch variant is `TagBatch::Owned`.
    ///
    /// # Returns
    ///
    /// An owned [`Vec<String>`] containing all tag names.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::IntoTags;
    ///
    /// let batch = ["Tag1"].into_tag_batch();
    /// assert_eq!(batch.into_vec(), vec!["Tag1".to_string()]);
    /// ```
    #[must_use]
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Self::Static(slice) => slice.iter().map(|&s| s.to_string()).collect(),
            Self::StaticSingle(s) => vec![s.to_string()],
            Self::InlineSingle(buf, len) => {
                let s = std::str::from_utf8(&buf[..len as usize]).unwrap_or_default();
                vec![s.to_string()]
            }
            Self::Shared(slice) => slice.to_vec(),
            Self::Owned(vec) => vec,
            Self::OwnedSingle(s) => vec![s],
        }
    }
}

/// Zero-allocation iterator over tags within a [`TagBatch`].
#[derive(Clone, Debug)]
pub enum TagBatchIter<'a> {
    /// Iterating over borrowed static string slices.
    Static(std::slice::Iter<'a, &'static str>),
    /// Iterating over a single string item.
    Single(Option<&'a str>),
    /// Iterating over borrowed heap strings.
    Ref(std::slice::Iter<'a, String>),
}

impl<'a> Iterator for TagBatchIter<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Static(it) => it.next().copied(),
            Self::Single(opt) => opt.take(),
            Self::Ref(it) => it.next().map(String::as_str),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Static(it) => it.size_hint(),
            Self::Single(opt) => {
                let n = usize::from(opt.is_some());
                (n, Some(n))
            }
            Self::Ref(it) => it.size_hint(),
        }
    }
}

impl ExactSizeIterator for TagBatchIter<'_> {}

/// Conversion trait that turns static slices, arrays, borrowed strings, and owned collections
/// into a [`TagBatch`] without upfront heap allocation.
pub trait IntoTags: Send {
    /// Convert `self` into a [`TagBatch`].
    fn into_tag_batch(self) -> TagBatch;
}

impl IntoTags for TagBatch {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        self
    }
}

impl IntoTags for &'static [&'static str] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Static(self)
    }
}

impl<const N: usize> IntoTags for &'static [&'static str; N] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Static(self.as_slice())
    }
}

impl<const N: usize> IntoTags for [&'static str; N] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Owned(self.into_iter().map(String::from).collect())
    }
}

impl IntoTags for &'static str {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::StaticSingle(self)
    }
}

impl IntoTags for Vec<String> {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Owned(self)
    }
}

impl IntoTags for &[String] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Owned(self.to_vec())
    }
}

impl IntoTags for Arc<[String]> {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Shared(self)
    }
}

impl IntoTags for String {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::OwnedSingle(self)
    }
}

impl From<Vec<String>> for TagBatch {
    #[inline]
    fn from(v: Vec<String>) -> Self {
        Self::Owned(v)
    }
}

impl From<&'static [&'static str]> for TagBatch {
    #[inline]
    fn from(s: &'static [&'static str]) -> Self {
        Self::Static(s)
    }
}

impl<const N: usize> From<&'static [&'static str; N]> for TagBatch {
    #[inline]
    fn from(s: &'static [&'static str; N]) -> Self {
        Self::Static(s.as_slice())
    }
}

impl From<&'static str> for TagBatch {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::StaticSingle(s)
    }
}

impl From<String> for TagBatch {
    #[inline]
    fn from(s: String) -> Self {
        Self::OwnedSingle(s)
    }
}

impl From<Arc<[String]>> for TagBatch {
    #[inline]
    fn from(a: Arc<[String]>) -> Self {
        Self::Shared(a)
    }
}

impl IntoIterator for TagBatch {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a TagBatch {
    type Item = &'a str;
    type IntoIter = TagBatchIter<'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_str()
    }
}
