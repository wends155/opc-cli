//! Canonical domain types for OPC DA client operations.
//!
//! Provides type-safe representations of handles, group states, server statuses,
//! namespace browsing types, and OPC DA quality flags.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::errors::OpcError;

/// Opaque handle for an OPC group.
///
/// This wrapper type enhances type safety when interacting with OPC COM interfaces,
/// preventing accidental mixing of group and item handles.
///
/// # Examples
///
/// ```
/// use opc_da_client::GroupHandle;
/// let handle = GroupHandle::new(123u32);
/// assert_eq!(handle.as_raw(), 123u32);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GroupHandle(u32);

impl GroupHandle {
    /// Creates a new `GroupHandle` from a raw 32-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw 32-bit handle value.
    #[inline]
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for GroupHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<GroupHandle> for u32 {
    #[inline]
    fn from(handle: GroupHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for GroupHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Opaque handle for an OPC item.
///
/// Similar to [`GroupHandle`], this ensures type-safe identification of tags
/// within an OPC group.
///
/// # Examples
///
/// ```
/// use opc_da_client::ItemHandle;
/// let handle = ItemHandle::new(456u32);
/// assert_eq!(handle.as_raw(), 456u32);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ItemHandle(u32);

impl ItemHandle {
    /// Creates a new `ItemHandle` from a raw 32-bit unsigned integer.
    #[inline]
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw 32-bit handle value.
    #[inline]
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

impl From<u32> for ItemHandle {
    #[inline]
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<ItemHandle> for u32 {
    #[inline]
    fn from(handle: ItemHandle) -> Self {
        handle.0
    }
}

impl fmt::Display for ItemHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Canonical OPC Value Variant ─────────────────────────────────────

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

/// Major OPC DA quality status (bits 6-7, mask `0xC0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualityMajor {
    /// The value is good and can be trusted (`0xC0`).
    #[default]
    Good,
    /// The quality of the value is uncertain, but may be usable (`0x40`).
    Uncertain,
    /// The value is bad and unusable (`0x00`).
    Bad,
}

/// Limit status for an OPC DA quality word (bits 0-1, mask `0x03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualityLimit {
    /// The value is free to move in both directions (`0x00`).
    #[default]
    NotLimited,
    /// The value has reached its lower operational limit and cannot decrease further (`0x01`).
    LowLimited,
    /// The value has reached its upper operational limit and cannot increase further (`0x02`).
    HighLimited,
    /// The value is constant and cannot change (`0x03`).
    Constant,
}

/// Detailed substatus for an OPC DA quality word (bits 2-5, mask `0x3C`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualitySubstatus {
    /// No specific substatus information available (`0x00`).
    #[default]
    NonSpecific,
    /// The configuration of the server or item is invalid (`0x04`).
    ConfigurationError,
    /// The item address or identity is not connected to a data source (`0x08`).
    NotConnected,
    /// The device failure has prevented data acquisition (`0x0C`).
    DeviceFailure,
    /// A sensor or transducer failure was detected (`0x10`).
    SensorFailure,
    /// Communication with the underlying device or PLC has failed (`0x18`).
    CommFailure,
    /// The communication channel or device is out of service (`0x1C`).
    OutOfService,
    /// The value was generated by a last-known-value holding routine (`0x14`).
    LastUsableValue,
    /// The sensor was calibrated or adjusted and is currently unverified (`0x20`).
    SensorCalibrating,
    /// The acquired value has exceeded the configured Engineering Units (EGU) limits (`0x14` / `0x20`).
    EguExceeded,
    /// Multiple data sources disagreed, or subnormal values were averaged (`0x24`).
    SubNormal,
    /// The value has been overridden locally by an operator or manual simulation (`0x18`).
    LocalOverride,
}

/// Strongly-typed 16-bit OPC DA quality word.
///
/// Decomposes the raw 16-bit OPC DA quality field into:
/// - **Major** (bits 6-7): `Good` (`0xC0`), `Uncertain` (`0x40`), `Bad` (`0x00`).
/// - **Substatus** (bits 2-5): Detailed reason (e.g. `CommFailure`, `OutOfService`, `EguExceeded`).
/// - **Limit** (bits 0-1): Limit status (e.g. `LowLimited`, `HighLimited`, `Constant`).
/// - **Raw** (`u16`): The complete 16-bit quality word received from the OPC server.
///
/// # Examples
///
/// ```
/// use opc_da_client::{OpcQuality, QualityMajor, QualitySubstatus};
///
/// let quality = OpcQuality::from(0x00C0);
/// assert!(quality.is_good());
/// assert_eq!(quality.major, QualityMajor::Good);
/// assert_eq!(quality.substatus, QualitySubstatus::NonSpecific);
/// assert_eq!(quality.to_string(), "Good");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpcQuality {
    /// Major quality category.
    pub major: QualityMajor,
    /// Specific substatus explanation.
    pub substatus: QualitySubstatus,
    /// Limit condition flag.
    pub limit: QualityLimit,
    /// Original 16-bit raw quality word from the COM server.
    pub raw: u16,
}

impl OpcQuality {
    /// Standard Good quality (`0x00C0`).
    pub const GOOD: Self = Self {
        major: QualityMajor::Good,
        substatus: QualitySubstatus::NonSpecific,
        limit: QualityLimit::NotLimited,
        raw: 0x00C0,
    };

    /// Standard Bad quality (`0x0000`).
    pub const BAD: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::NonSpecific,
        limit: QualityLimit::NotLimited,
        raw: 0x0000,
    };

    /// Standard Uncertain quality (`0x0040`).
    pub const UNCERTAIN: Self = Self {
        major: QualityMajor::Uncertain,
        substatus: QualitySubstatus::NonSpecific,
        limit: QualityLimit::NotLimited,
        raw: 0x0040,
    };

    /// Bad quality due to configuration error (`0x0004`).
    pub const BAD_CONFIG_ERROR: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::ConfigurationError,
        limit: QualityLimit::NotLimited,
        raw: 0x0004,
    };

    /// Bad quality due to communication failure (`0x0018`).
    pub const BAD_COMM_FAILURE: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::CommFailure,
        limit: QualityLimit::NotLimited,
        raw: 0x0018,
    };

    /// Returns `true` if the major quality is `Good`.
    #[must_use]
    pub const fn is_good(&self) -> bool {
        matches!(self.major, QualityMajor::Good)
    }

    /// Returns `true` if the major quality is `Bad`.
    #[must_use]
    pub const fn is_bad(&self) -> bool {
        matches!(self.major, QualityMajor::Bad)
    }

    /// Returns `true` if the major quality is `Uncertain`.
    #[must_use]
    pub const fn is_uncertain(&self) -> bool {
        matches!(self.major, QualityMajor::Uncertain)
    }

    /// Returns `true` if the quality has active limits.
    #[must_use]
    pub const fn is_limited(&self) -> bool {
        !matches!(self.limit, QualityLimit::NotLimited)
    }
}

impl From<u16> for OpcQuality {
    fn from(raw: u16) -> Self {
        let major = match raw & 0x00C0 {
            0x00C0 => QualityMajor::Good,
            0x0040 => QualityMajor::Uncertain,
            _ => QualityMajor::Bad,
        };

        let limit = match raw & 0x0003 {
            0x0001 => QualityLimit::LowLimited,
            0x0002 => QualityLimit::HighLimited,
            0x0003 => QualityLimit::Constant,
            _ => QualityLimit::NotLimited,
        };

        let substatus_bits = raw & 0x003C;
        let substatus = match major {
            QualityMajor::Bad => match substatus_bits {
                0x0004 => QualitySubstatus::ConfigurationError,
                0x0008 => QualitySubstatus::NotConnected,
                0x000C => QualitySubstatus::DeviceFailure,
                0x0010 => QualitySubstatus::SensorFailure,
                0x0014 => QualitySubstatus::LastUsableValue,
                0x0018 => QualitySubstatus::CommFailure,
                0x001C => QualitySubstatus::OutOfService,
                _ => QualitySubstatus::NonSpecific,
            },
            QualityMajor::Uncertain => match substatus_bits {
                0x0004 => QualitySubstatus::LastUsableValue,
                0x0010 => QualitySubstatus::SensorCalibrating,
                0x0014 => QualitySubstatus::EguExceeded,
                0x0018 => QualitySubstatus::SubNormal,
                _ => QualitySubstatus::NonSpecific,
            },
            QualityMajor::Good => match substatus_bits {
                0x0018 => QualitySubstatus::LocalOverride,
                _ => QualitySubstatus::NonSpecific,
            },
        };

        Self {
            major,
            substatus,
            limit,
            raw,
        }
    }
}

impl From<OpcQuality> for u16 {
    #[inline]
    fn from(q: OpcQuality) -> Self {
        q.raw
    }
}

/// Error returned when parsing an invalid quality string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseQualityError(pub String);

impl fmt::Display for ParseQualityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid OPC quality string: '{}'", self.0)
    }
}

impl std::error::Error for ParseQualityError {}

impl std::str::FromStr for OpcQuality {
    type Err = ParseQualityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "good" => Ok(Self::GOOD),
            "uncertain" => Ok(Self::UNCERTAIN),
            "bad" => Ok(Self::BAD),
            other => Err(ParseQualityError(other.to_string())),
        }
    }
}

impl fmt::Display for OpcQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let major_str = match self.major {
            QualityMajor::Good => "Good",
            QualityMajor::Uncertain => "Uncertain",
            QualityMajor::Bad => "Bad",
        };

        let sub_str = match self.substatus {
            QualitySubstatus::NonSpecific => None,
            QualitySubstatus::ConfigurationError => Some("Configuration Error"),
            QualitySubstatus::NotConnected => Some("Not Connected"),
            QualitySubstatus::DeviceFailure => Some("Device Failure"),
            QualitySubstatus::SensorFailure => Some("Sensor Failure"),
            QualitySubstatus::CommFailure => Some("Comm Failure"),
            QualitySubstatus::OutOfService => Some("Out of Service"),
            QualitySubstatus::LastUsableValue => Some("Last Usable Value"),
            QualitySubstatus::SensorCalibrating => Some("Sensor Calibrating"),
            QualitySubstatus::EguExceeded => Some("EGU Exceeded"),
            QualitySubstatus::SubNormal => Some("Sub-Normal"),
            QualitySubstatus::LocalOverride => Some("Local Override"),
        };

        let limit_str = match self.limit {
            QualityLimit::NotLimited => None,
            QualityLimit::LowLimited => Some("Low Limited"),
            QualityLimit::HighLimited => Some("High Limited"),
            QualityLimit::Constant => Some("Constant"),
        };

        if f.width().is_none() {
            match (sub_str, limit_str) {
                (None, None) => write!(f, "{major_str}"),
                (Some(s), None) => write!(f, "{major_str} ({s})"),
                (None, Some(l)) => write!(f, "{major_str} [{l}]"),
                (Some(s), Some(l)) => write!(f, "{major_str} ({s}) [{l}]"),
            }
        } else {
            let formatted = match (sub_str, limit_str) {
                (None, None) => major_str.to_string(),
                (Some(s), None) => format!("{major_str} ({s})"),
                (None, Some(l)) => format!("{major_str} [{l}]"),
                (Some(s), Some(l)) => format!("{major_str} ({s}) [{l}]"),
            };
            f.pad(&formatted)
        }
    }
}

/// Supported OPC DA Specification versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// OPC Data Access 1.0a specification.
    V1,
    /// OPC Data Access 2.05a specification.
    V2,
    /// OPC Data Access 3.0 specification.
    V3,
}

/// Current state and properties of an active OPC group.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupState {
    /// Actual update rate in milliseconds (may differ from requested).
    pub update_rate: u32,
    /// Whether the group is currently active (processing updates).
    pub active: bool,
    /// The unique name of the group.
    pub name: String,
    /// Time zone bias in minutes from UTC.
    pub time_bias: i32,
    /// Percent change for a tag value required to trigger an update.
    pub percent_deadband: f32,
    /// Locale ID used for formatting strings in this group.
    pub locale_id: u32,
    /// Handle assigned by the client for this group.
    pub client_handle: GroupHandle,
    /// Handle assigned by the server for this group.
    pub server_handle: GroupHandle,
}

/// Current running state of the OPC server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// The server is running normally and actively processing data.
    Running,
    /// The server has encountered an unrecoverable failure and is not functioning.
    Failed,
    /// The server is running but has no configuration loaded.
    NoConfig,
    /// The server is temporarily suspended and not collecting data.
    Suspended,
    /// The server is operating in test or diagnostic mode.
    Test,
    /// The server cannot communicate with the underlying physical devices or network.
    CommunicationFault,
}

/// Operational status and metadata of the connected server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStatus {
    /// Time when the server was started.
    pub start_time: std::time::SystemTime,
    /// Current time at the server.
    pub current_time: std::time::SystemTime,
    /// Time of the last update sent by the server.
    pub last_update_time: std::time::SystemTime,
    /// Current operational state of the server.
    pub server_state: ServerState,
    /// Number of active groups managed by the server.
    pub group_count: u32,
    /// Current bandwidth utilization as reported by the server.
    pub band_width: u32,
    /// Major version of the server software.
    pub major_version: u16,
    /// Minor version of the server software.
    pub minor_version: u16,
    /// Build or revision number of the server software.
    pub build_number: u16,
    /// Vendor information string provided by the server.
    pub vendor_info: String,
}

/// OPC DA address space browse type — replaces raw u32 constants.
///
/// # Examples
///
/// ```
/// use opc_da_client::BrowseType;
///
/// let b = BrowseType::Branch;
/// assert_eq!(u32::from(b), 1);
/// assert_eq!(BrowseType::try_from(2).ok(), Some(BrowseType::Leaf));
/// ```
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseType {
    /// Browse branch nodes (directories / containers) within the namespace.
    Branch = 1,
    /// Browse leaf nodes (individual process tags) within the namespace.
    Leaf = 2,
    /// Browse flat unorganized namespace items.
    Flat = 3,
}

impl From<BrowseType> for u32 {
    #[inline]
    fn from(browse_type: BrowseType) -> Self {
        browse_type as Self
    }
}

impl TryFrom<u32> for BrowseType {
    type Error = crate::errors::OpcError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Branch),
            2 => Ok(Self::Leaf),
            3 => Ok(Self::Flat),
            _ => Err(crate::errors::OpcError::Conversion(format!(
                "Invalid BrowseType: {value}"
            ))),
        }
    }
}

/// OPC DA browse direction — replaces raw u32 constants.
///
/// # Examples
///
/// ```
/// use opc_da_client::BrowseDirection;
///
/// let dir = BrowseDirection::Up;
/// assert_eq!(u32::from(dir), 1);
/// assert_eq!(BrowseDirection::try_from(2).ok(), Some(BrowseDirection::Down));
/// ```
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseDirection {
    /// Navigate up to the parent branch.
    Up = 1,
    /// Navigate down into a child branch.
    Down = 2,
    /// Navigate to a specific named node (DA 3.0).
    To = 3,
}

impl From<BrowseDirection> for u32 {
    #[inline]
    fn from(dir: BrowseDirection) -> Self {
        dir as Self
    }
}

impl TryFrom<u32> for BrowseDirection {
    type Error = crate::errors::OpcError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Up),
            2 => Ok(Self::Down),
            3 => Ok(Self::To),
            _ => Err(crate::errors::OpcError::Conversion(format!(
                "Invalid BrowseDirection: {value}"
            ))),
        }
    }
}

const _: () = assert!(BrowseType::Branch as u32 == 1);
const _: () = assert!(BrowseType::Leaf as u32 == 2);
const _: () = assert!(BrowseType::Flat as u32 == 3);
const _: () = assert!(BrowseDirection::Up as u32 == 1);
const _: () = assert!(BrowseDirection::Down as u32 == 2);
const _: () = assert!(BrowseDirection::To as u32 == 3);

/// Granular filter for enumeration results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseFilter {
    /// Enumerate all available nodes regardless of type.
    All,
    /// Enumerate only branch (container) nodes.
    Branches,
    /// Enumerate only leaf (tag item) nodes.
    Items,
}

/// Typology of the server's address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamespaceType {
    /// Flat namespace without hierarchical folders or branches.
    Flat,
    /// Hierarchical tree-structured namespace with nested branches and leaves.
    Hierarchy,
}

// ── Server Identification & Endpoint Types ─────────────────────────

/// Helper to parse a standard GUID string into a [`windows::core::GUID`].
fn parse_guid(s: &str) -> Option<windows::core::GUID> {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .unwrap_or(trimmed);
    let parts: Vec<&str> = inner.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    if parts[0].len() != 8
        || parts[1].len() != 4
        || parts[2].len() != 4
        || parts[3].len() != 4
        || parts[4].len() != 12
    {
        return None;
    }
    let data1 = u32::from_str_radix(parts[0], 16).ok()?;
    let data2 = u16::from_str_radix(parts[1], 16).ok()?;
    let data3 = u16::from_str_radix(parts[2], 16).ok()?;
    let d4_a = u16::from_str_radix(parts[3], 16).ok()?;
    let d4_b = u64::from_str_radix(parts[4], 16).ok()?;

    let mut data4 = [0u8; 8];
    data4[..2].copy_from_slice(&d4_a.to_be_bytes());
    data4[2..8].copy_from_slice(&d4_b.to_be_bytes()[2..8]);

    Some(windows::core::GUID {
        data1,
        data2,
        data3,
        data4,
    })
}

/// Strongly-typed identifier for an OPC DA server.
///
/// An OPC server can be referenced either by its human-readable Programmatic
/// Identifier (`ProgID`), or directly by its 128-bit Windows COM Class ID (`CLSID`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ServerIdentifier {
    /// Server referenced by human-readable ProgID (e.g., `"Matrikon.OPC.Simulation.1"`).
    ProgId(String),
    /// Server referenced directly by Windows COM CLSID.
    Clsid(windows::core::GUID),
}

impl ServerIdentifier {
    /// Returns a borrowed reference to the ProgID if this is a [`ServerIdentifier::ProgId`].
    #[must_use]
    pub fn as_prog_id(&self) -> Option<&str> {
        match self {
            Self::ProgId(prog_id) => Some(prog_id.as_str()),
            Self::Clsid(_) => None,
        }
    }

    /// Returns a borrowed reference to the CLSID GUID if this is a [`ServerIdentifier::Clsid`].
    #[must_use]
    pub const fn as_clsid(&self) -> Option<&windows::core::GUID> {
        match self {
            Self::Clsid(guid) => Some(guid),
            Self::ProgId(_) => None,
        }
    }

    /// Returns `true` if this identifier is a direct [`ServerIdentifier::Clsid`].
    #[must_use]
    pub fn is_clsid(&self) -> bool {
        matches!(self, Self::Clsid(_))
    }

    /// Returns `true` if this identifier is a [`ServerIdentifier::ProgId`].
    #[must_use]
    pub fn is_prog_id(&self) -> bool {
        matches!(self, Self::ProgId(_))
    }
}

/// Formats a 128-bit COM GUID into a bracketed registry/DCOM string:
/// `"{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}"`.
#[must_use]
pub(crate) fn format_guid_bracketed(guid: &windows::core::GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

impl fmt::Display for ServerIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgId(prog_id) => write!(f, "{prog_id}"),
            Self::Clsid(guid) => write!(f, "{}", format_guid_bracketed(guid)),
        }
    }
}

impl From<windows::core::GUID> for ServerIdentifier {
    fn from(guid: windows::core::GUID) -> Self {
        Self::Clsid(guid)
    }
}

impl From<&str> for ServerIdentifier {
    fn from(s: &str) -> Self {
        if let Some(guid) = parse_guid(s) {
            Self::Clsid(guid)
        } else {
            Self::ProgId(s.to_string())
        }
    }
}

impl From<String> for ServerIdentifier {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

/// Rich catalog metadata for an enumerated OPC DA server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcServerInfo {
    /// Programmatic Identifier of the server (e.g., `"Matrikon.OPC.Simulation.1"`).
    pub prog_id: String,
    /// 128-bit COM Class ID of the server.
    pub clsid: windows::core::GUID,
    /// Human-readable server title from catalog metadata, or `None` if absent.
    pub user_type: Option<String>,
    /// Target host machine (or `None` for localhost).
    pub host: Option<String>,
}

impl OpcServerInfo {
    /// Returns the user-friendly title if available, otherwise falls back to [`OpcServerInfo::prog_id`].
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.user_type.as_deref().unwrap_or(&self.prog_id)
    }

    /// Creates an [`OpcServerEndpoint`] referencing this server.
    #[must_use]
    pub fn endpoint(&self) -> OpcServerEndpoint {
        OpcServerEndpoint {
            host: self.host.clone(),
            identifier: ServerIdentifier::ProgId(self.prog_id.clone()),
        }
    }
}

/// Connection endpoint defining a target host machine and OPC server identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpcServerEndpoint {
    /// Target machine hostname or IP address (`None` or `"localhost"` for local connection).
    pub host: Option<String>,
    /// Strongly-typed server identifier (ProgID or CLSID).
    pub identifier: ServerIdentifier,
}

impl OpcServerEndpoint {
    /// Creates a new endpoint targeting the local machine.
    #[must_use]
    pub fn local(identifier: impl Into<ServerIdentifier>) -> Self {
        Self {
            host: None,
            identifier: identifier.into(),
        }
    }

    /// Creates a new endpoint targeting a remote machine.
    #[must_use]
    pub fn remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self {
        Self {
            host: Some(host.into()),
            identifier: identifier.into(),
        }
    }
}

impl fmt::Display for OpcServerEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(host) = &self.host {
            write!(f, r"\\{}\{}", host, self.identifier)
        } else {
            write!(f, "{}", self.identifier)
        }
    }
}

impl From<ServerIdentifier> for OpcServerEndpoint {
    fn from(identifier: ServerIdentifier) -> Self {
        Self {
            host: None,
            identifier,
        }
    }
}

impl From<&str> for OpcServerEndpoint {
    fn from(s: &str) -> Self {
        Self::from(ServerIdentifier::from(s))
    }
}

impl From<String> for OpcServerEndpoint {
    fn from(s: String) -> Self {
        Self::from(ServerIdentifier::from(s))
    }
}

// ── Zero-Allocation Tag Batching & Conversions ────────────────────────

/// Represents a batch of OPC tag names, enabling zero-allocation conversion
/// across static literals, fixed-size arrays, borrowed string slices, and owned collections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagBatch {
    /// Borrowed slice of static string slices (e.g. `&["Random.Int4", "Random.Real8"]`).
    Static(&'static [&'static str]),
    /// Single static string slice literal (e.g. `"Random.Int4"`).
    StaticSingle(&'static str),
    /// Shared reference-counted slice of strings.
    Shared(Arc<[String]>),
    /// Owned vector of heap strings (e.g. from `browse_tags` or dynamic configuration).
    Owned(Vec<String>),
    /// Single owned heap string.
    OwnedSingle(String),
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
            Self::StaticSingle(_) | Self::OwnedSingle(_) => 1,
            Self::Shared(slice) => slice.len(),
            Self::Owned(vec) => vec.len(),
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
            Self::Shared(slice) => TagBatchIter::Ref(slice.iter()),
            Self::Owned(vec) => TagBatchIter::Ref(vec.iter()),
            Self::OwnedSingle(s) => TagBatchIter::Single(Some(s.as_str())),
        }
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

impl IntoTags for &Vec<String> {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::Owned(self.clone())
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

// ── Canonical Tag Read / Write & Collector Models ─────────────────────

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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagValue {
    /// The fully qualified tag identifier (e.g., `"Channel1.Device1.Tag1"`).
    pub tag_id: String,
    /// The decoded value, or `None` if the tag read failed or value is unavailable.
    pub value: Option<OpcValue>,
    /// OPC quality status, decomposed into major quality, substatus, and limit bits.
    pub quality: OpcQuality,
    /// Timestamp of the last value change (UTC-based), or `None` if unavailable.
    pub timestamp: Option<std::time::SystemTime>,
    /// Underlying error if the tag read failed on the server.
    pub error: Option<OpcError>,
}

impl TagValue {
    /// Creates a new `TagValue` without error.
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
            value,
            quality,
            timestamp,
            error: None,
        }
    }

    /// Creates a new `TagValue` with an error.
    #[inline]
    #[must_use]
    pub fn with_error(tag_id: impl Into<String>, quality: OpcQuality, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            value: None,
            quality,
            timestamp: None,
            error: Some(error),
        }
    }

    /// Returns `true` if quality is good, a value is present, and no error occurred.
    ///
    /// # Returns
    ///
    /// `true` if [`TagValue::quality`] satisfies [`OpcQuality::is_good`], [`TagValue::value`] is `Some`,
    /// and [`TagValue::error`] is `None`.
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
        self.quality.is_good() && self.value.is_some() && self.error.is_none()
    }

    /// Returns `true` if quality is bad, value is missing, or an error occurred.
    ///
    /// # Returns
    ///
    /// `true` if the tag read encountered an error or quality is bad; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, TagValue};
    ///
    /// let tv = TagValue {
    ///     tag_id: "Tag1".into(),
    ///     value: None,
    ///     quality: OpcQuality::BAD_COMM_FAILURE,
    ///     timestamp: None,
    ///     error: None,
    /// };
    /// assert!(tv.is_error());
    /// ```
    #[must_use]
    pub fn is_error(&self) -> bool {
        !self.is_good()
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
        match &self.value {
            Some(v) => v.to_string(),
            None => "Error".to_string(),
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
}

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} = {} [{}] @ {}",
            self.tag_id,
            self.value.display(),
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
        Self::Conversion(err.to_string())
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
        self.get(tag).and_then(|tv| tv.value.as_ref())
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
        let item = self
            .get(tag)
            .ok_or_else(|| TagExtractError::NotRequested(tag.to_string()))?;
        if let Some(ref err) = item.error {
            return Err(TagExtractError::ReadFailed {
                tag: tag.to_string(),
                source: err.clone(),
            });
        }
        let val = item
            .value
            .as_ref()
            .ok_or_else(|| TagExtractError::NoValue(tag.to_string()))?;
        match val {
            OpcValue::Float(f) => Ok(*f),
            OpcValue::Int(i) => Ok(f64::from(*i)),
            OpcValue::Empty | OpcValue::Null => Err(TagExtractError::NoValue(tag.to_string())),
            other => Err(TagExtractError::TypeMismatch {
                tag: tag.to_string(),
                value: other.to_string(),
                expected: "f64",
            }),
        }
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
        let item = self
            .get(tag)
            .ok_or_else(|| TagExtractError::NotRequested(tag.to_string()))?;
        if let Some(ref err) = item.error {
            return Err(TagExtractError::ReadFailed {
                tag: tag.to_string(),
                source: err.clone(),
            });
        }
        let val = item
            .value
            .as_ref()
            .ok_or_else(|| TagExtractError::NoValue(tag.to_string()))?;
        match val {
            OpcValue::Int(i) => Ok(*i),
            OpcValue::Float(f) => {
                if f.is_nan()
                    || f.is_infinite()
                    || *f < f64::from(i32::MIN)
                    || *f > f64::from(i32::MAX)
                    || f.fract() != 0.0
                {
                    Err(TagExtractError::TypeMismatch {
                        tag: tag.to_string(),
                        value: val.to_string(),
                        expected: "i32",
                    })
                } else {
                    #[allow(clippy::cast_possible_truncation)]
                    Ok(*f as i32)
                }
            }
            OpcValue::Empty | OpcValue::Null => Err(TagExtractError::NoValue(tag.to_string())),
            other => Err(TagExtractError::TypeMismatch {
                tag: tag.to_string(),
                value: other.to_string(),
                expected: "i32",
            }),
        }
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
        let item = self
            .get(tag)
            .ok_or_else(|| TagExtractError::NotRequested(tag.to_string()))?;
        if let Some(ref err) = item.error {
            return Err(TagExtractError::ReadFailed {
                tag: tag.to_string(),
                source: err.clone(),
            });
        }
        let val = item
            .value
            .as_ref()
            .ok_or_else(|| TagExtractError::NoValue(tag.to_string()))?;
        match val {
            OpcValue::Bool(b) => Ok(*b),
            OpcValue::Empty | OpcValue::Null => Err(TagExtractError::NoValue(tag.to_string())),
            other => Err(TagExtractError::TypeMismatch {
                tag: tag.to_string(),
                value: other.to_string(),
                expected: "bool",
            }),
        }
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
        let item = self
            .get(tag)
            .ok_or_else(|| TagExtractError::NotRequested(tag.to_string()))?;
        if let Some(ref err) = item.error {
            return Err(TagExtractError::ReadFailed {
                tag: tag.to_string(),
                source: err.clone(),
            });
        }
        let val = item
            .value
            .as_ref()
            .ok_or_else(|| TagExtractError::NoValue(tag.to_string()))?;
        match val {
            OpcValue::String(s) => Ok(s.as_str()),
            OpcValue::Empty | OpcValue::Null => Err(TagExtractError::NoValue(tag.to_string())),
            other => Err(TagExtractError::TypeMismatch {
                tag: tag.to_string(),
                value: other.to_string(),
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
}

/// Thread-safe accumulator, capacity limiter, and progress monitor for OPC tag namespace browsing.
///
/// `TagCollector` encapsulates incremental tag accumulation, explicit `max_tags` bounding,
/// lock-free progress tracking, and cooperative cancellation across thread and async boundaries.
#[derive(Debug, Clone)]
pub struct TagCollector {
    inner: Arc<TagCollectorInner>,
}

#[derive(Debug)]
struct TagCollectorInner {
    tags: std::sync::Mutex<Vec<String>>,
    count: AtomicUsize,
    max_tags: usize,
    cancelled: AtomicBool,
}

impl TagCollector {
    /// Standard default capacity cap when unconstrained (10,000 tags).
    pub const DEFAULT_MAX_TAGS: usize = 10_000;

    /// Creates a new `TagCollector` bounded to at most `max_tags` items.
    #[must_use]
    pub fn new(max_tags: usize) -> Self {
        Self {
            inner: Arc::new(TagCollectorInner {
                tags: std::sync::Mutex::new(Vec::with_capacity(max_tags.min(1024))),
                count: AtomicUsize::new(0),
                max_tags,
                cancelled: AtomicBool::new(false),
            }),
        }
    }

    /// Creates an unbounded `TagCollector` (`max_tags = usize::MAX`).
    #[must_use]
    pub fn unbounded() -> Self {
        Self::new(usize::MAX)
    }

    /// Returns the maximum capacity cap.
    #[must_use]
    pub fn max_tags(&self) -> usize {
        self.inner.max_tags
    }

    /// Returns the number of tags collected so far without acquiring a mutex lock.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.count.load(Ordering::Acquire)
    }

    /// Returns true if no tags have been collected yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the collector has reached or exceeded its `max_tags` capacity cap.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() >= self.inner.max_tags
    }

    /// Signals cancellation to the background browse worker.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
    }

    /// Returns true if cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Returns a cloned snapshot of all tags collected so far without draining.
    #[must_use]
    pub fn snapshot(&self) -> Vec<String> {
        let guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clone()
    }

    /// Drains and returns all collected tags, resetting the buffer and atomic count.
    #[must_use]
    pub fn harvest(&self) -> Vec<String> {
        let mut guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let harvested = std::mem::take(&mut *guard);
        drop(guard);
        self.inner.count.store(0, Ordering::Release);
        harvested
    }

    /// Pushes a tag into the collector if not full or cancelled.
    pub fn push(&self, tag: String) -> bool {
        if self.is_cancelled() || self.is_full() {
            return false;
        }
        let mut guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.len() >= self.inner.max_tags {
            return false;
        }
        guard.push(tag);
        drop(guard);
        self.inner.count.fetch_add(1, Ordering::Release);
        true
    }
}

impl Default for TagCollector {
    /// Creates a default `TagCollector` with `DEFAULT_MAX_TAGS` capacity.
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_TAGS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_type_from_roundtrip() {
        for (variant, expected) in [
            (BrowseType::Branch, 1u32),
            (BrowseType::Leaf, 2u32),
            (BrowseType::Flat, 3u32),
        ] {
            let raw: u32 = variant.into();
            assert_eq!(raw, expected);
            let back = BrowseType::try_from(raw).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn browse_type_try_from_rejects_invalid() {
        assert!(BrowseType::try_from(0u32).is_err());
        assert!(BrowseType::try_from(4u32).is_err());
        assert!(BrowseType::try_from(u32::MAX).is_err());
    }

    #[test]
    fn browse_direction_from_roundtrip() {
        for (variant, expected) in [
            (BrowseDirection::Up, 1u32),
            (BrowseDirection::Down, 2u32),
            (BrowseDirection::To, 3u32),
        ] {
            let raw: u32 = variant.into();
            assert_eq!(raw, expected);
            let back = BrowseDirection::try_from(raw).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn browse_direction_try_from_rejects_invalid() {
        assert!(BrowseDirection::try_from(0u32).is_err());
        assert!(BrowseDirection::try_from(4u32).is_err());
        assert!(BrowseDirection::try_from(u32::MAX).is_err());
    }

    #[test]
    fn test_opc_quality_good_standard() {
        let q = OpcQuality::from(0x00C0);
        assert_eq!(q.major, QualityMajor::Good);
        assert_eq!(q.substatus, QualitySubstatus::NonSpecific);
        assert_eq!(q.limit, QualityLimit::NotLimited);
        assert_eq!(q.raw, 0x00C0);
        assert!(q.is_good());
        assert!(!q.is_bad());
        assert!(!q.is_uncertain());
        assert!(!q.is_limited());
        assert_eq!(q.to_string(), "Good");
    }

    #[test]
    fn test_opc_quality_good_local_override() {
        let q = OpcQuality::from(0x00D8);
        assert_eq!(q.major, QualityMajor::Good);
        assert_eq!(q.substatus, QualitySubstatus::LocalOverride);
        assert_eq!(q.limit, QualityLimit::NotLimited);
        assert_eq!(q.to_string(), "Good (Local Override)");
    }

    #[test]
    fn test_opc_quality_bad_comm_failure() {
        let q = OpcQuality::from(0x0018);
        assert_eq!(q.major, QualityMajor::Bad);
        assert_eq!(q.substatus, QualitySubstatus::CommFailure);
        assert_eq!(q.limit, QualityLimit::NotLimited);
        assert!(q.is_bad());
        assert_eq!(q.to_string(), "Bad (Comm Failure)");
    }

    #[test]
    fn test_opc_quality_uncertain_limits() {
        let q = OpcQuality::from(0x0056);
        assert_eq!(q.major, QualityMajor::Uncertain);
        assert_eq!(q.substatus, QualitySubstatus::EguExceeded);
        assert_eq!(q.limit, QualityLimit::HighLimited);
        assert!(q.is_uncertain());
        assert!(q.is_limited());
        assert_eq!(q.to_string(), "Uncertain (EGU Exceeded) [High Limited]");
    }

    #[test]
    fn test_opc_quality_roundtrip_u16() {
        let words = [
            0x00C0, 0x0000, 0x0040, 0x0004, 0x0018, 0x0008, 0x00D8, 0x0056,
        ];
        for &w in &words {
            let q = OpcQuality::from(w);
            let back: u16 = q.into();
            assert_eq!(back, w);
        }
    }

    #[test]
    fn test_opc_quality_from_str() {
        assert_eq!("good".parse::<OpcQuality>().unwrap(), OpcQuality::GOOD);
        assert_eq!("Good".parse::<OpcQuality>().unwrap(), OpcQuality::GOOD);
        assert_eq!("bad".parse::<OpcQuality>().unwrap(), OpcQuality::BAD);
        assert_eq!(
            "uncertain".parse::<OpcQuality>().unwrap(),
            OpcQuality::UNCERTAIN
        );
        assert!("other".parse::<OpcQuality>().is_err());
    }

    #[test]
    fn test_server_identifier_conversions_and_display() {
        let prog_id = ServerIdentifier::from("Matrikon.OPC.Simulation.1");
        assert_eq!(
            prog_id,
            ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
        );
        assert_eq!(prog_id.to_string(), "Matrikon.OPC.Simulation.1");
        assert!(prog_id.is_prog_id());
        assert!(!prog_id.is_clsid());

        let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
        let parsed = ServerIdentifier::from(clsid_str);
        assert!(parsed.is_clsid());
        assert_eq!(parsed.to_string().to_uppercase(), clsid_str.to_uppercase());

        let direct_guid = windows::core::GUID::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
        let from_guid = ServerIdentifier::from(direct_guid);
        assert!(from_guid.is_clsid());
    }

    #[test]
    fn test_opc_server_info_display_name_and_endpoint() {
        let info_with_user_type = OpcServerInfo {
            prog_id: "Matrikon.OPC.Simulation.1".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Matrikon Simulation Server".into()),
            host: None,
        };
        assert_eq!(
            info_with_user_type.display_name(),
            "Matrikon Simulation Server"
        );
        assert_eq!(
            info_with_user_type.endpoint().identifier,
            ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
        );

        let info_without_user_type = OpcServerInfo {
            prog_id: "Kepware.KEPServerEX.V6".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: None,
            host: Some("192.168.1.10".into()),
        };
        assert_eq!(
            info_without_user_type.display_name(),
            "Kepware.KEPServerEX.V6"
        );
        assert_eq!(
            info_without_user_type.endpoint().host.as_deref(),
            Some("192.168.1.10")
        );
    }

    #[test]
    fn test_format_guid_bracketed() {
        let guid = windows::core::GUID::zeroed();
        assert_eq!(
            format_guid_bracketed(&guid),
            "{00000000-0000-0000-0000-000000000000}"
        );
        assert_eq!(
            format_guid_bracketed(&guid),
            ServerIdentifier::Clsid(guid).to_string()
        );

        let custom_guid = windows::core::GUID::from_u128(0x01234567_89AB_CDEF_0123_456789ABCDEF);
        assert_eq!(
            format_guid_bracketed(&custom_guid),
            ServerIdentifier::Clsid(custom_guid).to_string()
        );
    }

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

        // 6. &Vec<String>
        let ref_vec = &vec!["Ref1".to_string(), "Ref2".to_string()];
        let batch = ref_vec.into_tag_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Ref1", "Ref2"]);

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
    fn test_feature_independence_no_default_features() {
        let tv = TagValue::new("TagX", Some(OpcValue::Int(10)), OpcQuality::GOOD, None);
        assert!(tv.is_good());
        assert_eq!(tv.tag_id, "TagX");
    }

    #[test]
    fn test_tag_values_collection_and_lenient_coercion() {
        let items = vec![
            TagValue::new(
                "Simulation.Int",
                Some(OpcValue::Int(42)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Simulation.Float",
                Some(OpcValue::Float(12.345)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Simulation.Bool",
                Some(OpcValue::Bool(true)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Simulation.String",
                Some(OpcValue::String("Running".into())),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new("Simulation.NoVal", None, OpcQuality::BAD_CONFIG_ERROR, None),
            TagValue::with_error(
                "Simulation.Failed",
                OpcQuality::BAD_CONFIG_ERROR,
                OpcError::Internal("COM error 0x80040154".into()),
            ),
        ];

        let tvs = TagValues::new(items);
        assert_eq!(tvs.len(), 6);
        assert!(!tvs.is_empty());

        // Case-insensitive lookups
        assert!(tvs.get("simulation.int").is_some());
        assert_eq!(tvs.get_value("SIMULATION.INT"), Some(&OpcValue::Int(42)));

        // Direct typed extractions
        assert_eq!(tvs.get_i32("Simulation.Int").unwrap(), 42);
        assert!((tvs.get_f64("Simulation.Float").unwrap() - 12.345).abs() < 1e-5);
        assert!(tvs.get_bool("Simulation.Bool").unwrap());
        assert_eq!(tvs.get_str("Simulation.String").unwrap(), "Running");

        // Lenient lossless coercion: i32 -> f64
        assert!((tvs.get_f64("Simulation.Int").unwrap() - 42.0).abs() < 1e-5);

        // Errors:
        // 1. NotRequested
        assert_eq!(
            tvs.get_f64("Unknown.Tag"),
            Err(TagExtractError::NotRequested("Unknown.Tag".into()))
        );

        // 2. ReadFailed (preserves underlying error)
        match tvs.get_f64("Simulation.Failed") {
            Err(TagExtractError::ReadFailed { tag, source }) => {
                assert_eq!(tag, "Simulation.Failed");
                assert!(source.to_string().contains("0x80040154"));
            }
            other => panic!("Expected ReadFailed, got {other:?}"),
        }

        // 3. NoValue
        assert_eq!(
            tvs.get_f64("Simulation.NoVal"),
            Err(TagExtractError::NoValue("Simulation.NoVal".into()))
        );

        // 4. TypeMismatch
        match tvs.get_f64("Simulation.String") {
            Err(TagExtractError::TypeMismatch {
                tag,
                value,
                expected,
            }) => {
                assert_eq!(tag, "Simulation.String");
                assert_eq!(value, "Running");
                assert_eq!(expected, "f64");
            }
            other => panic!("Expected TypeMismatch, got {other:?}"),
        }

        // Error conversion to OpcError
        let err: OpcError = TagExtractError::NotRequested("TagA".into()).into();
        assert!(matches!(err, OpcError::Conversion(_)));
    }

    #[test]
    fn test_tag_values_coercion_overflow_and_null_edge_cases() {
        let items = vec![
            TagValue::new(
                "Overflow.Float",
                Some(OpcValue::Float(1e25)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Nan.Float",
                Some(OpcValue::Float(f64::NAN)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Inf.Float",
                Some(OpcValue::Float(f64::INFINITY)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Frac.Float",
                Some(OpcValue::Float(42.75)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new(
                "Exact.Float",
                Some(OpcValue::Float(50.0)),
                OpcQuality::GOOD,
                None,
            ),
            TagValue::new("Null.Tag", Some(OpcValue::Null), OpcQuality::GOOD, None),
            TagValue::new("Empty.Tag", Some(OpcValue::Empty), OpcQuality::GOOD, None),
            TagValue::new("Num.Str", Some(OpcValue::Int(123)), OpcQuality::GOOD, None),
        ];

        let tvs = TagValues::new(items);

        // Overflow: 1e25 into i32 must fail safely with TypeMismatch, not wrap or panic
        assert!(matches!(
            tvs.get_i32("Overflow.Float"),
            Err(TagExtractError::TypeMismatch { .. })
        ));

        // NaN into i32 must fail
        assert!(matches!(
            tvs.get_i32("Nan.Float"),
            Err(TagExtractError::TypeMismatch { .. })
        ));

        // Infinity into i32 must fail
        assert!(matches!(
            tvs.get_i32("Inf.Float"),
            Err(TagExtractError::TypeMismatch { .. })
        ));

        // Fractional float into i32 must fail (lossy)
        assert!(matches!(
            tvs.get_i32("Frac.Float"),
            Err(TagExtractError::TypeMismatch { .. })
        ));

        // Exact integer float into i32 succeeds (lossless)
        assert_eq!(tvs.get_i32("Exact.Float").unwrap(), 50);

        // Null and Empty map to NoValue
        assert_eq!(
            tvs.get_f64("Null.Tag"),
            Err(TagExtractError::NoValue("Null.Tag".into()))
        );
        assert_eq!(
            tvs.get_f64("Empty.Tag"),
            Err(TagExtractError::NoValue("Empty.Tag".into()))
        );

        // get_str on non-string returns TypeMismatch
        assert!(matches!(
            tvs.get_str("Num.Str"),
            Err(TagExtractError::TypeMismatch { .. })
        ));
    }
}
