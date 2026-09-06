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
    /// 32-bit integer (`VT_I4`).
    Int(i32),
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
    /// Returns `Some(i32)` if this value is [`OpcValue::Int`], or `None` otherwise.
    #[inline]
    #[must_use]
    pub const fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(i) => Some(*i),
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
        Self::Int(val)
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
        if let Ok(i) = trimmed.parse::<i32>() {
            return Ok(Self::Int(i));
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
