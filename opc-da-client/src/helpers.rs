use anyhow::Context;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Com::{CoTaskMemFree, ProgIDFromCLSID};
use windows::Win32::System::Variant::VARIANT;

/// Maps known COM/DCOM error codes to actionable user hints.
///
/// # Examples
/// ```
/// use opc_da_client::friendly_com_hint;
///
/// let err = anyhow::anyhow!("COM error 0x80040154");
/// assert_eq!(
///     friendly_com_hint(&err),
///     Some("Server is not registered on this machine"),
/// );
///
/// let unknown = anyhow::anyhow!("Something else");
/// assert_eq!(friendly_com_hint(&unknown), None);
/// ```
pub fn friendly_com_hint(err: &anyhow::Error) -> Option<&'static str> {
    let msg = format!("{:?}", err);
    if msg.contains("0x80040112") {
        Some("Server license does not permit OPC client connections")
    } else if msg.contains("0x80080005") {
        Some("Server process failed to start — check if it is installed and running")
    } else if msg.contains("0x80070005") {
        Some("Access denied — DCOM launch/activation permissions not configured for this user")
    } else if msg.contains("0x800706BA") {
        Some("RPC server unavailable — the target host may be offline or blocking RPC")
    } else if msg.contains("0x800706F4") {
        Some("COM marshalling error — try restarting the OPC server")
    } else if msg.contains("0x80040154") {
        Some("Server is not registered on this machine")
    } else if msg.contains("0x80004003") {
        Some(
            "Invalid pointer — likely a known issue with the OPC DA crate's iterator initialization",
        )
    } else {
        None
    }
}

/// Returns `true` for E_POINTER errors that are known to be caused by
/// the `opc_da` crate's `StringIterator` initialization bug (index starts
/// at 0 with null-pointer cache, producing 16 phantom errors per iterator).
pub(crate) fn is_known_iterator_bug(err: &windows::core::Error) -> bool {
    err.code().0 as u32 == 0x80004003 // E_POINTER
}

/// Helper to convert GUID to ProgID using Windows API
pub(crate) fn guid_to_progid(guid: &windows::core::GUID) -> anyhow::Result<String> {
    unsafe {
        let progid = ProgIDFromCLSID(guid).context("Failed to get ProgID from CLSID")?;

        let result = if progid.is_null() {
            String::new()
        } else {
            progid
                .to_string()
                .map_err(|e| anyhow::anyhow!("Failed into convert PWSTR: {}", e))?
        };

        if !progid.is_null() {
            CoTaskMemFree(Some(progid.as_ptr() as *const _));
        }

        Ok(result)
    }
}

/// Convert OPC DA VARIANT to a displayable string.
pub(crate) fn variant_to_string(variant: &VARIANT) -> String {
    unsafe {
        let vt = variant.Anonymous.Anonymous.vt;
        match vt.0 {
            0 => "Empty".to_string(),                                       // VT_EMPTY
            1 => "Null".to_string(),                                        // VT_NULL
            2 => format!("{}", variant.Anonymous.Anonymous.Anonymous.iVal), // VT_I2
            3 => format!("{}", variant.Anonymous.Anonymous.Anonymous.lVal), // VT_I4
            4 => format!("{:.2}", variant.Anonymous.Anonymous.Anonymous.fltVal), // VT_R4
            5 => format!("{:.2}", variant.Anonymous.Anonymous.Anonymous.dblVal), // VT_R8
            8 => {
                // VT_BSTR - string
                let bstr = &variant.Anonymous.Anonymous.Anonymous.bstrVal;
                if bstr.is_empty() {
                    "\"\"".to_string()
                } else {
                    format!("\"{}\"", &**bstr)
                }
            }
            11 => format!("{}", variant.Anonymous.Anonymous.Anonymous.boolVal.0 != 0), // VT_BOOL
            _ => format!("(VT {:?})", vt),
        }
    }
}

/// Map OPC quality code to a human-readable label.
pub(crate) fn quality_to_string(quality: u16) -> String {
    let quality_bits = quality & 0xC0; // Top 2 bits define Good/Bad/Uncertain
    match quality_bits {
        0xC0 => "Good".to_string(),
        0x00 => "Bad".to_string(),
        0x40 => "Uncertain".to_string(),
        _ => format!("Unknown(0x{:04X})", quality),
    }
}

/// Convert FILETIME to a human-readable local time string.
pub(crate) fn filetime_to_string(ft: &FILETIME) -> String {
    if ft.dwHighDateTime == 0 && ft.dwLowDateTime == 0 {
        return "N/A".to_string();
    }
    let intervals = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    let unix_secs = (intervals / 10_000_000).saturating_sub(11_644_473_600);
    let nanos = ((intervals % 10_000_000) * 100) as u32;

    match chrono::DateTime::from_timestamp(unix_secs as i64, nanos) {
        Some(utc) => utc
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "Invalid".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friendly_com_hint_known_codes() {
        let err = anyhow::anyhow!("COM error 0x800706F4");
        assert_eq!(
            friendly_com_hint(&err),
            Some("COM marshalling error — try restarting the OPC server")
        );

        let err = anyhow::anyhow!("COM error 0x80040154");
        assert_eq!(
            friendly_com_hint(&err),
            Some("Server is not registered on this machine")
        );
    }

    #[test]
    fn test_friendly_com_hint_unknown_code() {
        let err = anyhow::anyhow!("Some other error");
        assert_eq!(friendly_com_hint(&err), None);
    }

    #[test]
    fn test_filetime_to_string_zero() {
        let ft = FILETIME {
            dwHighDateTime: 0,
            dwLowDateTime: 0,
        };
        assert_eq!(filetime_to_string(&ft), "N/A");
    }

    #[test]
    fn test_filetime_to_string_nonzero() {
        let ft = FILETIME {
            dwHighDateTime: 0x01DC9EF1,
            dwLowDateTime: 0xA3BDF80,
        };
        let result = filetime_to_string(&ft);
        assert!(result.contains("-"));
    }
}
