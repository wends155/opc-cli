//! Type conversion, discriminant parsing, and string formatting errors.

/// Errors occurring during data conversion, string parsing, or enum discriminant conversion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConversionError {
    /// Invalid browse type integer discriminant.
    #[error("Invalid browse type discriminant: {0}")]
    InvalidBrowseType(u32),

    /// Invalid browse direction integer discriminant.
    #[error("Invalid browse direction discriminant: {0}")]
    InvalidBrowseDirection(u32),

    /// Invalid server endpoint URI or identifier syntax.
    #[error("Invalid server endpoint: {0}")]
    InvalidEndpoint(String),

    /// Integer conversion out of range.
    #[error("Integer conversion failed: {0}")]
    IntConversion(#[from] std::num::TryFromIntError),

    /// Data type mismatch during value conversion.
    #[error("Type mismatch: cannot convert {actual} to {expected}")]
    TypeMismatch {
        /// Actual source value representation.
        actual: String,
        /// Expected target type name.
        expected: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion_error_typed_variants() {
        let browse_type_err = ConversionError::InvalidBrowseType(99);
        assert_eq!(
            browse_type_err.to_string(),
            "Invalid browse type discriminant: 99"
        );

        let browse_dir_err = ConversionError::InvalidBrowseDirection(12);
        assert_eq!(
            browse_dir_err.to_string(),
            "Invalid browse direction discriminant: 12"
        );

        let endpoint_err = ConversionError::InvalidEndpoint("opc://bad uri".to_string());
        assert_eq!(
            endpoint_err.to_string(),
            "Invalid server endpoint: opc://bad uri"
        );

        let mismatch_err = ConversionError::TypeMismatch {
            actual: "string".to_string(),
            expected: "i32",
        };
        assert_eq!(
            mismatch_err.to_string(),
            "Type mismatch: cannot convert string to i32"
        );
    }
}
