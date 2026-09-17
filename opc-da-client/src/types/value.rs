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

impl From<&Self> for OpcValue {
    #[inline]
    fn from(val: &Self) -> Self {
        val.clone()
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
            OpcValue::UInt(u) => i64::try_from(u).map_err(crate::errors::OpcError::from),
            OpcValue::Float(f)
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f < i64::MAX as f64 =>
            {
                Ok(f as i64)
            }
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "i64",
                },
            )),
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
            OpcValue::Int(i) => i32::try_from(i).map_err(crate::errors::OpcError::from),
            OpcValue::UInt(u) => i32::try_from(u).map_err(crate::errors::OpcError::from),
            OpcValue::Float(f)
                if f.fract() == 0.0 && f >= i32::MIN as f64 && f <= i32::MAX as f64 =>
            {
                Ok(f as i32)
            }
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "i32",
                },
            )),
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
            OpcValue::Int(i) => u64::try_from(i).map_err(crate::errors::OpcError::from),
            OpcValue::Float(f) if f.fract() == 0.0 && f >= 0.0 && f < u64::MAX as f64 => {
                Ok(f as u64)
            }
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "u64",
                },
            )),
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
            OpcValue::UInt(u) => u32::try_from(u).map_err(crate::errors::OpcError::from),
            OpcValue::Int(i) => u32::try_from(i).map_err(crate::errors::OpcError::from),
            OpcValue::Float(f) if f.fract() == 0.0 && f >= 0.0 && f <= u32::MAX as f64 => {
                Ok(f as u32)
            }
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "u32",
                },
            )),
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
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "f64",
                },
            )),
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
                        crate::errors::ConversionError::TypeMismatch {
                            actual: format!("{f}"),
                            expected: "f32",
                        },
                    ))
                }
            }
            OpcValue::Int(i) => Ok(i as Self),
            OpcValue::UInt(u) => Ok(u as Self),
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "f32",
                },
            )),
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
            other => Err(crate::errors::OpcError::conversion(
                crate::errors::ConversionError::TypeMismatch {
                    actual: format!("{other:?}"),
                    expected: "bool",
                },
            )),
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
/// Implements [`std::fmt::Display`] to stream a standard UTC timestamp formatted as
/// `"YYYY-MM-DD HH:MM:SS"` or a fallback string directly into the output formatter
/// without heap allocations and without external crate dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayOptionTimestamp<'a> {
    opt: Option<std::time::SystemTime>,
    fallback: &'a str,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::unreadable_literal,
    clippy::many_single_char_names
)]
#[inline]
pub(crate) const fn secs_to_civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86400);
    let rem_secs = secs.rem_euclid(86400) as u32;
    let hour = rem_secs / 3600;
    let min = (rem_secs % 3600) / 60;
    let sec = rem_secs % 60;

    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }).div_euclid(146097);
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, hour, min, sec)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
#[inline]
pub(crate) fn format_system_time_buf(ts: std::time::SystemTime) -> Option<[u8; 19]> {
    let dur = ts.duration_since(std::time::SystemTime::UNIX_EPOCH).ok()?;
    let (y, m, d, hour, min, sec) = secs_to_civil(dur.as_secs() as i64);
    if !(0..=9999).contains(&y) {
        return None;
    }

    let mut buf = [0u8; 19];
    let y_u = y as u32;
    buf[0] = b'0' + ((y_u / 1000) % 10) as u8;
    buf[1] = b'0' + ((y_u / 100) % 10) as u8;
    buf[2] = b'0' + ((y_u / 10) % 10) as u8;
    buf[3] = b'0' + (y_u % 10) as u8;
    buf[4] = b'-';
    buf[5] = b'0' + ((m / 10) % 10) as u8;
    buf[6] = b'0' + (m % 10) as u8;
    buf[7] = b'-';
    buf[8] = b'0' + ((d / 10) % 10) as u8;
    buf[9] = b'0' + (d % 10) as u8;
    buf[10] = b' ';
    buf[11] = b'0' + ((hour / 10) % 10) as u8;
    buf[12] = b'0' + (hour % 10) as u8;
    buf[13] = b':';
    buf[14] = b'0' + ((min / 10) % 10) as u8;
    buf[15] = b'0' + (min % 10) as u8;
    buf[16] = b':';
    buf[17] = b'0' + ((sec / 10) % 10) as u8;
    buf[18] = b'0' + (sec % 10) as u8;

    Some(buf)
}

impl std::fmt::Display for DisplayOptionTimestamp<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self.opt {
            Some(ts) if ts != std::time::SystemTime::UNIX_EPOCH => {
                if let Some(buf) = format_system_time_buf(ts) {
                    let Ok(s) = std::str::from_utf8(&buf) else {
                        return f.pad(self.fallback);
                    };
                    return f.pad(s);
                }
                f.pad(self.fallback)
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

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::similar_names)]
    fn test_try_from_f64_precision_saturation() {
        let f_i64 = i64::MAX as f64; // 2^63 (9223372036854775808.0), exceeds i64::MAX
        let val_f64_i64 = OpcValue::Float(f_i64);
        let res_i64 = i64::try_from(val_f64_i64);
        assert!(
            res_i64.is_err(),
            "Float exceeding i64 precision should return error, got {:?}",
            res_i64
        );

        let f_u64 = u64::MAX as f64; // 2^64 (18446744073709551616.0), exceeds u64::MAX
        let val_f64_u64 = OpcValue::Float(f_u64);
        let res_u64 = u64::try_from(val_f64_u64);
        assert!(
            res_u64.is_err(),
            "Float exceeding u64 precision should return error, got {:?}",
            res_u64
        );
    }

    #[test]
    fn test_opc_value_f32_and_default() {
        assert_eq!(OpcValue::default(), OpcValue::Empty);
        let val: OpcValue = 12.5f32.into();
        assert_eq!(val, OpcValue::Float(12.5));
        let f: f32 = val.try_into().unwrap();
        assert!((f - 12.5).abs() < 1e-6);

        let int_val = OpcValue::Int(42);
        let f_from_int: f32 = int_val.try_into().unwrap();
        assert!((f_from_int - 42.0f32).abs() < f32::EPSILON);
    }

    #[test]
    #[allow(clippy::many_single_char_names, clippy::duration_suboptimal_units)]
    fn test_display_option_timestamp_civil() {
        use std::time::{Duration, SystemTime};

        // 1. Unix Epoch
        assert_eq!(format!("{}", Some(SystemTime::UNIX_EPOCH).display()), "N/A");

        // 2. Fixed Timestamp: 1700000000 = 2023-11-14 22:13:20 UTC
        let ts = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(format!("{}", Some(ts).display()), "2023-11-14 22:13:20");

        // 3. Leap Year: 2024-02-29 12:00:00 UTC = 1709208000
        let leap_ts = SystemTime::UNIX_EPOCH + Duration::from_secs(1_709_208_000);
        assert_eq!(
            format!("{}", Some(leap_ts).display()),
            "2024-02-29 12:00:00"
        );

        // 4. Pre-1970 via secs_to_civil: -86400 = 1969-12-31 00:00:00
        let (y, m, d, h, min, s) = secs_to_civil(-86400);
        assert_eq!((y, m, d, h, min, s), (1969, 12, 31, 0, 0, 0));
    }

    #[test]
    fn test_opc_value_display() {
        assert_eq!(OpcValue::String("hello".into()).to_string(), "hello");
        assert_eq!(OpcValue::Int(100).to_string(), "100");
        assert_eq!(OpcValue::Float(12.34).to_string(), "12.34");
        assert_eq!(OpcValue::Bool(true).to_string(), "true");
        assert_eq!(OpcValue::Bool(false).to_string(), "false");
        assert_eq!(OpcValue::Empty.to_string(), "Empty");
        assert_eq!(OpcValue::Null.to_string(), "Null");
    }

    #[test]
    fn test_opc_value_option_ext_some() {
        let val_opt = Some(OpcValue::Int(42));
        assert_eq!(format!("{}", val_opt.display()), "42");
        assert_eq!(format!("{}", val_opt.display_or("Custom")), "42");

        let val_ref = val_opt.as_ref();
        assert_eq!(format!("{}", val_ref.display()), "42");
        assert_eq!(format!("{}", val_ref.display_or("Custom")), "42");
    }

    #[test]
    fn test_opc_value_option_ext_none() {
        let val_opt: Option<OpcValue> = None;
        assert_eq!(format!("{}", val_opt.display()), "Error");
        assert_eq!(format!("{}", val_opt.display_or("Custom")), "Custom");

        let val_ref = val_opt.as_ref();
        assert_eq!(format!("{}", val_ref.display()), "Error");
        assert_eq!(format!("{}", val_ref.display_or("Custom")), "Custom");
    }

    #[test]
    fn test_system_time_option_ext_some() {
        use std::time::SystemTime;

        // Non-epoch time (1700000000 = 2023-11-14 22:13:20 UTC)
        let ts = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let ts_opt = Some(ts);
        let expected = "2023-11-14 22:13:20";
        assert_eq!(format!("{}", ts_opt.display()), expected);
        assert_eq!(format!("{}", ts_opt.display_or("Custom")), expected);
    }

    #[test]
    fn test_system_time_option_ext_none_and_epoch() {
        use std::time::SystemTime;

        let ts_none: Option<SystemTime> = None;
        assert_eq!(format!("{}", ts_none.display()), "N/A");
        assert_eq!(format!("{}", ts_none.display_or("Custom")), "Custom");

        let ts_epoch = Some(SystemTime::UNIX_EPOCH);
        assert_eq!(format!("{}", ts_epoch.display()), "N/A");
        assert_eq!(format!("{}", ts_epoch.display_or("Custom")), "Custom");
    }

    #[test]
    fn test_opc_value_from_ref() {
        let cases = vec![
            OpcValue::Int(-12345),
            OpcValue::UInt(67890),
            OpcValue::Float(3.14159),
            OpcValue::Bool(true),
            OpcValue::String("OPC DA Tag".to_string()),
            OpcValue::Empty,
            OpcValue::Null,
        ];

        for val in &cases {
            let cloned = OpcValue::from(val);
            assert_eq!(&cloned, val);
        }
    }
}
