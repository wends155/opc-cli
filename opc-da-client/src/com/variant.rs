//! Windows COM VARIANT and SafeArray conversions.
//!
//! Provides conversion utilities between Win32 COM [`VARIANT`] structures
//! and strongly-typed pure-Rust [`OpcValue`] instances.
//!
//! This module is private to the `com` subsystem (`pub(crate)`) ensuring
//! low-level COM FFI structures do not leak into Tier 1 domain code.

use crate::raw::hresult::friendly_hresult_hint as friendly_com_hresult_hint;
use crate::types::OpcValue;
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayGetDim, SafeArrayGetElemsize, SafeArrayGetLBound,
    SafeArrayGetUBound, SafeArrayUnaccessData,
};
use windows::Win32::System::Variant::{
    VARIANT, VT_BOOL, VT_BSTR, VT_EMPTY, VT_I4, VT_I8, VT_NULL, VT_R8, VT_UI4, VT_UI8,
};
use windows::core::BSTR;

/// Maximum recursion depth for nested SafeArray traversal to prevent stack overflow.
const MAX_VARIANT_RECURSION_DEPTH: usize = 8;

/// Convert OPC DA VARIANT to a displayable string.
pub fn variant_to_string(variant: &VARIANT) -> String {
    variant_to_string_bounded(variant, 0)
}

#[allow(clippy::too_many_lines)]
fn variant_to_string_bounded(variant: &VARIANT, depth: usize) -> String {
    if depth >= MAX_VARIANT_RECURSION_DEPTH {
        return "<nested>".to_string();
    }

    // SAFETY: Accessing the VARIANT union fields. Caller guarantees VARIANT was produced by COM.
    // SAFETY: The `vt` discriminant correctly identifies which union arm is active.
    unsafe {
        let vt = variant.Anonymous.Anonymous.vt;
        let base_type = vt.0 & 0x0FFF; // strip VT_ARRAY (0x2000) / VT_BYREF (0x4000)
        let is_array = (vt.0 & 0x2000) != 0;

        if is_array {
            // Iterate 1-D SafeArrays and display actual element values
            let parray = variant.Anonymous.Anonymous.Anonymous.parray;
            if parray.is_null() {
                return "Array[?]".to_string();
            }
            let dims = SafeArrayGetDim(parray);
            if dims == 0 {
                return "Array[0]".to_string();
            }
            // For 1-D arrays compute count; for multi-dim just show dims
            if dims == 1 {
                let lb = SafeArrayGetLBound(parray, 1).unwrap_or(0);
                let ub = SafeArrayGetUBound(parray, 1).unwrap_or(-1);
                let count = (ub - lb + 1).max(0);
                let mut elements = Vec::new();
                let display_count = count.min(20);

                if base_type == windows::Win32::System::Variant::VT_VARIANT.0 {
                    let mut data_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
                    if SafeArrayAccessData(parray, &raw mut data_ptr).is_ok() {
                        #[allow(clippy::cast_sign_loss)]
                        let vars =
                            std::slice::from_raw_parts(data_ptr as *const VARIANT, count as usize);
                        for i in 0..display_count {
                            #[allow(clippy::cast_sign_loss)]
                            elements.push(variant_to_string_bounded(&vars[i as usize], depth + 1));
                        }
                        let _ = SafeArrayUnaccessData(parray);
                    }
                } else {
                    let elem_size = SafeArrayGetElemsize(parray) as usize;
                    let mut data_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
                    if SafeArrayAccessData(parray, &raw mut data_ptr).is_ok() {
                        for i in 0..display_count {
                            let mut temp_var = VARIANT::default();
                            (*temp_var.Anonymous.Anonymous).vt =
                                windows::Win32::System::Variant::VARENUM(base_type);

                            #[allow(clippy::cast_sign_loss)]
                            let src_ptr = (data_ptr as *const u8).add((i as usize) * elem_size);
                            let dst_ptr =
                                std::ptr::addr_of_mut!((*temp_var.Anonymous.Anonymous).Anonymous)
                                    .cast::<u8>();

                            let union_cap =
                                core::mem::size_of_val(&(*temp_var.Anonymous.Anonymous).Anonymous);
                            let copy_len = elem_size.min(union_cap);

                            std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, copy_len);

                            elements.push(variant_to_string_bounded(&temp_var, depth + 1));
                        }
                        let _ = SafeArrayUnaccessData(parray);
                    }
                }

                let elided = if count > 20 { ", ..." } else { "" };
                return format!("[{}{elided}]", elements.join(", "));
            }
            return format!("Array[{dims}D]");
        }

        match vt.0 {
            0 => "Empty".to_string(), // VT_EMPTY
            1 => "Null".to_string(),  // VT_NULL
            2 => format!("{val}", val = variant.Anonymous.Anonymous.Anonymous.iVal), // VT_I2
            3 => format!("{val}", val = variant.Anonymous.Anonymous.Anonymous.lVal), // VT_I4
            4 => format!(
                "{val:.2}",
                val = variant.Anonymous.Anonymous.Anonymous.fltVal
            ), // VT_R4
            5 => format!(
                "{val:.2}",
                val = variant.Anonymous.Anonymous.Anonymous.dblVal
            ), // VT_R8
            6 => {
                // VT_CY - currency, 64-bit fixed-point scaled by 10,000
                let raw = variant.Anonymous.Anonymous.Anonymous.cyVal.int64;
                let whole = raw / 10_000;
                let frac = (raw % 10_000).unsigned_abs();
                format!("{whole}.{frac:04}")
            }
            7 => {
                // VT_DATE - OLE Automation date (f64, day 0 = 1899-12-30)
                let ole_date = variant.Anonymous.Anonymous.Anonymous.date;
                ole_date_to_string(ole_date)
            }
            8 => {
                // VT_BSTR - string
                let bstr = &variant.Anonymous.Anonymous.Anonymous.bstrVal;
                if bstr.is_empty() {
                    "\"\"".to_string()
                } else {
                    format!("\"{}\"", **bstr)
                }
            }
            10 => {
                // VT_ERROR - contains an HRESULT status code
                let scode = variant.Anonymous.Anonymous.Anonymous.scode;
                let hr = windows::core::HRESULT(scode);
                let hex = format!("0x{:08X}", hr.0.cast_unsigned());
                match friendly_com_hresult_hint(hr) {
                    Some(msg) => format!("Error: {msg} ({hex})"),
                    None => format!("Error ({hex})"),
                }
            }
            11 => format!(
                "{val}",
                val = variant.Anonymous.Anonymous.Anonymous.boolVal.0 != 0
            ), // VT_BOOL
            16 => {
                #[allow(clippy::cast_possible_wrap)]
                let val = variant.Anonymous.Anonymous.Anonymous.bVal as i8;
                format!("{val}")
            } // VT_I1
            17 => format!("{val}", val = variant.Anonymous.Anonymous.Anonymous.bVal), // VT_UI1
            18 => format!("{val}", val = variant.Anonymous.Anonymous.Anonymous.uiVal), // VT_UI2
            19 => format!("{val}", val = variant.Anonymous.Anonymous.Anonymous.ulVal), // VT_UI4
            20 => {
                // VT_I8: read 8 bytes as i64 via pointer cast
                let p = (&raw const variant.Anonymous.Anonymous.Anonymous).cast::<i64>();
                // SAFETY: p is a valid pointer to the variant union
                let val = *p;
                format!("{val}")
            }
            21 => {
                // VT_UI8: read 8 bytes as u64 via pointer cast
                let p = (&raw const variant.Anonymous.Anonymous.Anonymous).cast::<u64>();
                // SAFETY: p is a valid pointer to the variant union
                let val = *p;
                format!("{val}")
            }
            _ => format!("(VT {vt:?})"),
        }
    }
}

/// Convert an OLE Automation date (f64) to a local datetime string.
/// OLE date epoch is 1899-12-30; integer part = days, fraction = time-of-day.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
fn ole_date_to_string(ole_date: f64) -> String {
    // OLE epoch: 1899-12-30 00:00:00
    const OLE_EPOCH_DAYS: i64 = 25569; // days from 1899-12-30 to 1970-01-01
    let total_secs = (ole_date - OLE_EPOCH_DAYS as f64) * 86400.0;
    chrono::DateTime::from_timestamp(total_secs as i64, 0).map_or_else(
        || format!("{ole_date:.6}"),
        |utc| {
            utc.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        },
    )
}

/// Convert a COM [`VARIANT`] into a strongly-typed [`OpcValue`].
#[allow(clippy::cast_possible_wrap)]
pub fn variant_to_opc_value(variant: &VARIANT) -> OpcValue {
    // SAFETY: Reading VARIANT union fields per vt discriminant.
    unsafe {
        let vt = variant.Anonymous.Anonymous.vt.0;
        match vt {
            0 => OpcValue::Empty,
            1 => OpcValue::Null,
            16 => {
                let val = (*variant.Anonymous.Anonymous).Anonymous.cVal;
                OpcValue::Int(i64::from(val))
            }
            17 => OpcValue::UInt(u64::from((*variant.Anonymous.Anonymous).Anonymous.bVal)),
            2 => OpcValue::Int(i64::from((*variant.Anonymous.Anonymous).Anonymous.iVal)),
            18 => OpcValue::UInt(u64::from((*variant.Anonymous.Anonymous).Anonymous.uiVal)),
            3 => OpcValue::Int(i64::from((*variant.Anonymous.Anonymous).Anonymous.lVal)),
            19 => OpcValue::UInt(u64::from((*variant.Anonymous.Anonymous).Anonymous.ulVal)),
            20 => OpcValue::Int((*variant.Anonymous.Anonymous).Anonymous.llVal),
            21 => OpcValue::UInt((*variant.Anonymous.Anonymous).Anonymous.ullVal),
            22 => OpcValue::Int(i64::from((*variant.Anonymous.Anonymous).Anonymous.intVal)),
            23 => OpcValue::UInt(u64::from((*variant.Anonymous.Anonymous).Anonymous.uintVal)),
            4 => OpcValue::Float(f64::from((*variant.Anonymous.Anonymous).Anonymous.fltVal)),
            5 => OpcValue::Float((*variant.Anonymous.Anonymous).Anonymous.dblVal),
            11 => OpcValue::Bool((*variant.Anonymous.Anonymous).Anonymous.boolVal.0 != 0),
            8 => {
                let bstr = &(*variant.Anonymous.Anonymous).Anonymous.bstrVal;
                OpcValue::String(bstr.to_string())
            }
            _ => OpcValue::String(variant_to_string(variant)),
        }
    }
}

/// Convert an [`OpcValue`] into a COM [`VARIANT`] for writing.
///
/// Implements adaptive integer coercion:
/// - If `OpcValue::Int(i)` fits in 32-bit signed bounds (`i32::MIN..=i32::MAX`), emit `VT_I4`.
/// - If `OpcValue::Int(i)` exceeds 32 bits, emit `VT_I8`.
/// - If `OpcValue::UInt(u)` fits in 32-bit unsigned bounds (`u32::MIN..=u32::MAX`), emit `VT_UI4`.
/// - If `OpcValue::UInt(u)` exceeds 32 bits, emit `VT_UI8`.
///
/// This guarantees 100% compatibility with classic 32-bit OPC DA 2.05a servers.
pub fn opc_value_to_variant(value: &OpcValue) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: We set the `vt` discriminant and the corresponding union field atomically.
    // SAFETY: The VARIANT is returned by value, so no aliasing. ManuallyDrop on BSTR prevents double-free.
    unsafe {
        match value {
            OpcValue::String(s) => {
                (*variant.Anonymous.Anonymous).vt = VT_BSTR;
                (*variant.Anonymous.Anonymous).Anonymous.bstrVal =
                    std::mem::ManuallyDrop::new(BSTR::from(s));
            }
            OpcValue::Int(i) => {
                if let Ok(i32_val) = i32::try_from(*i) {
                    (*variant.Anonymous.Anonymous).vt = VT_I4;
                    (*variant.Anonymous.Anonymous).Anonymous.lVal = i32_val;
                } else {
                    (*variant.Anonymous.Anonymous).vt = VT_I8;
                    (*variant.Anonymous.Anonymous).Anonymous.llVal = *i;
                }
            }
            OpcValue::UInt(u) => {
                if let Ok(u32_val) = u32::try_from(*u) {
                    (*variant.Anonymous.Anonymous).vt = VT_UI4;
                    (*variant.Anonymous.Anonymous).Anonymous.ulVal = u32_val;
                } else {
                    (*variant.Anonymous.Anonymous).vt = VT_UI8;
                    (*variant.Anonymous.Anonymous).Anonymous.ullVal = *u;
                }
            }
            OpcValue::Float(f) => {
                (*variant.Anonymous.Anonymous).vt = VT_R8;
                (*variant.Anonymous.Anonymous).Anonymous.dblVal = *f;
            }
            OpcValue::Bool(b) => {
                (*variant.Anonymous.Anonymous).vt = VT_BOOL;
                (*variant.Anonymous.Anonymous).Anonymous.boolVal =
                    VARIANT_BOOL(if *b { -1 } else { 0 });
            }
            OpcValue::Empty => {
                (*variant.Anonymous.Anonymous).vt = VT_EMPTY;
            }
            OpcValue::Null => {
                (*variant.Anonymous.Anonymous).vt = VT_NULL;
            }
        }
    }
    variant
}

/// RAII wrapper for a Win32 COM [`VARIANT`].
///
/// Ensures deterministic resource cleanup by calling [`windows::Win32::System::Variant::VariantClear`]
/// when dropped, freeing any contained OLE Automation resources (such as `BSTR` or `SAFEARRAY`).
///
/// Marked `#[repr(transparent)]` so that `&[ScopedVariant]` or `Vec<ScopedVariant>` has the exact
/// memory layout of `&[VARIANT]` or `Vec<VARIANT>`, allowing safe zero-copy pointer casting across FFI.
#[repr(transparent)]
#[derive(Debug)]
#[allow(dead_code)]
pub struct ScopedVariant(VARIANT);

#[allow(dead_code)]
impl ScopedVariant {
    /// Creates an empty [`ScopedVariant`] with `vt` initialized to `VT_EMPTY`.
    #[must_use]
    pub fn empty() -> Self {
        Self(VARIANT::default())
    }

    /// Converts an [`OpcValue`] into a [`ScopedVariant`].
    #[must_use]
    pub fn from_opc_value(value: &OpcValue) -> Self {
        Self(opc_value_to_variant(value))
    }

    /// Returns an immutable reference to the inner raw [`VARIANT`].
    #[must_use]
    pub const fn as_raw(&self) -> &VARIANT {
        &self.0
    }

    /// Returns a mutable reference to the inner raw [`VARIANT`].
    pub fn as_raw_mut(&mut self) -> &mut VARIANT {
        &mut self.0
    }

    /// Replaces the inner variant after safely clearing the existing one.
    pub fn replace(&mut self, new_val: VARIANT) {
        self.clear();
        self.0 = new_val;
    }

    /// Consumes the guard and extracts the inner raw [`VARIANT`] without running the destructor.
    #[must_use]
    pub fn into_inner(mut self) -> VARIANT {
        let inner = std::mem::take(&mut self.0);
        std::mem::forget(self);
        inner
    }

    /// Clears any allocated resources in the variant and resets its type to `VT_EMPTY`.
    pub fn clear(&mut self) {
        // SAFETY: `self.0` is a valid VARIANT; VariantClear releases BSTR/SAFEARRAY and resets vt to VT_EMPTY.
        unsafe {
            let _ = windows::Win32::System::Variant::VariantClear(&raw mut self.0);
        }
    }
}

impl From<VARIANT> for ScopedVariant {
    fn from(v: VARIANT) -> Self {
        Self(v)
    }
}

impl std::ops::Deref for ScopedVariant {
    type Target = VARIANT;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ScopedVariant {
    fn drop(&mut self) {
        self.clear();
    }
}

/// RAII guard for an array of COM-allocated [`tagOPCITEMSTATE`].
///
/// Ensures [`windows::Win32::System::Variant::VariantClear`] is deterministically called
/// on every item's `vDataValue` when the guard drops IF AND ONLY IF `errors[i].is_ok()` is true,
/// preventing OLE Automation heap leaks for `BSTR` or `SAFEARRAY` data while guaranteeing
/// safety against uninitialized COM memory on failed reads.
pub struct ItemStatesGuard<'a> {
    states: &'a mut [crate::raw::bindings::da::tagOPCITEMSTATE],
    errors: &'a [windows::core::HRESULT],
}

impl<'a> ItemStatesGuard<'a> {
    /// Creates a new `ItemStatesGuard` wrapping the states slice and the COM errors slice.
    ///
    /// This constructor is infallible to prevent leaking COM variants if construction panics.
    #[must_use]
    pub fn new(
        states: &'a mut [crate::raw::bindings::da::tagOPCITEMSTATE],
        errors: &'a [windows::core::HRESULT],
    ) -> Self {
        Self { states, errors }
    }

    /// Validates that both slices match the expected batch length.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError::Internal`] if lengths do not match `expected_len`.
    pub fn validate_lengths(&self, expected_len: usize) -> crate::errors::OpcResult<()> {
        if self.states.len() != expected_len || self.errors.len() != expected_len {
            return Err(crate::errors::OpcError::Internal(format!(
                "Item states guard length mismatch: expected {expected_len}, got states={}, errors={}",
                self.states.len(),
                self.errors.len()
            )));
        }
        Ok(())
    }
}

impl std::fmt::Debug for ItemStatesGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemStatesGuard")
            .field("len", &self.states.len())
            .finish()
    }
}

impl std::ops::Deref for ItemStatesGuard<'_> {
    type Target = [crate::raw::bindings::da::tagOPCITEMSTATE];

    fn deref(&self) -> &Self::Target {
        self.states
    }
}

impl Drop for ItemStatesGuard<'_> {
    fn drop(&mut self) {
        let cleanup_len = self.states.len().min(self.errors.len());
        for i in 0..cleanup_len {
            if self.errors[i].is_ok() {
                /* SAFETY: Invariant REV-01: COM leaves vDataValue uninitialized if the read fails.
                VariantClear MUST only be invoked if the item read succeeded (err.is_ok()). */
                unsafe {
                    let _ = windows::Win32::System::Variant::VariantClear(
                        &raw mut self.states[i].vDataValue,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_char_pattern,
        clippy::cast_possible_wrap,
        clippy::ptr_as_ptr,
        clippy::borrow_as_ptr,
        clippy::mixed_attributes_style,
        clippy::unreadable_literal,
        clippy::undocumented_unsafe_blocks
    )]
    use super::*;

    #[test]
    fn test_opc_value_to_variant_int() {
        let v = opc_value_to_variant(&OpcValue::Int(42));
        unsafe {
            assert_eq!(v.Anonymous.Anonymous.vt, VT_I4);
            assert_eq!(v.Anonymous.Anonymous.Anonymous.lVal, 42);
        }
    }

    #[test]
    fn test_opc_value_to_variant_float() {
        let v = opc_value_to_variant(&OpcValue::Float(3.5));
        unsafe {
            assert_eq!(v.Anonymous.Anonymous.vt, VT_R8);
            assert!((v.Anonymous.Anonymous.Anonymous.dblVal - 3.5).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_opc_value_to_variant_bool_true() {
        let v = opc_value_to_variant(&OpcValue::Bool(true));
        unsafe {
            assert_eq!(v.Anonymous.Anonymous.vt, VT_BOOL);
            assert_eq!(v.Anonymous.Anonymous.Anonymous.boolVal.0, -1);
        }
    }

    #[test]
    fn test_opc_value_to_variant_bool_false() {
        let v = opc_value_to_variant(&OpcValue::Bool(false));
        unsafe {
            assert_eq!(v.Anonymous.Anonymous.vt, VT_BOOL);
            assert_eq!(v.Anonymous.Anonymous.Anonymous.boolVal.0, 0);
        }
    }

    #[test]
    fn test_opc_value_to_variant_string() {
        let v = opc_value_to_variant(&OpcValue::String("hello".into()));
        unsafe {
            assert_eq!(v.Anonymous.Anonymous.vt, VT_BSTR);
            let bstr = &v.Anonymous.Anonymous.Anonymous.bstrVal;
            assert_eq!(&**bstr, "hello");
        }
    }

    #[test]
    fn test_variant_roundtrip() {
        // Int roundtrip
        let v = opc_value_to_variant(&OpcValue::Int(99));
        assert_eq!(variant_to_string(&v), "99");

        // Float roundtrip
        let v = opc_value_to_variant(&OpcValue::Float(3.5));
        assert_eq!(variant_to_string(&v), "3.50");

        // Bool true roundtrip
        let v = opc_value_to_variant(&OpcValue::Bool(true));
        assert_eq!(variant_to_string(&v), "true");

        // Bool false roundtrip
        let v = opc_value_to_variant(&OpcValue::Bool(false));
        assert_eq!(variant_to_string(&v), "false");

        // String roundtrip
        let v = opc_value_to_variant(&OpcValue::String("world".into()));
        assert_eq!(variant_to_string(&v), "\"world\"");

        // Empty roundtrip
        let v = opc_value_to_variant(&OpcValue::Empty);
        assert_eq!(variant_to_opc_value(&v), OpcValue::Empty);

        // Null roundtrip
        let v = opc_value_to_variant(&OpcValue::Null);
        assert_eq!(variant_to_opc_value(&v), OpcValue::Null);
    }

    #[test]
    fn test_variant_to_string_cy() {
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Com::CY;
        use windows::Win32::System::Variant::{
            VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_CY,
        };

        {
            let cy_val = CY { int64: 123_456_789 };
            let inner_union = VARIANT_0_0_0 { cyVal: cy_val };
            let middle_struct = VARIANT_0_0 {
                vt: VT_CY,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: inner_union,
            };
            let outer_union = VARIANT_0 {
                Anonymous: ManuallyDrop::new(middle_struct),
            };
            let v = VARIANT {
                Anonymous: outer_union,
            };
            assert_eq!(variant_to_string(&v), "12345.6789");
        }

        {
            let cy_val = CY { int64: -500_001 };
            let inner_union = VARIANT_0_0_0 { cyVal: cy_val };
            let middle_struct = VARIANT_0_0 {
                vt: VT_CY,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: inner_union,
            };
            let outer_union = VARIANT_0 {
                Anonymous: ManuallyDrop::new(middle_struct),
            };
            let v = VARIANT {
                Anonymous: outer_union,
            };
            assert_eq!(variant_to_string(&v), "-50.0001");
        }
    }

    #[test]
    fn test_variant_to_string_empty() {
        let v = VARIANT::default();
        assert_eq!(variant_to_string(&v), "Empty");
    }

    #[test]
    fn test_variant_to_string_null() {
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Variant::{
            VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_NULL,
        };

        let inner = VARIANT_0_0_0 { llVal: 0 };
        let middle = VARIANT_0_0 {
            vt: VT_NULL,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: inner,
        };
        let outer = VARIANT_0 {
            Anonymous: ManuallyDrop::new(middle),
        };
        let v = VARIANT { Anonymous: outer };
        assert_eq!(variant_to_string(&v), "Null");
    }

    #[test]
    fn test_variant_to_string_i2_and_r4() {
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Variant::{
            VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I2, VT_R4,
        };

        // VT_I2
        let inner = VARIANT_0_0_0 { iVal: -42 };
        let middle = VARIANT_0_0 {
            vt: VT_I2,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: inner,
        };
        let outer = VARIANT_0 {
            Anonymous: ManuallyDrop::new(middle),
        };
        let v = VARIANT { Anonymous: outer };
        assert_eq!(variant_to_string(&v), "-42");

        // VT_R4
        let inner = VARIANT_0_0_0 { fltVal: 1.5 };
        let middle = VARIANT_0_0 {
            vt: VT_R4,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: inner,
        };
        let outer = VARIANT_0 {
            Anonymous: ManuallyDrop::new(middle),
        };
        let v = VARIANT { Anonymous: outer };
        assert_eq!(variant_to_string(&v), "1.50");
    }

    #[test]
    fn test_variant_to_string_unknown_vt() {
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Variant::{
            VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0,
        };

        let inner = VARIANT_0_0_0 { llVal: 0 };
        let middle = VARIANT_0_0 {
            vt: VARENUM(999),
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: inner,
        };
        let outer = VARIANT_0 {
            Anonymous: ManuallyDrop::new(middle),
        };
        let v = VARIANT { Anonymous: outer };
        let result = variant_to_string(&v);
        assert!(
            result.starts_with("(VT "),
            "Expected '(VT ...)' but got: {result}"
        );
    }

    #[test]
    fn test_variant_to_string_safearray_i4() {
        use std::ffi::c_void;
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Ole::{
            SafeArrayAccessData, SafeArrayCreateVector, SafeArrayUnaccessData,
        };
        use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VT_ARRAY, VT_I4};

        unsafe {
            let parray = SafeArrayCreateVector(VT_I4, 0, 3);
            let mut ptr: *mut c_void = std::ptr::null_mut();
            SafeArrayAccessData(parray, &raw mut ptr).unwrap();
            let slice = std::slice::from_raw_parts_mut(ptr.cast::<i32>(), 3);
            slice[0] = 10;
            slice[1] = 20;
            slice[2] = 30;
            SafeArrayUnaccessData(parray).unwrap();

            let mut middle = VARIANT_0_0 {
                vt: windows::Win32::System::Variant::VARENUM(VT_I4.0 | VT_ARRAY.0),
                ..Default::default()
            };
            middle.Anonymous.parray = parray;

            let v = VARIANT {
                Anonymous: VARIANT_0 {
                    Anonymous: ManuallyDrop::new(middle),
                },
            };

            assert_eq!(variant_to_string(&v), "[10, 20, 30]");
        }
    }

    #[test]
    fn test_variant_to_string_vt_error_known() {
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Variant::{
            VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0,
        };

        // 0xC0040007 is OPC_E_UNKNOWNITEMID
        let inner = VARIANT_0_0_0 {
            scode: -1_073_479_673,
        }; // 0xC0040007 as i32
        let middle = VARIANT_0_0 {
            vt: VARENUM(10), // VT_ERROR
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: inner,
        };
        let outer = VARIANT_0 {
            Anonymous: ManuallyDrop::new(middle),
        };
        let v = VARIANT { Anonymous: outer };

        assert_eq!(
            variant_to_string(&v),
            "Error: Item ID not found in server address space (OPC_E_UNKNOWNITEMID) (0xC0040007)"
        );
    }

    #[test]
    fn test_variant_to_string_vt_error_unknown() {
        use std::mem::ManuallyDrop;
        use windows::Win32::System::Variant::{
            VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0,
        };

        let inner = VARIANT_0_0_0 {
            scode: -559_038_737,
        }; // 0xDEADBEEF as i32
        let middle = VARIANT_0_0 {
            vt: VARENUM(10), // VT_ERROR
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: inner,
        };
        let outer = VARIANT_0 {
            Anonymous: ManuallyDrop::new(middle),
        };
        let v = VARIANT { Anonymous: outer };

        assert_eq!(variant_to_string(&v), "Error (0xDEADBEEF)");
    }

    #[test]
    fn test_scoped_variant_drop_clears_bstr_and_resets_vt() {
        let raw = opc_value_to_variant(&OpcValue::String("verification_test_bstr".into()));
        let mut scoped = ScopedVariant::from(raw);
        unsafe {
            assert_eq!(scoped.as_raw().Anonymous.Anonymous.vt, VT_BSTR);
        }
        scoped.clear();
        unsafe {
            assert_eq!(scoped.as_raw().Anonymous.Anonymous.vt, VT_EMPTY);
        }
    }

    #[test]
    fn test_scoped_variant_into_inner_disarm() {
        let scoped = ScopedVariant::from_opc_value(&OpcValue::Int(42));
        let mut raw = scoped.into_inner();
        unsafe {
            assert_eq!(raw.Anonymous.Anonymous.vt, VT_I4);
            let _ = windows::Win32::System::Variant::VariantClear(&mut raw);
        }
    }

    #[test]
    fn test_item_states_guard_drop_clears_variants() {
        use crate::raw::bindings::da::tagOPCITEMSTATE;

        let mut states = vec![
            tagOPCITEMSTATE {
                hClient: 1,
                vDataValue: opc_value_to_variant(&OpcValue::String("guard_test_bstr".into())),
                ..Default::default()
            },
            tagOPCITEMSTATE {
                hClient: 2,
                vDataValue: VARIANT::default(),
                ..Default::default()
            },
        ];

        unsafe {
            assert_eq!(states[0].vDataValue.Anonymous.Anonymous.vt, VT_BSTR);
        }

        let errors = [windows::core::HRESULT(0), windows::core::HRESULT(0)];
        {
            let _guard = ItemStatesGuard::new(&mut states, &errors);
        }

        unsafe {
            assert_eq!(states[0].vDataValue.Anonymous.Anonymous.vt, VT_EMPTY);
            assert_eq!(states[1].vDataValue.Anonymous.Anonymous.vt, VT_EMPTY);
        }
    }

    #[test]
    fn test_scoped_variant_empty_and_as_raw_mut() {
        let mut scoped = ScopedVariant::empty();
        unsafe {
            assert_eq!(scoped.as_raw().Anonymous.Anonymous.vt, VT_EMPTY);
            (*scoped.as_raw_mut().Anonymous.Anonymous).vt = VT_I4;
            assert_eq!(scoped.as_raw().Anonymous.Anonymous.vt, VT_I4);
        }
    }

    #[test]
    fn test_item_states_guard_partial_failure_s_false_uninitialized_safety() {
        use windows::Win32::Foundation::{E_FAIL, S_OK};
        use windows::Win32::System::Variant::{VT_BSTR, VT_DISPATCH};
        use windows::core::HRESULT;

        let mut states: [crate::raw::bindings::da::tagOPCITEMSTATE; 2] =
            unsafe { std::mem::zeroed() };
        let errors: [HRESULT; 2] = [S_OK, E_FAIL];

        unsafe {
            let bstr = windows::core::BSTR::from("ValidTagValue");
            (*states[0].vDataValue.Anonymous.Anonymous).vt = VT_BSTR;
            (*states[0].vDataValue.Anonymous.Anonymous)
                .Anonymous
                .bstrVal = std::mem::ManuallyDrop::new(bstr);

            // Poison bits in failed item: uninitialized VT_DISPATCH pointer
            (*states[1].vDataValue.Anonymous.Anonymous).vt = VT_DISPATCH;
            (*states[1].vDataValue.Anonymous.Anonymous)
                .Anonymous
                .punkVal = std::mem::ManuallyDrop::new(std::mem::transmute::<
                usize,
                Option<windows::core::IUnknown>,
            >(0xDEAD_BEEF));
        }

        {
            let _guard = crate::com::variant::ItemStatesGuard::new(&mut states, &errors);
        }

        assert_eq!(unsafe { states[0].vDataValue.Anonymous.Anonymous.vt.0 }, 0); // Cleared to VT_EMPTY
        assert_eq!(
            unsafe { states[1].vDataValue.Anonymous.Anonymous.vt.0 },
            VT_DISPATCH.0
        ); // Skipped because error was E_FAIL

        // Reset poison bits to safe empty so test stack unwinding doesn't trigger spurious runtime checks
        unsafe {
            (*states[1].vDataValue.Anonymous.Anonymous).vt = VT_EMPTY;
            (*states[1].vDataValue.Anonymous.Anonymous)
                .Anonymous
                .punkVal = std::mem::ManuallyDrop::new(None);
        }
    }

    #[test]
    fn test_safearray_unpacking_buffer_bounds_canary_protection() {
        #[repr(C)]
        struct CanaryBuffer {
            before: [u8; 16],
            var: windows::Win32::System::Variant::VARIANT,
            after: [u8; 16],
        }
        let mut buf = CanaryBuffer {
            before: [0xAA; 16],
            var: windows::Win32::System::Variant::VARIANT::default(),
            after: [0xBB; 16],
        };
        let union_cap = unsafe { std::mem::size_of_val(&(*buf.var.Anonymous.Anonymous).Anonymous) };
        let copy_len = (16usize).min(union_cap);
        let src = [0x55u8; 16];
        let dst = unsafe {
            std::ptr::addr_of_mut!((*buf.var.Anonymous.Anonymous).Anonymous).cast::<u8>()
        };
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst, copy_len);
        }
        assert_eq!(buf.before, [0xAA; 16]);
        assert_eq!(buf.after, [0xBB; 16]);
    }
}
