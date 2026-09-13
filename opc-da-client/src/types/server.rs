//! OPC DA server identification, connection endpoints, and catalog metadata.

use std::fmt;
use std::str::FromStr;

use crate::errors::OpcError;
use crate::types::clsid::Clsid;

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

/// Strongly-typed identifier for an OPC DA server.
///
/// An OPC server can be referenced either by its human-readable Programmatic
/// Identifier (`ProgID`), or directly by its 128-bit Windows COM Class ID (`CLSID`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ServerIdentifier {
    /// Server referenced by human-readable ProgID (e.g., `"Matrikon.OPC.Simulation.1"`).
    ProgId(String),
    /// Server referenced directly by Windows COM CLSID.
    Clsid(Clsid),
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

    /// Returns a borrowed reference to the CLSID if this is a [`ServerIdentifier::Clsid`].
    ///
    /// # Returns
    ///
    /// `Some(&Clsid)` containing the CLSID, or `None` if this identifier is a ProgID.
    #[must_use]
    pub const fn as_clsid(&self) -> Option<&Clsid> {
        match self {
            Self::Clsid(clsid) => Some(clsid),
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
#[cfg(all(test, feature = "opc-da-backend"))]
#[must_use]
pub(crate) fn format_guid_bracketed(guid: &windows_core::GUID) -> String {
    Clsid::from_windows_guid(*guid).to_bracketed()
}

impl fmt::Display for ServerIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgId(prog_id) => write!(f, "{prog_id}"),
            Self::Clsid(clsid) => write!(f, "{clsid}"),
        }
    }
}

impl From<Clsid> for ServerIdentifier {
    fn from(clsid: Clsid) -> Self {
        Self::Clsid(clsid)
    }
}

#[cfg(feature = "opc-da-backend")]
impl From<windows_core::GUID> for ServerIdentifier {
    fn from(guid: windows_core::GUID) -> Self {
        Self::Clsid(Clsid::from_windows_guid(guid))
    }
}

impl From<&str> for ServerIdentifier {
    fn from(s: &str) -> Self {
        if let Some(clsid) = Clsid::parse(s) {
            Self::Clsid(clsid)
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
    pub clsid: Clsid,
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
        clsid: impl Into<Clsid>,
        user_type: Option<String>,
        host: Option<String>,
    ) -> Self {
        Self {
            prog_id: prog_id.into(),
            clsid: clsid.into(),
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
                crate::errors::ConversionError::InvalidEndpoint(
                    "Server identifier cannot be empty".into(),
                ),
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
                        crate::errors::ConversionError::InvalidEndpoint(
                            "Missing server identifier in endpoint UNC path".into(),
                        ),
                    ));
                }
                Ok(Self {
                    host: host.map(str::to_string),
                    identifier: ServerIdentifier::from(server),
                })
            } else {
                Err(OpcError::Conversion(
                    crate::errors::ConversionError::InvalidEndpoint(
                        "Invalid endpoint UNC path: expected host and server separated by '\\'"
                            .into(),
                    ),
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
