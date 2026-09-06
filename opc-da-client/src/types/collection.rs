//! Canonical tag read and collection models.

use super::quality::OpcQuality;
use super::value::{OpcValue, OpcValueOptionExt, SystemTimeOptionExt};
use crate::errors::OpcError;

/// A single tag's read result.
///
/// Returned by tag read operations.
///
/// # Examples
///
/// ```
/// use opc_da_client::{OpcQuality, OpcValue, TagValue};
/// use std::time::SystemTime;
///
/// let tv = TagValue::new(
///     "Simulation.Random.1",
///     Some(OpcValue::Float(42.5)),
///     OpcQuality::GOOD,
///     Some(SystemTime::UNIX_EPOCH),
/// );
/// assert_eq!(tv.tag_id, "Simulation.Random.1");
/// assert!(tv.is_good());
/// assert_eq!(tv.display_value(), "42.5");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TagValue {
    /// The fully qualified tag identifier (e.g., `"Channel1.Device1.Tag1"`).
    pub tag_id: String,
    /// The read outcome containing either the successfully decoded value or the server/communication error.
    pub outcome: Result<OpcValue, OpcError>,
    /// OPC quality status, decomposed into major quality, substatus, and limit bits.
    pub quality: OpcQuality,
    /// Timestamp of the last value change (UTC-based), or `None` if unavailable.
    pub timestamp: Option<std::time::SystemTime>,
}

impl Default for TagValue {
    fn default() -> Self {
        Self {
            tag_id: String::new(),
            outcome: Ok(OpcValue::Empty),
            quality: OpcQuality::default(),
            timestamp: None,
        }
    }
}

impl TagValue {
    /// Creates a new `TagValue`.
    #[inline]
    #[must_use]
    pub fn new(
        tag_id: impl Into<String>,
        value: Option<OpcValue>,
        quality: OpcQuality,
        timestamp: Option<std::time::SystemTime>,
    ) -> Self {
        Self {
            tag_id: tag_id.into(),
            outcome: Ok(value.unwrap_or(OpcValue::Empty)),
            quality,
            timestamp,
        }
    }

    /// Creates a new successful `TagValue`.
    #[inline]
    #[must_use]
    pub fn success(
        tag_id: impl Into<String>,
        value: OpcValue,
        quality: OpcQuality,
        timestamp: Option<std::time::SystemTime>,
    ) -> Self {
        Self {
            tag_id: tag_id.into(),
            outcome: Ok(value),
            quality,
            timestamp,
        }
    }

    /// Creates a new `TagValue` with an error.
    #[inline]
    #[must_use]
    pub fn with_error(tag_id: impl Into<String>, quality: OpcQuality, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            outcome: Err(error),
            quality,
            timestamp: None,
        }
    }

    /// Returns a reference to the decoded value if present and successful.
    #[inline]
    #[must_use]
    pub fn value(&self) -> Option<&OpcValue> {
        self.outcome.as_ref().ok()
    }

    /// Returns a reference to the underlying error if the read failed.
    #[inline]
    #[must_use]
    pub fn error(&self) -> Option<&OpcError> {
        self.outcome.as_ref().err()
    }

    /// Returns a reference to the read outcome.
    #[inline]
    pub fn outcome(&self) -> Result<&OpcValue, &OpcError> {
        self.outcome.as_ref()
    }

    /// Returns `true` if quality is good and the read was successful.
    ///
    /// # Returns
    ///
    /// `true` if [`TagValue::quality`] satisfies [`OpcQuality::is_good`] and [`TagValue::outcome`] is `Ok(_)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue};
    ///
    /// let tv = TagValue::new("Tag1", Some(OpcValue::Int(10)), OpcQuality::GOOD, None);
    /// assert!(tv.is_good());
    /// ```
    #[must_use]
    pub fn is_good(&self) -> bool {
        self.quality.is_good() && self.outcome.is_ok()
    }

    /// Returns `true` if quality is uncertain and the read was successful.
    #[must_use]
    pub fn is_uncertain(&self) -> bool {
        self.quality.is_uncertain() && self.outcome.is_ok()
    }

    /// Returns `true` if quality is bad or the read encountered an error.
    #[must_use]
    pub fn is_bad(&self) -> bool {
        self.quality.is_bad() || self.outcome.is_err()
    }

    /// Returns `true` if the read operation encountered an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.outcome.is_err()
    }

    /// Returns a human-readable display string for the value (or `"Error"` if missing).
    ///
    /// For zero-allocation formatting into a formatter or stream, prefer using
    /// [`OpcValueOptionExt::display`] or [`OpcValueOptionExt::display_or`] on [`TagValue::value`].
    ///
    /// # Returns
    ///
    /// A newly allocated [`String`] representation of the value, or `"Error"` if [`TagValue::value`] is `None`.
    #[must_use]
    pub fn display_value(&self) -> String {
        match &self.outcome {
            Ok(v) => v.to_string(),
            Err(_) => "Error".to_string(),
        }
    }

    /// Returns a human-readable formatted local timestamp string (or `"N/A"` if missing).
    ///
    /// For zero-allocation formatting into a formatter or stream, prefer using
    /// [`SystemTimeOptionExt::display`] or [`SystemTimeOptionExt::display_or`] on [`TagValue::timestamp`].
    ///
    /// # Returns
    ///
    /// A [`String`] formatted as `"YYYY-MM-DD HH:MM:SS"` in local time, or `"N/A"` if missing or epoch.
    #[must_use]
    pub fn formatted_timestamp(&self) -> String {
        self.timestamp.display().to_string()
    }

    /// Converts this `TagValue` into a strongly-typed [`TagResult`].
    ///
    /// If an error is present or value is missing, returns `Err(TagFailure)`.
    /// Otherwise returns `Ok(TagSuccess)`.
    pub fn into_result(self) -> TagResult {
        match self.outcome {
            Ok(val) => Ok(TagSuccess {
                tag_id: self.tag_id,
                value: val,
                quality: self.quality,
                timestamp: self.timestamp,
            }),
            Err(err) => Err(TagFailure {
                tag_id: self.tag_id,
                quality: self.quality,
                error: err,
            }),
        }
    }

    /// Converts a reference to this `TagValue` into a [`TagResult`].
    pub fn to_result(&self) -> TagResult {
        match &self.outcome {
            Ok(val) => Ok(TagSuccess {
                tag_id: self.tag_id.clone(),
                value: val.clone(),
                quality: self.quality,
                timestamp: self.timestamp,
            }),
            Err(err) => Err(TagFailure {
                tag_id: self.tag_id.clone(),
                quality: self.quality,
                error: err.clone(),
            }),
        }
    }
}

/// A successful tag read outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct TagSuccess {
    /// The fully qualified tag identifier.
    pub tag_id: String,
    /// The decoded value.
    pub value: OpcValue,
    /// OPC quality status.
    pub quality: OpcQuality,
    /// Timestamp of the last value change (UTC-based), or `None` if unavailable.
    pub timestamp: Option<std::time::SystemTime>,
}

impl TagSuccess {
    /// Creates a new `TagSuccess`.
    #[must_use]
    pub fn new(
        tag_id: impl Into<String>,
        value: OpcValue,
        quality: OpcQuality,
        timestamp: Option<std::time::SystemTime>,
    ) -> Self {
        Self {
            tag_id: tag_id.into(),
            value,
            quality,
            timestamp,
        }
    }
}

/// A failed tag read outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct TagFailure {
    /// The fully qualified tag identifier.
    pub tag_id: String,
    /// OPC quality status associated with the failure.
    pub quality: OpcQuality,
    /// Underlying error explaining the read failure.
    pub error: OpcError,
}

impl TagFailure {
    /// Creates a new `TagFailure`.
    #[must_use]
    pub fn new(tag_id: impl Into<String>, quality: OpcQuality, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            quality,
            error,
        }
    }
}

/// Strongly-typed canonical result for an individual tag read operation.
pub type TagResult = Result<TagSuccess, TagFailure>;

impl From<TagSuccess> for TagValue {
    fn from(s: TagSuccess) -> Self {
        Self {
            tag_id: s.tag_id,
            outcome: Ok(s.value),
            quality: s.quality,
            timestamp: s.timestamp,
        }
    }
}

impl From<TagFailure> for TagValue {
    fn from(f: TagFailure) -> Self {
        Self {
            tag_id: f.tag_id,
            outcome: Err(f.error),
            quality: f.quality,
            timestamp: None,
        }
    }
}

impl From<TagResult> for TagValue {
    fn from(res: TagResult) -> Self {
        match res {
            Ok(s) => s.into(),
            Err(f) => f.into(),
        }
    }
}

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} = {} [{}] @ {}",
            self.tag_id,
            self.value().display(),
            self.quality,
            self.timestamp.display()
        )
    }
}

/// Errors that can occur when extracting typed tag values from a [`TagValues`] collection.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum TagExtractError {
    /// The requested tag identifier was not included in the read batch.
    #[error("Tag '{0}' was not requested in this read batch")]
    NotRequested(String),

    /// The tag was requested, but reading failed on the server.
    #[error("Tag '{tag}' read failed on server: {source}")]
    ReadFailed {
        /// The failed tag identifier.
        tag: String,
        /// The underlying server or communication error.
        source: OpcError,
    },

    /// The tag exists, but returned a null, empty, or missing value.
    #[error("Tag '{0}' returned no value (null or missing)")]
    NoValue(String),

    /// The tag value could not be coerced to the requested target type without data loss.
    #[error("Tag '{tag}' value '{value}' cannot be converted to {expected}")]
    TypeMismatch {
        /// The tag identifier.
        tag: String,
        /// The string representation of the actual value.
        value: String,
        /// The expected target type description (e.g. `"f64"`, `"i32"`).
        expected: &'static str,
    },
}

impl From<TagExtractError> for OpcError {
    fn from(err: TagExtractError) -> Self {
        match err {
            TagExtractError::ReadFailed { source, .. } => source,
            other => Self::Conversion(other.to_string()),
        }
    }
}

/// A rich collection of read tag values providing case-insensitive lookups,
/// zero-allocation linear scanning, lenient typed extractions, and error preservation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagValues {
    items: Vec<TagValue>,
}

impl TagValues {
    /// Creates a new `TagValues` collection wrapping the given vector of items.
    ///
    /// # Arguments
    ///
    /// * `items` - Vector of [`TagValue`] items.
    ///
    /// # Returns
    ///
    /// A new [`TagValues`] instance containing the provided items.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let items = vec![TagValue::new("Tag1", Some(OpcValue::Int(10)), OpcQuality::GOOD, None)];
    /// let values = TagValues::new(items);
    /// assert_eq!(values.len(), 1);
    /// ```
    #[inline]
    #[must_use]
    pub fn new(items: Vec<TagValue>) -> Self {
        Self { items }
    }

    /// Returns the number of tag values in this collection.
    ///
    /// # Returns
    ///
    /// The number of [`TagValue`] items in this collection.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::TagValues;
    ///
    /// let values = TagValues::default();
    /// assert_eq!(values.len(), 0);
    /// ```
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if this collection contains no items.
    ///
    /// # Returns
    ///
    /// `true` if the collection contains 0 items; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::TagValues;
    ///
    /// let values = TagValues::default();
    /// assert!(values.is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Looks up a tag by identifier using case-insensitive comparison.
    ///
    /// Performs a zero-allocation linear scan over the internal items slice.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// An [`Option`] containing a reference to the matching [`TagValue`], or `None` if not found.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("Channel1.Device1.Tag1", Some(OpcValue::Int(42)), OpcQuality::GOOD, None)]);
    /// assert!(values.get("channel1.device1.tag1").is_some());
    /// assert!(values.get("NonExistent").is_none());
    /// ```
    #[must_use]
    pub fn get(&self, tag: &str) -> Option<&TagValue> {
        self.items
            .iter()
            .find(|tv| tv.tag_id.eq_ignore_ascii_case(tag))
    }

    /// Looks up an OPC value by tag identifier using case-insensitive comparison.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// An [`Option`] containing a reference to the [`OpcValue`] if the tag exists and its value is `Some`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("Tag1", Some(OpcValue::Int(42)), OpcQuality::GOOD, None)]);
    /// assert_eq!(values.get_value("tag1"), Some(&OpcValue::Int(42)));
    /// ```
    #[must_use]
    pub fn get_value(&self, tag: &str) -> Option<&OpcValue> {
        self.get(tag).and_then(|tv| tv.value())
    }

    /// Helper to look up an item and validate that it has an available value.
    fn get_value_checked(&self, tag: &str) -> Result<&OpcValue, TagExtractError> {
        let item = self
            .get(tag)
            .ok_or_else(|| TagExtractError::NotRequested(tag.to_string()))?;
        match &item.outcome {
            Ok(val) => match val {
                OpcValue::Empty | OpcValue::Null => Err(TagExtractError::NoValue(tag.to_string())),
                _ => Ok(val),
            },
            Err(err) => Err(TagExtractError::ReadFailed {
                tag: tag.to_string(),
                source: err.clone(),
            }),
        }
    }

    /// Extracts a value coerced into the requested target type `T` using lossless lenient conversion.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded value converted to `T`.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value cannot be losslessly converted to `T`.
    pub fn get_as<T>(&self, tag: &str) -> Result<T, TagExtractError>
    where
        T: TryFrom<OpcValue>,
    {
        let val = self.get_value_checked(tag)?;
        T::try_from(val.clone()).map_err(|_| TagExtractError::TypeMismatch {
            tag: tag.to_string(),
            value: val.to_string(),
            expected: std::any::type_name::<T>(),
        })
    }

    /// Extracts a 64-bit floating point value for the given tag,
    /// losslessly coercing integer values to float.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `f64` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value cannot be losslessly converted to `f64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("Sensor.Temp", Some(OpcValue::Float(98.6)), OpcQuality::GOOD, None)]);
    /// assert_eq!(values.get_f64("sensor.temp").unwrap(), 98.6);
    /// ```
    pub fn get_f64(&self, tag: &str) -> Result<f64, TagExtractError> {
        self.get_as::<f64>(tag)
    }

    /// Extracts a 32-bit floating point value for the given tag,
    /// losslessly coercing integer values to float.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `f32` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value cannot be losslessly converted to `f32`.
    pub fn get_f32(&self, tag: &str) -> Result<f32, TagExtractError> {
        self.get_as::<f32>(tag)
    }

    /// Extracts a 32-bit signed integer value for the given tag,
    /// losslessly converting exact whole-number floats without fractional parts.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `i32` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value has a fractional part, is out of range, or is not numeric.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("Counter", Some(OpcValue::Int(100)), OpcQuality::GOOD, None)]);
    /// assert_eq!(values.get_i32("counter").unwrap(), 100);
    /// ```
    pub fn get_i32(&self, tag: &str) -> Result<i32, TagExtractError> {
        self.get_as::<i32>(tag)
    }

    /// Extracts a 64-bit signed integer value for the given tag,
    /// losslessly converting exact whole-number floats without fractional parts.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `i64` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value has a fractional part, is out of range, or is not numeric.
    pub fn get_i64(&self, tag: &str) -> Result<i64, TagExtractError> {
        self.get_as::<i64>(tag)
    }

    /// Extracts a 32-bit unsigned integer value for the given tag,
    /// losslessly converting exact positive whole-number floats without fractional parts.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `u32` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value has a fractional part, is negative, is out of range, or is not numeric.
    pub fn get_u32(&self, tag: &str) -> Result<u32, TagExtractError> {
        self.get_as::<u32>(tag)
    }

    /// Extracts a 64-bit unsigned integer value for the given tag,
    /// losslessly converting exact positive whole-number floats without fractional parts.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `u64` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value has a fractional part, is negative, is out of range, or is not numeric.
    pub fn get_u64(&self, tag: &str) -> Result<u64, TagExtractError> {
        self.get_as::<u64>(tag)
    }

    /// Extracts a boolean value for the given tag.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// The decoded `bool` value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value is not a boolean.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("Pump.Status", Some(OpcValue::Bool(true)), OpcQuality::GOOD, None)]);
    /// assert_eq!(values.get_bool("pump.status").unwrap(), true);
    /// ```
    pub fn get_bool(&self, tag: &str) -> Result<bool, TagExtractError> {
        self.get_as::<bool>(tag)
    }

    /// Extracts a borrowed string slice for the given tag.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// A borrowed `&str` reference to the string value.
    ///
    /// # Errors
    ///
    /// * [`TagExtractError::NotRequested`] - Tag was not included in this read batch.
    /// * [`TagExtractError::ReadFailed`] - Tag read failed on the server.
    /// * [`TagExtractError::NoValue`] - Tag returned a null or empty value.
    /// * [`TagExtractError::TypeMismatch`] - Value is not a string.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("System.Mode", Some(OpcValue::String("RUN".into())), OpcQuality::GOOD, None)]);
    /// assert_eq!(values.get_str("system.mode").unwrap(), "RUN");
    /// ```
    pub fn get_str(&self, tag: &str) -> Result<&str, TagExtractError> {
        let val = self.get_value_checked(tag)?;
        match val {
            OpcValue::String(s) => Ok(s.as_str()),
            _ => Err(TagExtractError::TypeMismatch {
                tag: tag.to_string(),
                value: val.to_string(),
                expected: "&str",
            }),
        }
    }

    /// Consumes this collection and returns the inner vector of [`TagValue`] items.
    ///
    /// # Returns
    ///
    /// The underlying [`Vec<TagValue>`].
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::TagValues;
    ///
    /// let values = TagValues::default();
    /// let vec = values.into_vec();
    /// assert!(vec.is_empty());
    /// ```
    #[must_use]
    pub fn into_vec(self) -> Vec<TagValue> {
        self.items
    }

    /// Borrows the items as a slice of [`TagValue`].
    ///
    /// # Returns
    ///
    /// A borrowed slice `&[TagValue]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::TagValues;
    ///
    /// let values = TagValues::default();
    /// assert!(values.as_slice().is_empty());
    /// ```
    #[must_use]
    pub fn as_slice(&self) -> &[TagValue] {
        &self.items
    }

    /// Returns an iterator over references to [`TagValue`] in this collection.
    ///
    /// # Returns
    ///
    /// An iterator yielding `&TagValue` references.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::TagValues;
    ///
    /// let values = TagValues::default();
    /// assert_eq!(values.iter().count(), 0);
    /// ```
    pub fn iter(&self) -> std::slice::Iter<'_, TagValue> {
        self.items.iter()
    }

    /// Returns an iterator yielding strongly-typed [`TagResult`] outcomes for each tag.
    pub fn iter_results(&self) -> impl Iterator<Item = TagResult> + '_ {
        self.items.iter().map(TagValue::to_result)
    }

    /// Looks up a tag value by numeric index in the collection.
    #[inline]
    #[must_use]
    pub fn get_index(&self, index: usize) -> Option<&TagValue> {
        self.items.get(index)
    }

    /// Clears all tag values from this collection.
    #[inline]
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Appends a new tag value to the end of this collection.
    #[inline]
    pub fn push(&mut self, item: TagValue) {
        self.items.push(item);
    }
}

impl std::ops::Deref for TagValues {
    type Target = [TagValue];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl AsRef<[TagValue]> for TagValues {
    #[inline]
    fn as_ref(&self) -> &[TagValue] {
        &self.items
    }
}

impl From<Vec<TagValue>> for TagValues {
    #[inline]
    fn from(items: Vec<TagValue>) -> Self {
        Self::new(items)
    }
}

impl From<TagValues> for Vec<TagValue> {
    #[inline]
    fn from(tvs: TagValues) -> Self {
        tvs.into_vec()
    }
}

impl IntoIterator for TagValues {
    type Item = TagValue;
    type IntoIter = std::vec::IntoIter<TagValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a TagValues {
    type Item = &'a TagValue;
    type IntoIter = std::slice::Iter<'a, TagValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl FromIterator<TagValue> for TagValues {
    #[inline]
    fn from_iter<I: IntoIterator<Item = TagValue>>(iter: I) -> Self {
        Self::new(iter.into_iter().collect())
    }
}
