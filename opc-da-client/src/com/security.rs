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

/// Returns the RPC authentication level depending on whether legacy DCOM is requested.
///
/// Under modern Windows environments (post-KB5004442), `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` (5)
/// is required unless `legacy_dcom` is set to `true`, in which case `RPC_C_AUTHN_LEVEL_CONNECT` (2)
/// is used.
#[inline]
#[must_use]
pub const fn authn_level_for(legacy_dcom: bool) -> u32 {
    if legacy_dcom {
        RPC_C_AUTHN_LEVEL_CONNECT
    } else {
        RPC_C_AUTHN_LEVEL_PKT_INTEGRITY
    }
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
pub fn apply_proxy_blanket<T: Interface>(proxy: &T, legacy_dcom: bool) {
    let authn_level = authn_level_for(legacy_dcom);
    let unk: windows::core::IUnknown = match proxy.cast() {
        Ok(u) => u,
        Err(e) => {
            tracing::debug!(error = ?e, "Interface does not cast to IUnknown for proxy blanketing");
            return;
        }
    };
    // SAFETY: Calling CoSetProxyBlanket on valid COM interface pointer with standard NT security.
    let hr = unsafe {
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
    };
    if let Err(e) = hr {
        tracing::warn!(
            error = ?e,
            authn_level,
            "Failed to set COM proxy blanket (continuing without security blanket)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authn_level_defaults_and_legacy() {
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
