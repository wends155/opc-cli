//! Low-level Win32 COM HRESULT constants, diagnostics, and formatters.

use windows::core::HRESULT;

pub const E_POINTER: HRESULT = HRESULT(0x8000_4003_u32.cast_signed());
pub const E_ACCESSDENIED: HRESULT = HRESULT(0x8007_0005_u32.cast_signed());
pub const RPC_S_SERVER_UNAVAILABLE: HRESULT = HRESULT(0x8007_06BA_u32.cast_signed());
pub const RPC_S_CALL_FAILED: HRESULT = HRESULT(0x8007_06BE_u32.cast_signed());
pub const RPC_S_SERVER_TOO_BUSY: HRESULT = HRESULT(0x8007_06BF_u32.cast_signed());
pub const RPC_S_CALL_FAILED_DNE: HRESULT = HRESULT(0x8007_06F4_u32.cast_signed());
pub const CLASS_E_NOTLICENSED: HRESULT = HRESULT(0x8004_0112_u32.cast_signed());
pub const REGDB_E_CLASSNOTREG: HRESULT = HRESULT(0x8004_0154_u32.cast_signed());
pub const CO_E_SERVER_EXEC_FAILURE: HRESULT = HRESULT(0x8008_0005_u32.cast_signed());
pub const OPC_E_BADRIGHTS: HRESULT = HRESULT(0xC004_0004_u32.cast_signed());
pub const OPC_E_BADTYPE: HRESULT = HRESULT(0xC004_0006_u32.cast_signed());
pub const OPC_E_UNKNOWNITEMID: HRESULT = HRESULT(0xC004_0007_u32.cast_signed());
pub const OPC_E_INVALIDITEMID: HRESULT = HRESULT(0xC004_0008_u32.cast_signed());

/// Maps known COM/DCOM and OPC error codes to actionable user hints.
#[must_use]
pub fn friendly_hresult_hint(hr: HRESULT) -> Option<&'static str> {
    match hr {
        CLASS_E_NOTLICENSED => Some("Server license does not permit OPC client connections"),
        CO_E_SERVER_EXEC_FAILURE => {
            Some("Server process failed to start — check if it is installed and running")
        }
        E_ACCESSDENIED => {
            Some("Access denied — DCOM launch/activation permissions not configured for this user")
        }
        RPC_S_SERVER_UNAVAILABLE => {
            Some("RPC server unavailable — the target host may be offline or blocking RPC")
        }
        RPC_S_CALL_FAILED => Some("RPC call failed — network or remote server connection dropped"),
        RPC_S_SERVER_TOO_BUSY => Some("RPC server is too busy to complete this operation"),
        RPC_S_CALL_FAILED_DNE => Some("COM marshalling error — try restarting the OPC server"),
        REGDB_E_CLASSNOTREG => Some("Server is not registered on this machine"),
        E_POINTER => Some("Invalid pointer (E_POINTER)"),
        OPC_E_BADRIGHTS => {
            Some("Server rejected write — the item may be read-only (OPC_E_BADRIGHTS)")
        }
        OPC_E_BADTYPE => {
            Some("Data type mismatch — server cannot convert the written value (OPC_E_BADTYPE)")
        }
        OPC_E_UNKNOWNITEMID => {
            Some("Item ID not found in server address space (OPC_E_UNKNOWNITEMID)")
        }
        OPC_E_INVALIDITEMID => {
            Some("Item ID syntax is invalid for this server (OPC_E_INVALIDITEMID)")
        }
        _ => None,
    }
}

/// Formats an HRESULT with a hexadecimal representation and optional friendly hint.
#[allow(dead_code)]
#[must_use]
pub fn format_hresult(hr: HRESULT) -> String {
    let hex = format!("0x{:08X}", hr.0.cast_unsigned());
    match friendly_hresult_hint(hr) {
        Some(hint) => format!("{hex}: {hint}"),
        None => hex,
    }
}

/// Determines whether an HRESULT indicates a network or transport connection failure.
#[must_use]
pub fn is_connection_hresult(hr: HRESULT) -> bool {
    matches!(
        hr,
        RPC_S_SERVER_UNAVAILABLE
            | RPC_S_CALL_FAILED
            | RPC_S_SERVER_TOO_BUSY
            | CO_E_SERVER_EXEC_FAILURE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friendly_hresult_hint_known() {
        assert_eq!(
            friendly_hresult_hint(E_POINTER),
            Some("Invalid pointer (E_POINTER)")
        );
        assert_eq!(
            friendly_hresult_hint(RPC_S_SERVER_UNAVAILABLE),
            Some("RPC server unavailable — the target host may be offline or blocking RPC")
        );
        assert_eq!(
            friendly_hresult_hint(REGDB_E_CLASSNOTREG),
            Some("Server is not registered on this machine")
        );
        assert_eq!(
            friendly_hresult_hint(OPC_E_BADRIGHTS),
            Some("Server rejected write — the item may be read-only (OPC_E_BADRIGHTS)")
        );
    }

    #[test]
    fn test_friendly_hresult_hint_unknown() {
        assert_eq!(
            friendly_hresult_hint(windows::core::HRESULT(0x1234_5678)),
            None
        );
    }

    #[test]
    fn test_is_connection_hresult() {
        assert!(is_connection_hresult(RPC_S_SERVER_UNAVAILABLE));
        assert!(is_connection_hresult(RPC_S_CALL_FAILED));
        assert!(is_connection_hresult(RPC_S_SERVER_TOO_BUSY));
        assert!(is_connection_hresult(CO_E_SERVER_EXEC_FAILURE));
        assert!(!is_connection_hresult(E_POINTER));
        assert!(!is_connection_hresult(REGDB_E_CLASSNOTREG));
    }

    #[test]
    fn test_format_hresult() {
        assert_eq!(
            format_hresult(E_POINTER),
            "0x80004003: Invalid pointer (E_POINTER)"
        );
        assert_eq!(
            format_hresult(windows::core::HRESULT(0x1234_5678)),
            "0x12345678"
        );
    }
}
