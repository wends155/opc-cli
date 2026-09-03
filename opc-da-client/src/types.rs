#![allow(warnings)]

use crate::com::memory::{
    IntoBridge, LocalPointer, RemoteArray, ToNative, TryFromNative, TryToNative,
};
use crate::try_from_native;

/// Opaque handle for an OPC group.
///
/// This wrapper type enhances type safety when interacting with OPC COM interfaces,
/// preventing accidental mixing of group and item handles.
///
/// # Examples
///
/// ```
/// use opc_da_client::GroupHandle;
/// let handle = GroupHandle(123u32);
/// assert_eq!(handle.0, 123u32);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GroupHandle(pub u32);

/// Opaque handle for an OPC item.
///
/// Similar to [`GroupHandle`], this ensures type-safe identification of tags
/// within an OPC group.
///
/// # Examples
///
/// ```
/// use opc_da_client::ItemHandle;
/// let handle = ItemHandle(456u32);
/// assert_eq!(handle.0, 456u32);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ItemHandle(pub u32);

/// Major OPC DA quality status (bits 6-7, mask `0xC0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualityMajor {
    /// The value is good and can be trusted (`0xC0`).
    #[default]
    Good,
    /// The value is bad and cannot be trusted (`0x00`).
    Bad,
    /// The value is uncertain; use caution when relying on it (`0x40`).
    Uncertain,
    /// Unrecognized or vendor-specific major quality bitmask.
    Unknown(u8),
}

/// Limit condition of an OPC DA item value (bits 0-1, mask `0x03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualityLimit {
    /// The value is free to move in either direction (`0x00`).
    #[default]
    NotLimited,
    /// The value has reached its lower limit and cannot drop further (`0x01`).
    LowLimited,
    /// The value has reached its upper limit and cannot rise further (`0x02`).
    HighLimited,
    /// The value is constant and cannot move (`0x03`).
    Constant,
}

/// Substatus detailing the reason for a given major quality (bits 2-5, mask `0x3C`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualitySubstatus {
    /// No specific substatus is provided.
    #[default]
    NonSpecific,

    // --- Bad Substatuses (Major = Bad) ---
    /// The server or device configuration is invalid or missing.
    ConfigurationError,
    /// The underlying device or physical source is disconnected.
    NotConnected,
    /// The server or device detected a catastrophic hardware/device failure.
    DeviceFailure,
    /// The sensor reporting data has failed.
    SensorFailure,
    /// Communications were lost; returning the last known good value.
    LastKnownValue,
    /// A communications link to the data source has failed.
    CommFailure,
    /// The tag or block has been placed out of service.
    OutOfService,
    /// The server is awaiting initial data from the device.
    WaitingForInitialData,

    // --- Uncertain Substatuses (Major = Uncertain) ---
    /// The value is outside normal range; returning the last usable value.
    LastUsableValue,
    /// The sensor requires calibration.
    SensorCalNeeded,
    /// The engineering unit range has been exceeded.
    EguExceeded,
    /// The value was computed from fewer sources than normal.
    SubNormal,

    // --- Good Substatuses (Major = Good) ---
    /// The value was manually overridden locally.
    LocalOverride,

    // --- Fallback ---
    /// An unrecognized or vendor-specific substatus code.
    Raw(u8),
}

/// Fully decomposed, zero-allocation 16-bit OPC DA quality word.
///
/// Encapsulates the complete 16-bit quality definition from the OPC DA 2.05a specification §6.8:
/// - Major quality status ([`QualityMajor`]: Good, Bad, Uncertain)
/// - Substatus ([`QualitySubstatus`]: Comm failure, sensor failure, local override, etc.)
/// - Limit status ([`QualityLimit`]: Low limited, high limited, constant)
/// - The raw 16-bit word (`raw: u16`)
///
/// Implements [`From<u16>`] for bidirectional conversion from COM native `wQuality`,
/// and [`std::fmt::Display`] to format rich human-readable diagnostic details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpcQuality {
    /// Major quality status classification.
    pub major: QualityMajor,
    /// Substatus detailing the specific operational condition.
    pub substatus: QualitySubstatus,
    /// Limit condition flag.
    pub limit: QualityLimit,
    /// The raw 16-bit OPC DA quality word as returned by the COM server.
    pub raw: u16,
}

impl OpcQuality {
    /// Predefined constant for standard Good quality (`0x00C0`).
    pub const GOOD: Self = Self {
        major: QualityMajor::Good,
        substatus: QualitySubstatus::NonSpecific,
        limit: QualityLimit::NotLimited,
        raw: 0x00C0,
    };

    /// Predefined constant for standard Bad quality (`0x0000`).
    pub const BAD: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::NonSpecific,
        limit: QualityLimit::NotLimited,
        raw: 0x0000,
    };

    /// Predefined constant for standard Uncertain quality (`0x0040`).
    pub const UNCERTAIN: Self = Self {
        major: QualityMajor::Uncertain,
        substatus: QualitySubstatus::NonSpecific,
        limit: QualityLimit::NotLimited,
        raw: 0x0040,
    };

    /// Predefined constant for Bad - Configuration Error (`0x0004`).
    pub const BAD_CONFIG_ERROR: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::ConfigurationError,
        limit: QualityLimit::NotLimited,
        raw: 0x0004,
    };

    /// Predefined constant for Bad - Comm Failure (`0x0018`).
    pub const BAD_COMM_FAILURE: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::CommFailure,
        limit: QualityLimit::NotLimited,
        raw: 0x0018,
    };

    /// Predefined constant for Bad - Not Connected (`0x0008`).
    pub const BAD_NOT_CONNECTED: Self = Self {
        major: QualityMajor::Bad,
        substatus: QualitySubstatus::NotConnected,
        limit: QualityLimit::NotLimited,
        raw: 0x0008,
    };

    /// Returns `true` if the major quality is [`QualityMajor::Good`].
    #[must_use]
    pub const fn is_good(&self) -> bool {
        matches!(self.major, QualityMajor::Good)
    }

    /// Returns `true` if the major quality is [`QualityMajor::Bad`].
    #[must_use]
    pub const fn is_bad(&self) -> bool {
        matches!(self.major, QualityMajor::Bad)
    }

    /// Returns `true` if the major quality is [`QualityMajor::Uncertain`].
    #[must_use]
    pub const fn is_uncertain(&self) -> bool {
        matches!(self.major, QualityMajor::Uncertain)
    }

    /// Returns `true` if the value has any limit condition active.
    #[must_use]
    pub const fn is_limited(&self) -> bool {
        !matches!(self.limit, QualityLimit::NotLimited)
    }
}

impl From<u16> for OpcQuality {
    fn from(raw: u16) -> Self {
        let major_bits = (raw & 0xC0) as u8;
        let major = match major_bits {
            0xC0 => QualityMajor::Good,
            0x00 => QualityMajor::Bad,
            0x40 => QualityMajor::Uncertain,
            other => QualityMajor::Unknown(other >> 6),
        };

        let limit_bits = (raw & 0x03) as u8;
        let limit = match limit_bits {
            0x01 => QualityLimit::LowLimited,
            0x02 => QualityLimit::HighLimited,
            0x03 => QualityLimit::Constant,
            _ => QualityLimit::NotLimited,
        };

        let sub_bits = (raw & 0x3C) as u8;
        let substatus = match major {
            QualityMajor::Bad => match sub_bits {
                0x00 => QualitySubstatus::NonSpecific,
                0x04 => QualitySubstatus::ConfigurationError,
                0x08 => QualitySubstatus::NotConnected,
                0x0C => QualitySubstatus::DeviceFailure,
                0x10 => QualitySubstatus::SensorFailure,
                0x14 => QualitySubstatus::LastKnownValue,
                0x18 => QualitySubstatus::CommFailure,
                0x1C => QualitySubstatus::OutOfService,
                0x20 => QualitySubstatus::WaitingForInitialData,
                other => QualitySubstatus::Raw(other),
            },
            QualityMajor::Uncertain => match sub_bits {
                0x00 => QualitySubstatus::NonSpecific,
                0x04 => QualitySubstatus::LastUsableValue,
                0x10 => QualitySubstatus::SensorCalNeeded,
                0x14 => QualitySubstatus::EguExceeded,
                0x18 => QualitySubstatus::SubNormal,
                other => QualitySubstatus::Raw(other),
            },
            QualityMajor::Good => match sub_bits {
                0x00 => QualitySubstatus::NonSpecific,
                0x18 => QualitySubstatus::LocalOverride,
                other => QualitySubstatus::Raw(other),
            },
            QualityMajor::Unknown(_) => QualitySubstatus::Raw(sub_bits),
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
    fn from(q: OpcQuality) -> Self {
        q.raw
    }
}

impl From<&str> for OpcQuality {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "good" => Self::GOOD,
            "bad" => Self::BAD,
            "uncertain" => Self::UNCERTAIN,
            _ => Self::BAD,
        }
    }
}

impl std::fmt::Display for OpcQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let major_str = match self.major {
            QualityMajor::Good => "Good",
            QualityMajor::Bad => "Bad",
            QualityMajor::Uncertain => "Uncertain",
            QualityMajor::Unknown(_) => return write!(f, "Unknown(0x{:04X})", self.raw),
        };

        let sub_str = match self.substatus {
            QualitySubstatus::NonSpecific => None,
            QualitySubstatus::ConfigurationError => Some("Configuration Error"),
            QualitySubstatus::NotConnected => Some("Not Connected"),
            QualitySubstatus::DeviceFailure => Some("Device Failure"),
            QualitySubstatus::SensorFailure => Some("Sensor Failure"),
            QualitySubstatus::LastKnownValue => Some("Last Known Value"),
            QualitySubstatus::CommFailure => Some("Comm Failure"),
            QualitySubstatus::OutOfService => Some("Out of Service"),
            QualitySubstatus::WaitingForInitialData => Some("Waiting for Initial Data"),
            QualitySubstatus::LastUsableValue => Some("Last Usable Value"),
            QualitySubstatus::SensorCalNeeded => Some("Sensor Calibration Needed"),
            QualitySubstatus::EguExceeded => Some("EGU Exceeded"),
            QualitySubstatus::SubNormal => Some("Sub-Normal"),
            QualitySubstatus::LocalOverride => Some("Local Override"),
            QualitySubstatus::Raw(_) => None,
        };

        let limit_str = match self.limit {
            QualityLimit::NotLimited => None,
            QualityLimit::LowLimited => Some("Low Limited"),
            QualityLimit::HighLimited => Some("High Limited"),
            QualityLimit::Constant => Some("Constant"),
        };

        match (sub_str, limit_str) {
            (None, None) => write!(f, "{major_str}"),
            (Some(s), None) => write!(f, "{major_str} ({s})"),
            (None, Some(l)) => write!(f, "{major_str} [{l}]"),
            (Some(s), Some(l)) => write!(f, "{major_str} ({s}) [{l}]"),
        }
    }
}

/// Supported OPC DA Specification versions.
#[derive(Debug, Clone, PartialEq)]
pub enum Version {
    V1,
    V2,
    V3,
}

/// Current state and properties of an active OPC group.
///
/// This structure encapsulates both the requested and currently active properties
/// of an OPC group, as reported by the server.
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

/// Operational status and metadata of the connected server.
///
/// This structure provides a snapshot of the server's health, current load,
/// and version information.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerStatus {
    /// Time when the server was started.
    pub start_time: std::time::SystemTime,
    /// Current time according to the server.
    pub current_time: std::time::SystemTime,
    /// Time of the last data update.
    pub last_update_time: std::time::SystemTime,
    /// The current operational state of the server.
    pub server_state: ServerState,
    /// Number of groups currently managed by the server.
    pub group_count: u32,
    /// Current bandwidth utilization as reported by the server.
    pub band_width: u32,
    /// Major version of the server software.
    pub major_version: u16,
    /// Minor version of the server software.
    pub minor_version: u16,
    /// Build or revision number of the server software.
    pub build_number: u16,
    /// Descriptive vendor-specific information.
    pub vendor_info: String,
}

impl TryFromNative<crate::bindings::da::tagOPCSERVERSTATUS> for ServerStatus {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCSERVERSTATUS,
    ) -> windows::core::Result<Self> {
        Ok(Self {
            start_time: try_from_native!(&native.ftStartTime),
            current_time: try_from_native!(&native.ftCurrentTime),
            last_update_time: try_from_native!(&native.ftLastUpdateTime),
            server_state: try_from_native!(&native.dwServerState),
            group_count: native.dwGroupCount,
            band_width: native.dwBandWidth,
            major_version: native.wMajorVersion,
            minor_version: native.wMinorVersion,
            build_number: native.wBuildNumber,
            vendor_info: try_from_native!(&native.szVendorInfo),
        })
    }
}

/// Definition required to add a new item to an OPC group.
///
/// This structure contains the parameters needed for the server to identify
/// and initialize a tag within a group.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDef {
    /// Optional access path for the item (server-specific).
    pub access_path: String,
    /// The unique identifier of the tag within the server namespace.
    pub item_id: String,
    /// Whether the item should be added in an active state.
    pub active: bool,
    /// Handle assigned by the client for this item.
    pub client_handle: ItemHandle,
    /// Requested canonical data type (0 for server default).
    pub data_type: u16,
    /// Optional opaque blob for the item.
    pub blob: Vec<u8>,
}

/// FFI-safe bridge struct for `ItemDef`.
pub struct ItemDefBridge {
    pub access_path: LocalPointer<Vec<u16>>,
    pub item_id: LocalPointer<Vec<u16>>,
    pub active: bool,
    pub item_client_handle: u32,
    pub requested_data_type: u16,
    pub blob: LocalPointer<Vec<u8>>,
}

impl IntoBridge<ItemDefBridge> for ItemDef {
    fn into_bridge(self) -> ItemDefBridge {
        ItemDefBridge {
            access_path: LocalPointer::from(&self.access_path),
            item_id: LocalPointer::from(&self.item_id),
            active: self.active,
            item_client_handle: self.client_handle.0,
            requested_data_type: self.data_type,
            blob: LocalPointer::new(Some(self.blob)),
        }
    }
}

impl TryToNative<crate::bindings::da::tagOPCITEMDEF> for ItemDefBridge {
    fn try_to_native(&self) -> windows::core::Result<crate::bindings::da::tagOPCITEMDEF> {
        Ok(crate::bindings::da::tagOPCITEMDEF {
            szAccessPath: self.access_path.as_pwstr(),
            szItemID: self.item_id.as_pwstr(),
            bActive: self.active.into(),
            hClient: self.item_client_handle,
            vtRequestedDataType: self.requested_data_type,
            dwBlobSize: self.blob.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Blob size exceeds u32 maximum value",
                )
            })?,
            pBlob: self.blob.as_array_ptr() as *mut _,
            wReserved: 0,
        })
    }
}

/// Result properties of an item after being added to a group.
///
/// This structure contains the server-assigned properties for an item
/// that was successfully added.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemResult {
    /// Handle assigned by the server for this item.
    pub server_handle: ItemHandle,
    /// The actual canonical data type supported by the server for this item.
    pub data_type: u16,
    /// Access rights for this item (read/write permissions).
    pub access_rights: u32,
    /// Optional opaque blob returned by the server.
    pub blob: Vec<u8>,
}

impl TryFromNative<crate::bindings::da::tagOPCITEMRESULT> for ItemResult {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCITEMRESULT,
    ) -> windows::core::Result<Self> {
        Ok(Self {
            server_handle: ItemHandle(native.hServer),
            data_type: native.vtCanonicalDataType,
            access_rights: native.dwAccessRights,
            blob: RemoteArray::from_mut_ptr(native.pBlob, native.dwBlobSize)
                .as_slice()
                .to_vec(),
        })
    }
}

/// Current running state of the OPC server.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerState {
    Running,
    Failed,
    NoConfig,
    Suspended,
    Test,
    CommunicationFault,
}

impl TryFromNative<crate::bindings::da::tagOPCSERVERSTATE> for ServerState {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCSERVERSTATE,
    ) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_STATUS_RUNNING => Ok(ServerState::Running),
            crate::bindings::da::OPC_STATUS_FAILED => Ok(ServerState::Failed),
            crate::bindings::da::OPC_STATUS_NOCONFIG => Ok(ServerState::NoConfig),
            crate::bindings::da::OPC_STATUS_SUSPENDED => Ok(ServerState::Suspended),
            crate::bindings::da::OPC_STATUS_TEST => Ok(ServerState::Test),
            crate::bindings::da::OPC_STATUS_COMM_FAULT => Ok(ServerState::CommunicationFault),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown server state: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<crate::bindings::da::tagOPCSERVERSTATE> for ServerState {
    fn to_native(&self) -> crate::bindings::da::tagOPCSERVERSTATE {
        match self {
            ServerState::Running => crate::bindings::da::OPC_STATUS_RUNNING,
            ServerState::Failed => crate::bindings::da::OPC_STATUS_FAILED,
            ServerState::NoConfig => crate::bindings::da::OPC_STATUS_NOCONFIG,
            ServerState::Suspended => crate::bindings::da::OPC_STATUS_SUSPENDED,
            ServerState::Test => crate::bindings::da::OPC_STATUS_TEST,
            ServerState::CommunicationFault => crate::bindings::da::OPC_STATUS_COMM_FAULT,
        }
    }
}

/// Scope for enumerating server items or connections.
#[derive(Debug, Clone, PartialEq)]
pub enum EnumScope {
    PrivateConnections,
    PublicConnections,
    AllConnections,
    Public,
    Private,
    All,
}

impl TryFromNative<crate::bindings::da::tagOPCENUMSCOPE> for EnumScope {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCENUMSCOPE,
    ) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_ENUM_PRIVATE_CONNECTIONS => Ok(EnumScope::PrivateConnections),
            crate::bindings::da::OPC_ENUM_PUBLIC_CONNECTIONS => Ok(EnumScope::PublicConnections),
            crate::bindings::da::OPC_ENUM_ALL_CONNECTIONS => Ok(EnumScope::AllConnections),
            crate::bindings::da::OPC_ENUM_PUBLIC => Ok(EnumScope::Public),
            crate::bindings::da::OPC_ENUM_PRIVATE => Ok(EnumScope::Private),
            crate::bindings::da::OPC_ENUM_ALL => Ok(EnumScope::All),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown enum scope: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<crate::bindings::da::tagOPCENUMSCOPE> for EnumScope {
    fn to_native(&self) -> crate::bindings::da::tagOPCENUMSCOPE {
        match self {
            EnumScope::PrivateConnections => crate::bindings::da::OPC_ENUM_PRIVATE_CONNECTIONS,
            EnumScope::PublicConnections => crate::bindings::da::OPC_ENUM_PUBLIC_CONNECTIONS,
            EnumScope::AllConnections => crate::bindings::da::OPC_ENUM_ALL_CONNECTIONS,
            EnumScope::Public => crate::bindings::da::OPC_ENUM_PUBLIC,
            EnumScope::Private => crate::bindings::da::OPC_ENUM_PRIVATE,
            EnumScope::All => crate::bindings::da::OPC_ENUM_ALL,
        }
    }
}

/// Full attribute set of a single OPC item.
pub struct ItemAttributes {
    pub access_path: String,
    pub item_id: String,
    pub active: bool,
    pub client_handle: ItemHandle,
    pub server_handle: ItemHandle,
    pub access_rights: u32,
    pub blob: Vec<u8>,
    pub requested_data_type: u16,
    pub canonical_data_type: u16,
    pub eu_type: EuType,
    pub eu_info: windows::Win32::System::Variant::VARIANT,
}

impl TryFromNative<crate::bindings::da::tagOPCITEMATTRIBUTES> for ItemAttributes {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCITEMATTRIBUTES,
    ) -> windows::core::Result<Self> {
        Ok(Self {
            access_path: try_from_native!(&native.szAccessPath),
            item_id: try_from_native!(&native.szItemID),
            active: native.bActive.into(),
            client_handle: ItemHandle(native.hClient),
            server_handle: ItemHandle(native.hServer),
            access_rights: native.dwAccessRights,
            blob: RemoteArray::from_mut_ptr(native.pBlob, native.dwBlobSize)
                .as_slice()
                .to_vec(),
            requested_data_type: native.vtRequestedDataType,
            canonical_data_type: native.vtCanonicalDataType,
            eu_type: try_from_native!(&native.dwEUType),
            eu_info: native.vEUInfo.clone(),
        })
    }
}

/// Engineering Units (EU) classification type.
pub enum EuType {
    NoEnum,
    Analog,
    Enumerated,
}

impl TryFromNative<crate::bindings::da::tagOPCEUTYPE> for EuType {
    fn try_from_native(native: &crate::bindings::da::tagOPCEUTYPE) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_NOENUM => Ok(EuType::NoEnum),
            crate::bindings::da::OPC_ANALOG => Ok(EuType::Analog),
            crate::bindings::da::OPC_ENUMERATED => Ok(EuType::Enumerated),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown EU type: {unknown:?}"),
            )),
        }
    }
}

/// Current state of a watched OPC item including value, quality, and time.
pub struct ItemState {
    pub client_handle: ItemHandle,
    pub timestamp: std::time::SystemTime,
    pub quality: u16,
    pub data_value: windows::Win32::System::Variant::VARIANT,
}

impl TryFromNative<crate::bindings::da::tagOPCITEMSTATE> for ItemState {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCITEMSTATE,
    ) -> windows::core::Result<Self> {
        Ok(Self {
            client_handle: ItemHandle(native.hClient),
            timestamp: try_from_native!(&native.ftTimeStamp),
            quality: native.wQuality,
            data_value: native.vDataValue.clone(),
        })
    }
}

/// Reading source preference (Cache or Device).
pub enum DataSourceTarget {
    ForceCache,
    ForceDevice,
    WithMaxAge(u32),
}

impl DataSourceTarget {
    pub fn max_age(&self) -> u32 {
        match self {
            DataSourceTarget::WithMaxAge(max_age) => *max_age,
            DataSourceTarget::ForceCache => u32::MAX,
            DataSourceTarget::ForceDevice => 0,
        }
    }
}

impl TryFromNative<crate::bindings::da::tagOPCDATASOURCE> for DataSourceTarget {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCDATASOURCE,
    ) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_DS_CACHE => Ok(DataSourceTarget::ForceCache),
            crate::bindings::da::OPC_DS_DEVICE => Ok(DataSourceTarget::ForceDevice),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown data source: {unknown:?}"),
            )),
        }
    }
}

impl TryToNative<crate::bindings::da::tagOPCDATASOURCE> for DataSourceTarget {
    fn try_to_native(&self) -> windows::core::Result<crate::bindings::da::tagOPCDATASOURCE> {
        match self {
            DataSourceTarget::ForceCache => Ok(crate::bindings::da::OPC_DS_CACHE),
            DataSourceTarget::ForceDevice => Ok(crate::bindings::da::OPC_DS_DEVICE),
            DataSourceTarget::WithMaxAge(_) => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "MaxAge data source requires a value",
            )),
        }
    }
}

/// Full read value result carrying value, quality, and timestamp.
pub struct ItemValue {
    pub value: windows::Win32::System::Variant::VARIANT,
    pub quality: u16,
    pub timestamp: std::time::SystemTime,
}

impl
    TryFromNative<(
        RemoteArray<windows::Win32::System::Variant::VARIANT>,
        RemoteArray<u16>,
        RemoteArray<windows::Win32::Foundation::FILETIME>,
        RemoteArray<windows::core::HRESULT>,
    )> for Vec<windows::core::Result<ItemValue>>
{
    fn try_from_native(
        native: &(
            RemoteArray<windows::Win32::System::Variant::VARIANT>,
            RemoteArray<u16>,
            RemoteArray<windows::Win32::Foundation::FILETIME>,
            RemoteArray<windows::core::HRESULT>,
        ),
    ) -> windows::core::Result<Self> {
        let (values, qualities, timestamps, errors) = native;

        if values.len() != qualities.len()
            || values.len() != timestamps.len()
            || values.len() != errors.len()
        {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "Arrays have different lengths",
            ));
        }

        Ok(values
            .as_slice()
            .iter()
            .zip(qualities.as_slice())
            .zip(timestamps.as_slice())
            .zip(errors.as_slice())
            .map(|(((value, quality), timestamp), error)| {
                if error.is_ok() {
                    Ok(ItemValue {
                        value: value.clone(),
                        quality: *quality,
                        timestamp: try_from_native!(timestamp),
                    })
                } else {
                    Err((*error).into())
                }
            })
            .collect())
    }
}

/// Item value struct for writes or partial updates.
pub struct ItemPartialValue {
    pub value: windows::Win32::System::Variant::VARIANT,
    pub quality: Option<u16>,
    pub timestamp: Option<std::time::SystemTime>,
}

// try to native
impl TryToNative<crate::bindings::da::tagOPCITEMVQT> for ItemPartialValue {
    fn try_to_native(&self) -> windows::core::Result<crate::bindings::da::tagOPCITEMVQT> {
        Ok(crate::bindings::da::tagOPCITEMVQT {
            vDataValue: self.value.clone(),
            bQualitySpecified: self.quality.is_some().into(),
            wQuality: self.quality.unwrap_or_default(),
            bTimeStampSpecified: self.timestamp.is_some().into(),
            ftTimeStamp: self
                .timestamp
                .map(|t| t.try_to_native())
                .transpose()?
                .unwrap_or_default(),
            wReserved: 0,
            dwReserved: 0,
        })
    }
}

/// Filter type for navigating the namespace (Branch, Leaf, Flat).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseType {
    Branch = 1,
    Leaf = 2,
    Flat = 3,
}

impl From<BrowseType> for u32 {
    #[inline]
    fn from(browse_type: BrowseType) -> Self {
        browse_type as u32
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
        dir as u32
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

impl TryFromNative<crate::bindings::da::tagOPCBROWSETYPE> for BrowseType {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCBROWSETYPE,
    ) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_BRANCH => Ok(BrowseType::Branch),
            crate::bindings::da::OPC_LEAF => Ok(BrowseType::Leaf),
            crate::bindings::da::OPC_FLAT => Ok(BrowseType::Flat),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown browse type: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<crate::bindings::da::tagOPCBROWSETYPE> for BrowseType {
    fn to_native(&self) -> crate::bindings::da::tagOPCBROWSETYPE {
        match self {
            BrowseType::Branch => crate::bindings::da::OPC_BRANCH,
            BrowseType::Leaf => crate::bindings::da::OPC_LEAF,
            BrowseType::Flat => crate::bindings::da::OPC_FLAT,
        }
    }
}

/// Granular filter for enumeration results.
pub enum BrowseFilter {
    All,
    Branches,
    Items,
}

impl TryFromNative<crate::bindings::da::tagOPCBROWSEFILTER> for BrowseFilter {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCBROWSEFILTER,
    ) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_BROWSE_FILTER_ALL => Ok(BrowseFilter::All),
            crate::bindings::da::OPC_BROWSE_FILTER_BRANCHES => Ok(BrowseFilter::Branches),
            crate::bindings::da::OPC_BROWSE_FILTER_ITEMS => Ok(BrowseFilter::Items),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown browse filter: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<crate::bindings::da::tagOPCBROWSEFILTER> for BrowseFilter {
    fn to_native(&self) -> crate::bindings::da::tagOPCBROWSEFILTER {
        match self {
            BrowseFilter::All => crate::bindings::da::OPC_BROWSE_FILTER_ALL,
            BrowseFilter::Branches => crate::bindings::da::OPC_BROWSE_FILTER_BRANCHES,
            BrowseFilter::Items => crate::bindings::da::OPC_BROWSE_FILTER_ITEMS,
        }
    }
}

/// Typology of the server's address space.
pub enum NamespaceType {
    Flat,
    Hierarchy,
}

impl TryFromNative<crate::bindings::da::tagOPCNAMESPACETYPE> for NamespaceType {
    fn try_from_native(
        native: &crate::bindings::da::tagOPCNAMESPACETYPE,
    ) -> windows::core::Result<Self> {
        match *native {
            crate::bindings::da::OPC_NS_HIERARCHIAL => Ok(NamespaceType::Hierarchy),
            crate::bindings::da::OPC_NS_FLAT => Ok(NamespaceType::Flat),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown namespace type: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<crate::bindings::da::tagOPCNAMESPACETYPE> for NamespaceType {
    fn to_native(&self) -> crate::bindings::da::tagOPCNAMESPACETYPE {
        match self {
            NamespaceType::Hierarchy => crate::bindings::da::OPC_NS_HIERARCHIAL,
            NamespaceType::Flat => crate::bindings::da::OPC_NS_FLAT,
        }
    }
}

// COSERVERINFO
/// Information defining how to connect to a remote server.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerInfo {
    pub name: String,
    pub auth_info: AuthInfo,
}

/// FFI-safe bridge for `ServerInfo` (COSERVERINFO).
pub struct ServerInfoBridge {
    pub name: LocalPointer<Vec<u16>>,
    pub auth_info: AuthInfoBridge,
}

impl IntoBridge<ServerInfoBridge> for ServerInfo {
    fn into_bridge(self) -> ServerInfoBridge {
        ServerInfoBridge {
            name: LocalPointer::from(&self.name),
            auth_info: self.auth_info.into_bridge(),
        }
    }
}

impl TryToNative<windows::Win32::System::Com::COSERVERINFO> for ServerInfoBridge {
    fn try_to_native(&self) -> windows::core::Result<windows::Win32::System::Com::COSERVERINFO> {
        Ok(windows::Win32::System::Com::COSERVERINFO {
            dwReserved1: 0,
            dwReserved2: 0,
            pwszName: self.name.as_pwstr(),
            pAuthInfo: &self.auth_info.try_to_native()? as *const _ as *mut _,
        })
    }
}

/// Authentication and authorization settings for DCOM.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthInfo {
    pub authn_svc: u32,
    pub authz_svc: u32,
    pub server_principal_name: String,
    pub authn_level: u32,
    pub impersonation_level: u32,
    pub auth_identity_data: AuthIdentity,
    pub capabilities: u32,
}

/// FFI-safe bridge for `AuthInfo` (COAUTHINFO).
pub struct AuthInfoBridge {
    pub authn_svc: u32,
    pub authz_svc: u32,
    pub server_principal_name: LocalPointer<Vec<u16>>,
    pub authn_level: u32,
    pub impersonation_level: u32,
    pub auth_identity_data: AuthIdentityBridge,
    pub capabilities: u32,
}

impl IntoBridge<AuthInfoBridge> for AuthInfo {
    fn into_bridge(self) -> AuthInfoBridge {
        AuthInfoBridge {
            authn_svc: self.authn_svc,
            authz_svc: self.authz_svc,
            server_principal_name: LocalPointer::from(&self.server_principal_name),
            authn_level: self.authn_level,
            impersonation_level: self.impersonation_level,
            auth_identity_data: self.auth_identity_data.into_bridge(),
            capabilities: self.capabilities,
        }
    }
}

impl TryToNative<windows::Win32::System::Com::COAUTHINFO> for AuthInfoBridge {
    fn try_to_native(&self) -> windows::core::Result<windows::Win32::System::Com::COAUTHINFO> {
        Ok(windows::Win32::System::Com::COAUTHINFO {
            dwAuthnSvc: self.authn_svc,
            dwAuthzSvc: self.authz_svc,
            pwszServerPrincName: self.server_principal_name.as_pwstr(),
            dwAuthnLevel: self.authn_level,
            dwImpersonationLevel: self.impersonation_level,
            pAuthIdentityData: &self.auth_identity_data.try_to_native()? as *const _ as *mut _,
            dwCapabilities: self.capabilities,
        })
    }
}

/// DCOM authentication credentials.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthIdentity {
    pub user: String,
    pub domain: String,
    pub password: String,
    pub flags: u32,
}

/// FFI-safe bridge for `AuthIdentity` (COAUTHIDENTITY).
pub struct AuthIdentityBridge {
    pub user: LocalPointer<Vec<u16>>,
    pub domain: LocalPointer<Vec<u16>>,
    pub password: LocalPointer<Vec<u16>>,
    pub flags: u32,
}

impl IntoBridge<AuthIdentityBridge> for AuthIdentity {
    fn into_bridge(self) -> AuthIdentityBridge {
        AuthIdentityBridge {
            user: LocalPointer::from(&self.user),
            domain: LocalPointer::from(&self.domain),
            password: LocalPointer::from(&self.password),
            flags: self.flags,
        }
    }
}

impl TryToNative<windows::Win32::System::Com::COAUTHIDENTITY> for AuthIdentityBridge {
    fn try_to_native(&self) -> windows::core::Result<windows::Win32::System::Com::COAUTHIDENTITY> {
        Ok(windows::Win32::System::Com::COAUTHIDENTITY {
            User: self.user.as_pwstr().0,
            UserLength: self.user.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "User name exceeds u32 maximum length",
                )
            })?,
            Domain: self.domain.as_pwstr().0,
            DomainLength: self.domain.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Domain name exceeds u32 maximum length",
                )
            })?,
            Password: self.password.as_pwstr().0,
            PasswordLength: self.password.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Password exceeds u32 maximum length",
                )
            })?,
            Flags: self.flags,
        })
    }
}

/// COM instantiation context flags (CLSCTX).
#[derive(Debug, Clone, PartialEq)]
pub enum ClassContext {
    All,
    InProcServer,
    InProcHandler,
    LocalServer,
    InProcServer16,
    RemoteServer,
    InProcHandler16,
    NoCodeDownload,
    NoCustomMarshal,
    EnableCodeDownload,
    NoFailureLog,
    DisableAAA,
    EnableAAA,
    FromDefaultContext,
    ActivateX86Server,
    Activate32BitServer,
    Activate64BitServer,
    EnableCloaking,
    AppContainer,
    ActivateAAAAsIU,
    ActivateARM32Server,
    AllowLowerTrustRegistration,
    PsDll,
}

impl ToNative<windows::Win32::System::Com::CLSCTX> for ClassContext {
    fn to_native(&self) -> windows::Win32::System::Com::CLSCTX {
        match self {
            ClassContext::All => windows::Win32::System::Com::CLSCTX_ALL,
            ClassContext::InProcServer => windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            ClassContext::InProcHandler => windows::Win32::System::Com::CLSCTX_INPROC_HANDLER,
            ClassContext::LocalServer => windows::Win32::System::Com::CLSCTX_LOCAL_SERVER,
            ClassContext::InProcServer16 => windows::Win32::System::Com::CLSCTX_INPROC_SERVER16,
            ClassContext::RemoteServer => windows::Win32::System::Com::CLSCTX_REMOTE_SERVER,
            ClassContext::InProcHandler16 => windows::Win32::System::Com::CLSCTX_INPROC_HANDLER16,
            ClassContext::NoCodeDownload => windows::Win32::System::Com::CLSCTX_NO_CODE_DOWNLOAD,
            ClassContext::NoCustomMarshal => windows::Win32::System::Com::CLSCTX_NO_CUSTOM_MARSHAL,
            ClassContext::EnableCodeDownload => {
                windows::Win32::System::Com::CLSCTX_ENABLE_CODE_DOWNLOAD
            }
            ClassContext::NoFailureLog => windows::Win32::System::Com::CLSCTX_NO_FAILURE_LOG,
            ClassContext::DisableAAA => windows::Win32::System::Com::CLSCTX_DISABLE_AAA,
            ClassContext::EnableAAA => windows::Win32::System::Com::CLSCTX_ENABLE_AAA,
            ClassContext::FromDefaultContext => {
                windows::Win32::System::Com::CLSCTX_FROM_DEFAULT_CONTEXT
            }
            ClassContext::ActivateX86Server => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_X86_SERVER
            }
            ClassContext::Activate32BitServer => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_32_BIT_SERVER
            }
            ClassContext::Activate64BitServer => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_64_BIT_SERVER
            }
            ClassContext::EnableCloaking => windows::Win32::System::Com::CLSCTX_ENABLE_CLOAKING,
            ClassContext::AppContainer => windows::Win32::System::Com::CLSCTX_APPCONTAINER,
            ClassContext::ActivateAAAAsIU => windows::Win32::System::Com::CLSCTX_ACTIVATE_AAA_AS_IU,
            ClassContext::ActivateARM32Server => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_ARM32_SERVER
            }
            ClassContext::AllowLowerTrustRegistration => {
                windows::Win32::System::Com::CLSCTX_ALLOW_LOWER_TRUST_REGISTRATION
            }
            ClassContext::PsDll => windows::Win32::System::Com::CLSCTX_PS_DLL,
        }
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
        let q = OpcQuality::from(0x00D8); // 192 | 24
        assert_eq!(q.major, QualityMajor::Good);
        assert_eq!(q.substatus, QualitySubstatus::LocalOverride);
        assert_eq!(q.limit, QualityLimit::NotLimited);
        assert_eq!(q.to_string(), "Good (Local Override)");
    }

    #[test]
    fn test_opc_quality_bad_comm_failure() {
        let q = OpcQuality::from(0x0018); // 0 | 24
        assert_eq!(q.major, QualityMajor::Bad);
        assert_eq!(q.substatus, QualitySubstatus::CommFailure);
        assert_eq!(q.limit, QualityLimit::NotLimited);
        assert!(q.is_bad());
        assert_eq!(q.to_string(), "Bad (Comm Failure)");
    }

    #[test]
    fn test_opc_quality_uncertain_limits() {
        let q = OpcQuality::from(0x0056); // 64 (Uncertain) | 20 (EGU Exceeded) | 2 (High Limited)
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
        assert_eq!(OpcQuality::from("good"), OpcQuality::GOOD);
        assert_eq!(OpcQuality::from("Good"), OpcQuality::GOOD);
        assert_eq!(OpcQuality::from("bad"), OpcQuality::BAD);
        assert_eq!(OpcQuality::from("uncertain"), OpcQuality::UNCERTAIN);
        assert_eq!(OpcQuality::from("other"), OpcQuality::BAD);
    }
}
