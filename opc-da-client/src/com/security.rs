//! DCOM security configuration, authentication levels, and proxy blanketing.
//!
//! Provides common security utilities and standard CLSIDs used across
//! connector and discovery subsystems to satisfy modern Windows DCOM
//! authentication requirements (KB5004442).

use windows::Win32::System::Com::{
    CoSetProxyBlanket, EOAC_NONE, RPC_C_AUTHN_LEVEL, RPC_C_IMP_LEVEL,
};
use windows::core::Interface;

/// Standard OPC Foundation OPCEnum CLSID (`{13486D51-4821-11D2-A494-3CB306C10000}`) for remote server discovery.
pub const CLSID_OPC_SERVER_LIST: windows::core::GUID =
    windows::core::GUID::from_u128(0x1348_6d51_4821_11d2_a494_3cb3_06c1_0000);

/// RPC authentication level for connect (`RPC_C_AUTHN_LEVEL_CONNECT` = 2).
pub const RPC_C_AUTHN_LEVEL_CONNECT: u32 = 2;

/// RPC authentication level for packet integrity (`RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` = 5).
pub const RPC_C_AUTHN_LEVEL_PKT_INTEGRITY: u32 = 5;

/// NTLM / Kerberos Windows NT authentication service (`RPC_C_AUTHN_WINNT` = 10).
pub const RPC_C_AUTHN_WINNT: u32 = 10;

/// Default authorization service (`RPC_C_AUTHZ_NONE` = 0).
pub const RPC_C_AUTHZ_NONE: u32 = 0;

/// Impersonation level (`RPC_C_IMP_LEVEL_IMPERSONATE` = 3).
pub const RPC_C_IMP_LEVEL_IMPERSONATE: u32 = 3;

/// DCOM authentication security level for remote RPC connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DcomSecurityLevel {
    /// Modern Windows default (post-KB5004442): packet integrity authentication (`RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` = 5).
    #[default]
    PacketIntegrity,
    /// Legacy DCOM connect authentication (`RPC_C_AUTHN_LEVEL_CONNECT` = 2).
    Connect,
}

impl DcomSecurityLevel {
    /// Maps from a boolean flag where `true` indicates legacy DCOM connect mode.
    #[inline]
    #[must_use]
    pub const fn from_legacy_flag(legacy_dcom: bool) -> Self {
        if legacy_dcom {
            Self::Connect
        } else {
            Self::PacketIntegrity
        }
    }

    /// Returns the raw Win32 RPC authentication level constant.
    #[inline]
    #[must_use]
    pub const fn rpc_authn_level(self) -> u32 {
        match self {
            Self::Connect => RPC_C_AUTHN_LEVEL_CONNECT,
            Self::PacketIntegrity => RPC_C_AUTHN_LEVEL_PKT_INTEGRITY,
        }
    }
}

/// Returns the RPC authentication level depending on whether legacy DCOM is requested.
///
/// Under modern Windows environments (post-KB5004442), `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` (5)
/// is required unless `legacy_dcom` is set to `true`, in which case `RPC_C_AUTHN_LEVEL_CONNECT` (2)
/// is used.
#[inline]
#[must_use]
pub const fn authn_level_for(legacy_dcom: bool) -> u32 {
    DcomSecurityLevel::from_legacy_flag(legacy_dcom).rpc_authn_level()
}

/// Applies DCOM security blanketing to a COM interface proxy.
///
/// In modern Windows environments (post-KB5004442), DCOM RPC requires `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` (5)
/// by default, or `RPC_C_AUTHN_LEVEL_CONNECT` (2) if `legacy_dcom` is enabled.
///
/// # Arguments
///
/// * `proxy` - COM interface proxy reference to blanket.
/// * `legacy_dcom` - When `true`, uses `RPC_C_AUTHN_LEVEL_CONNECT` instead of `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`.
///
/// # Errors
///
/// Returns [`crate::errors::OpcError`] if interface casting or `CoSetProxyBlanket` fails.
pub fn apply_proxy_blanket<T: Interface>(
    proxy: &T,
    legacy_dcom: bool,
) -> crate::errors::OpcResult<()> {
    let authn_level = authn_level_for(legacy_dcom);
    let unk: windows::core::IUnknown = proxy.cast().map_err(crate::errors::OpcError::from)?;
    // SAFETY: Calling CoSetProxyBlanket on valid COM interface pointer with standard NT security.
    unsafe {
        CoSetProxyBlanket(
            &unk,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL(authn_level),
            RPC_C_IMP_LEVEL(RPC_C_IMP_LEVEL_IMPERSONATE),
            None,
            EOAC_NONE,
        )
    }
    .map_err(crate::errors::OpcError::from)
}

/// Creates an instance of a COM class on a remote host via DCOM `CoCreateInstanceEx`,
/// and applies proxy blanketing to satisfy modern Windows security requirements (KB5004442).
///
/// # Arguments
///
/// * `clsid` - Target CLSID GUID.
/// * `host` - Remote hostname or IP address.
/// * `legacy_dcom` - When `true`, uses `RPC_C_AUTHN_LEVEL_CONNECT` instead of `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`.
///
/// # Errors
///
/// Returns [`crate::errors::OpcError`] if remote activation or interface query fails.
pub fn create_remote_instance<T: Interface>(
    clsid: &windows::core::GUID,
    host: &str,
    legacy_dcom: bool,
) -> crate::errors::OpcResult<T> {
    use windows::Win32::System::Com::{
        CLSCTX_REMOTE_SERVER, COAUTHINFO, COSERVERINFO, CoCreateInstanceEx, MULTI_QI,
    };

    let host_lp = crate::raw::memory::LocalPointer::from(host);
    let authn_level = authn_level_for(legacy_dcom);
    let auth_info = COAUTHINFO {
        dwAuthnSvc: RPC_C_AUTHN_WINNT,
        dwAuthzSvc: RPC_C_AUTHZ_NONE,
        pwszServerPrincName: windows::core::PWSTR::null(),
        dwAuthnLevel: authn_level,
        dwImpersonationLevel: RPC_C_IMP_LEVEL_IMPERSONATE,
        pAuthIdentityData: std::ptr::null_mut(),
        dwCapabilities: 0,
    };
    // SAFETY: host_lp outlives server_info and CoCreateInstanceEx invocation.
    let host_pwstr = unsafe { host_lp.as_pwstr() };
    let server_info = COSERVERINFO {
        dwReserved1: 0,
        pwszName: host_pwstr,
        pAuthInfo: (&raw const auth_info).cast_mut(),
        dwReserved2: 0,
    };
    let mqi = MULTI_QI {
        pIID: &T::IID,
        pItf: std::mem::ManuallyDrop::new(None),
        hr: windows::core::HRESULT(0),
    };
    let mut mqi_slice = [mqi];

    // SAFETY: Calling CoCreateInstanceEx with remote host and valid MULTI_QI for T::IID.
    unsafe {
        CoCreateInstanceEx(
            clsid,
            None,
            CLSCTX_REMOTE_SERVER,
            Some(&raw const server_info),
            &mut mqi_slice,
        )
    }
    .map_err(crate::errors::OpcError::from)?;

    let [mut result_mqi] = mqi_slice;
    if result_mqi.hr.is_err() {
        return Err(crate::errors::OpcError::from(windows::core::Error::from(
            result_mqi.hr,
        )));
    }

    // SAFETY: CoCreateInstanceEx succeeded with S_OK and populated result_mqi.pItf with a valid COM pointer.
    let unk = unsafe { std::mem::ManuallyDrop::take(&mut result_mqi.pItf) }.ok_or_else(|| {
        crate::errors::OpcError::Internal(
            "CoCreateInstanceEx returned null interface pointer".into(),
        )
    })?;

    // Apply proxy blanket to the remote instance
    apply_proxy_blanket(&unk, legacy_dcom)?;

    unk.cast().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authn_level_defaults_and_legacy() {
        assert_eq!(
            DcomSecurityLevel::default(),
            DcomSecurityLevel::PacketIntegrity
        );
        assert_eq!(
            DcomSecurityLevel::PacketIntegrity.rpc_authn_level(),
            RPC_C_AUTHN_LEVEL_PKT_INTEGRITY
        );
        assert_eq!(
            DcomSecurityLevel::Connect.rpc_authn_level(),
            RPC_C_AUTHN_LEVEL_CONNECT
        );
        assert_eq!(authn_level_for(false), RPC_C_AUTHN_LEVEL_PKT_INTEGRITY);
        assert_eq!(authn_level_for(true), RPC_C_AUTHN_LEVEL_CONNECT);
    }

    #[test]
    fn test_clsid_opc_server_list_constant() {
        assert_eq!(
            CLSID_OPC_SERVER_LIST,
            windows::core::GUID::from_u128(0x1348_6d51_4821_11d2_a494_3cb3_06c1_0000)
        );
    }
}
