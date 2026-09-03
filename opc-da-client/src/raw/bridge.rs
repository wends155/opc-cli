//! Low-level COM and FFI bridge types.
//!
//! These structures provide FFI-safe representations and conversion traits
//! (`IntoBridge`, `TryToNative`, `TryFromNative`) for interacting directly with
//! Win32 COM interfaces (`tagOPCITEMDEF`, `tagOPCITEMSTATE`, `COSERVERINFO`, etc.).

#![allow(warnings)]
#![allow(clippy::all, clippy::pedantic, clippy::restriction)]

use crate::raw::bindings::da::*;
use crate::raw::memory::{
    IntoBridge, LocalPointer, RemoteArray, ToNative, TryFromNative, TryToNative,
};
use crate::try_from_native;
use crate::types::{
    BrowseFilter, BrowseType, ItemHandle, NamespaceType, ServerState, ServerStatus,
};

/// Parameters needed for the server to identify and initialize a tag within a group.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDef {
    /// Optional access path for the item (server-specific).
    pub access_path: String,
    /// The unique identifier of the tag within the server namespace.
    pub item_id: String,
    /// Whether the item should be added in an active state.
    pub active: bool,
    /// Handle assigned by the client for this item.
    pub client_handle: ItemHandle,
    /// Requested canonical data type (0 for server default).
    pub data_type: u16,
    /// Optional opaque blob for the item.
    pub blob: Vec<u8>,
}

/// FFI-safe bridge struct for [`ItemDef`].
pub struct ItemDefBridge {
    pub access_path: LocalPointer<Vec<u16>>,
    pub item_id: LocalPointer<Vec<u16>>,
    pub active: bool,
    pub item_client_handle: u32,
    pub requested_data_type: u16,
    pub blob: LocalPointer<Vec<u8>>,
}

impl IntoBridge<ItemDefBridge> for ItemDef {
    fn into_bridge(self) -> ItemDefBridge {
        ItemDefBridge {
            access_path: LocalPointer::from(&self.access_path),
            item_id: LocalPointer::from(&self.item_id),
            active: self.active,
            item_client_handle: self.client_handle.0,
            requested_data_type: self.data_type,
            blob: LocalPointer::new(Some(self.blob)),
        }
    }
}

impl TryToNative<tagOPCITEMDEF> for ItemDefBridge {
    fn try_to_native(&self) -> windows::core::Result<tagOPCITEMDEF> {
        Ok(tagOPCITEMDEF {
            szAccessPath: self.access_path.as_pwstr(),
            szItemID: self.item_id.as_pwstr(),
            bActive: self.active.into(),
            hClient: self.item_client_handle,
            vtRequestedDataType: self.requested_data_type,
            dwBlobSize: self.blob.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Blob size exceeds u32 maximum value",
                )
            })?,
            pBlob: self.blob.as_array_ptr().cast_mut().cast(),
            wReserved: 0,
        })
    }
}

/// Result properties of an item after being added to a group.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemResult {
    /// Handle assigned by the server for this item.
    pub server_handle: ItemHandle,
    /// The actual canonical data type supported by the server for this item.
    pub data_type: u16,
    /// Access rights for this item (read/write permissions).
    pub access_rights: u32,
    /// Optional opaque blob returned by the server.
    pub blob: Vec<u8>,
}

impl TryFromNative<tagOPCITEMRESULT> for ItemResult {
    fn try_from_native(native: &tagOPCITEMRESULT) -> windows::core::Result<Self> {
        Ok(Self {
            server_handle: ItemHandle(native.hServer),
            data_type: native.vtCanonicalDataType,
            access_rights: native.dwAccessRights,
            blob: RemoteArray::from_mut_ptr(native.pBlob, native.dwBlobSize)
                .as_slice()
                .to_vec(),
        })
    }
}

/// Scope for enumerating server items or connections.
#[derive(Debug, Clone, PartialEq)]
pub enum EnumScope {
    PrivateConnections,
    PublicConnections,
    AllConnections,
    Public,
    Private,
    All,
}

impl TryFromNative<tagOPCENUMSCOPE> for EnumScope {
    fn try_from_native(native: &tagOPCENUMSCOPE) -> windows::core::Result<Self> {
        match *native {
            OPC_ENUM_PRIVATE_CONNECTIONS => Ok(EnumScope::PrivateConnections),
            OPC_ENUM_PUBLIC_CONNECTIONS => Ok(EnumScope::PublicConnections),
            OPC_ENUM_ALL_CONNECTIONS => Ok(EnumScope::AllConnections),
            OPC_ENUM_PUBLIC => Ok(EnumScope::Public),
            OPC_ENUM_PRIVATE => Ok(EnumScope::Private),
            OPC_ENUM_ALL => Ok(EnumScope::All),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown enum scope: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<tagOPCENUMSCOPE> for EnumScope {
    fn to_native(&self) -> tagOPCENUMSCOPE {
        match self {
            EnumScope::PrivateConnections => OPC_ENUM_PRIVATE_CONNECTIONS,
            EnumScope::PublicConnections => OPC_ENUM_PUBLIC_CONNECTIONS,
            EnumScope::AllConnections => OPC_ENUM_ALL_CONNECTIONS,
            EnumScope::Public => OPC_ENUM_PUBLIC,
            EnumScope::Private => OPC_ENUM_PRIVATE,
            EnumScope::All => OPC_ENUM_ALL,
        }
    }
}

/// Full attribute set of a single OPC item.
pub struct ItemAttributes {
    pub access_path: String,
    pub item_id: String,
    pub active: bool,
    pub client_handle: ItemHandle,
    pub server_handle: ItemHandle,
    pub access_rights: u32,
    pub blob: Vec<u8>,
    pub requested_data_type: u16,
    pub canonical_data_type: u16,
    pub eu_type: EuType,
    pub eu_info: windows::Win32::System::Variant::VARIANT,
}

impl TryFromNative<tagOPCITEMATTRIBUTES> for ItemAttributes {
    fn try_from_native(native: &tagOPCITEMATTRIBUTES) -> windows::core::Result<Self> {
        Ok(Self {
            access_path: try_from_native!(&native.szAccessPath),
            item_id: try_from_native!(&native.szItemID),
            active: native.bActive.into(),
            client_handle: ItemHandle(native.hClient),
            server_handle: ItemHandle(native.hServer),
            access_rights: native.dwAccessRights,
            blob: RemoteArray::from_mut_ptr(native.pBlob, native.dwBlobSize)
                .as_slice()
                .to_vec(),
            requested_data_type: native.vtRequestedDataType,
            canonical_data_type: native.vtCanonicalDataType,
            eu_type: try_from_native!(&native.dwEUType),
            eu_info: native.vEUInfo.clone(),
        })
    }
}

/// Engineering Units (EU) classification type.
pub enum EuType {
    NoEnum,
    Analog,
    Enumerated,
}

impl TryFromNative<tagOPCEUTYPE> for EuType {
    fn try_from_native(native: &tagOPCEUTYPE) -> windows::core::Result<Self> {
        match *native {
            OPC_NOENUM => Ok(EuType::NoEnum),
            OPC_ANALOG => Ok(EuType::Analog),
            OPC_ENUMERATED => Ok(EuType::Enumerated),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown EU type: {unknown:?}"),
            )),
        }
    }
}

/// Current state of a watched OPC item including value, quality, and time.
pub struct ItemState {
    pub client_handle: ItemHandle,
    pub timestamp: std::time::SystemTime,
    pub quality: u16,
    pub data_value: windows::Win32::System::Variant::VARIANT,
}

impl TryFromNative<tagOPCITEMSTATE> for ItemState {
    fn try_from_native(native: &tagOPCITEMSTATE) -> windows::core::Result<Self> {
        Ok(Self {
            client_handle: ItemHandle(native.hClient),
            timestamp: try_from_native!(&native.ftTimeStamp),
            quality: native.wQuality,
            data_value: native.vDataValue.clone(),
        })
    }
}

/// Reading source preference (Cache or Device).
pub enum DataSourceTarget {
    ForceCache,
    ForceDevice,
    WithMaxAge(u32),
}

impl DataSourceTarget {
    #[must_use]
    pub fn max_age(&self) -> u32 {
        match self {
            DataSourceTarget::WithMaxAge(max_age) => *max_age,
            DataSourceTarget::ForceCache => u32::MAX,
            DataSourceTarget::ForceDevice => 0,
        }
    }
}

impl TryFromNative<tagOPCDATASOURCE> for DataSourceTarget {
    fn try_from_native(native: &tagOPCDATASOURCE) -> windows::core::Result<Self> {
        match *native {
            OPC_DS_CACHE => Ok(DataSourceTarget::ForceCache),
            OPC_DS_DEVICE => Ok(DataSourceTarget::ForceDevice),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown data source: {unknown:?}"),
            )),
        }
    }
}

impl TryToNative<tagOPCDATASOURCE> for DataSourceTarget {
    fn try_to_native(&self) -> windows::core::Result<tagOPCDATASOURCE> {
        match self {
            DataSourceTarget::ForceCache => Ok(OPC_DS_CACHE),
            DataSourceTarget::ForceDevice => Ok(OPC_DS_DEVICE),
            DataSourceTarget::WithMaxAge(_) => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "MaxAge data source requires a value",
            )),
        }
    }
}

/// Full read value result carrying value, quality, and timestamp.
pub struct ItemValue {
    pub value: windows::Win32::System::Variant::VARIANT,
    pub quality: u16,
    pub timestamp: std::time::SystemTime,
}

impl
    TryFromNative<(
        RemoteArray<windows::Win32::System::Variant::VARIANT>,
        RemoteArray<u16>,
        RemoteArray<windows::Win32::Foundation::FILETIME>,
        RemoteArray<windows::core::HRESULT>,
    )> for Vec<windows::core::Result<ItemValue>>
{
    fn try_from_native(
        native: &(
            RemoteArray<windows::Win32::System::Variant::VARIANT>,
            RemoteArray<u16>,
            RemoteArray<windows::Win32::Foundation::FILETIME>,
            RemoteArray<windows::core::HRESULT>,
        ),
    ) -> windows::core::Result<Self> {
        let (values, qualities, timestamps, errors) = native;

        if values.len() != qualities.len()
            || values.len() != timestamps.len()
            || values.len() != errors.len()
        {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                "Arrays have different lengths",
            ));
        }

        Ok(values
            .as_slice()
            .iter()
            .zip(qualities.as_slice())
            .zip(timestamps.as_slice())
            .zip(errors.as_slice())
            .map(|(((value, quality), timestamp), error)| {
                if error.is_ok() {
                    Ok(ItemValue {
                        value: value.clone(),
                        quality: *quality,
                        timestamp: try_from_native!(timestamp),
                    })
                } else {
                    Err((*error).into())
                }
            })
            .collect())
    }
}

/// Item value struct for writes or partial updates.
pub struct ItemPartialValue {
    pub value: windows::Win32::System::Variant::VARIANT,
    pub quality: Option<u16>,
    pub timestamp: Option<std::time::SystemTime>,
}

impl TryToNative<tagOPCITEMVQT> for ItemPartialValue {
    fn try_to_native(&self) -> windows::core::Result<tagOPCITEMVQT> {
        Ok(tagOPCITEMVQT {
            vDataValue: self.value.clone(),
            bQualitySpecified: self.quality.is_some().into(),
            wQuality: self.quality.unwrap_or_default(),
            bTimeStampSpecified: self.timestamp.is_some().into(),
            ftTimeStamp: self
                .timestamp
                .map(|t| t.try_to_native())
                .transpose()?
                .unwrap_or_default(),
            wReserved: 0,
            dwReserved: 0,
        })
    }
}

/// Information defining how to connect to a remote server.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerInfo {
    pub name: String,
    pub auth_info: AuthInfo,
}

/// FFI-safe bridge for [`ServerInfo`] (`COSERVERINFO`).
pub struct ServerInfoBridge {
    pub name: LocalPointer<Vec<u16>>,
    pub auth_info: AuthInfoBridge,
}

impl IntoBridge<ServerInfoBridge> for ServerInfo {
    fn into_bridge(self) -> ServerInfoBridge {
        ServerInfoBridge {
            name: LocalPointer::from(&self.name),
            auth_info: self.auth_info.into_bridge(),
        }
    }
}

impl TryToNative<windows::Win32::System::Com::COSERVERINFO> for ServerInfoBridge {
    fn try_to_native(&self) -> windows::core::Result<windows::Win32::System::Com::COSERVERINFO> {
        let auth_native = self.auth_info.try_to_native()?;
        Ok(windows::Win32::System::Com::COSERVERINFO {
            dwReserved1: 0,
            dwReserved2: 0,
            pwszName: self.name.as_pwstr(),
            pAuthInfo: Box::into_raw(Box::new(auth_native)),
        })
    }
}

/// Authentication and authorization settings for DCOM.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthInfo {
    pub authn_svc: u32,
    pub authz_svc: u32,
    pub server_principal_name: String,
    pub authn_level: u32,
    pub impersonation_level: u32,
    pub auth_identity_data: AuthIdentity,
    pub capabilities: u32,
}

/// FFI-safe bridge for [`AuthInfo`] (`COAUTHINFO`).
pub struct AuthInfoBridge {
    pub authn_svc: u32,
    pub authz_svc: u32,
    pub server_principal_name: LocalPointer<Vec<u16>>,
    pub authn_level: u32,
    pub impersonation_level: u32,
    pub auth_identity_data: AuthIdentityBridge,
    pub capabilities: u32,
}

impl IntoBridge<AuthInfoBridge> for AuthInfo {
    fn into_bridge(self) -> AuthInfoBridge {
        AuthInfoBridge {
            authn_svc: self.authn_svc,
            authz_svc: self.authz_svc,
            server_principal_name: LocalPointer::from(&self.server_principal_name),
            authn_level: self.authn_level,
            impersonation_level: self.impersonation_level,
            auth_identity_data: self.auth_identity_data.into_bridge(),
            capabilities: self.capabilities,
        }
    }
}

impl TryToNative<windows::Win32::System::Com::COAUTHINFO> for AuthInfoBridge {
    fn try_to_native(&self) -> windows::core::Result<windows::Win32::System::Com::COAUTHINFO> {
        let ident_native = self.auth_identity_data.try_to_native()?;
        Ok(windows::Win32::System::Com::COAUTHINFO {
            dwAuthnSvc: self.authn_svc,
            dwAuthzSvc: self.authz_svc,
            pwszServerPrincName: self.server_principal_name.as_pwstr(),
            dwAuthnLevel: self.authn_level,
            dwImpersonationLevel: self.impersonation_level,
            pAuthIdentityData: Box::into_raw(Box::new(ident_native)).cast(),
            dwCapabilities: self.capabilities,
        })
    }
}

/// DCOM authentication credentials.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthIdentity {
    pub user: String,
    pub domain: String,
    pub password: String,
    pub flags: u32,
}

/// FFI-safe bridge for [`AuthIdentity`] (`COAUTHIDENTITY`).
pub struct AuthIdentityBridge {
    pub user: LocalPointer<Vec<u16>>,
    pub domain: LocalPointer<Vec<u16>>,
    pub password: LocalPointer<Vec<u16>>,
    pub flags: u32,
}

impl IntoBridge<AuthIdentityBridge> for AuthIdentity {
    fn into_bridge(self) -> AuthIdentityBridge {
        AuthIdentityBridge {
            user: LocalPointer::from(&self.user),
            domain: LocalPointer::from(&self.domain),
            password: LocalPointer::from(&self.password),
            flags: self.flags,
        }
    }
}

impl TryToNative<windows::Win32::System::Com::COAUTHIDENTITY> for AuthIdentityBridge {
    fn try_to_native(&self) -> windows::core::Result<windows::Win32::System::Com::COAUTHIDENTITY> {
        Ok(windows::Win32::System::Com::COAUTHIDENTITY {
            User: self.user.as_pwstr().0,
            UserLength: self.user.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "User name exceeds u32 maximum length",
                )
            })?,
            Domain: self.domain.as_pwstr().0,
            DomainLength: self.domain.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Domain name exceeds u32 maximum length",
                )
            })?,
            Password: self.password.as_pwstr().0,
            PasswordLength: self.password.len().try_into().map_err(|_| {
                windows::core::Error::new(
                    windows::Win32::Foundation::E_INVALIDARG,
                    "Password exceeds u32 maximum length",
                )
            })?,
            Flags: self.flags,
        })
    }
}

/// COM instantiation context flags (`CLSCTX`).
#[derive(Debug, Clone, PartialEq)]
pub enum ClassContext {
    All,
    InProcServer,
    InProcHandler,
    LocalServer,
    InProcServer16,
    RemoteServer,
    InProcHandler16,
    NoCodeDownload,
    NoCustomMarshal,
    EnableCodeDownload,
    NoFailureLog,
    DisableAAA,
    EnableAAA,
    FromDefaultContext,
    ActivateX86Server,
    Activate32BitServer,
    Activate64BitServer,
    EnableCloaking,
    AppContainer,
    ActivateAAAAsIU,
    ActivateARM32Server,
    AllowLowerTrustRegistration,
    PsDll,
}

impl ToNative<windows::Win32::System::Com::CLSCTX> for ClassContext {
    fn to_native(&self) -> windows::Win32::System::Com::CLSCTX {
        match self {
            ClassContext::All => windows::Win32::System::Com::CLSCTX_ALL,
            ClassContext::InProcServer => windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            ClassContext::InProcHandler => windows::Win32::System::Com::CLSCTX_INPROC_HANDLER,
            ClassContext::LocalServer => windows::Win32::System::Com::CLSCTX_LOCAL_SERVER,
            ClassContext::InProcServer16 => windows::Win32::System::Com::CLSCTX_INPROC_SERVER16,
            ClassContext::RemoteServer => windows::Win32::System::Com::CLSCTX_REMOTE_SERVER,
            ClassContext::InProcHandler16 => windows::Win32::System::Com::CLSCTX_INPROC_HANDLER16,
            ClassContext::NoCodeDownload => windows::Win32::System::Com::CLSCTX_NO_CODE_DOWNLOAD,
            ClassContext::NoCustomMarshal => windows::Win32::System::Com::CLSCTX_NO_CUSTOM_MARSHAL,
            ClassContext::EnableCodeDownload => {
                windows::Win32::System::Com::CLSCTX_ENABLE_CODE_DOWNLOAD
            }
            ClassContext::NoFailureLog => windows::Win32::System::Com::CLSCTX_NO_FAILURE_LOG,
            ClassContext::DisableAAA => windows::Win32::System::Com::CLSCTX_DISABLE_AAA,
            ClassContext::EnableAAA => windows::Win32::System::Com::CLSCTX_ENABLE_AAA,
            ClassContext::FromDefaultContext => {
                windows::Win32::System::Com::CLSCTX_FROM_DEFAULT_CONTEXT
            }
            ClassContext::ActivateX86Server => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_X86_SERVER
            }
            ClassContext::Activate32BitServer => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_32_BIT_SERVER
            }
            ClassContext::Activate64BitServer => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_64_BIT_SERVER
            }
            ClassContext::EnableCloaking => windows::Win32::System::Com::CLSCTX_ENABLE_CLOAKING,
            ClassContext::AppContainer => windows::Win32::System::Com::CLSCTX_APPCONTAINER,
            ClassContext::ActivateAAAAsIU => windows::Win32::System::Com::CLSCTX_ACTIVATE_AAA_AS_IU,
            ClassContext::ActivateARM32Server => {
                windows::Win32::System::Com::CLSCTX_ACTIVATE_ARM32_SERVER
            }
            ClassContext::AllowLowerTrustRegistration => {
                windows::Win32::System::Com::CLSCTX_ALLOW_LOWER_TRUST_REGISTRATION
            }
            ClassContext::PsDll => windows::Win32::System::Com::CLSCTX_PS_DLL,
        }
    }
}

impl TryFromNative<tagOPCSERVERSTATUS> for ServerStatus {
    fn try_from_native(native: &tagOPCSERVERSTATUS) -> windows::core::Result<Self> {
        Ok(Self {
            start_time: try_from_native!(&native.ftStartTime),
            current_time: try_from_native!(&native.ftCurrentTime),
            last_update_time: try_from_native!(&native.ftLastUpdateTime),
            server_state: try_from_native!(&native.dwServerState),
            group_count: native.dwGroupCount,
            band_width: native.dwBandWidth,
            major_version: native.wMajorVersion,
            minor_version: native.wMinorVersion,
            build_number: native.wBuildNumber,
            vendor_info: try_from_native!(&native.szVendorInfo),
        })
    }
}

impl TryFromNative<tagOPCSERVERSTATE> for ServerState {
    fn try_from_native(native: &tagOPCSERVERSTATE) -> windows::core::Result<Self> {
        match *native {
            OPC_STATUS_RUNNING => Ok(ServerState::Running),
            OPC_STATUS_FAILED => Ok(ServerState::Failed),
            OPC_STATUS_NOCONFIG => Ok(ServerState::NoConfig),
            OPC_STATUS_SUSPENDED => Ok(ServerState::Suspended),
            OPC_STATUS_TEST => Ok(ServerState::Test),
            OPC_STATUS_COMM_FAULT => Ok(ServerState::CommunicationFault),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown server state: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<tagOPCSERVERSTATE> for ServerState {
    fn to_native(&self) -> tagOPCSERVERSTATE {
        match self {
            ServerState::Running => OPC_STATUS_RUNNING,
            ServerState::Failed => OPC_STATUS_FAILED,
            ServerState::NoConfig => OPC_STATUS_NOCONFIG,
            ServerState::Suspended => OPC_STATUS_SUSPENDED,
            ServerState::Test => OPC_STATUS_TEST,
            ServerState::CommunicationFault => OPC_STATUS_COMM_FAULT,
        }
    }
}

impl TryFromNative<tagOPCBROWSETYPE> for BrowseType {
    fn try_from_native(native: &tagOPCBROWSETYPE) -> windows::core::Result<Self> {
        match *native {
            OPC_BRANCH => Ok(BrowseType::Branch),
            OPC_LEAF => Ok(BrowseType::Leaf),
            OPC_FLAT => Ok(BrowseType::Flat),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown browse type: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<tagOPCBROWSETYPE> for BrowseType {
    fn to_native(&self) -> tagOPCBROWSETYPE {
        match self {
            BrowseType::Branch => OPC_BRANCH,
            BrowseType::Leaf => OPC_LEAF,
            BrowseType::Flat => OPC_FLAT,
        }
    }
}

impl TryFromNative<tagOPCBROWSEFILTER> for BrowseFilter {
    fn try_from_native(native: &tagOPCBROWSEFILTER) -> windows::core::Result<Self> {
        match *native {
            OPC_BROWSE_FILTER_ALL => Ok(BrowseFilter::All),
            OPC_BROWSE_FILTER_BRANCHES => Ok(BrowseFilter::Branches),
            OPC_BROWSE_FILTER_ITEMS => Ok(BrowseFilter::Items),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown browse filter: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<tagOPCBROWSEFILTER> for BrowseFilter {
    fn to_native(&self) -> tagOPCBROWSEFILTER {
        match self {
            BrowseFilter::All => OPC_BROWSE_FILTER_ALL,
            BrowseFilter::Branches => OPC_BROWSE_FILTER_BRANCHES,
            BrowseFilter::Items => OPC_BROWSE_FILTER_ITEMS,
        }
    }
}

impl TryFromNative<tagOPCNAMESPACETYPE> for NamespaceType {
    fn try_from_native(native: &tagOPCNAMESPACETYPE) -> windows::core::Result<Self> {
        match *native {
            OPC_NS_HIERARCHIAL => Ok(NamespaceType::Hierarchy),
            OPC_NS_FLAT => Ok(NamespaceType::Flat),
            unknown => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("Unknown namespace type: {unknown:?}"),
            )),
        }
    }
}

impl ToNative<tagOPCNAMESPACETYPE> for NamespaceType {
    fn to_native(&self) -> tagOPCNAMESPACETYPE {
        match self {
            NamespaceType::Hierarchy => OPC_NS_HIERARCHIAL,
            NamespaceType::Flat => OPC_NS_FLAT,
        }
    }
}
