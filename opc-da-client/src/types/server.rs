//! OPC DA server identification, group configuration, and status models.

use super::GroupHandle;
use std::fmt;

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
