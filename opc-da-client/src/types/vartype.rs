//! Strongly-typed COM Automation `VARTYPE` discriminant wrapper and flags.

use std::fmt;

/// Base COM Automation variant type discriminants (without modifier flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseVarType {
    /// Empty / uninitialized variant (`VT_EMPTY = 0`).
    Empty,
    /// SQL-style NULL value (`VT_NULL = 1`).
    Null,
    /// 2-byte signed integer (`VT_I2 = 2`).
    I2,
    /// 4-byte signed integer (`VT_I4 = 3`).
    I4,
    /// 4-byte IEEE floating point (`VT_R4 = 4`).
    R4,
    /// 8-byte IEEE floating point (`VT_R8 = 5`).
    R8,
    /// Currency (`VT_CY = 6`).
    Cy,
    /// Date / time (`VT_DATE = 7`).
    Date,
    /// OLE Automation BSTR (`VT_BSTR = 8`).
    Bstr,
    /// COM IDispatch pointer (`VT_DISPATCH = 9`).
    Dispatch,
    /// SCODE / HRESULT error code (`VT_ERROR = 10`).
    Error,
    /// Boolean (`VT_BOOL = 11`).
    Bool,
    /// VARIANT pointer (`VT_VARIANT = 12`).
    Variant,
    /// COM IUnknown pointer (`VT_UNKNOWN = 13`).
    UnknownInterface,
    /// 16-byte fixed-point decimal (`VT_DECIMAL = 14`).
    Decimal,
    /// 1-byte signed integer (`VT_I1 = 16`).
    I1,
    /// 1-byte unsigned integer (`VT_UI1 = 17`).
    Ui1,
    /// 2-byte unsigned integer (`VT_UI2 = 18`).
    Ui2,
    /// 4-byte unsigned integer (`VT_UI4 = 19`).
    Ui4,
    /// 8-byte signed integer (`VT_I8 = 20`).
    I8,
    /// 8-byte unsigned integer (`VT_UI8 = 21`).
    Ui8,
    /// Machine signed integer (`VT_INT = 22`).
    Int,
    /// Machine unsigned integer (`VT_UINT = 23`).
    Uint,
    /// Other or unmapped raw discriminant.
    Other(u16),
}

impl BaseVarType {
    /// Returns the canonical COM symbol string for the base type.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "VT_EMPTY",
            Self::Null => "VT_NULL",
            Self::I2 => "VT_I2",
            Self::I4 => "VT_I4",
            Self::R4 => "VT_R4",
            Self::R8 => "VT_R8",
            Self::Cy => "VT_CY",
            Self::Date => "VT_DATE",
            Self::Bstr => "VT_BSTR",
            Self::Dispatch => "VT_DISPATCH",
            Self::Error => "VT_ERROR",
            Self::Bool => "VT_BOOL",
            Self::Variant => "VT_VARIANT",
            Self::UnknownInterface => "VT_UNKNOWN",
            Self::Decimal => "VT_DECIMAL",
            Self::I1 => "VT_I1",
            Self::Ui1 => "VT_UI1",
            Self::Ui2 => "VT_UI2",
            Self::Ui4 => "VT_UI4",
            Self::I8 => "VT_I8",
            Self::Ui8 => "VT_UI8",
            Self::Int => "VT_INT",
            Self::Uint => "VT_UINT",
            Self::Other(_) => "VT_OTHER",
        }
    }
}

impl fmt::Display for BaseVarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(raw) => write!(f, "VT_OTHER(0x{raw:04X})"),
            _ => f.write_str(self.as_str()),
        }
    }
}

/// Strongly-typed 16-bit COM Automation `VARTYPE` newtype.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarType(u16);

impl VarType {
    /// Modifier flag indicating the value is a vector (`VT_VECTOR = 0x1000`).
    pub const VT_VECTOR_FLAG: u16 = 0x1000;
    /// Modifier flag indicating the value is a safe array (`VT_ARRAY = 0x2000`).
    pub const VT_ARRAY_FLAG: u16 = 0x2000;
    /// Modifier flag indicating the value is passed by reference (`VT_BYREF = 0x4000`).
    pub const VT_BYREF_FLAG: u16 = 0x4000;
    /// Bitmask isolating the base scalar type (`0x0FFF`).
    pub const VT_TYPEMASK: u16 = 0x0FFF;

    /// Unspecified / empty variant (`VT_EMPTY = 0`).
    pub const EMPTY: Self = Self(0);
    /// SQL NULL variant (`VT_NULL = 1`).
    pub const NULL: Self = Self(1);
    /// 2-byte signed integer (`VT_I2 = 2`).
    pub const I2: Self = Self(2);
    /// 4-byte signed integer (`VT_I4 = 3`).
    pub const I4: Self = Self(3);
    /// 4-byte IEEE floating point (`VT_R4 = 4`).
    pub const R4: Self = Self(4);
    /// 8-byte IEEE floating point (`VT_R8 = 5`).
    pub const R8: Self = Self(5);
    /// Currency (`VT_CY = 6`).
    pub const CY: Self = Self(6);
    /// Date / time (`VT_DATE = 7`).
    pub const DATE: Self = Self(7);
    /// Binary string pointer (`VT_BSTR = 8`).
    pub const BSTR: Self = Self(8);
    /// COM IDispatch pointer (`VT_DISPATCH = 9`).
    pub const DISPATCH: Self = Self(9);
    /// SCODE / HRESULT error code (`VT_ERROR = 10`).
    pub const ERROR: Self = Self(10);
    /// Boolean (`VT_BOOL = 11`).
    pub const BOOL: Self = Self(11);
    /// VARIANT pointer (`VT_VARIANT = 12`).
    pub const VARIANT: Self = Self(12);
    /// COM IUnknown pointer (`VT_UNKNOWN = 13`).
    pub const UNKNOWN_INTERFACE: Self = Self(13);
    /// 16-byte fixed-point decimal (`VT_DECIMAL = 14`).
    pub const DECIMAL: Self = Self(14);
    /// 1-byte signed integer (`VT_I1 = 16`).
    pub const I1: Self = Self(16);
    /// 1-byte unsigned integer (`VT_UI1 = 17`).
    pub const UI1: Self = Self(17);
    /// 2-byte unsigned integer (`VT_UI2 = 18`).
    pub const UI2: Self = Self(18);
    /// 4-byte unsigned integer (`VT_UI4 = 19`).
    pub const UI4: Self = Self(19);
    /// 8-byte signed integer (`VT_I8 = 20`).
    pub const I8: Self = Self(20);
    /// 8-byte unsigned integer (`VT_UI8 = 21`).
    pub const UI8: Self = Self(21);
    /// Machine signed integer (`VT_INT = 22`).
    pub const INT: Self = Self(22);
    /// Machine unsigned integer (`VT_UINT = 23`).
    pub const UINT: Self = Self(23);

    /// Constructs a `VarType` from an unvalidated raw 16-bit integer.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw 16-bit word including modifier flags.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Strips modifier flags (`0xF000`) and returns the raw base scalar type integer.
    #[must_use]
    pub const fn base_raw(self) -> u16 {
        self.0 & Self::VT_TYPEMASK
    }

    /// Returns true if the `VT_ARRAY` flag (`0x2000`) is set.
    #[must_use]
    pub const fn is_array(self) -> bool {
        (self.0 & Self::VT_ARRAY_FLAG) != 0
    }

    /// Returns true if the `VT_BYREF` flag (`0x4000`) is set.
    #[must_use]
    pub const fn is_byref(self) -> bool {
        (self.0 & Self::VT_BYREF_FLAG) != 0
    }

    /// Returns true if the `VT_VECTOR` flag (`0x1000`) is set.
    #[must_use]
    pub const fn is_vector(self) -> bool {
        (self.0 & Self::VT_VECTOR_FLAG) != 0
    }

    /// Decomposes the variant into its base scalar `BaseVarType` without flags.
    #[must_use]
    pub const fn base_type(self) -> BaseVarType {
        match self.base_raw() {
            0 => BaseVarType::Empty,
            1 => BaseVarType::Null,
            2 => BaseVarType::I2,
            3 => BaseVarType::I4,
            4 => BaseVarType::R4,
            5 => BaseVarType::R8,
            6 => BaseVarType::Cy,
            7 => BaseVarType::Date,
            8 => BaseVarType::Bstr,
            9 => BaseVarType::Dispatch,
            10 => BaseVarType::Error,
            11 => BaseVarType::Bool,
            12 => BaseVarType::Variant,
            13 => BaseVarType::UnknownInterface,
            14 => BaseVarType::Decimal,
            16 => BaseVarType::I1,
            17 => BaseVarType::Ui1,
            18 => BaseVarType::Ui2,
            19 => BaseVarType::Ui4,
            20 => BaseVarType::I8,
            21 => BaseVarType::Ui8,
            22 => BaseVarType::Int,
            23 => BaseVarType::Uint,
            other => BaseVarType::Other(other),
        }
    }

    /// Constructs a `VarType` from a scalar `BaseVarType` without modifier flags.
    #[must_use]
    pub const fn from_base(base: BaseVarType) -> Self {
        match base {
            BaseVarType::Empty => Self::EMPTY,
            BaseVarType::Null => Self::NULL,
            BaseVarType::I2 => Self::I2,
            BaseVarType::I4 => Self::I4,
            BaseVarType::R4 => Self::R4,
            BaseVarType::R8 => Self::R8,
            BaseVarType::Cy => Self::CY,
            BaseVarType::Date => Self::DATE,
            BaseVarType::Bstr => Self::BSTR,
            BaseVarType::Dispatch => Self::DISPATCH,
            BaseVarType::Error => Self::ERROR,
            BaseVarType::Bool => Self::BOOL,
            BaseVarType::Variant => Self::VARIANT,
            BaseVarType::UnknownInterface => Self::UNKNOWN_INTERFACE,
            BaseVarType::Decimal => Self::DECIMAL,
            BaseVarType::I1 => Self::I1,
            BaseVarType::Ui1 => Self::UI1,
            BaseVarType::Ui2 => Self::UI2,
            BaseVarType::Ui4 => Self::UI4,
            BaseVarType::I8 => Self::I8,
            BaseVarType::Ui8 => Self::UI8,
            BaseVarType::Int => Self::INT,
            BaseVarType::Uint => Self::UINT,
            BaseVarType::Other(raw) => Self(raw),
        }
    }
}

impl From<u16> for VarType {
    #[inline]
    fn from(raw: u16) -> Self {
        Self(raw)
    }
}

impl From<VarType> for u16 {
    #[inline]
    fn from(vt: VarType) -> Self {
        vt.0
    }
}

impl From<BaseVarType> for VarType {
    #[inline]
    fn from(base: BaseVarType) -> Self {
        Self::from_base(base)
    }
}

impl fmt::Display for VarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_array() {
            f.write_str("VT_ARRAY | ")?;
        }
        if self.is_byref() {
            f.write_str("VT_BYREF | ")?;
        }
        if self.is_vector() {
            f.write_str("VT_VECTOR | ")?;
        }
        write!(f, "{}", self.base_type())
    }
}

#[cfg(feature = "opc-da-backend")]
impl From<windows::Win32::System::Variant::VARENUM> for VarType {
    #[inline]
    fn from(vt: windows::Win32::System::Variant::VARENUM) -> Self {
        Self(vt.0)
    }
}

#[cfg(feature = "opc-da-backend")]
impl From<VarType> for windows::Win32::System::Variant::VARENUM {
    #[inline]
    fn from(vt: VarType) -> Self {
        Self(vt.0)
    }
}
