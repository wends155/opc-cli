//! Server discovery, catalog adapter, and Windows registry inspection.
//!
//! Provides rich catalog queries through `OpcServerListCatalog` (adapting
//! `IOPCServerList2` and `IOPCServerList`), and diagnostic inspection of local
//! machine COM registrations via [`inspect_local_registration`].

use crate::errors::{OpcError, OpcOperation, OpcResult};
use crate::log_opc_err;
use crate::raw::memory::RemotePointer;
use crate::types::OpcServerInfo;
use windows::core::Interface;

/// Execution model of an installed COM OPC DA server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpcServerType {
    /// Out-of-process executable server (`LocalServer32`).
    LocalServer32,
    /// In-process DLL server (`InprocServer32`).
    InprocServer32,
}

impl std::fmt::Display for OpcServerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalServer32 => write!(f, "LocalServer32 (Executable)"),
            Self::InprocServer32 => write!(f, "InprocServer32 (DLL)"),
        }
    }
}

/// Detailed Windows registry configuration for an OPC DA server class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcServerRegistration {
    /// 128-bit COM Class ID.
    pub clsid: windows::core::GUID,
    /// Programmatic Identifier.
    pub prog_id: String,
    /// Version-independent ProgID, or `None` if unassigned.
    pub version_independent_prog_id: Option<String>,
    /// Resolved executable or DLL file path on disk.
    pub binary_path: std::path::PathBuf,
    /// Execution model classification.
    pub server_type: OpcServerType,
}

/// Strips enclosing quotes and trailing CLI arguments from a raw command string.
pub(crate) fn sanitize_binary_path(raw: &str) -> std::path::PathBuf {
    let trimmed = raw.trim();
    let path_str = if let Some(stripped) = trimmed.strip_prefix('"') {
        stripped.split('"').next().unwrap_or(stripped).trim()
    } else {
        let mut end_idx = trimmed.len();
        for flag in [" /", " -"] {
            if let Some(pos) = trimmed.find(flag) {
                end_idx = end_idx.min(pos);
            }
        }
        trimmed[..end_idx].trim()
    };
    std::path::PathBuf::from(path_str)
}

struct RegKeyGuard(windows::Win32::System::Registry::HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: Closing valid opened registry key handle.
            unsafe {
                let _ = windows::Win32::System::Registry::RegCloseKey(self.0);
            }
        }
    }
}

/// Opens a Windows registry subkey safely with specified access/view flags.
fn open_reg_key(
    parent: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    view_flag: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> Option<RegKeyGuard> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{KEY_READ, RegOpenKeyExW};
    use windows::core::PCWSTR;

    let wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let mut key = windows::Win32::System::Registry::HKEY::default();

    // SAFETY: Calling Win32 RegOpenKeyExW with null-terminated PCWSTR and valid parent HKEY.
    let status = unsafe {
        RegOpenKeyExW(
            parent,
            PCWSTR(wide.as_ptr()),
            None,
            KEY_READ | view_flag,
            &raw mut key,
        )
    };

    if status == ERROR_SUCCESS {
        Some(RegKeyGuard(key))
    } else {
        None
    }
}

/// Expands Win32 environment variable strings (e.g. `%SystemRoot%\System32`).
fn expand_environment_string(raw: &str) -> String {
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    use windows::core::PCWSTR;

    if !raw.contains('%') {
        return raw.to_string();
    }

    let wide: Vec<u16> = raw.encode_utf16().chain(std::iter::once(0)).collect();

    let mut buf = [0u16; 512];

    // SAFETY: Calling Win32 ExpandEnvironmentStringsW with null-terminated wide string slice.
    let req_size = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };

    if req_size == 0 {
        tracing::warn!(
            raw = %raw,
            error = ?std::io::Error::last_os_error(),
            "failed to expand environment string in registry value; using raw string"
        );
        return raw.to_string();
    }

    if (req_size as usize) <= buf.len() {
        let valid_len = (req_size as usize).saturating_sub(1);
        String::from_utf16_lossy(&buf[..valid_len])
    } else {
        let mut dynamic_buf = vec![0u16; req_size as usize];
        // SAFETY: Calling Win32 ExpandEnvironmentStringsW with dynamically sized buffer slice.
        let dynamic_req =
            unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut dynamic_buf)) };
        if dynamic_req == 0 {
            tracing::warn!(
                raw = %raw,
                error = ?std::io::Error::last_os_error(),
                "failed to expand dynamic environment string in registry value; using raw string"
            );
            raw.to_string()
        } else {
            let valid_len = (dynamic_req as usize).saturating_sub(1);
            String::from_utf16_lossy(&dynamic_buf[..valid_len])
        }
    }
}

fn open_clsid_key(
    clsid_str: &str,
    view_flag: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> Option<RegKeyGuard> {
    open_reg_key(
        windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
        &format!(r"CLSID\{clsid_str}"),
        view_flag,
    )
}

fn read_string_from_key(target: windows::Win32::System::Registry::HKEY) -> Option<String> {
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{REG_EXPAND_SZ, REG_VALUE_TYPE, RegQueryValueExW};
    use windows::core::PCWSTR;

    let mut buf = [0u16; 512];
    let mut len = u32::try_from(std::mem::size_of_val(&buf)).unwrap_or(1024);
    let mut val_type = REG_VALUE_TYPE::default();

    // SAFETY: Calling Win32 RegQueryValueExW with raw pointers to stack buffers.
    let mut status = unsafe {
        RegQueryValueExW(
            target,
            PCWSTR::null(),
            None,
            Some(&raw mut val_type),
            Some(buf.as_mut_ptr().cast::<u8>()),
            Some(&raw mut len),
        )
    };

    let raw_string = if status == ERROR_SUCCESS && len > 1 {
        let valid_u16_count = (len as usize) / std::mem::size_of::<u16>();
        let val = String::from_utf16_lossy(&buf[..valid_u16_count.min(buf.len())]);
        val.trim_matches('\0').trim().to_string()
    } else if status == ERROR_MORE_DATA && len > 1 {
        // Two-phase query: dynamically reallocate buffer sized according to required len
        let u16_needed = (len as usize) / std::mem::size_of::<u16>() + 1;
        let mut dynamic_buf = vec![0u16; u16_needed];
        let mut dynamic_len = len;

        // SAFETY: Calling Win32 RegQueryValueExW with dynamically sized vector buffer.
        status = unsafe {
            RegQueryValueExW(
                target,
                PCWSTR::null(),
                None,
                Some(&raw mut val_type),
                Some(dynamic_buf.as_mut_ptr().cast::<u8>()),
                Some(&raw mut dynamic_len),
            )
        };

        if status == ERROR_SUCCESS && dynamic_len > 1 {
            let valid_u16_count = (dynamic_len as usize) / std::mem::size_of::<u16>();
            let val =
                String::from_utf16_lossy(&dynamic_buf[..valid_u16_count.min(dynamic_buf.len())]);
            val.trim_matches('\0').trim().to_string()
        } else {
            return None;
        }
    } else {
        return None;
    };

    if raw_string.is_empty() {
        return None;
    }

    if val_type == REG_EXPAND_SZ {
        let expanded = expand_environment_string(&raw_string);
        let trimmed = expanded.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        Some(raw_string)
    }
}

fn read_default_string(
    parent: windows::Win32::System::Registry::HKEY,
    subkey_name: Option<&str>,
    view_flag: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> Option<String> {
    if let Some(sub) = subkey_name {
        let guard = open_reg_key(parent, sub, view_flag)?;
        read_string_from_key(guard.0)
    } else {
        read_string_from_key(parent)
    }
}

/// Resolve a COM CLSID GUID to its Programmatic Identifier (ProgID).
///
/// Calls Win32 `ProgIDFromCLSID` and converts the returned string pointer
/// into a Rust [`String`].
///
/// # Errors
///
/// Returns an [`OpcError`] if `ProgIDFromCLSID` fails.
#[tracing::instrument(level = "debug", err)]
pub(crate) fn guid_to_progid(guid: &windows::core::GUID) -> OpcResult<String> {
    // SAFETY: `ProgIDFromCLSID` is a Win32 FFI call that allocates a PWSTR via COM allocator.
    let progid = unsafe { windows::Win32::System::Com::ProgIDFromCLSID(guid) }?;

    if progid.is_null() {
        return Ok(String::new());
    }

    RemotePointer::from(progid).into_string()
}

/// Inspects the local machine Windows registry for an OPC DA server's registration details.
///
/// Queries `HKCR\CLSID\{...}` in both native and 32-bit (`KEY_WOW64_32KEY`) registry views
/// to detect execution mode (`LocalServer32` executable vs `InprocServer32` DLL) and binary path.
///
/// # Arguments
/// * `clsid` - 128-bit COM Class ID of the server.
/// * `host` - Target host machine. If `Some` and not localhost/127.0.0.1, returns [`OpcError::NotImplemented`].
///
/// # Errors
/// Returns [`OpcError::NotImplemented`] if `host` is a remote machine.
/// Returns [`OpcError::Server`] if the CLSID is not found or neither `LocalServer32` nor `InprocServer32` exists.
#[tracing::instrument(level = "info", skip(clsid), err)]
pub fn inspect_local_registration(
    clsid: &windows::core::GUID,
    host: Option<&str>,
) -> OpcResult<OpcServerRegistration> {
    if let Some(h) = host {
        let trimmed = h.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("localhost")
            && trimmed != "127.0.0.1"
        {
            let err = OpcError::NotImplemented(
                "Remote machine registry inspection is not supported".into(),
            );
            log_opc_err!(&err, OpcOperation::InspectRegistration, host = %h);
            return Err(err);
        }
    }

    let clsid_str = crate::types::format_guid_bracketed(clsid);

    use windows::Win32::System::Registry::{KEY_WOW64_32KEY, REG_SAM_FLAGS};
    let views = [REG_SAM_FLAGS(0), KEY_WOW64_32KEY];

    for view in views {
        if let Some(key_guard) = open_clsid_key(&clsid_str, view) {
            let key = key_guard.0;
            let prog_id = read_default_string(key, Some("ProgID"), view)
                .or_else(|| guid_to_progid(clsid).ok())
                .unwrap_or_else(|| clsid_str.clone());

            let version_independent_prog_id =
                read_default_string(key, Some("VersionIndependentProgID"), view);

            let local_server = read_default_string(key, Some("LocalServer32"), view);
            let inproc_server = read_default_string(key, Some("InprocServer32"), view);

            let (server_type, raw_path) = match (local_server, inproc_server) {
                (Some(exe), _) => (OpcServerType::LocalServer32, exe),
                (None, Some(dll)) => (OpcServerType::InprocServer32, dll),
                (None, None) => continue,
            };

            let binary_path = sanitize_binary_path(&raw_path);

            return Ok(OpcServerRegistration {
                clsid: *clsid,
                prog_id,
                version_independent_prog_id,
                binary_path,
                server_type,
            });
        }
    }

    let err = OpcError::Server(
        format!("No LocalServer32 or InprocServer32 registry key found for CLSID {clsid_str}"),
        crate::raw::hresult::REGDB_E_CLASSNOTREG.0.cast_unsigned(),
    );
    log_opc_err!(&err, OpcOperation::InspectRegistration, clsid = %clsid_str);
    Err(err)
}

/// COM-backed catalog adapter combining `IOPCServerList` and `IOPCServerList2`.
pub(crate) struct OpcServerListCatalog {
    v1: crate::raw::bindings::comn::IOPCServerList,
    v2: Option<crate::raw::bindings::comn::IOPCServerList2>,
}

impl OpcServerListCatalog {
    /// Creates a new catalog adapter by instantiating `OPC.ServerList.1`.
    pub(crate) fn new() -> OpcResult<Self> {
        // SAFETY: Calling Win32 CLSIDFromProgID with static wide string literal.
        let id = unsafe {
            windows::Win32::System::Com::CLSIDFromProgID(windows::core::w!("OPC.ServerList.1"))?
        };

        // SAFETY: Instantiating IOPCServerList COM interface.
        let v1: crate::raw::bindings::comn::IOPCServerList = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &raw const id,
                None,
                windows::Win32::System::Com::CLSCTX_ALL,
            )?
        };

        let v2 = v1
            .cast::<crate::raw::bindings::comn::IOPCServerList2>()
            .ok();

        Ok(Self { v1, v2 })
    }

    /// Enumerates server classes and extracts rich [`OpcServerInfo`] records.
    pub(crate) fn enumerate_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        let versions = [crate::raw::bindings::da::CATID_OPCDAServer20::IID];

        // SAFETY: Calling EnumClassesOfCategories via IOPCServerList (v1) returning standard IEnumGUID.
        let iter = unsafe {
            self.v1
                .EnumClassesOfCategories(&versions, &versions)
                .inspect_err(|e| tracing::warn!(error = ?e, "Failed to enumerate server classes"))?
        };

        let guid_iter = crate::com::iterator::GuidIterator::new(iter);
        let mut servers = Vec::new();

        let host_opt =
            if host.is_empty() || host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
                None
            } else {
                Some(host.to_string())
            };

        for guid in guid_iter.flatten() {
            if guid == windows::core::GUID::zeroed() {
                continue;
            }

            let mut prog_id_opt = None;
            let mut user_type_opt = None;

            // Attempt 1: Query v2 IOPCServerList2::GetClassDetails if available
            if let Some(ref list2) = self.v2 {
                let mut progid_ptr = windows::core::PWSTR::null();
                let mut usertype_ptr = windows::core::PWSTR::null();
                let mut verind_ptr = windows::core::PWSTR::null();

                // SAFETY: Calling GetClassDetails with mutable pointer addresses.
                let status = unsafe {
                    list2.GetClassDetails(
                        &raw const guid,
                        &raw mut progid_ptr,
                        &raw mut usertype_ptr,
                        &raw mut verind_ptr,
                    )
                };

                if status.is_ok() {
                    let pid_res = RemotePointer::from(progid_ptr).into_string();
                    let ut_res = Option::<String>::try_from(RemotePointer::from(usertype_ptr));
                    let _ = Option::<String>::try_from(RemotePointer::from(verind_ptr));

                    if let Ok(pid) = pid_res
                        && !pid.trim().is_empty()
                    {
                        prog_id_opt = Some(pid);
                        user_type_opt = ut_res.ok().flatten().filter(|s| !s.trim().is_empty());
                    }
                }
            }

            // Attempt 2: Fallback to v1 IOPCServerList::GetClassDetails
            if prog_id_opt.is_none() {
                let mut progid_ptr = windows::core::PWSTR::null();
                let mut usertype_ptr = windows::core::PWSTR::null();

                // SAFETY: Calling GetClassDetails with mutable pointer addresses.
                let status = unsafe {
                    self.v1.GetClassDetails(
                        &raw const guid,
                        &raw mut progid_ptr,
                        &raw mut usertype_ptr,
                    )
                };

                if status.is_ok() {
                    let pid_res = RemotePointer::from(progid_ptr).into_string();
                    let ut_res = Option::<String>::try_from(RemotePointer::from(usertype_ptr));

                    if let Ok(pid) = pid_res
                        && !pid.trim().is_empty()
                    {
                        prog_id_opt = Some(pid);
                        user_type_opt = ut_res.ok().flatten().filter(|s| !s.trim().is_empty());
                    }
                }
            }

            // Attempt 3: Fallback to guid_to_progid
            if prog_id_opt.is_none()
                && let Ok(pid) = guid_to_progid(&guid)
                && !pid.trim().is_empty()
            {
                prog_id_opt = Some(pid);
            }

            if let Some(prog_id) = prog_id_opt {
                servers.push(OpcServerInfo {
                    prog_id,
                    clsid: guid,
                    user_type: user_type_opt,
                    host: host_opt.clone(),
                });
            } else {
                tracing::warn!(guid = ?guid, "Skipping unresolvable OPC server class");
            }
        }

        servers.sort_by(|a, b| a.prog_id.cmp(&b.prog_id));
        servers.dedup_by(|a, b| a.prog_id == b.prog_id);
        Ok(servers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_local_registration_remote_rejected() {
        let clsid = windows::core::GUID::zeroed();
        let err = inspect_local_registration(&clsid, Some("192.168.1.100")).unwrap_err();
        match err {
            OpcError::NotImplemented(msg) => {
                assert!(msg.contains("Remote machine registry"));
            }
            other => panic!("Expected NotImplemented, got {other:?}"),
        }
    }

    #[test]
    fn test_sanitize_binary_path_quoted() {
        let raw = r#""C:\Program Files\Matrikon\OPC\Simulation.exe" /automation"#;
        let path = sanitize_binary_path(raw);
        assert_eq!(
            path,
            std::path::PathBuf::from(r"C:\Program Files\Matrikon\OPC\Simulation.exe")
        );
    }

    #[test]
    fn test_sanitize_binary_path_unquoted_with_flag() {
        let raw = r"C:\OPC\Server.exe -Embedding";
        let path = sanitize_binary_path(raw);
        assert_eq!(path, std::path::PathBuf::from(r"C:\OPC\Server.exe"));
    }

    #[test]
    fn test_opc_server_type_display() {
        assert_eq!(
            OpcServerType::LocalServer32.to_string(),
            "LocalServer32 (Executable)"
        );
        assert_eq!(
            OpcServerType::InprocServer32.to_string(),
            "InprocServer32 (DLL)"
        );
    }

    #[test]
    fn test_open_reg_key_invalid() {
        use windows::Win32::System::Registry::{HKEY_CLASSES_ROOT, REG_SAM_FLAGS};
        let key = open_reg_key(
            HKEY_CLASSES_ROOT,
            r"CLSID\{NONEXISTENT-0000-0000-0000-000000000000}\Invalid",
            REG_SAM_FLAGS(0),
        );
        assert!(key.is_none());
    }

    #[test]
    fn test_expand_environment_string() {
        // Standard variable expansion
        let expanded = expand_environment_string(r"%SystemRoot%\System32");
        assert!(!expanded.contains("%SystemRoot%"));
        assert!(expanded.to_ascii_lowercase().contains(r"\system32"));

        // Plain string without variables
        let plain = expand_environment_string(r"C:\Program Files\OPC\server.exe");
        assert_eq!(plain, r"C:\Program Files\OPC\server.exe");

        // Non-existent environment variable (Win32 preserves literal unexpanded token)
        let nonexistent = expand_environment_string(r"%NONEXISTENT_OPC_VAR_12345%\server.exe");
        assert!(nonexistent.contains("%NONEXISTENT_OPC_VAR_12345%"));

        // Malformed / single % token
        let malformed = expand_environment_string(r"C:\100%_working\server.exe");
        assert_eq!(malformed, r"C:\100%_working\server.exe");

        // Empty string
        let empty = expand_environment_string("");
        assert_eq!(empty, "");

        // Comprehensive stress test: oversized synthetic string (> 512 characters) exercising dynamic buffer allocation
        let repeated_token = "%SystemRoot%\\".repeat(50);
        let oversized_raw = format!("{repeated_token}server.exe");
        assert!(oversized_raw.len() > 512);
        let oversized_expanded = expand_environment_string(&oversized_raw);
        assert!(!oversized_expanded.contains("%SystemRoot%"));
        assert!(oversized_expanded.ends_with("server.exe"));
    }

    #[test]
    fn test_inspect_local_registration_nonexistent_returns_classnotreg() {
        let nonexistent_clsid =
            windows::core::GUID::from_u128(0xFEEDFACE_CAFE_BEEF_0123_456789ABCDEF);
        let err = inspect_local_registration(&nonexistent_clsid, None).unwrap_err();
        match err {
            OpcError::Server(msg, code) => {
                assert_eq!(
                    code,
                    crate::raw::hresult::REGDB_E_CLASSNOTREG.0.cast_unsigned()
                );
                assert!(msg.contains("No LocalServer32 or InprocServer32 registry key found"));
            }
            other => panic!("Expected OpcError::Server with REGDB_E_CLASSNOTREG, got {other:?}"),
        }
    }

    #[test]
    fn test_guid_to_progid_zeroed_guid_returns_com_error() {
        let zeroed = windows::core::GUID::zeroed();
        let result = guid_to_progid(&zeroed);
        assert!(result.is_err());
        if let Err(OpcError::Com { .. }) = result {
            // Expected: structured COM error preserved
        } else {
            panic!("Expected OpcError::Com, got: {result:?}");
        }
    }
}
