//! OPC DA server identification, connection endpoints, and catalog metadata.

use std::fmt;
use std::str::FromStr;

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
#[must_use]
pub(crate) fn normalize_host_str(host: Option<&str>) -> Option<&str> {
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
/// (case-insensitive) are normalized to `None`. All other hosts return
/// `Some(lowercase_host)` using ASCII case folding.
///
/// # Arguments
///
/// * `host` - Optional host name or IP address to normalize.
///
/// # Returns
///
/// Returns `Some(String)` containing the lowercase remote host, or `None` if the host represents localhost.
#[must_use]
pub(crate) fn normalize_host(host: Option<&str>) -> Option<String> {
    normalize_host_str(host).map(str::to_ascii_lowercase)
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
#[inline]
#[must_use]
pub(crate) fn is_remote_host(host: Option<&str>) -> bool {
    normalize_host_str(host).is_some()
}

/// Error returned when parsing a [`ServerIdentifier`] from an invalid string representation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseServerIdError {
    /// Input string was empty or whitespace-only.
    #[error("Server identifier cannot be empty")]
    Empty,
    /// ProgID exceeded the maximum allowed length of 255 characters.
    #[error("ProgID length {0} exceeds maximum allowed 255 characters")]
    ProgIdTooLong(usize),
    /// ProgID contained invalid syntax, characters, or dot placement.
    #[error("Invalid characters or syntax in ProgID: '{0}'")]
    InvalidProgId(String),
    /// CLSID parsing failed.
    #[error("Invalid CLSID syntax: {0}")]
    InvalidClsid(#[from] crate::types::clsid::ParseClsidError),
}

/// Error returned when parsing an [`OpcServerEndpoint`] from an invalid string representation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseEndpointError {
    /// Input string was empty or whitespace-only.
    #[error("Server endpoint cannot be empty")]
    Empty,
    /// Missing server identifier in endpoint path.
    #[error("Missing server identifier in endpoint path: '{0}'")]
    MissingServer(String),
    /// Malformed endpoint URI or path syntax.
    #[error("Invalid endpoint syntax: '{0}'")]
    InvalidFormat(String),
    /// Server identifier in endpoint failed validation.
    #[error("Invalid server identifier in endpoint: {0}")]
    InvalidServerId(#[from] ParseServerIdError),
}

impl From<ParseEndpointError> for crate::errors::ConversionError {
    fn from(err: ParseEndpointError) -> Self {
        Self::InvalidEndpoint(err.to_string())
    }
}

impl From<ParseEndpointError> for crate::errors::OpcError {
    fn from(err: ParseEndpointError) -> Self {
        Self::Conversion(crate::errors::ConversionError::from(err))
    }
}

impl From<ParseServerIdError> for crate::errors::ConversionError {
    fn from(err: ParseServerIdError) -> Self {
        ParseEndpointError::from(err).into()
    }
}

impl From<ParseServerIdError> for crate::errors::OpcError {
    fn from(err: ParseServerIdError) -> Self {
        Self::Conversion(crate::errors::ConversionError::from(err))
    }
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

    /// Returns the CLSID by value if this identifier is a [`ServerIdentifier::Clsid`].
    ///
    /// # Returns
    ///
    /// `Some(Clsid)` containing a copy of the CLSID, or `None` if this identifier is a ProgID.
    #[must_use]
    pub const fn clsid(&self) -> Option<Clsid> {
        match self {
            Self::Clsid(clsid) => Some(*clsid),
            Self::ProgId(_) => None,
        }
    }

    /// Compares two server identifiers for semantic equivalence under OPC DA naming rules.
    ///
    /// Programmatic Identifiers ([`ServerIdentifier::ProgId`]) are compared case-insensitively
    /// using ASCII folding. Class IDs ([`ServerIdentifier::Clsid`]) are compared for exact
    /// 128-bit numerical equality. Cross-variant comparisons always return `false` because
    /// resolving a ProgID to its CLSID requires Windows COM registry activation
    /// (`CLSIDFromProgID`), which is intentionally excluded from pure in-memory comparison.
    ///
    /// Unlike the derived [`PartialEq`] implementation (which is byte-exact for map and set
    /// key stability), `matches` implements domain-level equivalence.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::ServerIdentifier;
    ///
    /// let id1 = ServerIdentifier::new("Matrikon.OPC.Simulation.1").unwrap();
    /// let id2 = ServerIdentifier::new("matrikon.opc.simulation.1").unwrap();
    /// assert!(id1.matches(&id2));
    /// assert_ne!(id1, id2); // Byte-exact structural equality remains false
    /// ```
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ProgId(a), Self::ProgId(b)) => a.eq_ignore_ascii_case(b),
            (Self::Clsid(a), Self::Clsid(b)) => a == b,
            _ => false,
        }
    }

    /// Validates and constructs a new `ServerIdentifier` from a string slice (ProgID or CLSID).
    ///
    /// # Errors
    /// Returns [`ParseServerIdError`] if string is empty, exceeds 255 chars, contains invalid characters,
    /// or represents an invalid CLSID format.
    pub fn new(s: &str) -> Result<Self, ParseServerIdError> {
        s.parse()
    }
}

/// Validates that a string slice conforms to OPC DA Programmatic Identifier (ProgID) rules.
///
/// A valid ProgID:
/// - Must have length between 1 and 255 characters inclusive.
/// - Must contain only ASCII alphanumeric characters, dots (`.`), underscores (`_`), or hyphens (`-`).
/// - Cannot begin or end with a dot.
/// - Cannot contain consecutive dots (`..`).
/// - Cannot be empty or whitespace-only.
pub(crate) fn validate_prog_id(s: &str) -> Result<(), ParseServerIdError> {
    if s.is_empty() || s.chars().all(char::is_whitespace) {
        return Err(ParseServerIdError::Empty);
    }
    if s.len() > 255 {
        return Err(ParseServerIdError::ProgIdTooLong(s.len()));
    }
    if s.starts_with('.') || s.ends_with('.') || s.contains("..") {
        return Err(ParseServerIdError::InvalidProgId(s.to_string()));
    }
    for b in s.bytes() {
        if !(b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-') {
            return Err(ParseServerIdError::InvalidProgId(s.to_string()));
        }
    }
    Ok(())
}

impl FromStr for ServerIdentifier {
    type Err = ParseServerIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseServerIdError::Empty);
        }
        if trimmed.contains('\0') {
            return Err(ParseServerIdError::InvalidProgId(trimmed.to_string()));
        }

        // 1. If explicit curly braces wrap the string, it MUST be a valid CLSID
        if trimmed.starts_with('{') {
            let clsid = Clsid::from_str(trimmed).map_err(ParseServerIdError::InvalidClsid)?;
            return Ok(Self::Clsid(clsid));
        }

        // 2. Attempt zero-allocation branchless CLSID parsing for unbracketed standard GUIDs
        if let Some(clsid) = Clsid::parse(trimmed) {
            return Ok(Self::Clsid(clsid));
        }

        // 3. Otherwise validate as ProgID (which legitimately permits hyphens)
        validate_prog_id(trimmed)?;
        Ok(Self::ProgId(trimmed.to_string()))
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

impl From<windows_core::GUID> for ServerIdentifier {
    fn from(guid: windows_core::GUID) -> Self {
        Self::Clsid(Clsid::from_windows_guid(guid))
    }
}

impl TryFrom<&str> for ServerIdentifier {
    type Error = ParseServerIdError;

    #[inline]
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<String> for ServerIdentifier {
    type Error = ParseServerIdError;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Rich catalog metadata for an enumerated OPC DA server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcServerInfo {
    /// Programmatic Identifier of the server (e.g., `"Matrikon.OPC.Simulation.1"`).
    prog_id: String,
    /// 128-bit COM Class ID of the server.
    clsid: Clsid,
    /// Human-readable server title from catalog metadata, or `None` if absent.
    user_type: Option<String>,
    /// Target host machine (or `None` for localhost).
    host: Option<String>,
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

    /// Returns the server Programmatic Identifier (ProgID).
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{Clsid, OpcServerInfo};
    ///
    /// let info = OpcServerInfo::new("Matrikon.OPC.Simulation.1", Clsid::zeroed(), None, None);
    /// assert_eq!(info.prog_id(), "Matrikon.OPC.Simulation.1");
    /// ```
    #[must_use]
    pub fn prog_id(&self) -> &str {
        &self.prog_id
    }

    /// Consumes the server metadata, returning its owned Programmatic Identifier (ProgID).
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{Clsid, OpcServerInfo};
    ///
    /// let info = OpcServerInfo::new("Matrikon.OPC.Simulation.1", Clsid::zeroed(), None, None);
    /// assert_eq!(info.into_prog_id(), "Matrikon.OPC.Simulation.1");
    /// ```
    #[must_use]
    pub fn into_prog_id(self) -> String {
        self.prog_id
    }

    /// Returns the 128-bit COM Class ID (`Clsid`) of the server.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{Clsid, OpcServerInfo};
    ///
    /// let info = OpcServerInfo::new("Matrikon.OPC.Simulation.1", Clsid::zeroed(), None, None);
    /// assert_eq!(info.clsid(), Clsid::zeroed());
    /// ```
    #[must_use]
    pub const fn clsid(&self) -> Clsid {
        self.clsid
    }

    /// Returns the human-readable server title from catalog metadata, or `None` if absent.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{Clsid, OpcServerInfo};
    ///
    /// let info = OpcServerInfo::new(
    ///     "Matrikon.OPC.Simulation.1",
    ///     Clsid::zeroed(),
    ///     Some("Matrikon".into()),
    ///     None,
    /// );
    /// assert_eq!(info.user_type(), Some("Matrikon"));
    /// ```
    #[must_use]
    pub fn user_type(&self) -> Option<&str> {
        self.user_type.as_deref()
    }

    /// Returns the target host machine name or IP address, or `None` if targeting localhost.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{Clsid, OpcServerInfo};
    ///
    /// let info = OpcServerInfo::new(
    ///     "Matrikon.OPC.Simulation.1",
    ///     Clsid::zeroed(),
    ///     None,
    ///     Some("192.168.1.50".into()),
    /// );
    /// assert_eq!(info.host(), Some("192.168.1.50"));
    /// ```
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
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

/// Synthesizes a vector of [`OpcServerInfo`] records from a collection of ProgIDs and an optional host string,
/// automatically normalizing localhost/loopback representations to `None`.
pub fn server_info_from_prog_ids(
    servers: impl IntoIterator<Item = impl Into<String>>,
    host: Option<&str>,
) -> Vec<OpcServerInfo> {
    let host_opt = normalize_host(host);
    servers
        .into_iter()
        .map(|prog_id| {
            OpcServerInfo::new(
                prog_id.into(),
                crate::types::Clsid::zeroed(),
                None,
                host_opt.clone(),
            )
        })
        .collect()
}

/// Connection endpoint defining a target host machine and OPC server identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpcServerEndpoint {
    /// Target machine hostname or IP address (`None` or `"localhost"` for local connection).
    pub(crate) host: Option<String>,
    /// Strongly-typed server identifier (ProgID or CLSID).
    pub(crate) identifier: ServerIdentifier,
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
    /// let ep = OpcServerEndpoint::local_prog_id("Matrikon.OPC.Simulation.1");
    /// assert!(!ep.is_remote());
    /// ```
    /// Parses an endpoint from a string representation.
    ///
    /// Symmetrical with [`ServerIdentifier::new`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseEndpointError`] if parsing or validation fails.
    pub fn new(s: &str) -> Result<Self, ParseEndpointError> {
        s.parse()
    }

    /// Creates a local endpoint directly from a raw ProgID string without validation.
    ///
    /// Convenient for tests and trusted internal call sites.
    #[must_use]
    pub fn local_prog_id(prog_id: impl Into<String>) -> Self {
        Self {
            host: None,
            identifier: ServerIdentifier::ProgId(prog_id.into()),
        }
    }

    /// Creates a remote endpoint directly from a host and raw ProgID string.
    ///
    /// Automatically normalizes `host`.
    #[must_use]
    pub fn remote_prog_id(host: impl Into<String>, prog_id: impl Into<String>) -> Self {
        let host_str = host.into();
        Self {
            host: normalize_host(Some(&host_str)),
            identifier: ServerIdentifier::ProgId(prog_id.into()),
        }
    }

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
    /// let ep = OpcServerEndpoint::remote_prog_id("192.168.1.10", "Matrikon.OPC.Simulation.1");
    /// assert!(ep.is_remote());
    /// assert_eq!(ep.host(), Some("192.168.1.10"));
    /// ```
    #[must_use]
    pub fn remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self {
        let host_str = host.into();
        Self {
            host: normalize_host(Some(&host_str)),
            identifier: identifier.into(),
        }
    }

    /// Returns a borrowed reference to the target host if configured, or `None` if targeting the local machine.
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    /// Returns a borrowed reference to the [`ServerIdentifier`].
    #[must_use]
    pub const fn identifier(&self) -> &ServerIdentifier {
        &self.identifier
    }

    /// Returns `true` if this endpoint targets a remote machine.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::OpcServerEndpoint;
    ///
    /// assert!(!OpcServerEndpoint::local_prog_id("Server").is_remote());
    /// assert!(OpcServerEndpoint::remote_prog_id("10.0.0.1", "Server").is_remote());
    /// ```
    #[must_use]
    pub fn is_remote(&self) -> bool {
        is_remote_host(self.host.as_deref())
    }

    /// Compares two endpoints for semantic equivalence under OPC DA and network naming rules.
    ///
    /// Hostnames are compared case-insensitively using ASCII folding. Server identifiers
    /// delegate to [`ServerIdentifier::matches`].
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::OpcServerEndpoint;
    ///
    /// let ep1 = OpcServerEndpoint::remote_prog_id("HOST-01", "Matrikon.OPC.Simulation.1");
    /// let ep2 = OpcServerEndpoint::remote_prog_id("host-01", "matrikon.opc.simulation.1");
    /// assert!(ep1.matches(&ep2));
    /// ```
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        let host_matches = match (self.host.as_deref(), other.host.as_deref()) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (None, None) => true,
            _ => false,
        };
        host_matches && self.identifier.matches(&other.identifier)
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
/// Supports UNC syntax (`r"\\<host>\<server>"` or `"//<host>/<server>"`), URI schemes
/// (`opc://<host>/<server>`, `opc.da://<host>/<server>`), raw slash syntax (`"<host>/<server>"`),
/// as well as standalone server identifiers (local connection).
///
/// # Examples
///
/// ```
/// use opc_da_client::types::OpcServerEndpoint;
///
/// let ep: OpcServerEndpoint = r"\\192.168.1.50\Matrikon.OPC.Simulation.1".parse().unwrap();
/// assert!(ep.is_remote());
/// assert_eq!(ep.host(), Some("192.168.1.50"));
///
/// let local_ep: OpcServerEndpoint = "Matrikon.OPC.Simulation.1".parse().unwrap();
/// assert!(!local_ep.is_remote());
/// ```
impl FromStr for OpcServerEndpoint {
    type Err = ParseEndpointError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseEndpointError::Empty);
        }
        if trimmed.contains('\0') {
            return Err(ParseEndpointError::InvalidFormat(
                "Endpoint string cannot contain interior null bytes".to_string(),
            ));
        }

        // Check for URI scheme: <scheme>://<host>/<server>
        if let Some(pos) = trimmed.find("://") {
            let after_scheme = &trimmed[pos + 3..];
            if let Some((raw_host, raw_server)) = after_scheme.split_once('/') {
                let server = raw_server.trim();
                if server.is_empty() {
                    return Err(ParseEndpointError::MissingServer(trimmed.to_string()));
                }
                let host = normalize_host(Some(raw_host));
                let identifier = ServerIdentifier::from_str(server)?;
                return Ok(Self { host, identifier });
            }
            return Err(ParseEndpointError::InvalidFormat(format!(
                "Expected host and server separated by delimiter in '{trimmed}'"
            )));
        }

        // Check for UNC prefix: \\host\server or //host/server
        if let Some(rest) = trimmed
            .strip_prefix(r"\\")
            .or_else(|| trimmed.strip_prefix("//"))
        {
            if let Some(sep_idx) = rest.find(['\\', '/']) {
                let raw_host = &rest[..sep_idx];
                let raw_server = &rest[sep_idx + 1..];
                let server = raw_server.trim();
                if server.is_empty() {
                    return Err(ParseEndpointError::MissingServer(trimmed.to_string()));
                }
                let host = normalize_host(Some(raw_host));
                let identifier = ServerIdentifier::from_str(server)?;
                return Ok(Self { host, identifier });
            }
            return Err(ParseEndpointError::InvalidFormat(format!(
                "Expected host and server separated by delimiter in '{trimmed}'"
            )));
        }

        // Check for raw slash separating host and server: host/server or host\server
        if let Some(sep_idx) = trimmed.find(['\\', '/']) {
            let raw_host = &trimmed[..sep_idx];
            let raw_server = &trimmed[sep_idx + 1..];
            let server = raw_server.trim();
            if server.is_empty() {
                return Err(ParseEndpointError::MissingServer(trimmed.to_string()));
            }
            let host = normalize_host(Some(raw_host));
            let identifier = ServerIdentifier::from_str(server)?;
            return Ok(Self { host, identifier });
        }

        // Standalone local server identifier
        let identifier = ServerIdentifier::from_str(trimmed)?;
        Ok(Self {
            host: None,
            identifier,
        })
    }
}

impl TryFrom<&str> for OpcServerEndpoint {
    type Error = ParseEndpointError;

    #[inline]
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<String> for OpcServerEndpoint {
    type Error = ParseEndpointError;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hyphenated_prog_ids_parse_as_prog_id() {
        let ids = [
            "KEPServerEX-V6.1",
            "Schneider-OpcServer.1",
            "ABB.IndustrialIT-Server.1",
            "My-Custom-Server.DA.2",
        ];
        for id in ids {
            let parsed: ServerIdentifier = id
                .parse()
                .expect("hyphenated ProgID should parse successfully");
            assert!(
                matches!(parsed, ServerIdentifier::ProgId(ref s) if s == id),
                "expected ProgId({id}), got {parsed:?}"
            );
        }

        // Verify CLSIDs still parse correctly
        let bracketed = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
        let parsed_bracketed: ServerIdentifier = bracketed
            .parse()
            .expect("valid bracketed CLSID should parse");
        assert!(matches!(parsed_bracketed, ServerIdentifier::Clsid(_)));

        let unbracketed = "28E68F9A-8D75-11D1-8DC3-3C302A000000";
        let parsed_unbracketed: ServerIdentifier = unbracketed
            .parse()
            .expect("valid unbracketed CLSID should parse");
        assert!(matches!(parsed_unbracketed, ServerIdentifier::Clsid(_)));

        // Verify malformed bracketed GUID returns InvalidClsid error
        let bad_guid = "{not-a-valid-guid-at-all}";
        let err = bad_guid.parse::<ServerIdentifier>().unwrap_err();
        assert!(matches!(err, ParseServerIdError::InvalidClsid(_)));
    }

    #[test]
    fn test_server_identifier_null_byte_rejection() {
        let bad_prog_ids = [
            "Server\0Name",
            "\0LeadingNull",
            "TrailingNull\0",
            "Embedded\0Null.ProgId.1",
        ];
        for id in bad_prog_ids {
            let res = id.parse::<ServerIdentifier>();
            assert!(
                matches!(res, Err(ParseServerIdError::InvalidProgId(_))),
                "expected InvalidProgId for {id:?}, got {res:?}"
            );
        }
    }

    #[test]
    fn test_opc_server_endpoint_parse_null_byte_rejection() {
        let bad_endpoints = [
            "\\\\host\0name\\Server",
            "//host\0name/Server",
            "opc.da://host\0name/Server",
            "host\0name/Server",
            "host\0name\\Server",
        ];
        for ep_str in bad_endpoints {
            let res = ep_str.parse::<OpcServerEndpoint>();
            assert!(
                matches!(res, Err(ParseEndpointError::InvalidFormat(_))),
                "expected InvalidFormat for {ep_str:?}, got {res:?}"
            );
        }
    }

    #[test]
    fn test_server_identifier_try_from_str_and_string() {
        // Valid ProgID
        let id_str: Result<ServerIdentifier, _> = "Matrikon.OPC.Simulation.1".try_into();
        assert!(matches!(id_str, Ok(ServerIdentifier::ProgId(_))));

        let id_string: Result<ServerIdentifier, _> =
            "Matrikon.OPC.Simulation.1".to_string().try_into();
        assert!(matches!(id_string, Ok(ServerIdentifier::ProgId(_))));

        // Valid CLSID
        let clsid_str: Result<ServerIdentifier, _> =
            "{28E68F9A-8D75-11D1-8DC3-3C302A000000}".try_into();
        assert!(matches!(clsid_str, Ok(ServerIdentifier::Clsid(_))));

        // Empty string
        let empty_res: Result<ServerIdentifier, _> = "".try_into();
        assert!(matches!(empty_res, Err(ParseServerIdError::Empty)));

        // Invalid ProgID with null byte
        let null_res: Result<ServerIdentifier, _> = "Bad\0ProgId".try_into();
        assert!(matches!(
            null_res,
            Err(ParseServerIdError::InvalidProgId(_))
        ));
    }

    #[test]
    fn test_opc_server_endpoint_try_from_str_and_string() {
        // Local endpoint
        let ep_local: Result<OpcServerEndpoint, _> = "Matrikon.OPC.Simulation.1".try_into();
        assert!(matches!(ep_local, Ok(ep) if !ep.is_remote()));

        // Remote endpoint
        let ep_remote: Result<OpcServerEndpoint, _> =
            "\\\\192.168.1.10\\Matrikon.OPC.Simulation.1".try_into();
        assert!(matches!(ep_remote, Ok(ep) if ep.is_remote() && ep.host() == Some("192.168.1.10")));

        // Empty endpoint
        let ep_empty: Result<OpcServerEndpoint, _> = "".try_into();
        assert!(matches!(ep_empty, Err(ParseEndpointError::Empty)));
    }

    #[test]
    fn test_server_info_from_prog_ids_empty() {
        let infos = server_info_from_prog_ids(Vec::<String>::new(), None);
        assert!(infos.is_empty());
    }

    #[test]
    fn test_server_info_from_prog_ids_localhost_normalized() {
        let servers = vec!["Matrikon.OPC.Simulation".to_string()];
        let infos = server_info_from_prog_ids(servers, Some("localhost"));
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].prog_id(), "Matrikon.OPC.Simulation");
        assert!(infos[0].host().is_none());
    }

    #[test]
    fn test_server_info_from_prog_ids_remote_host() {
        let servers = vec!["Server1".to_string(), "Server2".to_string()];
        let infos = server_info_from_prog_ids(servers, Some("192.168.1.50"));
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].host(), Some("192.168.1.50"));
        assert_eq!(infos[1].host(), Some("192.168.1.50"));
    }

    #[test]
    fn test_server_identifier_conversions_and_display() {
        let prog_id = ServerIdentifier::try_from("Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(
            prog_id,
            ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
        );
        assert_eq!(prog_id.to_string(), "Matrikon.OPC.Simulation.1");
        assert!(prog_id.is_prog_id());
        assert!(!prog_id.is_clsid());

        let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
        let parsed = ServerIdentifier::try_from(clsid_str).unwrap();
        assert!(parsed.is_clsid());
        assert_eq!(parsed.to_string().to_uppercase(), clsid_str.to_uppercase());

        let direct_guid = windows_core::GUID::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
        let from_guid = ServerIdentifier::from(direct_guid);
        assert!(from_guid.is_clsid());

        let direct_clsid = Clsid::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
        let from_clsid = ServerIdentifier::from(direct_clsid);
        assert!(from_clsid.is_clsid());
        assert_eq!(from_clsid.as_clsid(), Some(&direct_clsid));
    }

    #[test]
    fn test_server_identifier_validation_and_conversions() {
        assert!(ServerIdentifier::new("Matrikon.OPC.Simulation.1").is_ok());
        assert_eq!(ServerIdentifier::new(""), Err(ParseServerIdError::Empty));
        assert!(matches!(
            ServerIdentifier::new("Invalid\tProgId"),
            Err(ParseServerIdError::InvalidProgId(_))
        ));
        assert!(ServerIdentifier::new("{28E68F9A-8D75-11D1-8DC3-3C302A000000}").is_ok());
    }

    #[test]
    fn test_opc_server_info_display_name_and_endpoint() {
        let info_with_user_type = OpcServerInfo::new(
            "Matrikon.OPC.Simulation.1",
            Clsid::zeroed(),
            Some("Matrikon Simulation Server".into()),
            None,
        );
        assert_eq!(
            info_with_user_type.display_name(),
            "Matrikon Simulation Server"
        );
        assert_eq!(
            info_with_user_type.endpoint().identifier(),
            &ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
        );

        let info_without_user_type = OpcServerInfo::new(
            "Kepware.KEPServerEX.V6",
            Clsid::zeroed(),
            None,
            Some("192.168.1.10".into()),
        );
        assert_eq!(
            info_without_user_type.display_name(),
            "Kepware.KEPServerEX.V6"
        );
        assert_eq!(
            info_without_user_type.endpoint().host(),
            Some("192.168.1.10")
        );
    }

    #[test]
    fn test_format_guid_bracketed() {
        let guid = windows_core::GUID::zeroed();
        assert_eq!(
            format_guid_bracketed(&guid),
            "{00000000-0000-0000-0000-000000000000}"
        );
        assert_eq!(
            format_guid_bracketed(&guid),
            ServerIdentifier::Clsid(Clsid::from_windows_guid(guid)).to_string()
        );

        let custom_guid = windows_core::GUID::from_u128(0x01234567_89AB_CDEF_0123_456789ABCDEF);
        assert_eq!(
            format_guid_bracketed(&custom_guid),
            ServerIdentifier::Clsid(Clsid::from_windows_guid(custom_guid)).to_string()
        );
    }

    #[test]
    fn test_host_normalization_and_remote_detection() {
        assert_eq!(normalize_host_str(None), None);
        assert_eq!(normalize_host_str(Some("")), None);
        assert_eq!(normalize_host_str(Some("   \t\r\n")), None);
        assert_eq!(normalize_host_str(Some("localhost")), None);
        assert_eq!(normalize_host_str(Some("LocalHost")), None);
        assert_eq!(normalize_host_str(Some("LOCALHOST")), None);
        assert_eq!(normalize_host_str(Some("127.0.0.1")), None);
        assert_eq!(normalize_host_str(Some("::1")), None);
        assert_eq!(
            normalize_host_str(Some("192.168.1.50")),
            Some("192.168.1.50")
        );
        assert_eq!(
            normalize_host_str(Some("  remote-plc  ")),
            Some("remote-plc")
        );
        assert_eq!(
            normalize_host_str(Some("scada-node-01")),
            Some("scada-node-01")
        );

        assert_eq!(normalize_host(None), None);
        assert_eq!(normalize_host(Some("")), None);
        assert_eq!(normalize_host(Some("   ")), None);
        assert_eq!(normalize_host(Some("localhost")), None);
        assert_eq!(normalize_host(Some("LOCALHOST")), None);
        assert_eq!(normalize_host(Some("127.0.0.1")), None);
        assert_eq!(normalize_host(Some("::1")), None);

        assert_eq!(
            normalize_host(Some("192.168.1.50")),
            Some("192.168.1.50".to_string())
        );
        assert_eq!(
            normalize_host(Some("  plc-host  ")),
            Some("plc-host".to_string())
        );

        assert!(!is_remote_host(None));
        assert!(!is_remote_host(Some("")));
        assert!(!is_remote_host(Some("localhost")));
        assert!(!is_remote_host(Some("127.0.0.1")));
        assert!(!is_remote_host(Some("::1")));
        assert!(is_remote_host(Some("remote-server")));

        let local_ep = OpcServerEndpoint::local_prog_id("Test.Server");
        assert!(!local_ep.is_remote());
        assert_eq!(local_ep.host, None);

        let remote_local = OpcServerEndpoint::remote_prog_id("localhost", "Test.Server");
        assert!(!remote_local.is_remote());
        assert_eq!(remote_local.host, None);

        let remote_ep = OpcServerEndpoint::remote_prog_id("10.0.0.1", "Test.Server");
        assert!(remote_ep.is_remote());
        assert_eq!(remote_ep.host, Some("10.0.0.1".to_string()));

        let info = OpcServerInfo::new(
            "Test.Server",
            Clsid::zeroed(),
            Some("Test Title".to_string()),
            Some("localhost".to_string()),
        );
        assert_eq!(info.host(), None);
        assert_eq!(info.display_name(), "Test Title");
        let ep = info.endpoint();
        assert!(!ep.is_remote());
    }

    #[test]
    fn test_endpoint_unc_parsing_roundtrip() {
        use std::str::FromStr;

        // Windows UNC backslash path
        let ep1 = OpcServerEndpoint::from_str(r"\\192.168.1.50\Matrikon.OPC.Simulation").unwrap();
        assert_eq!(ep1.host(), Some("192.168.1.50"));
        assert_eq!(ep1.identifier().to_string(), "Matrikon.OPC.Simulation");
        assert!(ep1.is_remote());
        assert_eq!(ep1.to_string(), r"\\192.168.1.50\Matrikon.OPC.Simulation");

        // Unix-style forward slash path
        let ep2 = OpcServerEndpoint::from_str("//192.168.1.50/Matrikon.OPC.Simulation").unwrap();
        assert_eq!(ep2.host(), Some("192.168.1.50"));
        assert_eq!(ep2.identifier().to_string(), "Matrikon.OPC.Simulation");
        assert!(ep2.is_remote());

        // Localhost UNC normalized to local
        let ep_local = OpcServerEndpoint::from_str(r"\\localhost\Matrikon.OPC.Simulation").unwrap();
        assert_eq!(ep_local.host(), None);
        assert!(!ep_local.is_remote());
        assert_eq!(ep_local.to_string(), "Matrikon.OPC.Simulation");

        // Plain server name without host
        let ep_plain = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation").unwrap();
        assert_eq!(ep_plain.host(), None);
        assert!(!ep_plain.is_remote());
        assert_eq!(ep_plain.to_string(), "Matrikon.OPC.Simulation");

        // Roundtrip test via Display and FromStr
        let ep_remote = OpcServerEndpoint::remote_prog_id("10.0.0.5", "Kepware.KEPServerEX.V6");
        let display_str = ep_remote.to_string();
        let reparsed: OpcServerEndpoint = display_str.parse().unwrap();
        assert_eq!(ep_remote, reparsed);

        // FromStr delegates to parsing
        let ep_from_str: OpcServerEndpoint =
            r"\\192.168.1.50\Matrikon.OPC.Simulation".parse().unwrap();
        assert_eq!(ep_from_str, ep1);

        // Rejection cases
        assert!(OpcServerEndpoint::from_str("").is_err());
        assert!(OpcServerEndpoint::from_str("   ").is_err());
        assert!(OpcServerEndpoint::from_str(r"\\").is_err());
        assert!(OpcServerEndpoint::from_str(r"\\host\").is_err());
    }

    #[test]
    fn test_parse_errors_display_and_traits() {
        use std::str::FromStr;

        use crate::types::clsid::Clsid;

        // 1. ParseServerIdError variants & display
        let err_empty_id = ParseServerIdError::Empty;
        assert_eq!(
            format!("{err_empty_id}"),
            "Server identifier cannot be empty"
        );

        let err_too_long = ParseServerIdError::ProgIdTooLong(256);
        assert_eq!(
            format!("{err_too_long}"),
            "ProgID length 256 exceeds maximum allowed 255 characters"
        );

        let err_invalid_prog = ParseServerIdError::InvalidProgId("Invalid..ProgID".to_string());
        assert_eq!(
            format!("{err_invalid_prog}"),
            "Invalid characters or syntax in ProgID: 'Invalid..ProgID'"
        );

        let clsid_err = Clsid::from_str("not-a-guid").unwrap_err();
        let err_clsid = ParseServerIdError::InvalidClsid(clsid_err);
        assert!(format!("{err_clsid}").contains("Invalid CLSID GUID string: 'not-a-guid'"));

        // 2. ParseEndpointError variants & display
        let err_ep_empty = ParseEndpointError::Empty;
        assert_eq!(format!("{err_ep_empty}"), "Server endpoint cannot be empty");

        let err_ep_format = ParseEndpointError::InvalidFormat("bad-endpoint".to_string());
        assert_eq!(
            format!("{err_ep_format}"),
            "Invalid endpoint syntax: 'bad-endpoint'"
        );

        let err_ep_missing = ParseEndpointError::MissingServer(r"\\host\".to_string());
        assert_eq!(
            format!("{err_ep_missing}"),
            r"Missing server identifier in endpoint path: '\\host\'"
        );

        let err_ep_nested = ParseEndpointError::from(err_empty_id.clone());
        assert_eq!(
            format!("{err_ep_nested}"),
            "Invalid server identifier in endpoint: Server identifier cannot be empty"
        );

        // 3. std::error::Error source checking
        let std_err: &dyn std::error::Error = &err_ep_nested;
        assert!(std_err.source().is_some());
        let source = std_err.source().unwrap();
        assert_eq!(format!("{source}"), format!("{err_empty_id}"));
    }

    #[test]
    fn test_prog_id_validation_boundaries() {
        // Valid boundaries: length 1 and 255
        assert!(validate_prog_id("A").is_ok());
        assert!(validate_prog_id("A.B").is_ok());

        let prog_id_255 = "A".repeat(255);
        assert!(validate_prog_id(&prog_id_255).is_ok());

        let prog_id_dot_255 = format!("{}.{}", "A".repeat(127), "B".repeat(127));
        assert_eq!(prog_id_dot_255.len(), 255);
        assert!(validate_prog_id(&prog_id_dot_255).is_ok());

        // Boundary: length 0 (empty) and length 256
        assert_eq!(validate_prog_id(""), Err(ParseServerIdError::Empty));
        assert_eq!(validate_prog_id("   "), Err(ParseServerIdError::Empty));

        let prog_id_256 = "A".repeat(256);
        assert_eq!(
            validate_prog_id(&prog_id_256),
            Err(ParseServerIdError::ProgIdTooLong(256))
        );

        // Syntax rejection: leading or trailing dots
        assert!(matches!(
            validate_prog_id(".Server.Prog"),
            Err(ParseServerIdError::InvalidProgId(_))
        ));
        assert!(matches!(
            validate_prog_id("Server.Prog."),
            Err(ParseServerIdError::InvalidProgId(_))
        ));

        // Syntax rejection: consecutive dots
        assert!(matches!(
            validate_prog_id("Server..Prog"),
            Err(ParseServerIdError::InvalidProgId(_))
        ));

        // Syntax rejection: special characters & spaces
        let invalid_chars = [
            "Server Name",
            "Server@1",
            "Server#Tag",
            "Server$Val",
            "Server/1",
            "Server\\1",
            "Server:1",
        ];
        for invalid in invalid_chars {
            assert!(
                matches!(
                    validate_prog_id(invalid),
                    Err(ParseServerIdError::InvalidProgId(_))
                ),
                "Expected '{invalid}' to fail ProgID validation"
            );
        }
    }

    #[test]
    fn test_server_identifier_from_str_valid_and_invalid() {
        use std::str::FromStr;

        // 1. Valid ProgID
        let id_prog = ServerIdentifier::from_str("Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(id_prog.as_prog_id(), Some("Matrikon.OPC.Simulation.1"));
        assert!(id_prog.is_prog_id());
        assert!(!id_prog.is_clsid());

        // 2. Valid Bracketed CLSID
        let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
        let id_clsid = ServerIdentifier::from_str(clsid_str).unwrap();
        assert!(id_clsid.is_clsid());
        assert_eq!(
            id_clsid.to_string().to_uppercase(),
            clsid_str.to_uppercase()
        );

        // 3. TryFrom<&str>
        let id_from = ServerIdentifier::try_from("Kepware.KEPServerEX.V6").unwrap();
        assert_eq!(id_from.as_prog_id(), Some("Kepware.KEPServerEX.V6"));

        // 4. Invalid cases
        assert_eq!(
            ServerIdentifier::from_str(""),
            Err(ParseServerIdError::Empty)
        );
        assert_eq!(
            ServerIdentifier::from_str("   "),
            Err(ParseServerIdError::Empty)
        );

        assert!(matches!(
            ServerIdentifier::from_str("{not-a-valid-guid}"),
            Err(ParseServerIdError::InvalidClsid(_))
        ));

        assert!(matches!(
            ServerIdentifier::from_str("Invalid Server Identifier"),
            Err(ParseServerIdError::InvalidProgId(_))
        ));
    }

    #[test]
    fn test_endpoint_from_str_comprehensive_schemes() {
        use std::str::FromStr;

        // Windows UNC
        let ep_unc =
            OpcServerEndpoint::from_str(r"\\192.168.1.50\Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_unc.host.as_deref(), Some("192.168.1.50"));
        assert_eq!(ep_unc.identifier.to_string(), "Matrikon.OPC.Simulation.1");
        assert!(ep_unc.is_remote());
        assert_eq!(
            ep_unc.to_string(),
            r"\\192.168.1.50\Matrikon.OPC.Simulation.1"
        );

        // Unix forward slash
        let ep_unix =
            OpcServerEndpoint::from_str("//192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_unix.host.as_deref(), Some("192.168.1.50"));
        assert_eq!(ep_unix.identifier.to_string(), "Matrikon.OPC.Simulation.1");
        assert!(ep_unix.is_remote());

        // URI scheme: opc://
        let ep_uri =
            OpcServerEndpoint::from_str("opc://192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_uri.host.as_deref(), Some("192.168.1.50"));
        assert_eq!(ep_uri.identifier.to_string(), "Matrikon.OPC.Simulation.1");
        assert!(ep_uri.is_remote());

        // URI scheme: opc.da://
        let ep_da_uri =
            OpcServerEndpoint::from_str("opc.da://192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_da_uri.host.as_deref(), Some("192.168.1.50"));
        assert_eq!(
            ep_da_uri.identifier.to_string(),
            "Matrikon.OPC.Simulation.1"
        );
        assert!(ep_da_uri.is_remote());

        // Raw slash
        let ep_raw_slash =
            OpcServerEndpoint::from_str("192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_raw_slash.host.as_deref(), Some("192.168.1.50"));
        assert_eq!(
            ep_raw_slash.identifier.to_string(),
            "Matrikon.OPC.Simulation.1"
        );
        assert!(ep_raw_slash.is_remote());

        // Standalone local ProgID
        let ep_local_prog = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_local_prog.host, None);
        assert!(!ep_local_prog.is_remote());
        assert_eq!(
            ep_local_prog.identifier.to_string(),
            "Matrikon.OPC.Simulation.1"
        );

        // Standalone local CLSID
        let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
        let ep_local_clsid = OpcServerEndpoint::from_str(clsid_str).unwrap();
        assert_eq!(ep_local_clsid.host, None);
        assert!(!ep_local_clsid.is_remote());
        assert!(ep_local_clsid.identifier.is_clsid());
    }

    #[test]
    fn test_endpoint_from_str_localhost_normalization() {
        use std::str::FromStr;

        let local_inputs = [
            r"\\localhost\Matrikon.OPC.Simulation.1",
            r"\\LOCALHOST\Matrikon.OPC.Simulation.1",
            r"\\127.0.0.1\Matrikon.OPC.Simulation.1",
            r"\\::1\Matrikon.OPC.Simulation.1",
            "//localhost/Matrikon.OPC.Simulation.1",
            "//127.0.0.1/Matrikon.OPC.Simulation.1",
            "opc://localhost/Matrikon.OPC.Simulation.1",
            "opc.da://localhost/Matrikon.OPC.Simulation.1",
            "opc://127.0.0.1/Matrikon.OPC.Simulation.1",
            "localhost/Matrikon.OPC.Simulation.1",
        ];

        for input in local_inputs {
            let ep = OpcServerEndpoint::from_str(input).unwrap();
            assert_eq!(
                ep.host, None,
                "Host in '{input}' should be normalized to None"
            );
            assert!(!ep.is_remote(), "Endpoint '{input}' should not be remote");
            assert_eq!(ep.identifier.to_string(), "Matrikon.OPC.Simulation.1");
        }
    }

    #[test]
    fn test_endpoint_from_str_negative_syntax_cases() {
        use std::str::FromStr;

        assert_eq!(
            OpcServerEndpoint::from_str(""),
            Err(ParseEndpointError::Empty)
        );
        assert_eq!(
            OpcServerEndpoint::from_str("   "),
            Err(ParseEndpointError::Empty)
        );

        assert!(matches!(
            OpcServerEndpoint::from_str(r"\\host\"),
            Err(ParseEndpointError::MissingServer(_))
        ));
        assert_eq!(
            OpcServerEndpoint::from_str("//host"),
            Err(ParseEndpointError::InvalidFormat(
                "Expected host and server separated by delimiter in '//host'".into()
            ))
        );
        assert!(matches!(
            OpcServerEndpoint::from_str("opc://host/"),
            Err(ParseEndpointError::MissingServer(_))
        ));
        assert!(matches!(
            OpcServerEndpoint::from_str("opc.da://host/"),
            Err(ParseEndpointError::MissingServer(_))
        ));

        assert!(matches!(
            OpcServerEndpoint::from_str(r"\\192.168.1.50\Invalid Prog@ID"),
            Err(ParseEndpointError::InvalidServerId(
                ParseServerIdError::InvalidProgId(_)
            ))
        ));
    }

    #[test]
    fn test_endpoint_from_str_behavior() {
        use std::str::FromStr;

        let ep: OpcServerEndpoint = r"\\192.168.1.50\Matrikon.OPC.Simulation.1"
            .try_into()
            .unwrap();
        assert_eq!(ep.host.as_deref(), Some("192.168.1.50"));
        assert_eq!(ep.identifier.to_string(), "Matrikon.OPC.Simulation.1");

        let res = OpcServerEndpoint::from_str(r"\\192.168.1.50\Matrikon.OPC.Simulation.1");
        assert!(res.is_ok());

        let err_res = OpcServerEndpoint::from_str("");
        assert!(err_res.is_err());
    }

    #[test]
    fn test_opc_server_info_getters_and_encapsulation() {
        let clsid = Clsid::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
        let info = OpcServerInfo::new(
            "Matrikon.OPC.Simulation.1",
            clsid,
            Some("Matrikon Simulation Server".to_string()),
            Some("192.168.1.50".to_string()),
        );

        assert_eq!(info.prog_id(), "Matrikon.OPC.Simulation.1");
        assert_eq!(info.clsid(), clsid);
        assert_eq!(info.user_type(), Some("Matrikon Simulation Server"));
        assert_eq!(info.host(), Some("192.168.1.50"));
        assert_eq!(info.display_name(), "Matrikon Simulation Server");

        let ep = info.endpoint();
        assert!(ep.is_remote());
        assert_eq!(ep.host(), Some("192.168.1.50"));
        assert_eq!(
            ep.identifier(),
            &ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
        );

        // Consuming accessor verification
        assert_eq!(info.into_prog_id(), "Matrikon.OPC.Simulation.1");

        // Fallback display name and localhost normalization to None
        let local_info = OpcServerInfo::new(
            "Kepware.KEPServerEX.V6",
            Clsid::zeroed(),
            None,
            Some("localhost".to_string()),
        );

        assert_eq!(local_info.prog_id(), "Kepware.KEPServerEX.V6");
        assert_eq!(local_info.clsid(), Clsid::zeroed());
        assert_eq!(local_info.user_type(), None);
        assert_eq!(local_info.host(), None);
        assert_eq!(local_info.display_name(), "Kepware.KEPServerEX.V6");

        let local_ep = local_info.endpoint();
        assert!(!local_ep.is_remote());
        assert_eq!(local_ep.host(), None);
        assert_eq!(
            local_ep.identifier(),
            &ServerIdentifier::ProgId("Kepware.KEPServerEX.V6".into())
        );

        // Whitespace host normalization to None
        let trimmed_info = OpcServerInfo::new(
            "Yokogawa.Exaopc.1",
            Clsid::zeroed(),
            Some("Yokogawa Server".to_string()),
            Some("   ".to_string()),
        );
        assert_eq!(trimmed_info.host(), None);
    }

    #[test]
    fn test_normalize_host_lowercases_remote() {
        assert_eq!(
            normalize_host(Some("SCADA-01")),
            Some("scada-01".to_string())
        );
        assert_eq!(
            normalize_host(Some("Remote-PLC-01")),
            Some("remote-plc-01".to_string())
        );
        assert_eq!(
            normalize_host(Some("  SCADA-NODE-01  ")),
            Some("scada-node-01".to_string())
        );
        assert_eq!(
            normalize_host(Some("192.168.1.50")),
            Some("192.168.1.50".to_string())
        );
        assert_eq!(normalize_host(Some("localhost")), None);
        assert_eq!(normalize_host(Some("LOCALHOST")), None);
        assert_eq!(normalize_host(Some("127.0.0.1")), None);
        assert_eq!(normalize_host(Some("::1")), None);
        assert_eq!(normalize_host(None), None);
        assert_eq!(normalize_host_str(Some("SCADA-01")), Some("SCADA-01"));
    }

    #[test]
    fn test_endpoint_from_str_canonicalizes_host() {
        use std::collections::HashSet;
        use std::str::FromStr;

        let ep_unc =
            OpcServerEndpoint::from_str(r"\\SCADA-NODE1\Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_unc.host(), Some("scada-node1"));

        let ep_slash =
            OpcServerEndpoint::from_str("//SCADA-NODE1/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_slash.host(), Some("scada-node1"));

        let ep_uri =
            OpcServerEndpoint::from_str("opc.da://SCADA-NODE1/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_uri.host(), Some("scada-node1"));

        let ep_raw = OpcServerEndpoint::from_str("SCADA-NODE1/Matrikon.OPC.Simulation.1").unwrap();
        assert_eq!(ep_raw.host(), Some("scada-node1"));

        // O5: Connection pool deduplication guarantee
        let ep_upper = OpcServerEndpoint::from_str(r"\\SCADA-01\Server.1").unwrap();
        let ep_lower = OpcServerEndpoint::from_str(r"\\scada-01\Server.1").unwrap();
        assert_eq!(ep_upper, ep_lower);
        let pool: HashSet<OpcServerEndpoint> = HashSet::from([ep_upper, ep_lower]);
        assert_eq!(pool.len(), 1, "Case-varying endpoints must deduplicate");
    }

    #[test]
    fn test_server_identifier_matches_progid() {
        use std::str::FromStr;
        let id_mixed = ServerIdentifier::from_str("Matrikon.OPC.Simulation.1").unwrap();
        let id_lower = ServerIdentifier::from_str("matrikon.opc.simulation.1").unwrap();
        let id_upper = ServerIdentifier::from_str("MATRIKON.OPC.SIMULATION.1").unwrap();
        let id_different = ServerIdentifier::from_str("Kepware.KEPServerEX.V6").unwrap();

        assert!(id_mixed.matches(&id_lower));
        assert!(id_mixed.matches(&id_upper));
        assert!(id_lower.matches(&id_mixed));
        assert_ne!(id_mixed, id_lower);
        assert!(!id_mixed.matches(&id_different));

        let clsid_id =
            ServerIdentifier::from_str("{28E68F9A-8D75-11D1-8DC3-3C302A000000}").unwrap();
        assert!(!id_mixed.matches(&clsid_id));
    }

    #[test]
    fn test_server_identifier_matches_clsid() {
        use std::str::FromStr;
        let id_bracketed =
            ServerIdentifier::from_str("{28E68F9A-8D75-11D1-8DC3-3C302A000000}").unwrap();
        let id_unbracketed =
            ServerIdentifier::from_str("28e68f9a-8d75-11d1-8dc3-3c302a000000").unwrap();
        assert!(id_bracketed.matches(&id_unbracketed));

        let id_zero = ServerIdentifier::Clsid(Clsid::zeroed());
        assert!(!id_bracketed.matches(&id_zero));

        let id_prog = ServerIdentifier::from_str("Matrikon.OPC.Simulation.1").unwrap();
        assert!(!id_bracketed.matches(&id_prog));
    }

    #[test]
    fn test_endpoint_matches() {
        use std::str::FromStr;
        let ep1 = OpcServerEndpoint::from_str(r"\\SCADA-01\Matrikon.OPC.Simulation.1").unwrap();
        let ep2 = OpcServerEndpoint::from_str(r"\\scada-01\matrikon.opc.simulation.1").unwrap();
        assert!(ep1.matches(&ep2));

        let ep_other =
            OpcServerEndpoint::from_str(r"\\scada-02\matrikon.opc.simulation.1").unwrap();
        assert!(!ep1.matches(&ep_other));

        let local1 = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation.1").unwrap();
        let local2 = OpcServerEndpoint::from_str("matrikon.opc.simulation.1").unwrap();
        assert!(local1.matches(&local2));
        assert!(!local1.matches(&ep1));
    }
}
