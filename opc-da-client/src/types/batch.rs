//! Zero-allocation tag batching and collection conversions.

use std::sync::Arc;

/// Internal storage representation for [`TagBatch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TagBatchRepr {
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

/// Represents a batch of OPC tag names, enabling zero-allocation conversion
/// across static literals, borrowed string slices, and owned collections.
///
/// # Allocation Semantics
///
/// * **Zero-Allocation**: Borrowed static slices `&["Tag1", "Tag2"]` (`&'static [&'static str]`) or
///   references to fixed-size arrays `&["Tag1", "Tag2"]` map directly to internal static storage without heap allocation.
/// * **Heap Allocation**: Passing fixed-size arrays by value `["Tag1", "Tag2"]` (`[&'static str; N]`)
///   allocates owned [`String`] instances. When optimal throughput is required in hot loops,
///   prefer passing borrowed slice references `&["Tag1", "Tag2"]`.
#[derive(Debug, Clone)]
pub struct TagBatch {
    pub(crate) repr: TagBatchRepr,
}

impl PartialEq for TagBatch {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter_str().eq(other.iter_str())
    }
}

impl Eq for TagBatch {}

impl TagBatch {
    /// Creates a single-tag batch from a string slice without heap allocation
    /// if the string is 31 bytes or shorter.
    #[must_use]
    pub fn from_str_lenient(s: &str) -> Self {
        if s.len() <= 31 {
            let mut buf = [0u8; 31];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            #[allow(clippy::cast_possible_truncation)]
            Self {
                repr: TagBatchRepr::InlineSingle(buf, s.len() as u8),
            }
        } else {
            Self {
                repr: TagBatchRepr::OwnedSingle(s.to_string()),
            }
        }
    }

    /// Helper to safely slice inline SSO buffer into a valid UTF-8 string slice.
    fn inline_as_str(buf: &[u8; 31], len: u8) -> &str {
        let valid_len = (len as usize).min(31).min(buf.len());
        match std::str::from_utf8(&buf[..valid_len]) {
            Ok(s) => s,
            Err(e) => {
                let up_to = e.valid_up_to();
                std::str::from_utf8(&buf[..up_to]).unwrap_or_default()
            }
        }
    }

    /// Creates an empty static tag batch.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            repr: TagBatchRepr::Static(&[]),
        }
    }

    /// Creates a tag batch from a borrowed static slice without heap allocation.
    #[must_use]
    pub const fn from_static(slice: &'static [&'static str]) -> Self {
        Self {
            repr: TagBatchRepr::Static(slice),
        }
    }

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
        match &self.repr {
            TagBatchRepr::Static(slice) => slice.len(),
            TagBatchRepr::StaticSingle(_)
            | TagBatchRepr::InlineSingle(_, _)
            | TagBatchRepr::OwnedSingle(_) => 1,
            TagBatchRepr::Shared(slice) => slice.len(),
            TagBatchRepr::Owned(vec) => vec.len(),
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

    /// Returns a borrowed slice of the tags if backed by a contiguous heap allocation (`Owned` or `Shared`).
    #[must_use]
    pub fn as_slice(&self) -> Option<&[String]> {
        match &self.repr {
            TagBatchRepr::Owned(vec) => Some(vec.as_slice()),
            TagBatchRepr::Shared(slice) => Some(slice),
            _ => None,
        }
    }

    /// Returns a static slice of the tags if backed by a `'static` slice.
    #[must_use]
    pub const fn as_static_slice(&self) -> Option<&'static [&'static str]> {
        match &self.repr {
            TagBatchRepr::Static(slice) => Some(*slice),
            _ => None,
        }
    }

    /// Converts this batch into an efficiently cloneable representation, ensuring
    /// repeated clones (such as in subscription polling loops) avoid heap re-allocations.
    #[must_use]
    pub fn into_shareable(self) -> Self {
        match self.repr {
            TagBatchRepr::Static(slice) => Self {
                repr: TagBatchRepr::Static(slice),
            },
            TagBatchRepr::StaticSingle(s) => Self {
                repr: TagBatchRepr::StaticSingle(s),
            },
            TagBatchRepr::InlineSingle(buf, len) => Self {
                repr: TagBatchRepr::InlineSingle(buf, len),
            },
            TagBatchRepr::Shared(slice) => Self {
                repr: TagBatchRepr::Shared(slice),
            },
            TagBatchRepr::Owned(vec) => Self {
                repr: TagBatchRepr::Shared(Arc::from(vec.into_boxed_slice())),
            },
            TagBatchRepr::OwnedSingle(s) => Self {
                repr: TagBatchRepr::Shared(Arc::from(vec![s].into_boxed_slice())),
            },
        }
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
        match &self.repr {
            TagBatchRepr::Static(slice) => TagBatchIter::Static(slice.iter()),
            TagBatchRepr::StaticSingle(s) => TagBatchIter::Single(Some(*s)),
            TagBatchRepr::InlineSingle(buf, len) => {
                let s = Self::inline_as_str(buf, *len);
                TagBatchIter::Single(Some(s))
            }
            TagBatchRepr::Shared(slice) => TagBatchIter::Ref(slice.iter()),
            TagBatchRepr::Owned(vec) => TagBatchIter::Ref(vec.iter()),
            TagBatchRepr::OwnedSingle(s) => TagBatchIter::Single(Some(s.as_str())),
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
        match self.repr {
            TagBatchRepr::Static(slice) => slice.iter().map(|&s| s.to_string()).collect(),
            TagBatchRepr::StaticSingle(s) => vec![s.to_string()],
            TagBatchRepr::InlineSingle(buf, len) => {
                let s = Self::inline_as_str(&buf, len);
                vec![s.to_string()]
            }
            TagBatchRepr::Shared(slice) => slice.to_vec(),
            TagBatchRepr::Owned(vec) => vec,
            TagBatchRepr::OwnedSingle(s) => vec![s],
        }
    }
}

impl Default for TagBatch {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl<S: Into<String>> FromIterator<S> for TagBatch {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        Self {
            repr: TagBatchRepr::Owned(iter.into_iter().map(Into::into).collect()),
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
        TagBatch::from_static(self)
    }
}

impl<const N: usize> IntoTags for &'static [&'static str; N] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::from_static(self.as_slice())
    }
}

/// Converts a fixed-size array by value into a [`TagBatch`].
///
/// Note: This performs heap allocations to allocate owned [`String`] elements.
/// For zero-allocation batch reads, pass a borrowed slice reference instead (e.g. `&["Tag1", "Tag2"]`).
impl<const N: usize> IntoTags for [&'static str; N] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch {
            repr: TagBatchRepr::Owned(self.into_iter().map(String::from).collect()),
        }
    }
}

impl IntoTags for &'static str {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch {
            repr: TagBatchRepr::StaticSingle(self),
        }
    }
}

impl IntoTags for Vec<String> {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch {
            repr: TagBatchRepr::Owned(self),
        }
    }
}

impl IntoTags for &[String] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch {
            repr: TagBatchRepr::Owned(self.to_vec()),
        }
    }
}

impl IntoTags for Arc<[String]> {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch {
            repr: TagBatchRepr::Shared(self),
        }
    }
}

impl IntoTags for String {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch {
            repr: TagBatchRepr::OwnedSingle(self),
        }
    }
}

impl From<Vec<String>> for TagBatch {
    #[inline]
    fn from(v: Vec<String>) -> Self {
        Self {
            repr: TagBatchRepr::Owned(v),
        }
    }
}

impl From<&'static [&'static str]> for TagBatch {
    #[inline]
    fn from(s: &'static [&'static str]) -> Self {
        Self::from_static(s)
    }
}

impl<const N: usize> From<&'static [&'static str; N]> for TagBatch {
    #[inline]
    fn from(s: &'static [&'static str; N]) -> Self {
        Self::from_static(s.as_slice())
    }
}

impl From<&'static str> for TagBatch {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self {
            repr: TagBatchRepr::StaticSingle(s),
        }
    }
}

impl From<String> for TagBatch {
    #[inline]
    fn from(s: String) -> Self {
        Self {
            repr: TagBatchRepr::OwnedSingle(s),
        }
    }
}

impl From<Arc<[String]>> for TagBatch {
    #[inline]
    fn from(a: Arc<[String]>) -> Self {
        Self {
            repr: TagBatchRepr::Shared(a),
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn test_tag_batch_into_tags_conversions() {
        // 1. Static slice
        static STATIC_SLICE: &[&str] = &["Tag1", "Tag2"];
        let batch = STATIC_SLICE.into_tag_batch();
        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Tag1", "Tag2"]);
        assert_eq!(
            batch.into_vec(),
            vec!["Tag1".to_string(), "Tag2".to_string()]
        );

        // 2. Fixed-size array of static str
        let arr = ["TagA", "TagB", "TagC"];
        let batch = arr.into_tag_batch();
        assert_eq!(batch.len(), 3);
        assert_eq!(
            batch.iter_str().collect::<Vec<_>>(),
            vec!["TagA", "TagB", "TagC"]
        );
        assert_eq!(batch.into_vec(), vec!["TagA", "TagB", "TagC"]);

        // 3. Single static str literal
        let single_static = "SingleTag";
        let batch = single_static.into_tag_batch();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["SingleTag"]);
        assert_eq!(batch.into_vec(), vec!["SingleTag"]);

        // 4. Vec<String>
        let vec_strings = vec!["Dyn1".to_string(), "Dyn2".to_string()];
        let batch = vec_strings.into_tag_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Dyn1", "Dyn2"]);
        assert_eq!(batch.into_vec(), vec!["Dyn1", "Dyn2"]);

        // 5. &[String]
        let slice_strings: &[String] = &["S1".to_string(), "S2".to_string()];
        let batch = slice_strings.into_tag_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["S1", "S2"]);
        assert_eq!(batch.into_vec(), vec!["S1", "S2"]);

        // 6. TagBatch::from_str_lenient
        let inline_batch = TagBatch::from_str_lenient("Short.Tag");
        assert_eq!(inline_batch.len(), 1);
        assert_eq!(
            inline_batch.iter_str().collect::<Vec<_>>(),
            vec!["Short.Tag"]
        );
        assert_eq!(inline_batch.clone().into_vec(), vec!["Short.Tag"]);
        assert_eq!(inline_batch.into_shareable().len(), 1);

        let long_batch =
            TagBatch::from_str_lenient("Very.Long.Tag.That.Exceeds.Thirty.One.Bytes.Identifier");
        assert_eq!(long_batch.len(), 1);
        assert_eq!(
            long_batch.iter_str().collect::<Vec<_>>(),
            vec!["Very.Long.Tag.That.Exceeds.Thirty.One.Bytes.Identifier"]
        );

        // 7. Arc<[String]>
        let arc_slice: Arc<[String]> =
            Arc::from(vec!["Arc1".to_string(), "Arc2".to_string()].into_boxed_slice());
        let batch = arc_slice.into_tag_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Arc1", "Arc2"]);
        assert_eq!(batch.into_vec(), vec!["Arc1", "Arc2"]);

        // 8. Single owned String
        let single_string = "OwnedSingle".to_string();
        let batch = single_string.into_tag_batch();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["OwnedSingle"]);
        assert_eq!(batch.into_vec(), vec!["OwnedSingle"]);

        // 9. Empty static slice
        let empty_batch = (&[] as &[&str]).into_tag_batch();
        assert_eq!(empty_batch.len(), 0);
        assert!(empty_batch.is_empty());
        assert_eq!(empty_batch.iter_str().count(), 0);
        assert!(empty_batch.into_vec().is_empty());
    }

    #[test]
    fn test_tag_batch_into_shareable() {
        let owned_batch = vec!["TagA".to_string(), "TagB".to_string()].into_tag_batch();
        let shareable = owned_batch.into_shareable();
        assert!(matches!(shareable.repr, TagBatchRepr::Shared(_)));
        assert_eq!(shareable.len(), 2);
        assert_eq!(
            shareable.iter_str().collect::<Vec<_>>(),
            vec!["TagA", "TagB"]
        );

        let static_batch = ["Static1", "Static2"].into_tag_batch();
        let shareable_static = static_batch.into_shareable();
        assert_eq!(shareable_static.len(), 2);
    }

    #[test]
    fn test_tag_batch_encapsulation_methods() {
        static TAGS: &[&str] = &["Tag1", "Tag2"];
        let batch_static = TagBatch::from_static(TAGS);
        assert_eq!(batch_static.len(), 2);
        assert!(!batch_static.is_empty());
        assert_eq!(
            batch_static.iter().collect::<Vec<_>>(),
            vec!["Tag1", "Tag2"]
        );
        assert_eq!(
            batch_static.iter_str().collect::<Vec<_>>(),
            vec!["Tag1", "Tag2"]
        );
        assert_eq!(batch_static.as_static_slice(), Some(TAGS));
        assert_eq!(batch_static.as_slice(), None);

        let vec_tags = vec!["TagA".to_string(), "TagB".to_string()];
        let batch_owned = TagBatch::from(vec_tags);
        assert_eq!(batch_owned.len(), 2);
        assert_eq!(batch_owned.as_slice().unwrap(), &["TagA", "TagB"]);
        assert_eq!(batch_owned.as_static_slice(), None);

        let shared_tags: Arc<[String]> =
            Arc::from(vec!["S1".to_string(), "S2".to_string()].into_boxed_slice());
        let batch_shared = TagBatch::from(shared_tags);
        assert_eq!(batch_shared.len(), 2);
        assert_eq!(batch_shared.as_slice().unwrap(), &["S1", "S2"]);
        assert_eq!(batch_shared.as_static_slice(), None);

        let shareable_static = batch_static.into_shareable();
        assert_eq!(shareable_static.as_static_slice(), Some(TAGS));

        let shareable_owned = batch_owned.into_shareable();
        assert_eq!(shareable_owned.len(), 2);
        assert_eq!(shareable_owned.as_slice().unwrap(), &["TagA", "TagB"]);

        let empty = TagBatch::empty();
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.as_static_slice(), Some(&[][..]));
    }

    #[test]
    fn test_tag_batch_value_equality() {
        static TAGS: &[&str] = &["Tag1", "Tag2"];
        let b_static = TagBatch::from_static(TAGS);
        let b_owned = TagBatch::from(vec!["Tag1".to_string(), "Tag2".to_string()]);
        let b_sso = TagBatch::from_str_lenient("Tag1");
        let b_sso_owned = TagBatch::from(vec!["Tag1".to_string()]);

        assert_eq!(
            b_static, b_owned,
            "Static and Owned batches with identical items must be equal"
        );
        assert_eq!(
            b_sso, b_sso_owned,
            "SSO inline and OwnedSingle with identical items must be equal"
        );
        assert_ne!(b_static, b_sso);
    }

    #[test]
    fn test_tag_batch_from_iterator() {
        let strings = vec!["Tag1".to_string(), "Tag2".to_string()];
        let batch_owned: TagBatch = strings.into_iter().collect();
        assert_eq!(batch_owned.len(), 2);
        assert_eq!(
            batch_owned.iter_str().collect::<Vec<_>>(),
            vec!["Tag1", "Tag2"]
        );

        let slices = ["TagA", "TagB", "TagC"];
        let batch_slices: TagBatch = slices.into_iter().collect();
        assert_eq!(batch_slices.len(), 3);
        assert_eq!(
            batch_slices.iter_str().collect::<Vec<_>>(),
            vec!["TagA", "TagB", "TagC"]
        );

        let empty_batch: TagBatch = std::iter::empty::<String>().collect();
        assert_eq!(empty_batch.len(), 0);
        assert!(empty_batch.is_empty());
    }

    #[test]
    fn test_tag_batch_sso_boundary_and_multibyte_safety() {
        // 0-byte empty string
        let batch_empty = TagBatch::from_str_lenient("");
        assert_eq!(batch_empty.len(), 1);
        assert_eq!(batch_empty.iter_str().collect::<Vec<_>>(), vec![""]);
        assert_eq!(batch_empty.into_vec(), vec![""]);

        // Exactly 31-byte string
        let s31 = "1234567890123456789012345678901";
        assert_eq!(s31.len(), 31);
        let batch_31 = TagBatch::from_str_lenient(s31);
        assert_eq!(batch_31.len(), 1);
        assert_eq!(batch_31.iter_str().next(), Some(s31));
        assert_eq!(batch_31.into_vec(), vec![s31.to_string()]);

        // Exactly 32-byte string
        let s32 = "12345678901234567890123456789012";
        assert_eq!(s32.len(), 32);
        let batch_32 = TagBatch::from_str_lenient(s32);
        assert_eq!(batch_32.len(), 1);
        assert_eq!(batch_32.iter_str().next(), Some(s32));
        assert_eq!(batch_32.into_vec(), vec![s32.to_string()]);

        // Multibyte UTF-8: Japanese
        let jp = "タグ１２３";
        assert_eq!(jp.len(), 15);
        let batch_jp = TagBatch::from_str_lenient(jp);
        assert_eq!(batch_jp.len(), 1);
        assert_eq!(batch_jp.iter_str().next(), Some(jp));
        assert_eq!(batch_jp.into_vec(), vec![jp.to_string()]);

        // Multibyte UTF-8: Emoji
        let emoji = "🚀🏭⚡";
        assert_eq!(emoji.len(), 11);
        let batch_emoji = TagBatch::from_str_lenient(emoji);
        assert_eq!(batch_emoji.len(), 1);
        assert_eq!(batch_emoji.iter_str().next(), Some(emoji));

        // 33-byte multibyte fallback
        let s33 = format!("{}タグ", "A".repeat(27));
        assert_eq!(s33.len(), 33);
        let batch_33 = TagBatch::from_str_lenient(&s33);
        assert_eq!(batch_33.iter_str().next(), Some(s33.as_str()));
    }
}
