//! OPC DA server identification, connection endpoints, and catalog metadata.

use std::fmt;
use std::str::FromStr;

use crate::errors::OpcError;

/// Normalizes a host string slice, returning `None` if it represents the local machine.
///
/// Strings that are empty, whitespace-only, `"localhost"`, `"127.0.0.1"`, or `"::1"`
/// (case-insensitive) are normalized to `None`. All other hosts return `Some(trimmed_host)`.
///
/// # Arguments
///
/// * `host` - Optional host name or IP address slice to normalize.
///
/// # Returns
///
/// Returns `Some(&str)` containing the trimmed remote host, or `None` if the host represents localhost.
///
/// # Examples
///
/// ```
/// use opc_da_client::types::normalize_host_str;
///
/// assert_eq!(normalize_host_str(Some("localhost")), None);
/// assert_eq!(normalize_host_str(Some("127.0.0.1")), None);
/// assert_eq!(normalize_host_str(Some("  ")), None);
/// assert_eq!(normalize_host_str(Some("192.168.1.50")), Some("192.168.1.50"));
/// ```
#[must_use]
pub fn normalize_host_str(host: Option<&str>) -> Option<&str> {
    let h = host?.trim();
    if h.is_empty() || h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1" || h == "::1" {
        None
    } else {
        Some(h)
    }
}

/// Normalizes a host string, returning `None` if it represents the local machine.
///
/// Strings that are empty, whitespace-only, `"localhost"`, `"127.0.0.1"`, or `"::1"`
/// (case-insensitive) are normalized to `None`. All other hosts return `Some(trimmed_host)`.
///
/// # Arguments
///
/// * `host` - Optional host name or IP address to normalize.
///
/// # Returns
///
/// Returns `Some(String)` containing the trimmed remote host, or `None` if the host represents localhost.
///
/// # Examples
///
/// ```
/// use opc_da_client::types::normalize_host;
///
/// assert_eq!(normalize_host(Some("localhost")), None);
/// assert_eq!(normalize_host(Some("scada-node-01")), Some("scada-node-01".to_string()));
/// ```
#[must_use]
pub fn normalize_host(host: Option<&str>) -> Option<String> {
    normalize_host_str(host).map(str::to_string)
}

/// Determines if a host specification represents a remote machine without heap allocations.
///
/// Returns `false` if `host` is `None`, empty, whitespace-only, `"localhost"`, `"127.0.0.1"`, or `"::1"`.
///
/// # Arguments
///
/// * `host` - Optional host name or IP address slice to test.
///
/// # Returns
///
/// Returns `true` if `host` represents a remote address, `false` otherwise.
///
/// # Examples
///
/// ```
/// use opc_da_client::types::is_remote_host;
///
/// assert!(!is_remote_host(None));
/// assert!(!is_remote_host(Some("localhost")));
/// assert!(is_remote_host(Some("192.168.1.10")));
/// ```
#[inline]
#[must_use]
pub fn is_remote_host(host: Option<&str>) -> bool {
    normalize_host_str(host).is_some()
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
    ///
    /// # Returns
    ///
    /// `Some(&str)` containing the ProgID, or `None` if this identifier is a CLSID.
    #[must_use]
    pub fn as_prog_id(&self) -> Option<&str> {
        match self {
            Self::ProgId(prog_id) => Some(prog_id.as_str()),
            Self::Clsid(_) => None,
        }
    }

    /// Returns a borrowed reference to the CLSID GUID if this is a [`ServerIdentifier::Clsid`].
    ///
    /// # Returns
    ///
    /// `Some(&GUID)` containing the CLSID, or `None` if this identifier is a ProgID.
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
    /// Creates a new [`OpcServerInfo`] instance with normalized host specification.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        prog_id: impl Into<String>,
        clsid: windows::core::GUID,
        user_type: Option<String>,
        host: Option<String>,
    ) -> Self {
        Self {
            prog_id: prog_id.into(),
            clsid,
            user_type,
            host: normalize_host(host.as_deref()),
        }
    }

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
    ///
    /// # Arguments
    ///
    /// * `identifier` - The ProgID or CLSID identifying the target server.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::OpcServerEndpoint;
    ///
    /// let ep = OpcServerEndpoint::local("Matrikon.OPC.Simulation.1");
    /// assert!(!ep.is_remote());
    /// ```
    #[must_use]
    pub fn local(identifier: impl Into<ServerIdentifier>) -> Self {
        Self {
            host: None,
            identifier: identifier.into(),
        }
    }

    /// Creates a new endpoint targeting a remote machine.
    ///
    /// Automatically normalizes `host` so local references (`"localhost"`, `"127.0.0.1"`)
    /// map to `None`.
    ///
    /// # Arguments
    ///
    /// * `host` - Target hostname or IP address.
    /// * `identifier` - The ProgID or CLSID identifying the target server.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::OpcServerEndpoint;
    ///
    /// let ep = OpcServerEndpoint::remote("192.168.1.10", "Matrikon.OPC.Simulation.1");
    /// assert!(ep.is_remote());
    /// assert_eq!(ep.host.as_deref(), Some("192.168.1.10"));
    /// ```
    #[must_use]
    pub fn remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self {
        let host_str = host.into();
        Self {
            host: normalize_host(Some(&host_str)),
            identifier: identifier.into(),
        }
    }

    /// Returns `true` if this endpoint targets a remote machine.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::OpcServerEndpoint;
    ///
    /// assert!(!OpcServerEndpoint::local("Server").is_remote());
    /// assert!(OpcServerEndpoint::remote("10.0.0.1", "Server").is_remote());
    /// ```
    #[must_use]
    pub fn is_remote(&self) -> bool {
        is_remote_host(self.host.as_deref())
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

/// Parses an endpoint from a string slice.
///
/// Supports UNC syntax (`r"\\<host>\<server>"` or `"//<host>/<server>"`) as well as
/// standalone server identifiers (local connection).
///
/// # Examples
///
/// ```
/// use opc_da_client::types::OpcServerEndpoint;
///
/// let ep: OpcServerEndpoint = r"\\192.168.1.50\Matrikon.OPC.Simulation.1".parse().unwrap();
/// assert!(ep.is_remote());
/// assert_eq!(ep.host.as_deref(), Some("192.168.1.50"));
///
/// let local_ep: OpcServerEndpoint = "Matrikon.OPC.Simulation.1".parse().unwrap();
/// assert!(!local_ep.is_remote());
/// ```
impl FromStr for OpcServerEndpoint {
    type Err = OpcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(OpcError::Conversion(
                "Server identifier cannot be empty".into(),
            ));
        }

        if let Some(rest) = trimmed
            .strip_prefix(r"\\")
            .or_else(|| trimmed.strip_prefix("//"))
        {
            if let Some(sep_idx) = rest.find(['\\', '/']) {
                let raw_host = &rest[..sep_idx];
                let raw_server = &rest[sep_idx + 1..];
                let host = normalize_host_str(Some(raw_host));
                let server = raw_server.trim();
                if server.is_empty() {
                    return Err(OpcError::Conversion(
                        "Missing server identifier in endpoint UNC path".into(),
                    ));
                }
                Ok(Self {
                    host: host.map(str::to_string),
                    identifier: ServerIdentifier::from(server),
                })
            } else {
                Err(OpcError::Conversion(
                    "Invalid endpoint UNC path: expected host and server separated by '\\'".into(),
                ))
            }
        } else {
            Ok(Self {
                host: None,
                identifier: ServerIdentifier::from(trimmed),
            })
        }
    }
}

impl From<&str> for OpcServerEndpoint {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_else(|_| Self::local(s))
    }
}

impl From<String> for OpcServerEndpoint {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
