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

    /// Other arbitrary data conversion failure.
    #[error("Data conversion failed: {0}")]
    Other(String),
}

impl From<&str> for ConversionError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl From<String> for ConversionError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}
