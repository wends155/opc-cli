//! Strongly-typed 128-bit Windows COM Class Identifier (CLSID) domain representation.
//!
//! Encapsulates a standard 128-bit COM Class ID without leaking platform-specific
//! SDK crates into consumer types. Guaranteed to have identical memory layout and alignment
//! as Win32 / COM `GUID`, enabling zero-cost interoperability.

use std::fmt;
use std::str::FromStr;

/// Error returned when parsing a [`Clsid`] from an invalid string representation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Invalid CLSID GUID string: '{0}'")]
pub struct ParseClsidError(String);

impl ParseClsidError {
    /// Returns the raw input string that failed validation.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let err = "not-a-guid".parse::<Clsid>().unwrap_err();
    /// assert_eq!(err.raw(), "not-a-guid");
    /// ```
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.0
    }
}

/// Strongly-typed 128-bit Windows COM Class Identifier (CLSID).
///
/// Encapsulates a standard 128-bit DCE / COM UUID without requiring platform SDK
/// crates in pure domain models.
///
/// # Layout Guarantee
///
/// `Clsid` is explicitly marked `#[repr(C)]` with identical layout and alignment to
/// Win32 COM `GUID`:
/// - `data1`: `u32` (first 32 bits)
/// - `data2`: `u16` (next 16 bits)
/// - `data3`: `u16` (next 16 bits)
/// - `data4`: `[u8; 8]` (final 64 bits)
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Clsid {
    /// The first 32 bits of the 128-bit identifier.
    pub data1: u32,
    /// The next 16 bits of the 128-bit identifier.
    pub data2: u16,
    /// The next 16 bits of the 128-bit identifier.
    pub data3: u16,
    /// The final 64 bits (8 bytes) of the 128-bit identifier.
    pub data4: [u8; 8],
}

impl Clsid {
    /// Creates a new [`Clsid`] from raw integer components.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let clsid = Clsid::new(0x12345678, 0x1234, 0x5678, [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]);
    /// assert_eq!(clsid.data1, 0x12345678);
    /// ```
    #[inline]
    #[must_use]
    pub const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
        Self {
            data1,
            data2,
            data3,
            data4,
        }
    }

    /// Constructs a null (all-zeroes) [`Clsid`]: `{00000000-0000-0000-0000-000000000000}`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let clsid = Clsid::zeroed();
    /// assert!(clsid.is_zero());
    /// ```
    #[inline]
    #[must_use]
    pub const fn zeroed() -> Self {
        Self {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        }
    }

    /// Constructs a null [`Clsid`], identical to [`Clsid::zeroed()`].
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// assert_eq!(Clsid::nil(), Clsid::zeroed());
    /// ```
    #[inline]
    #[must_use]
    pub const fn nil() -> Self {
        Self::zeroed()
    }

    /// Creates a [`Clsid`] from a 128-bit unsigned integer in big-endian component layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let clsid = Clsid::from_u128(0x00000001_0002_0003_0405060708090a0b);
    /// assert_eq!(clsid.data1, 1);
    /// assert_eq!(clsid.data2, 2);
    /// ```
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub const fn from_u128(uuid: u128) -> Self {
        Self {
            data1: (uuid >> 96) as u32,
            data2: (uuid >> 80) as u16,
            data3: (uuid >> 64) as u16,
            data4: (uuid as u64).to_be_bytes(),
        }
    }

    /// Converts this [`Clsid`] into a 128-bit unsigned integer in big-endian component layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let val = 0x00000001_0002_0003_0405060708090a0b_u128;
    /// let clsid = Clsid::from_u128(val);
    /// assert_eq!(clsid.to_u128(), val);
    /// ```
    #[inline]
    #[must_use]
    pub const fn to_u128(&self) -> u128 {
        ((self.data1 as u128) << 96)
            | ((self.data2 as u128) << 80)
            | ((self.data3 as u128) << 64)
            | (u64::from_be_bytes(self.data4) as u128)
    }

    /// Returns `true` if this identifier is the null / zeroed CLSID.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// assert!(Clsid::zeroed().is_zero());
    /// assert!(!Clsid::from_u128(1).is_zero());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.data1 == 0 && self.data2 == 0 && self.data3 == 0 && u64::from_ne_bytes(self.data4) == 0
    }

    /// Converts this domain [`Clsid`] into a platform [`windows_core::GUID`].
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "opc-da-backend")]
    /// # {
    /// use opc_da_client::Clsid;
    /// let clsid = Clsid::zeroed();
    /// let guid = clsid.to_windows_guid();
    /// assert_eq!(guid.data1, 0);
    /// # }
    /// ```
    #[cfg(feature = "opc-da-backend")]
    #[inline]
    #[must_use]
    pub const fn to_windows_guid(&self) -> windows_core::GUID {
        windows_core::GUID {
            data1: self.data1,
            data2: self.data2,
            data3: self.data3,
            data4: self.data4,
        }
    }

    /// Creates a domain [`Clsid`] from a platform [`windows_core::GUID`].
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "opc-da-backend")]
    /// # {
    /// use opc_da_client::Clsid;
    /// let guid = windows_core::GUID::zeroed();
    /// let clsid = Clsid::from_windows_guid(guid);
    /// assert!(clsid.is_zero());
    /// # }
    /// ```
    #[cfg(feature = "opc-da-backend")]
    #[inline]
    #[must_use]
    pub const fn from_windows_guid(guid: windows_core::GUID) -> Self {
        Self {
            data1: guid.data1,
            data2: guid.data2,
            data3: guid.data3,
            data4: guid.data4,
        }
    }

    /// Parses a standard GUID string into a [`Clsid`].
    ///
    /// Accepts both bracketed `"{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}"` and unbracketed
    /// `"XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"`. Case-insensitive. Zero heap allocations.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let clsid = Clsid::parse("{13486D51-4821-11D2-A494-3CB306C10000}").unwrap();
    /// assert_eq!(clsid.data1, 0x13486D51);
    /// ```
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        let inner = if let Some(stripped) = trimmed.strip_prefix('{') {
            stripped.strip_suffix('}')?
        } else {
            trimmed
        };

        if !inner.is_ascii() || inner.len() != 36 {
            return None;
        }

        let bytes = inner.as_bytes();
        if bytes[8] != b'-' || bytes[13] != b'-' || bytes[18] != b'-' || bytes[23] != b'-' {
            return None;
        }

        let data1 = u32::from_str_radix(&inner[0..8], 16).ok()?;
        let data2 = u16::from_str_radix(&inner[9..13], 16).ok()?;
        let data3 = u16::from_str_radix(&inner[14..18], 16).ok()?;
        let d4_a = u16::from_str_radix(&inner[19..23], 16).ok()?;
        let d4_b = u64::from_str_radix(&inner[24..36], 16).ok()?;

        let mut data4 = [0u8; 8];
        data4[..2].copy_from_slice(&d4_a.to_be_bytes());
        data4[2..8].copy_from_slice(&d4_b.to_be_bytes()[2..8]);

        Some(Self::new(data1, data2, data3, data4))
    }

    /// Formats the CLSID into a bracketed uppercase registry string:
    /// `"{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::Clsid;
    /// let clsid = Clsid::zeroed();
    /// assert_eq!(clsid.to_bracketed(), "{00000000-0000-0000-0000-000000000000}");
    /// ```
    #[must_use]
    pub fn to_bracketed(&self) -> String {
        format!("{self}")
    }
}

impl fmt::Display for Clsid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7],
        )
    }
}

impl fmt::Debug for Clsid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Clsid({self})")
    }
}

impl FromStr for Clsid {
    type Err = ParseClsidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| ParseClsidError(s.to_string()))
    }
}

#[cfg(feature = "opc-da-backend")]
impl From<windows_core::GUID> for Clsid {
    #[inline]
    fn from(guid: windows_core::GUID) -> Self {
        Self::from_windows_guid(guid)
    }
}

#[cfg(feature = "opc-da-backend")]
impl From<Clsid> for windows_core::GUID {
    #[inline]
    fn from(clsid: Clsid) -> Self {
        clsid.to_windows_guid()
    }
}

impl From<u128> for Clsid {
    #[inline]
    fn from(val: u128) -> Self {
        Self::from_u128(val)
    }
}

impl From<Clsid> for u128 {
    #[inline]
    fn from(clsid: Clsid) -> Self {
        clsid.to_u128()
    }
}
