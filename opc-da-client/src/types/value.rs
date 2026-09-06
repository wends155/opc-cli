//! Canonical OPC DA Data Access value variant and zero-allocation formatting helpers.

use std::fmt;

/// Canonical OPC DA Data Access value variant.
///
/// Encapsulates all standard COM automation data types supported by OPC DA 2.05a / 3.0.
///
/// # Examples
///
/// ```
/// use opc_da_client::OpcValue;
///
/// let val: OpcValue = "42".parse().unwrap();
/// assert_eq!(val, OpcValue::Int(42));
/// assert_eq!(val.as_int(), Some(42));
/// assert_eq!(val.to_string(), "42");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum OpcValue {
    /// String value (`VT_BSTR`) — server may coerce to target type.
    String(String),
    /// 64-bit signed integer (`VT_I8` or widened `VT_I1`/`VT_I2`/`VT_I4`).
    Int(i64),
    /// 64-bit unsigned integer (`VT_UI8` or widened `VT_UI1`/`VT_UI2`/`VT_UI4`).
    UInt(u64),
    /// 64-bit float (`VT_R8`).
    Float(f64),
    /// Boolean (`VT_BOOL`).
    Bool(bool),
    /// Empty value (`VT_EMPTY`) — uninitialized or absent variant.
    Empty,
    /// Null value (`VT_NULL`) — explicitly null variant.
    Null,
}

impl fmt::Display for OpcValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::UInt(u) => write!(f, "{u}"),
            Self::Float(fl) => write!(f, "{fl}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Empty => write!(f, "Empty"),
            Self::Null => write!(f, "Null"),
        }
    }
}

impl OpcValue {
    /// Returns the integer value if this is an [`OpcValue::Int`].
    ///
    /// # Returns
    ///
    /// Returns `Some(i64)` if this value is [`OpcValue::Int`], or `None` otherwise.
    #[inline]
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the unsigned integer value if this is an [`OpcValue::UInt`].
    ///
    /// # Returns
    ///
    /// Returns `Some(u64)` if this value is [`OpcValue::UInt`], or `None` otherwise.
    #[inline]
    #[must_use]
    pub const fn as_uint(&self) -> Option<u64> {
        match self {
            Self::UInt(u) => Some(*u),
            _ => None,
        }
    }

    /// Returns the float value if this is an [`OpcValue::Float`].
    ///
    /// # Returns
    ///
    /// Returns `Some(f64)` if this value is [`OpcValue::Float`], or `None` otherwise.
    #[inline]
    #[must_use]
    pub const fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the boolean value if this is an [`OpcValue::Bool`].
    ///
    /// # Returns
    ///
    /// Returns `Some(bool)` if this value is [`OpcValue::Bool`], or `None` otherwise.
    #[inline]
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns a borrowed string slice if this is an [`OpcValue::String`].
    ///
    /// # Returns
    ///
    /// Returns `Some(&str)` if this value is [`OpcValue::String`], or `None` otherwise.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns `true` if this value is [`OpcValue::Empty`].
    ///
    /// # Returns
    ///
    /// Returns `true` if this value represents an uninitialized or empty variant, `false` otherwise.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns `true` if this value is [`OpcValue::Null`].
    ///
    /// # Returns
    ///
    /// Returns `true` if this value represents a SQL/COM NULL variant, `false` otherwise.
    #[inline]
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl From<i32> for OpcValue {
    #[inline]
    fn from(val: i32) -> Self {
        Self::Int(i64::from(val))
    }
}

impl From<i64> for OpcValue {
    #[inline]
    fn from(val: i64) -> Self {
        Self::Int(val)
    }
}

impl From<u32> for OpcValue {
    #[inline]
    fn from(val: u32) -> Self {
        Self::UInt(u64::from(val))
    }
}

impl From<u64> for OpcValue {
    #[inline]
    fn from(val: u64) -> Self {
        Self::UInt(val)
    }
}

impl From<u16> for OpcValue {
    #[inline]
    fn from(val: u16) -> Self {
        Self::UInt(u64::from(val))
    }
}

impl From<i16> for OpcValue {
    #[inline]
    fn from(val: i16) -> Self {
        Self::Int(i64::from(val))
    }
}

impl From<u8> for OpcValue {
    #[inline]
    fn from(val: u8) -> Self {
        Self::UInt(u64::from(val))
    }
}

impl From<i8> for OpcValue {
    #[inline]
    fn from(val: i8) -> Self {
        Self::Int(i64::from(val))
    }
}

impl Default for OpcValue {
    #[inline]
    fn default() -> Self {
        Self::Empty
    }
}

impl From<f32> for OpcValue {
    #[inline]
    fn from(val: f32) -> Self {
        Self::Float(f64::from(val))
    }
}

impl From<f64> for OpcValue {
    #[inline]
    fn from(val: f64) -> Self {
        Self::Float(val)
    }
}

impl From<bool> for OpcValue {
    #[inline]
    fn from(val: bool) -> Self {
        Self::Bool(val)
    }
}

impl From<String> for OpcValue {
    #[inline]
    fn from(val: String) -> Self {
        Self::String(val)
    }
}

impl From<&str> for OpcValue {
    #[inline]
    fn from(val: &str) -> Self {
        Self::String(val.to_string())
    }
}

#[allow(
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]
impl TryFrom<OpcValue> for i64 {
    type Error = crate::errors::OpcError;

    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::Int(i) => Ok(i),
            OpcValue::UInt(u) => i64::try_from(u)
                .map_err(|_| crate::errors::OpcError::Conversion("UInt exceeds i64 range".into())),
            OpcValue::Float(f)
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 =>
            {
                Ok(f as i64)
            }
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to i64"
            ))),
        }
    }
}

#[allow(
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]
impl TryFrom<OpcValue> for i32 {
    type Error = crate::errors::OpcError;

    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::Int(i) => i32::try_from(i)
                .map_err(|_| crate::errors::OpcError::Conversion("Int exceeds i32 range".into())),
            OpcValue::UInt(u) => i32::try_from(u)
                .map_err(|_| crate::errors::OpcError::Conversion("UInt exceeds i32 range".into())),
            OpcValue::Float(f)
                if f.fract() == 0.0 && f >= i32::MIN as f64 && f <= i32::MAX as f64 =>
            {
                Ok(f as i32)
            }
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to i32"
            ))),
        }
    }
}

#[allow(
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::cast_sign_loss
)]
impl TryFrom<OpcValue> for u64 {
    type Error = crate::errors::OpcError;

    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::UInt(u) => Ok(u),
            OpcValue::Int(i) => u64::try_from(i).map_err(|_| {
                crate::errors::OpcError::Conversion("Negative Int cannot convert to u64".into())
            }),
            OpcValue::Float(f) if f.fract() == 0.0 && f >= 0.0 && f <= u64::MAX as f64 => {
                Ok(f as u64)
            }
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to u64"
            ))),
        }
    }
}

#[allow(
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::cast_sign_loss
)]
impl TryFrom<OpcValue> for u32 {
    type Error = crate::errors::OpcError;

    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::UInt(u) => u32::try_from(u)
                .map_err(|_| crate::errors::OpcError::Conversion("UInt exceeds u32 range".into())),
            OpcValue::Int(i) => u32::try_from(i)
                .map_err(|_| crate::errors::OpcError::Conversion("Int out of u32 range".into())),
            OpcValue::Float(f) if f.fract() == 0.0 && f >= 0.0 && f <= u32::MAX as f64 => {
                Ok(f as u32)
            }
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to u32"
            ))),
        }
    }
}

impl TryFrom<OpcValue> for f64 {
    type Error = crate::errors::OpcError;

    #[allow(clippy::cast_precision_loss)]
    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::Float(f) => Ok(f),
            OpcValue::Int(i) => Ok(i as Self),
            OpcValue::UInt(u) => Ok(u as Self),
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to f64"
            ))),
        }
    }
}

impl TryFrom<OpcValue> for f32 {
    type Error = crate::errors::OpcError;

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::Float(f) => {
                if f.is_nan()
                    || f.is_infinite()
                    || (f >= f64::from(Self::MIN) && f <= f64::from(Self::MAX))
                {
                    Ok(f as Self)
                } else {
                    Err(crate::errors::OpcError::Conversion(
                        "Float exceeds f32 range".into(),
                    ))
                }
            }
            OpcValue::Int(i) => Ok(i as Self),
            OpcValue::UInt(u) => Ok(u as Self),
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to f32"
            ))),
        }
    }
}

impl TryFrom<OpcValue> for bool {
    type Error = crate::errors::OpcError;

    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::Bool(b) => Ok(b),
            OpcValue::Int(i) => Ok(i != 0),
            OpcValue::UInt(u) => Ok(u != 0),
            other => Err(crate::errors::OpcError::Conversion(format!(
                "Cannot convert {other:?} to bool"
            ))),
        }
    }
}

impl TryFrom<OpcValue> for String {
    type Error = crate::errors::OpcError;

    fn try_from(value: OpcValue) -> Result<Self, Self::Error> {
        match value {
            OpcValue::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }
}

impl std::str::FromStr for OpcValue {
    type Err = std::convert::Infallible;

    /// Parses a string into an [`OpcValue`], prioritizing boolean keywords,
    /// special variants, 32-bit integers, 64-bit floats, and defaulting to string.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();

        if trimmed.eq_ignore_ascii_case("true") {
            return Ok(Self::Bool(true));
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Ok(Self::Bool(false));
        }
        if trimmed.eq_ignore_ascii_case("empty") {
            return Ok(Self::Empty);
        }
        if trimmed.eq_ignore_ascii_case("null") {
            return Ok(Self::Null);
        }
        if let Ok(i) = trimmed.parse::<i64>() {
            return Ok(Self::Int(i));
        }
        if let Ok(u) = trimmed.parse::<u64>() {
            return Ok(Self::UInt(u));
        }
        if let Ok(f) = trimmed.parse::<f64>() {
            return Ok(Self::Float(f));
        }
        Ok(Self::String(s.to_string()))
    }
}

/// Zero-allocation display adapter for [`Option<OpcValue>`].
///
/// Implements [`std::fmt::Display`] to stream the formatted inner value or
/// a fallback string directly into the output formatter without heap allocations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayOptionOpcValue<'a> {
    opt: Option<&'a OpcValue>,
    fallback: &'a str,
}

impl std::fmt::Display for DisplayOptionOpcValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self.opt {
            Some(v) => {
                if f.width().is_some() {
                    let s = v.to_string();
                    f.pad(&s)
                } else {
                    write!(f, "{v}")
                }
            }
            None => f.pad(self.fallback),
        }
    }
}

/// Zero-allocation display adapter for [`Option<std::time::SystemTime>`].
///
/// Implements [`std::fmt::Display`] to stream a local formatted timestamp or
/// a fallback string directly into the output formatter without heap allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayOptionTimestamp<'a> {
    opt: Option<std::time::SystemTime>,
    fallback: &'a str,
}

impl std::fmt::Display for DisplayOptionTimestamp<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self.opt {
            Some(ts) if ts != std::time::SystemTime::UNIX_EPOCH => {
                let dt: chrono::DateTime<chrono::Local> = ts.into();
                let formatted = dt.format("%Y-%m-%d %H:%M:%S");
                if f.width().is_some() {
                    let s = formatted.to_string();
                    f.pad(&s)
                } else {
                    write!(f, "{formatted}")
                }
            }
            _ => f.pad(self.fallback),
        }
    }
}

/// Extension trait providing zero-allocation formatting helpers for [`Option<OpcValue>`].
pub trait OpcValueOptionExt {
    /// Returns a zero-allocation display adapter with custom fallback text.
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionOpcValue<'a>;

    /// Returns a zero-allocation display adapter with the canonical default fallback (`"Error"`).
    fn display(&self) -> DisplayOptionOpcValue<'_> {
        self.display_or("Error")
    }
}

impl OpcValueOptionExt for Option<OpcValue> {
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionOpcValue<'a> {
        DisplayOptionOpcValue {
            opt: self.as_ref(),
            fallback,
        }
    }
}

impl OpcValueOptionExt for Option<&OpcValue> {
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionOpcValue<'a> {
        DisplayOptionOpcValue {
            opt: *self,
            fallback,
        }
    }
}

/// Extension trait providing zero-allocation formatting helpers for [`Option<std::time::SystemTime>`].
pub trait SystemTimeOptionExt {
    /// Returns a zero-allocation display adapter with custom fallback text.
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionTimestamp<'a>;

    /// Returns a zero-allocation display adapter with the canonical default fallback (`"N/A"`).
    fn display(&self) -> DisplayOptionTimestamp<'_> {
        self.display_or("N/A")
    }
}

impl SystemTimeOptionExt for Option<std::time::SystemTime> {
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionTimestamp<'a> {
        DisplayOptionTimestamp {
            opt: *self,
            fallback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opc_value_int_uint_and_try_from() {
        let v_int = OpcValue::Int(42);
        assert_eq!(v_int.as_int(), Some(42));
        assert_eq!(v_int.as_uint(), None);
        assert_eq!(i32::try_from(v_int.clone()).unwrap(), 42);
        assert_eq!(i64::try_from(v_int.clone()).unwrap(), 42);
        assert_eq!(u32::try_from(v_int.clone()).unwrap(), 42);
        assert_eq!(u64::try_from(v_int).unwrap(), 42);

        let val_u64 = OpcValue::UInt(4_294_967_295);
        assert_eq!(val_u64.as_uint(), Some(4_294_967_295));
        assert_eq!(val_u64.as_int(), None);
        assert_eq!(u32::try_from(val_u64.clone()).unwrap(), 4_294_967_295);
        assert_eq!(u64::try_from(val_u64.clone()).unwrap(), 4_294_967_295);
        assert_eq!(i64::try_from(val_u64.clone()).unwrap(), 4_294_967_295);
        assert!(i32::try_from(val_u64).is_err()); // Exceeds i32 range

        let parsed: OpcValue = "100".parse().unwrap();
        assert_eq!(parsed, OpcValue::Int(100));

        let parsed_uint: OpcValue = "9223372036854775808".parse().unwrap();
        assert_eq!(parsed_uint, OpcValue::UInt(9_223_372_036_854_775_808));
    }
}
