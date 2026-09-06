//! OPC DA namespace browsing types and filters.

use crate::errors::OpcError;

/// OPC DA address space browse type — replaces raw u32 constants.
///
/// # Examples
///
/// ```
/// use opc_da_client::BrowseType;
///
/// let b = BrowseType::Branch;
/// assert_eq!(u32::from(b), 1);
/// assert_eq!(BrowseType::try_from(2).ok(), Some(BrowseType::Leaf));
/// ```
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseType {
    /// Browse branch nodes (directories / containers) within the namespace.
    Branch = 1,
    /// Browse leaf nodes (individual process tags) within the namespace.
    Leaf = 2,
    /// Browse flat unorganized namespace items.
    Flat = 3,
}

impl From<BrowseType> for u32 {
    #[inline]
    fn from(browse_type: BrowseType) -> Self {
        browse_type as Self
    }
}

impl TryFrom<u32> for BrowseType {
    type Error = OpcError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Branch),
            2 => Ok(Self::Leaf),
            3 => Ok(Self::Flat),
            _ => Err(OpcError::Conversion(format!("Invalid BrowseType: {value}"))),
        }
    }
}

/// OPC DA browse direction — replaces raw u32 constants.
///
/// # Examples
///
/// ```
/// use opc_da_client::BrowseDirection;
///
/// let dir = BrowseDirection::Up;
/// assert_eq!(u32::from(dir), 1);
/// assert_eq!(BrowseDirection::try_from(2).ok(), Some(BrowseDirection::Down));
/// ```
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseDirection {
    /// Navigate up to the parent branch.
    Up = 1,
    /// Navigate down into a child branch.
    Down = 2,
    /// Navigate to a specific named node (DA 3.0).
    To = 3,
}

impl From<BrowseDirection> for u32 {
    #[inline]
    fn from(dir: BrowseDirection) -> Self {
        dir as Self
    }
}

impl TryFrom<u32> for BrowseDirection {
    type Error = OpcError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Up),
            2 => Ok(Self::Down),
            3 => Ok(Self::To),
            _ => Err(OpcError::Conversion(format!(
                "Invalid BrowseDirection: {value}"
            ))),
        }
    }
}

const _: () = assert!(BrowseType::Branch as u32 == 1);
const _: () = assert!(BrowseType::Leaf as u32 == 2);
const _: () = assert!(BrowseType::Flat as u32 == 3);
const _: () = assert!(BrowseDirection::Up as u32 == 1);
const _: () = assert!(BrowseDirection::Down as u32 == 2);
const _: () = assert!(BrowseDirection::To as u32 == 3);

/// Granular filter for enumeration results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowseFilter {
    /// Enumerate all available nodes regardless of type.
    All,
    /// Enumerate only branch (container) nodes.
    Branches,
    /// Enumerate only leaf (tag item) nodes.
    Items,
}

/// Typology of the server's address space.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamespaceType {
    /// Hierarchical tree-structured namespace with nested branches and leaves (Win32 `OPC_NS_HIERARCHIAL` = 1).
    Hierarchy = 1,
    /// Flat namespace without hierarchical folders or branches (Win32 `OPC_NS_FLAT` = 2).
    Flat = 2,
}
