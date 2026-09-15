//! Unit tests for domain types, quality conversions, and tag collections.

use super::*;
use crate::errors::OpcError;
use std::sync::Arc;
use std::time::SystemTime;

#[test]
fn test_namespace_type_discriminants() {
    assert_eq!(NamespaceType::Hierarchy as u32, 1);
    assert_eq!(NamespaceType::Flat as u32, 2);
}

#[test]
fn browse_type_from_roundtrip() {
    for (variant, expected) in [
        (BrowseType::Branch, 1u32),
        (BrowseType::Leaf, 2u32),
        (BrowseType::Flat, 3u32),
    ] {
        let raw: u32 = variant.into();
        assert_eq!(raw, expected);
        let back = BrowseType::try_from(raw).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn browse_type_try_from_rejects_invalid() {
    assert!(BrowseType::try_from(0u32).is_err());
    assert!(BrowseType::try_from(4u32).is_err());
    assert!(BrowseType::try_from(u32::MAX).is_err());
}

#[test]
fn browse_direction_from_roundtrip() {
    for (variant, expected) in [
        (BrowseDirection::Up, 1u32),
        (BrowseDirection::Down, 2u32),
        (BrowseDirection::To, 3u32),
    ] {
        let raw: u32 = variant.into();
        assert_eq!(raw, expected);
        let back = BrowseDirection::try_from(raw).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn browse_direction_try_from_rejects_invalid() {
    assert!(BrowseDirection::try_from(0u32).is_err());
    assert!(BrowseDirection::try_from(4u32).is_err());
    assert!(BrowseDirection::try_from(u32::MAX).is_err());
}

#[test]
fn test_opc_quality_good_standard() {
    let q = OpcQuality::from(0x00C0);
    assert_eq!(q.major(), QualityMajor::Good);
    assert_eq!(q.substatus(), QualitySubstatus::NonSpecific);
    assert_eq!(q.limit(), QualityLimit::NotLimited);
    assert_eq!(q.raw(), 0x00C0);
    assert!(q.is_good());
    assert!(!q.is_bad());
    assert!(!q.is_uncertain());
    assert!(!q.is_limited());
    assert_eq!(q.to_string(), "Good");
}

#[test]
fn test_opc_quality_good_local_override() {
    let q = OpcQuality::from(0x00D8);
    assert_eq!(q.major(), QualityMajor::Good);
    assert_eq!(q.substatus(), QualitySubstatus::LocalOverride);
    assert_eq!(q.limit(), QualityLimit::NotLimited);
    assert_eq!(q.to_string(), "Good (Local Override)");
}

#[test]
fn test_opc_quality_bad_comm_failure() {
    let q = OpcQuality::from(0x0018);
    assert_eq!(q.major(), QualityMajor::Bad);
    assert_eq!(q.substatus(), QualitySubstatus::CommFailure);
    assert_eq!(q.limit(), QualityLimit::NotLimited);
    assert!(q.is_bad());
    assert_eq!(q.to_string(), "Bad (Comm Failure)");
}

#[test]
fn test_opc_quality_uncertain_limits() {
    let q = OpcQuality::from(0x0056);
    assert_eq!(q.major(), QualityMajor::Uncertain);
    assert_eq!(q.substatus(), QualitySubstatus::EguExceeded);
    assert_eq!(q.limit(), QualityLimit::HighLimited);
    assert!(q.is_uncertain());
    assert!(q.is_limited());
    assert_eq!(q.to_string(), "Uncertain (EGU Exceeded) [High Limited]");
}

#[test]
fn test_opc_quality_new_constructor() {
    let q = OpcQuality::new(
        QualityMajor::Uncertain,
        QualitySubstatus::EguExceeded,
        QualityLimit::HighLimited,
    );
    assert_eq!(q.major(), QualityMajor::Uncertain);
    assert_eq!(q.substatus(), QualitySubstatus::EguExceeded);
    assert_eq!(q.limit(), QualityLimit::HighLimited);
    assert_eq!(q.raw(), 0x0056);
}

#[test]
fn test_opc_quality_roundtrip_u16() {
    let words = [
        0x00C0, 0x0000, 0x0040, 0x0004, 0x0018, 0x0008, 0x00D8, 0x0056,
    ];
    for &w in &words {
        let q = OpcQuality::from(w);
        let back: u16 = q.into();
        assert_eq!(back, w);
    }
}

#[test]
fn test_opc_quality_from_str() {
    assert_eq!("good".parse::<OpcQuality>().unwrap(), OpcQuality::GOOD);
    assert_eq!("Good".parse::<OpcQuality>().unwrap(), OpcQuality::GOOD);
    assert_eq!("bad".parse::<OpcQuality>().unwrap(), OpcQuality::BAD);
    assert_eq!(
        "uncertain".parse::<OpcQuality>().unwrap(),
        OpcQuality::UNCERTAIN
    );
    assert!("other".parse::<OpcQuality>().is_err());
}

#[test]
fn test_server_identifier_conversions_and_display() {
    let prog_id = ServerIdentifier::from("Matrikon.OPC.Simulation.1");
    assert_eq!(
        prog_id,
        ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
    );
    assert_eq!(prog_id.to_string(), "Matrikon.OPC.Simulation.1");
    assert!(prog_id.is_prog_id());
    assert!(!prog_id.is_clsid());

    let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
    let parsed = ServerIdentifier::from(clsid_str);
    assert!(parsed.is_clsid());
    assert_eq!(parsed.to_string().to_uppercase(), clsid_str.to_uppercase());

    #[cfg(feature = "opc-da-backend")]
    {
        let direct_guid = windows_core::GUID::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
        let from_guid = ServerIdentifier::from(direct_guid);
        assert!(from_guid.is_clsid());
    }

    let direct_clsid = Clsid::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
    let from_clsid = ServerIdentifier::from(direct_clsid);
    assert!(from_clsid.is_clsid());
    assert_eq!(from_clsid.as_clsid(), Some(&direct_clsid));
}

#[test]
fn test_opc_server_info_display_name_and_endpoint() {
    let info_with_user_type = OpcServerInfo::new(
        "Matrikon.OPC.Simulation.1",
        Clsid::zeroed(),
        Some("Matrikon Simulation Server".into()),
        None,
    );
    assert_eq!(
        info_with_user_type.display_name(),
        "Matrikon Simulation Server"
    );
    assert_eq!(
        info_with_user_type.endpoint().identifier(),
        &ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
    );

    let info_without_user_type = OpcServerInfo::new(
        "Kepware.KEPServerEX.V6",
        Clsid::zeroed(),
        None,
        Some("192.168.1.10".into()),
    );
    assert_eq!(
        info_without_user_type.display_name(),
        "Kepware.KEPServerEX.V6"
    );
    assert_eq!(
        info_without_user_type.endpoint().host(),
        Some("192.168.1.10")
    );
}

#[cfg(feature = "opc-da-backend")]
#[test]
fn test_format_guid_bracketed() {
    let guid = windows_core::GUID::zeroed();
    assert_eq!(
        format_guid_bracketed(&guid),
        "{00000000-0000-0000-0000-000000000000}"
    );
    assert_eq!(
        format_guid_bracketed(&guid),
        ServerIdentifier::Clsid(Clsid::from_windows_guid(guid)).to_string()
    );

    let custom_guid = windows_core::GUID::from_u128(0x01234567_89AB_CDEF_0123_456789ABCDEF);
    assert_eq!(
        format_guid_bracketed(&custom_guid),
        ServerIdentifier::Clsid(Clsid::from_windows_guid(custom_guid)).to_string()
    );
}

#[test]
fn test_tag_batch_into_tags_conversions() {
    // 1. Static slice
    static STATIC_SLICE: &[&str] = &["Tag1", "Tag2"];
    let batch = STATIC_SLICE.into_tag_batch();
    assert_eq!(batch.len(), 2);
    assert!(!batch.is_empty());
    assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Tag1", "Tag2"]);
    assert_eq!(
        batch.into_vec(),
        vec!["Tag1".to_string(), "Tag2".to_string()]
    );

    // 2. Fixed-size array of static str
    let arr = ["TagA", "TagB", "TagC"];
    let batch = arr.into_tag_batch();
    assert_eq!(batch.len(), 3);
    assert_eq!(
        batch.iter_str().collect::<Vec<_>>(),
        vec!["TagA", "TagB", "TagC"]
    );
    assert_eq!(batch.into_vec(), vec!["TagA", "TagB", "TagC"]);

    // 3. Single static str literal
    let single_static = "SingleTag";
    let batch = single_static.into_tag_batch();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["SingleTag"]);
    assert_eq!(batch.into_vec(), vec!["SingleTag"]);

    // 4. Vec<String>
    let vec_strings = vec!["Dyn1".to_string(), "Dyn2".to_string()];
    let batch = vec_strings.into_tag_batch();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Dyn1", "Dyn2"]);
    assert_eq!(batch.into_vec(), vec!["Dyn1", "Dyn2"]);

    // 5. &[String]
    let slice_strings: &[String] = &["S1".to_string(), "S2".to_string()];
    let batch = slice_strings.into_tag_batch();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["S1", "S2"]);
    assert_eq!(batch.into_vec(), vec!["S1", "S2"]);

    // 6. TagBatch::from_str_lenient
    let inline_batch = TagBatch::from_str_lenient("Short.Tag");
    assert_eq!(inline_batch.len(), 1);
    assert_eq!(
        inline_batch.iter_str().collect::<Vec<_>>(),
        vec!["Short.Tag"]
    );
    assert_eq!(inline_batch.clone().into_vec(), vec!["Short.Tag"]);
    assert_eq!(inline_batch.into_shareable().len(), 1);

    let long_batch =
        TagBatch::from_str_lenient("Very.Long.Tag.That.Exceeds.Thirty.One.Bytes.Identifier");
    assert_eq!(long_batch.len(), 1);
    assert_eq!(
        long_batch.iter_str().collect::<Vec<_>>(),
        vec!["Very.Long.Tag.That.Exceeds.Thirty.One.Bytes.Identifier"]
    );

    // 7. Arc<[String]>
    let arc_slice: Arc<[String]> =
        Arc::from(vec!["Arc1".to_string(), "Arc2".to_string()].into_boxed_slice());
    let batch = arc_slice.into_tag_batch();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["Arc1", "Arc2"]);
    assert_eq!(batch.into_vec(), vec!["Arc1", "Arc2"]);

    // 8. Single owned String
    let single_string = "OwnedSingle".to_string();
    let batch = single_string.into_tag_batch();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch.iter_str().collect::<Vec<_>>(), vec!["OwnedSingle"]);
    assert_eq!(batch.into_vec(), vec!["OwnedSingle"]);

    // 9. Empty static slice
    let empty_batch = (&[] as &[&str]).into_tag_batch();
    assert_eq!(empty_batch.len(), 0);
    assert!(empty_batch.is_empty());
    assert_eq!(empty_batch.iter_str().count(), 0);
    assert!(empty_batch.into_vec().is_empty());
}

#[test]
fn test_tag_batch_into_shareable() {
    use crate::types::batch::TagBatchRepr;

    let owned_batch = vec!["TagA".to_string(), "TagB".to_string()].into_tag_batch();
    let shareable = owned_batch.into_shareable();
    assert!(matches!(shareable.repr, TagBatchRepr::Shared(_)));
    assert_eq!(shareable.len(), 2);
    assert_eq!(
        shareable.iter_str().collect::<Vec<_>>(),
        vec!["TagA", "TagB"]
    );

    let static_batch = ["Static1", "Static2"].into_tag_batch();
    let shareable_static = static_batch.into_shareable();
    assert_eq!(shareable_static.len(), 2);
}

#[test]
fn test_feature_independence_no_default_features() {
    let tv = TagValue::new("TagX", Some(OpcValue::Int(10)), OpcQuality::GOOD, None);
    assert!(tv.is_good());
    assert_eq!(tv.tag_id, "TagX");
}

#[test]
fn test_tag_values_collection_and_lenient_coercion() {
    let items = vec![
        TagValue::new(
            "Simulation.Int",
            Some(OpcValue::Int(42)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Simulation.Float",
            Some(OpcValue::Float(12.345)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Simulation.Bool",
            Some(OpcValue::Bool(true)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Simulation.String",
            Some(OpcValue::String("Running".into())),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new("Simulation.NoVal", None, OpcQuality::BAD_CONFIG_ERROR, None),
        TagValue::with_error(
            "Simulation.Failed",
            OpcQuality::BAD_CONFIG_ERROR,
            OpcError::Internal("COM error 0x80040154".into()),
        ),
    ];

    let tvs = TagValues::new(items);
    assert_eq!(tvs.len(), 6);
    assert!(!tvs.is_empty());

    // Case-insensitive lookups
    assert!(tvs.get("simulation.int").is_some());
    assert_eq!(tvs.get_value("SIMULATION.INT"), Some(&OpcValue::Int(42)));

    // Direct typed extractions
    assert_eq!(tvs.get_i32("Simulation.Int").unwrap(), 42);
    assert!((tvs.get_f64("Simulation.Float").unwrap() - 12.345).abs() < 1e-5);
    assert!(tvs.get_bool("Simulation.Bool").unwrap());
    assert_eq!(tvs.get_str("Simulation.String").unwrap(), "Running");

    // Lenient lossless coercion: i32 -> f64
    assert!((tvs.get_f64("Simulation.Int").unwrap() - 42.0).abs() < 1e-5);

    // Errors:
    // 1. NotRequested
    assert_eq!(
        tvs.get_f64("Unknown.Tag"),
        Err(TagExtractError::NotRequested("Unknown.Tag".into()))
    );

    // 2. ReadFailed (preserves underlying error)
    match tvs.get_f64("Simulation.Failed") {
        Err(TagExtractError::ReadFailed { tag, source }) => {
            assert_eq!(tag, "Simulation.Failed");
            assert!(source.to_string().contains("0x80040154"));
        }
        other => panic!("Expected ReadFailed, got {other:?}"),
    }

    // 3. NoValue
    assert_eq!(
        tvs.get_f64("Simulation.NoVal"),
        Err(TagExtractError::NoValue("Simulation.NoVal".into()))
    );

    // 4. TypeMismatch
    match tvs.get_f64("Simulation.String") {
        Err(TagExtractError::TypeMismatch {
            tag,
            value,
            expected,
        }) => {
            assert_eq!(tag, "Simulation.String");
            assert_eq!(value, "Running");
            assert_eq!(expected, "f64");
        }
        other => panic!("Expected TypeMismatch, got {other:?}"),
    }

    // Error conversion to OpcError
    let err: OpcError = TagExtractError::NotRequested("TagA".into()).into();
    assert!(matches!(err, OpcError::TagNotRequested(_)));
}

#[test]
fn test_tag_extract_error_conversion_fidelity() {
    use crate::errors::OpcError;
    use crate::types::TagExtractError;

    let err_not_req: OpcError = TagExtractError::NotRequested("Sensor.Temp".to_string()).into();
    match err_not_req {
        OpcError::TagNotRequested(tag) => {
            assert_eq!(tag, "Sensor.Temp");
        }
        other => panic!("Expected OpcError::TagNotRequested, got: {other:?}"),
    }

    let err_no_val: OpcError = TagExtractError::NoValue("Sensor.Pressure".to_string()).into();
    match err_no_val {
        OpcError::TagNoValue(tag) => {
            assert_eq!(tag, "Sensor.Pressure");
        }
        other => panic!("Expected OpcError::TagNoValue, got: {other:?}"),
    }
}

#[test]
fn test_tag_values_coercion_overflow_and_null_edge_cases() {
    let items = vec![
        TagValue::new(
            "Overflow.Float",
            Some(OpcValue::Float(1e25)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Nan.Float",
            Some(OpcValue::Float(f64::NAN)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Inf.Float",
            Some(OpcValue::Float(f64::INFINITY)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Frac.Float",
            Some(OpcValue::Float(42.75)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Exact.Float",
            Some(OpcValue::Float(50.0)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new("Null.Tag", Some(OpcValue::Null), OpcQuality::GOOD, None),
        TagValue::new("Empty.Tag", Some(OpcValue::Empty), OpcQuality::GOOD, None),
        TagValue::new("Num.Str", Some(OpcValue::Int(123)), OpcQuality::GOOD, None),
    ];

    let tvs = TagValues::new(items);

    // Overflow: 1e25 into i32 must fail safely with TypeMismatch, not wrap or panic
    assert!(matches!(
        tvs.get_i32("Overflow.Float"),
        Err(TagExtractError::TypeMismatch { .. })
    ));

    // NaN into i32 must fail
    assert!(matches!(
        tvs.get_i32("Nan.Float"),
        Err(TagExtractError::TypeMismatch { .. })
    ));

    // Infinity into i32 must fail
    assert!(matches!(
        tvs.get_i32("Inf.Float"),
        Err(TagExtractError::TypeMismatch { .. })
    ));

    // Fractional float into i32 must fail (lossy)
    assert!(matches!(
        tvs.get_i32("Frac.Float"),
        Err(TagExtractError::TypeMismatch { .. })
    ));

    // Exact integer float into i32 succeeds (lossless)
    assert_eq!(tvs.get_i32("Exact.Float").unwrap(), 50);

    // Null and Empty map to NoValue
    assert_eq!(
        tvs.get_f64("Null.Tag"),
        Err(TagExtractError::NoValue("Null.Tag".into()))
    );
    assert_eq!(
        tvs.get_f64("Empty.Tag"),
        Err(TagExtractError::NoValue("Empty.Tag".into()))
    );

    // get_str on non-string returns TypeMismatch
    assert!(matches!(
        tvs.get_str("Num.Str"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
}

#[test]
fn test_tag_value_outcome_facade() {
    let success = TagValue::success("Tag1", OpcValue::Int(10), OpcQuality::GOOD, None);
    assert!(success.is_good());
    assert!(!success.is_error());
    assert_eq!(success.value(), Some(&OpcValue::Int(10)));
    assert_eq!(success.error(), None);
    assert_eq!(success.outcome(), Ok(&OpcValue::Int(10)));
    assert_eq!(success.display_value(), "10");

    let failure = TagValue::with_error(
        "Tag2",
        OpcQuality::BAD_COMM_FAILURE,
        OpcError::Connection("Disconnected".into()),
    );
    assert!(!failure.is_good());
    assert!(failure.is_error());
    assert_eq!(failure.value(), None);
    assert!(failure.error().is_some());
    assert!(failure.outcome().is_err());
    assert_eq!(failure.display_value(), "Error");
}

#[test]
fn test_tag_values_deref() {
    let tv1 = TagValue::new("Tag1", Some(OpcValue::Int(1)), OpcQuality::GOOD, None);
    let tv2 = TagValue::new("Tag2", Some(OpcValue::Int(2)), OpcQuality::GOOD, None);
    let tvs = TagValues::new(vec![tv1, tv2]);

    // Test deref to slice
    assert_eq!(tvs.len(), 2);
    assert_eq!(tvs[0].tag_id, "Tag1");
    assert_eq!(tvs[1].tag_id, "Tag2");

    let slice: &[TagValue] = &tvs;
    assert_eq!(slice.len(), 2);
}

#[test]
fn test_host_normalization_and_remote_detection() {
    assert_eq!(normalize_host_str(None), None);
    assert_eq!(normalize_host_str(Some("")), None);
    assert_eq!(normalize_host_str(Some("   \t\r\n")), None);
    assert_eq!(normalize_host_str(Some("localhost")), None);
    assert_eq!(normalize_host_str(Some("LocalHost")), None);
    assert_eq!(normalize_host_str(Some("LOCALHOST")), None);
    assert_eq!(normalize_host_str(Some("127.0.0.1")), None);
    assert_eq!(normalize_host_str(Some("::1")), None);
    assert_eq!(
        normalize_host_str(Some("192.168.1.50")),
        Some("192.168.1.50")
    );
    assert_eq!(
        normalize_host_str(Some("  remote-plc  ")),
        Some("remote-plc")
    );
    assert_eq!(
        normalize_host_str(Some("scada-node-01")),
        Some("scada-node-01")
    );

    assert_eq!(normalize_host(None), None);
    assert_eq!(normalize_host(Some("")), None);
    assert_eq!(normalize_host(Some("   ")), None);
    assert_eq!(normalize_host(Some("localhost")), None);
    assert_eq!(normalize_host(Some("LOCALHOST")), None);
    assert_eq!(normalize_host(Some("127.0.0.1")), None);
    assert_eq!(normalize_host(Some("::1")), None);

    assert_eq!(
        normalize_host(Some("192.168.1.50")),
        Some("192.168.1.50".to_string())
    );
    assert_eq!(
        normalize_host(Some("  plc-host  ")),
        Some("plc-host".to_string())
    );

    assert!(!is_remote_host(None));
    assert!(!is_remote_host(Some("")));
    assert!(!is_remote_host(Some("localhost")));
    assert!(!is_remote_host(Some("127.0.0.1")));
    assert!(!is_remote_host(Some("::1")));
    assert!(is_remote_host(Some("remote-server")));

    let local_ep = OpcServerEndpoint::local("Test.Server");
    assert!(!local_ep.is_remote());
    assert_eq!(local_ep.host, None);

    let remote_local = OpcServerEndpoint::remote("localhost", "Test.Server");
    assert!(!remote_local.is_remote());
    assert_eq!(remote_local.host, None);

    let remote_ep = OpcServerEndpoint::remote("10.0.0.1", "Test.Server");
    assert!(remote_ep.is_remote());
    assert_eq!(remote_ep.host, Some("10.0.0.1".to_string()));

    let info = OpcServerInfo::new(
        "Test.Server",
        Clsid::zeroed(),
        Some("Test Title".to_string()),
        Some("localhost".to_string()),
    );
    assert_eq!(info.host(), None);
    assert_eq!(info.display_name(), "Test Title");
    let ep = info.endpoint();
    assert!(!ep.is_remote());
}

#[test]
fn test_tag_value_quality_semantic_matrix() {
    // Quadrant 1: GOOD quality + Ok(val)
    let q1 = TagValue::new("Q1", Some(OpcValue::Int(42)), OpcQuality::GOOD, None);
    assert!(q1.is_good());
    assert!(!q1.is_error());
    assert!(!q1.is_uncertain());
    assert!(!q1.is_bad());
    assert_eq!(q1.error(), None);
    assert!(q1.outcome().is_ok());

    // Quadrant 2: UNCERTAIN quality + Ok(val) (The Bug Quadrant)
    let q2 = TagValue::new("Q2", Some(OpcValue::Int(42)), OpcQuality::UNCERTAIN, None);
    assert!(!q2.is_good());
    assert!(q2.is_uncertain());
    assert!(!q2.is_error(), "Outcome is Ok, so is_error() must be false");
    assert!(!q2.is_bad());
    assert_eq!(q2.error(), None);
    assert!(q2.outcome().is_ok());

    // Quadrant 3: BAD quality + Ok(val) (Clamped/Stale cache)
    let q3 = TagValue::new(
        "Q3",
        Some(OpcValue::Int(42)),
        OpcQuality::BAD_CONFIG_ERROR,
        None,
    );
    assert!(!q3.is_good());
    assert!(!q3.is_uncertain());
    assert!(q3.is_bad());
    assert!(!q3.is_error());
    assert_eq!(q3.error(), None);
    assert!(q3.outcome().is_ok());

    // Quadrant 4: BAD quality + Err(OpcError) (Failed Read)
    let q4 = TagValue::with_error(
        "Q4",
        OpcQuality::BAD_COMM_FAILURE,
        OpcError::Connection("Drop".into()),
    );
    assert!(!q4.is_good());
    assert!(!q4.is_uncertain());
    assert!(q4.is_bad());
    assert!(q4.is_error());
    assert!(q4.error().is_some());
    assert!(q4.outcome().is_err());
}

#[test]
fn test_tag_extract_error_preserves_source() {
    let err = TagExtractError::ReadFailed {
        tag: "Faulty.Tag".into(),
        source: OpcError::Connection("DCOM RPC timeout 0x800706BA".into()),
    };
    let opc_err: OpcError = err.into();
    assert!(
        opc_err.is_connection_error(),
        "Converting TagExtractError::ReadFailed to OpcError must preserve connection error provenance"
    );
}

#[test]
fn test_opc_value_f32_and_default() {
    assert_eq!(OpcValue::default(), OpcValue::Empty);
    let val: OpcValue = 12.5f32.into();
    assert_eq!(val, OpcValue::Float(12.5));
    let f: f32 = val.try_into().unwrap();
    assert!((f - 12.5).abs() < 1e-6);

    let int_val = OpcValue::Int(42);
    let f_from_int: f32 = int_val.try_into().unwrap();
    assert!((f_from_int - 42.0f32).abs() < f32::EPSILON);
}

#[test]
fn test_tag_values_typed_getters() {
    let items = vec![
        TagValue::new("U32.Tag", Some(OpcValue::UInt(100)), OpcQuality::GOOD, None),
        TagValue::new(
            "U64.Tag",
            Some(OpcValue::UInt(5_000_000_000)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "I64.Tag",
            Some(OpcValue::Int(-1_000_000_000_000)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "F32.Tag",
            Some(OpcValue::Float(12.340_000_152_587_89)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "WholeFloat.Tag",
            Some(OpcValue::Float(42.0)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "FracFloat.Tag",
            Some(OpcValue::Float(42.7)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "StrNum.Tag",
            Some(OpcValue::String("42".into())),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::with_error(
            "Err.Tag",
            OpcQuality::BAD,
            OpcError::Internal("Hardware fault".into()),
        ),
    ];
    let tvs = TagValues::new(items);

    // Typed getters
    assert_eq!(tvs.get_u32("u32.tag").unwrap(), 100u32);
    assert_eq!(tvs.get_u64("u64.tag").unwrap(), 5_000_000_000u64);
    assert_eq!(tvs.get_i64("i64.tag").unwrap(), -1_000_000_000_000i64);
    assert!((tvs.get_f32("f32.tag").unwrap() - 12.34f32).abs() < 1e-5);

    // Generic get_as<T>
    assert_eq!(tvs.get_as::<u32>("u32.tag").unwrap(), 100u32);
    assert_eq!(tvs.get_as::<u64>("u64.tag").unwrap(), 5_000_000_000u64);
    assert_eq!(tvs.get_as::<i64>("i64.tag").unwrap(), -1_000_000_000_000i64);
    assert_eq!(tvs.get_as::<i32>("wholefloat.tag").unwrap(), 42i32);
    assert_eq!(tvs.get_as::<u32>("wholefloat.tag").unwrap(), 42u32);

    // Whole-number float to integer coercion succeeds
    assert_eq!(tvs.get_u32("wholefloat.tag").unwrap(), 42u32);
    assert_eq!(tvs.get_u64("wholefloat.tag").unwrap(), 42u64);
    assert_eq!(tvs.get_i64("wholefloat.tag").unwrap(), 42i64);

    // Fractional float to integer coercion fails with TypeMismatch
    assert!(matches!(
        tvs.get_u32("fracfloat.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
    assert!(matches!(
        tvs.get_u64("fracfloat.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
    assert!(matches!(
        tvs.get_i64("fracfloat.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
    assert!(matches!(
        tvs.get_as::<u32>("fracfloat.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));

    // String parsing is NOT implicitly performed (no stringly-typed magic)
    assert!(matches!(
        tvs.get_u32("strnum.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
    assert!(matches!(
        tvs.get_i32("strnum.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
    assert!(matches!(
        tvs.get_f64("strnum.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));
    assert!(matches!(
        tvs.get_as::<u32>("strnum.tag"),
        Err(TagExtractError::TypeMismatch { .. })
    ));

    // Missing tag returns NotRequested
    assert!(matches!(
        tvs.get_u32("nonexistent.tag"),
        Err(TagExtractError::NotRequested(_))
    ));

    // Read error preserves ReadFailed
    assert!(matches!(
        tvs.get_u32("err.tag"),
        Err(TagExtractError::ReadFailed { .. })
    ));
}

#[test]
fn test_endpoint_unc_parsing_roundtrip() {
    use std::str::FromStr;

    // Windows UNC backslash path
    let ep1 = OpcServerEndpoint::from_str(r"\\192.168.1.50\Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep1.host(), Some("192.168.1.50"));
    assert_eq!(ep1.identifier().to_string(), "Matrikon.OPC.Simulation");
    assert!(ep1.is_remote());
    assert_eq!(ep1.to_string(), r"\\192.168.1.50\Matrikon.OPC.Simulation");

    // Unix-style forward slash path
    let ep2 = OpcServerEndpoint::from_str("//192.168.1.50/Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep2.host(), Some("192.168.1.50"));
    assert_eq!(ep2.identifier().to_string(), "Matrikon.OPC.Simulation");
    assert!(ep2.is_remote());

    // Localhost UNC normalized to local
    let ep_local = OpcServerEndpoint::from_str(r"\\localhost\Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep_local.host(), None);
    assert!(!ep_local.is_remote());
    assert_eq!(ep_local.to_string(), "Matrikon.OPC.Simulation");

    // Plain server name without host
    let ep_plain = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep_plain.host(), None);
    assert!(!ep_plain.is_remote());
    assert_eq!(ep_plain.to_string(), "Matrikon.OPC.Simulation");

    // Roundtrip test via Display and FromStr
    let ep_remote = OpcServerEndpoint::remote("10.0.0.5", "Kepware.KEPServerEX.V6");
    let display_str = ep_remote.to_string();
    let reparsed: OpcServerEndpoint = display_str.parse().unwrap();
    assert_eq!(ep_remote, reparsed);

    // From<&str> delegates to parsing
    let ep_from_str: OpcServerEndpoint = r"\\192.168.1.50\Matrikon.OPC.Simulation".into();
    assert_eq!(ep_from_str, ep1);

    // Rejection cases
    assert!(OpcServerEndpoint::from_str("").is_err());
    assert!(OpcServerEndpoint::from_str("   ").is_err());
    assert!(OpcServerEndpoint::from_str(r"\\").is_err());
    assert!(OpcServerEndpoint::from_str(r"\\host\").is_err());
}

#[test]
fn test_parse_quality_error_raw_accessor() {
    use crate::types::quality::ParseQualityError;
    let err = ParseQualityError::new("CORRUPT_QUALITY");
    assert_eq!(err.raw(), "CORRUPT_QUALITY");
    assert_eq!(
        format!("{err}"),
        "Invalid OPC quality string: 'CORRUPT_QUALITY'"
    );
}

#[test]
#[allow(clippy::many_single_char_names, clippy::duration_suboptimal_units)]
fn test_display_option_timestamp_civil() {
    use crate::types::value::{SystemTimeOptionExt, secs_to_civil};
    use std::time::{Duration, SystemTime};

    // 1. Unix Epoch
    assert_eq!(format!("{}", Some(SystemTime::UNIX_EPOCH).display()), "N/A");

    // 2. Fixed Timestamp: 1700000000 = 2023-11-14 22:13:20 UTC
    let ts = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(format!("{}", Some(ts).display()), "2023-11-14 22:13:20");

    // 3. Leap Year: 2024-02-29 12:00:00 UTC = 1709208000
    let leap_ts = SystemTime::UNIX_EPOCH + Duration::from_secs(1_709_208_000);
    assert_eq!(
        format!("{}", Some(leap_ts).display()),
        "2024-02-29 12:00:00"
    );

    // 4. Pre-1970 via secs_to_civil: -86400 = 1969-12-31 00:00:00
    let (y, m, d, h, min, s) = secs_to_civil(-86400);
    assert_eq!((y, m, d, h, min, s), (1969, 12, 31, 0, 0, 0));
}

#[test]
fn test_write_result_co_located() {
    use crate::errors::OpcError;
    use crate::types::WriteResult;

    let ok = WriteResult::success("Channel.Device.Tag1");
    assert!(ok.is_success());
    assert!(!ok.is_error());
    assert_eq!(ok.tag_id, "Channel.Device.Tag1");
    assert_eq!(ok.status, Ok(()));
    assert!(ok.error().is_none());

    let err = WriteResult::failure(
        "Channel.Device.Tag2",
        OpcError::Connection("Disconnected".into()),
    );
    assert!(err.is_error());
    assert!(!err.is_success());
    assert_eq!(err.tag_id, "Channel.Device.Tag2");
    assert_eq!(
        err.error(),
        Some(&OpcError::Connection("Disconnected".into()))
    );
}

#[test]
fn test_clsid_construction_and_components() {
    use crate::types::clsid::Clsid;

    let clsid = Clsid::new(
        0x1234_5678,
        0x1234,
        0x5678,
        [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0],
    );
    assert_eq!(clsid.data1, 0x1234_5678);
    assert_eq!(clsid.data2, 0x1234);
    assert_eq!(clsid.data3, 0x5678);
    assert_eq!(
        clsid.data4,
        [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]
    );
    assert!(!clsid.is_zero());

    let zero = Clsid::zeroed();
    assert!(zero.is_zero());
    assert_eq!(zero, Clsid::nil());
    assert_eq!(zero, Clsid::default());
}

#[test]
fn test_clsid_u128_roundtrip() {
    use crate::types::clsid::Clsid;

    let val = 0x1234_5678_1234_5678_1234_5678_9abc_def0_u128;
    let clsid = Clsid::from_u128(val);
    assert_eq!(clsid.data1, 0x1234_5678);
    assert_eq!(clsid.data2, 0x1234);
    assert_eq!(clsid.data3, 0x5678);
    assert_eq!(
        clsid.data4,
        [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]
    );
    assert_eq!(clsid.to_u128(), val);

    let from_trait: Clsid = val.into();
    assert_eq!(from_trait, clsid);
    let to_trait: u128 = clsid.into();
    assert_eq!(to_trait, val);
}

#[test]
fn test_clsid_parse_and_display() {
    use crate::types::clsid::{Clsid, ParseClsidError};
    use std::str::FromStr;

    let raw = "{13486D51-4821-11D2-A494-3CB306C10000}";
    let clsid = Clsid::parse(raw).expect("should parse bracketed GUID");
    assert_eq!(clsid.data1, 0x1348_6D51);
    assert_eq!(clsid.data2, 0x4821);
    assert_eq!(clsid.data3, 0x11D2);
    assert_eq!(
        clsid.data4,
        [0xA4, 0x94, 0x3C, 0xB3, 0x06, 0xC1, 0x00, 0x00]
    );

    // Format matches canonical uppercase bracketed string
    assert_eq!(clsid.to_bracketed(), raw);
    assert_eq!(format!("{clsid}"), raw);
    assert_eq!(format!("{clsid:?}"), format!("Clsid({raw})"));

    // Unbracketed lowercase parses identically
    let lower_unbracketed = "13486d51-4821-11d2-a494-3cb306c10000";
    let clsid_lower =
        Clsid::from_str(lower_unbracketed).expect("should parse unbracketed lowercase");
    assert_eq!(clsid_lower, clsid);

    // Whitespace trimming
    let spaced = "  {13486D51-4821-11D2-A494-3CB306C10000} \n";
    assert_eq!(Clsid::parse(spaced), Some(clsid));

    // Error cases
    assert!(Clsid::parse("").is_none());
    assert!(Clsid::parse("not-a-guid").is_none());
    assert!(Clsid::parse("{13486D51-4821-11D2-A494-3CB306C1000}").is_none()); // short
    assert!(Clsid::parse("13486D51_4821_11D2_A494_3CB306C10000").is_none()); // wrong delimiter
    assert!(Clsid::parse("{13486D51-4821-11D2-A494-3CB306C10000").is_none()); // mismatched bracket

    // ParseClsidError verification
    let err: ParseClsidError = Clsid::from_str("invalid-clsid").unwrap_err();
    assert_eq!(err.raw(), "invalid-clsid");
    assert_eq!(
        format!("{err}"),
        "Invalid CLSID GUID string: 'invalid-clsid'"
    );
}

#[test]
fn test_clsid_windows_guid_conversion() {
    use crate::types::clsid::Clsid;

    let guid = windows_core::GUID {
        data1: 0x1348_6D51,
        data2: 0x4821,
        data3: 0x11D2,
        data4: [0xA4, 0x94, 0x3C, 0xB3, 0x06, 0xC1, 0x00, 0x00],
    };

    let clsid = Clsid::from_windows_guid(guid);
    assert_eq!(clsid.data1, guid.data1);
    assert_eq!(clsid.data2, guid.data2);
    assert_eq!(clsid.data3, guid.data3);
    assert_eq!(clsid.data4, guid.data4);

    let roundtrip_guid = clsid.to_windows_guid();
    assert_eq!(roundtrip_guid, guid);

    let from_into: Clsid = guid.into();
    assert_eq!(from_into, clsid);

    let to_into: windows_core::GUID = clsid.into();
    assert_eq!(to_into, guid);
}

#[test]
fn test_parse_errors_display_and_traits() {
    use crate::types::clsid::Clsid;
    use crate::types::server::{ParseEndpointError, ParseServerIdError};
    use std::str::FromStr;

    // 1. ParseServerIdError variants & display
    let err_empty_id = ParseServerIdError::Empty;
    assert_eq!(
        format!("{err_empty_id}"),
        "Server identifier cannot be empty"
    );

    let err_too_long = ParseServerIdError::ProgIdTooLong(256);
    assert_eq!(
        format!("{err_too_long}"),
        "ProgID length 256 exceeds maximum allowed 255 characters"
    );

    let err_invalid_prog = ParseServerIdError::InvalidProgId("Invalid..ProgID".to_string());
    assert_eq!(
        format!("{err_invalid_prog}"),
        "Invalid characters or syntax in ProgID: 'Invalid..ProgID'"
    );

    let clsid_err = Clsid::from_str("not-a-guid").unwrap_err();
    let err_clsid = ParseServerIdError::InvalidClsid(clsid_err);
    assert!(format!("{err_clsid}").contains("Invalid CLSID GUID string: 'not-a-guid'"));

    // 2. ParseEndpointError variants & display
    let err_ep_empty = ParseEndpointError::Empty;
    assert_eq!(format!("{err_ep_empty}"), "Server endpoint cannot be empty");

    let err_ep_format = ParseEndpointError::InvalidFormat("bad-endpoint".to_string());
    assert_eq!(
        format!("{err_ep_format}"),
        "Invalid endpoint syntax: 'bad-endpoint'"
    );

    let err_ep_missing = ParseEndpointError::MissingServer(r"\\host\".to_string());
    assert_eq!(
        format!("{err_ep_missing}"),
        r"Missing server identifier in endpoint path: '\\host\'"
    );

    let err_ep_nested = ParseEndpointError::from(err_empty_id.clone());
    assert_eq!(
        format!("{err_ep_nested}"),
        "Invalid server identifier in endpoint: Server identifier cannot be empty"
    );

    // 3. std::error::Error source checking
    let std_err: &dyn std::error::Error = &err_ep_nested;
    assert!(std_err.source().is_some());
    let source = std_err.source().unwrap();
    assert_eq!(format!("{source}"), format!("{err_empty_id}"));
}

#[test]
fn test_prog_id_validation_boundaries() {
    use crate::types::server::{ParseServerIdError, validate_prog_id};

    // Valid boundaries: length 1 and 255
    assert!(validate_prog_id("A").is_ok());
    assert!(validate_prog_id("A.B").is_ok());

    let prog_id_255 = "A".repeat(255);
    assert!(validate_prog_id(&prog_id_255).is_ok());

    let prog_id_dot_255 = format!("{}.{}", "A".repeat(127), "B".repeat(127));
    assert_eq!(prog_id_dot_255.len(), 255);
    assert!(validate_prog_id(&prog_id_dot_255).is_ok());

    // Boundary: length 0 (empty) and length 256
    assert_eq!(validate_prog_id(""), Err(ParseServerIdError::Empty));
    assert_eq!(validate_prog_id("   "), Err(ParseServerIdError::Empty));

    let prog_id_256 = "A".repeat(256);
    assert_eq!(
        validate_prog_id(&prog_id_256),
        Err(ParseServerIdError::ProgIdTooLong(256))
    );

    // Syntax rejection: leading or trailing dots
    assert!(matches!(
        validate_prog_id(".Server.Prog"),
        Err(ParseServerIdError::InvalidProgId(_))
    ));
    assert!(matches!(
        validate_prog_id("Server.Prog."),
        Err(ParseServerIdError::InvalidProgId(_))
    ));

    // Syntax rejection: consecutive dots
    assert!(matches!(
        validate_prog_id("Server..Prog"),
        Err(ParseServerIdError::InvalidProgId(_))
    ));

    // Syntax rejection: special characters & spaces
    let invalid_chars = [
        "Server Name",
        "Server@1",
        "Server#Tag",
        "Server$Val",
        "Server/1",
        "Server\\1",
        "Server:1",
    ];
    for invalid in invalid_chars {
        assert!(
            matches!(
                validate_prog_id(invalid),
                Err(ParseServerIdError::InvalidProgId(_))
            ),
            "Expected '{invalid}' to fail ProgID validation"
        );
    }
}

#[test]
fn test_server_identifier_from_str_valid_and_invalid() {
    use crate::types::server::{ParseServerIdError, ServerIdentifier};
    use std::str::FromStr;

    // 1. Valid ProgID
    let id_prog = ServerIdentifier::from_str("Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(id_prog.as_prog_id(), Some("Matrikon.OPC.Simulation.1"));
    assert!(id_prog.is_prog_id());
    assert!(!id_prog.is_clsid());

    // 2. Valid Bracketed CLSID
    let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
    let id_clsid = ServerIdentifier::from_str(clsid_str).unwrap();
    assert!(id_clsid.is_clsid());
    assert_eq!(
        id_clsid.to_string().to_uppercase(),
        clsid_str.to_uppercase()
    );

    // 3. From<&str>
    let id_from = ServerIdentifier::from("Kepware.KEPServerEX.V6");
    assert_eq!(id_from.as_prog_id(), Some("Kepware.KEPServerEX.V6"));

    // 4. Invalid cases
    assert_eq!(
        ServerIdentifier::from_str(""),
        Err(ParseServerIdError::Empty)
    );
    assert_eq!(
        ServerIdentifier::from_str("   "),
        Err(ParseServerIdError::Empty)
    );

    assert!(matches!(
        ServerIdentifier::from_str("{not-a-valid-guid}"),
        Err(ParseServerIdError::InvalidClsid(_))
    ));

    assert!(matches!(
        ServerIdentifier::from_str("Invalid Server Identifier"),
        Err(ParseServerIdError::InvalidProgId(_))
    ));
}

#[test]
fn test_endpoint_from_str_comprehensive_schemes() {
    use crate::types::server::OpcServerEndpoint;
    use std::str::FromStr;

    // Windows UNC
    let ep_unc = OpcServerEndpoint::from_str(r"\\192.168.1.50\Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_unc.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(ep_unc.identifier.to_string(), "Matrikon.OPC.Simulation.1");
    assert!(ep_unc.is_remote());
    assert_eq!(
        ep_unc.to_string(),
        r"\\192.168.1.50\Matrikon.OPC.Simulation.1"
    );

    // Unix forward slash
    let ep_unix = OpcServerEndpoint::from_str("//192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_unix.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(ep_unix.identifier.to_string(), "Matrikon.OPC.Simulation.1");
    assert!(ep_unix.is_remote());

    // URI scheme: opc://
    let ep_uri =
        OpcServerEndpoint::from_str("opc://192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_uri.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(ep_uri.identifier.to_string(), "Matrikon.OPC.Simulation.1");
    assert!(ep_uri.is_remote());

    // URI scheme: opc.da://
    let ep_da_uri =
        OpcServerEndpoint::from_str("opc.da://192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_da_uri.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(
        ep_da_uri.identifier.to_string(),
        "Matrikon.OPC.Simulation.1"
    );
    assert!(ep_da_uri.is_remote());

    // Raw slash
    let ep_raw_slash =
        OpcServerEndpoint::from_str("192.168.1.50/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_raw_slash.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(
        ep_raw_slash.identifier.to_string(),
        "Matrikon.OPC.Simulation.1"
    );
    assert!(ep_raw_slash.is_remote());

    // Standalone local ProgID
    let ep_local_prog = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_local_prog.host, None);
    assert!(!ep_local_prog.is_remote());
    assert_eq!(
        ep_local_prog.identifier.to_string(),
        "Matrikon.OPC.Simulation.1"
    );

    // Standalone local CLSID
    let clsid_str = "{28E68F9A-8D75-11D1-8DC3-3C302A000000}";
    let ep_local_clsid = OpcServerEndpoint::from_str(clsid_str).unwrap();
    assert_eq!(ep_local_clsid.host, None);
    assert!(!ep_local_clsid.is_remote());
    assert!(ep_local_clsid.identifier.is_clsid());
}

#[test]
fn test_endpoint_from_str_localhost_normalization() {
    use crate::types::server::OpcServerEndpoint;
    use std::str::FromStr;

    let local_inputs = [
        r"\\localhost\Matrikon.OPC.Simulation.1",
        r"\\LOCALHOST\Matrikon.OPC.Simulation.1",
        r"\\127.0.0.1\Matrikon.OPC.Simulation.1",
        r"\\::1\Matrikon.OPC.Simulation.1",
        "//localhost/Matrikon.OPC.Simulation.1",
        "//127.0.0.1/Matrikon.OPC.Simulation.1",
        "opc://localhost/Matrikon.OPC.Simulation.1",
        "opc.da://localhost/Matrikon.OPC.Simulation.1",
        "opc://127.0.0.1/Matrikon.OPC.Simulation.1",
        "localhost/Matrikon.OPC.Simulation.1",
    ];

    for input in local_inputs {
        let ep = OpcServerEndpoint::from_str(input).unwrap();
        assert_eq!(
            ep.host, None,
            "Host in '{input}' should be normalized to None"
        );
        assert!(!ep.is_remote(), "Endpoint '{input}' should not be remote");
        assert_eq!(ep.identifier.to_string(), "Matrikon.OPC.Simulation.1");
    }
}

#[test]
fn test_endpoint_from_str_negative_syntax_cases() {
    use crate::types::server::{OpcServerEndpoint, ParseEndpointError, ParseServerIdError};
    use std::str::FromStr;

    assert_eq!(
        OpcServerEndpoint::from_str(""),
        Err(ParseEndpointError::Empty)
    );
    assert_eq!(
        OpcServerEndpoint::from_str("   "),
        Err(ParseEndpointError::Empty)
    );

    assert!(matches!(
        OpcServerEndpoint::from_str(r"\\host\"),
        Err(ParseEndpointError::MissingServer(_))
    ));
    assert_eq!(
        OpcServerEndpoint::from_str("//host"),
        Err(ParseEndpointError::InvalidFormat(
            "Expected host and server separated by delimiter in '//host'".into()
        ))
    );
    assert!(matches!(
        OpcServerEndpoint::from_str("opc://host/"),
        Err(ParseEndpointError::MissingServer(_))
    ));
    assert!(matches!(
        OpcServerEndpoint::from_str("opc.da://host/"),
        Err(ParseEndpointError::MissingServer(_))
    ));

    assert!(matches!(
        OpcServerEndpoint::from_str(r"\\192.168.1.50\Invalid Prog@ID"),
        Err(ParseEndpointError::InvalidServerId(
            ParseServerIdError::InvalidProgId(_)
        ))
    ));
}

#[test]
fn test_endpoint_deprecated_from_str_behavior() {
    use crate::types::server::OpcServerEndpoint;
    use std::str::FromStr;

    #[allow(deprecated)]
    let ep: OpcServerEndpoint = r"\\192.168.1.50\Matrikon.OPC.Simulation.1".into();
    assert_eq!(ep.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(ep.identifier.to_string(), "Matrikon.OPC.Simulation.1");

    let res = OpcServerEndpoint::from_str(r"\\192.168.1.50\Matrikon.OPC.Simulation.1");
    assert!(res.is_ok());

    let err_res = OpcServerEndpoint::from_str("");
    assert!(err_res.is_err());
}

#[test]
fn test_tag_batch_encapsulation_methods() {
    use crate::types::batch::TagBatch;
    use std::sync::Arc;

    static TAGS: &[&str] = &["Tag1", "Tag2"];
    let batch_static = TagBatch::from_static(TAGS);
    assert_eq!(batch_static.len(), 2);
    assert!(!batch_static.is_empty());
    assert_eq!(
        batch_static.iter().collect::<Vec<_>>(),
        vec!["Tag1", "Tag2"]
    );
    assert_eq!(
        batch_static.iter_str().collect::<Vec<_>>(),
        vec!["Tag1", "Tag2"]
    );
    assert_eq!(batch_static.as_static_slice(), Some(TAGS));
    assert_eq!(batch_static.as_slice(), None);

    let vec_tags = vec!["TagA".to_string(), "TagB".to_string()];
    let batch_owned = TagBatch::from(vec_tags);
    assert_eq!(batch_owned.len(), 2);
    assert_eq!(batch_owned.as_slice().unwrap(), &["TagA", "TagB"]);
    assert_eq!(batch_owned.as_static_slice(), None);

    let shared_tags: Arc<[String]> =
        Arc::from(vec!["S1".to_string(), "S2".to_string()].into_boxed_slice());
    let batch_shared = TagBatch::from(shared_tags);
    assert_eq!(batch_shared.len(), 2);
    assert_eq!(batch_shared.as_slice().unwrap(), &["S1", "S2"]);
    assert_eq!(batch_shared.as_static_slice(), None);

    let shareable_static = batch_static.into_shareable();
    assert_eq!(shareable_static.as_static_slice(), Some(TAGS));

    let shareable_owned = batch_owned.into_shareable();
    assert_eq!(shareable_owned.len(), 2);
    assert_eq!(shareable_owned.as_slice().unwrap(), &["TagA", "TagB"]);

    let empty = TagBatch::empty();
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
    assert_eq!(empty.as_static_slice(), Some(&[][..]));
}

#[test]
fn test_tag_batch_value_equality() {
    use crate::types::batch::TagBatch;

    static TAGS: &[&str] = &["Tag1", "Tag2"];
    let b_static = TagBatch::from_static(TAGS);
    let b_owned = TagBatch::from(vec!["Tag1".to_string(), "Tag2".to_string()]);
    let b_sso = TagBatch::from_str_lenient("Tag1");
    let b_sso_owned = TagBatch::from(vec!["Tag1".to_string()]);

    assert_eq!(
        b_static, b_owned,
        "Static and Owned batches with identical items must be equal"
    );
    assert_eq!(
        b_sso, b_sso_owned,
        "SSO inline and OwnedSingle with identical items must be equal"
    );
    assert_ne!(b_static, b_sso);
}

#[test]
fn test_tag_batch_from_iterator() {
    use crate::types::batch::TagBatch;

    let strings = vec!["Tag1".to_string(), "Tag2".to_string()];
    let batch_owned: TagBatch = strings.into_iter().collect();
    assert_eq!(batch_owned.len(), 2);
    assert_eq!(
        batch_owned.iter_str().collect::<Vec<_>>(),
        vec!["Tag1", "Tag2"]
    );

    let slices = ["TagA", "TagB", "TagC"];
    let batch_slices: TagBatch = slices.into_iter().collect();
    assert_eq!(batch_slices.len(), 3);
    assert_eq!(
        batch_slices.iter_str().collect::<Vec<_>>(),
        vec!["TagA", "TagB", "TagC"]
    );

    let empty_batch: TagBatch = std::iter::empty::<String>().collect();
    assert_eq!(empty_batch.len(), 0);
    assert!(empty_batch.is_empty());
}

#[test]
fn test_tag_batch_sso_boundary_and_multibyte_safety() {
    use crate::types::batch::TagBatch;

    // 0-byte empty string
    let batch_empty = TagBatch::from_str_lenient("");
    assert_eq!(batch_empty.len(), 1);
    assert_eq!(batch_empty.iter_str().collect::<Vec<_>>(), vec![""]);
    assert_eq!(batch_empty.into_vec(), vec![""]);

    // Exactly 31-byte string
    let s31 = "1234567890123456789012345678901";
    assert_eq!(s31.len(), 31);
    let batch_31 = TagBatch::from_str_lenient(s31);
    assert_eq!(batch_31.len(), 1);
    assert_eq!(batch_31.iter_str().next(), Some(s31));
    assert_eq!(batch_31.into_vec(), vec![s31.to_string()]);

    // Exactly 32-byte string
    let s32 = "12345678901234567890123456789012";
    assert_eq!(s32.len(), 32);
    let batch_32 = TagBatch::from_str_lenient(s32);
    assert_eq!(batch_32.len(), 1);
    assert_eq!(batch_32.iter_str().next(), Some(s32));
    assert_eq!(batch_32.into_vec(), vec![s32.to_string()]);

    // Multibyte UTF-8: Japanese
    let jp = "タグ１２３";
    assert_eq!(jp.len(), 15);
    let batch_jp = TagBatch::from_str_lenient(jp);
    assert_eq!(batch_jp.len(), 1);
    assert_eq!(batch_jp.iter_str().next(), Some(jp));
    assert_eq!(batch_jp.into_vec(), vec![jp.to_string()]);

    // Multibyte UTF-8: Emoji
    let emoji = "🚀🏭⚡";
    assert_eq!(emoji.len(), 11);
    let batch_emoji = TagBatch::from_str_lenient(emoji);
    assert_eq!(batch_emoji.len(), 1);
    assert_eq!(batch_emoji.iter_str().next(), Some(emoji));

    // 33-byte multibyte fallback
    let s33 = format!("{}タグ", "A".repeat(27));
    assert_eq!(s33.len(), 33);
    let batch_33 = TagBatch::from_str_lenient(&s33);
    assert_eq!(batch_33.iter_str().next(), Some(s33.as_str()));
}

#[test]
fn test_tag_extract_error_tag_accessor() {
    let err_not_req = TagExtractError::NotRequested("Sensor.Temperature".to_string());
    assert_eq!(err_not_req.tag(), "Sensor.Temperature");

    let err_read_failed = TagExtractError::ReadFailed {
        tag: "Sensor.Pressure".to_string(),
        source: crate::errors::OpcError::Connection("DCOM RPC disconnected".into()),
    };
    assert_eq!(err_read_failed.tag(), "Sensor.Pressure");

    let err_no_val = TagExtractError::NoValue("Sensor.FlowRate".to_string());
    assert_eq!(err_no_val.tag(), "Sensor.FlowRate");

    let err_type_mismatch = TagExtractError::TypeMismatch {
        tag: "Sensor.StatusFlag".to_string(),
        value: "active".to_string(),
        expected: "bool",
    };
    assert_eq!(err_type_mismatch.tag(), "Sensor.StatusFlag");

    // Borrow verification: returns &str without heap reallocation
    let borrowed: &str = err_not_req.tag();
    assert_eq!(borrowed, "Sensor.Temperature");
}

#[test]
fn test_opc_server_info_getters_and_encapsulation() {
    let clsid = Clsid::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
    let info = OpcServerInfo::new(
        "Matrikon.OPC.Simulation.1",
        clsid,
        Some("Matrikon Simulation Server".to_string()),
        Some("192.168.1.50".to_string()),
    );

    assert_eq!(info.prog_id(), "Matrikon.OPC.Simulation.1");
    assert_eq!(info.clsid(), clsid);
    assert_eq!(info.user_type(), Some("Matrikon Simulation Server"));
    assert_eq!(info.host(), Some("192.168.1.50"));
    assert_eq!(info.display_name(), "Matrikon Simulation Server");

    let ep = info.endpoint();
    assert!(ep.is_remote());
    assert_eq!(ep.host(), Some("192.168.1.50"));
    assert_eq!(
        ep.identifier(),
        &ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
    );

    // Consuming accessor verification
    assert_eq!(info.into_prog_id(), "Matrikon.OPC.Simulation.1");

    // Fallback display name and localhost normalization to None
    let local_info = OpcServerInfo::new(
        "Kepware.KEPServerEX.V6",
        Clsid::zeroed(),
        None,
        Some("localhost".to_string()),
    );

    assert_eq!(local_info.prog_id(), "Kepware.KEPServerEX.V6");
    assert_eq!(local_info.clsid(), Clsid::zeroed());
    assert_eq!(local_info.user_type(), None);
    assert_eq!(local_info.host(), None);
    assert_eq!(local_info.display_name(), "Kepware.KEPServerEX.V6");

    let local_ep = local_info.endpoint();
    assert!(!local_ep.is_remote());
    assert_eq!(local_ep.host(), None);
    assert_eq!(
        local_ep.identifier(),
        &ServerIdentifier::ProgId("Kepware.KEPServerEX.V6".into())
    );

    // Whitespace host normalization to None
    let trimmed_info = OpcServerInfo::new(
        "Yokogawa.Exaopc.1",
        Clsid::zeroed(),
        Some("Yokogawa Server".to_string()),
        Some("   ".to_string()),
    );
    assert_eq!(trimmed_info.host(), None);
}

#[test]
fn test_vartype_constants_and_raw_roundtrip() {
    use crate::types::{BaseVarType, VarType};

    assert_eq!(VarType::EMPTY.raw(), 0);
    assert_eq!(VarType::NULL.raw(), 1);
    assert_eq!(VarType::I2.raw(), 2);
    assert_eq!(VarType::I4.raw(), 3);
    assert_eq!(VarType::R4.raw(), 4);
    assert_eq!(VarType::R8.raw(), 5);
    assert_eq!(VarType::CY.raw(), 6);
    assert_eq!(VarType::DATE.raw(), 7);
    assert_eq!(VarType::BSTR.raw(), 8);
    assert_eq!(VarType::DISPATCH.raw(), 9);
    assert_eq!(VarType::ERROR.raw(), 10);
    assert_eq!(VarType::BOOL.raw(), 11);
    assert_eq!(VarType::VARIANT.raw(), 12);
    assert_eq!(VarType::UNKNOWN_INTERFACE.raw(), 13);
    assert_eq!(VarType::DECIMAL.raw(), 14);
    assert_eq!(VarType::I1.raw(), 16);
    assert_eq!(VarType::UI1.raw(), 17);
    assert_eq!(VarType::UI2.raw(), 18);
    assert_eq!(VarType::UI4.raw(), 19);
    assert_eq!(VarType::I8.raw(), 20);
    assert_eq!(VarType::UI8.raw(), 21);
    assert_eq!(VarType::INT.raw(), 22);
    assert_eq!(VarType::UINT.raw(), 23);

    let vt = VarType::from(3u16);
    assert_eq!(vt, VarType::I4);
    assert_eq!(u16::from(vt), 3);
    assert_eq!(VarType::from_raw(8), VarType::BSTR);
    assert_eq!(VarType::from_base(BaseVarType::Bstr), VarType::BSTR);
    assert_eq!(VarType::default(), VarType::EMPTY);
}

#[test]
fn test_vartype_bitmasks_array_and_byref() {
    use crate::types::{BaseVarType, VarType};

    let scalar = VarType::I4;
    assert!(!scalar.is_array());
    assert!(!scalar.is_byref());
    assert!(!scalar.is_vector());
    assert_eq!(scalar.base_type(), BaseVarType::I4);

    let arr = VarType::from_raw(0x2003);
    assert!(arr.is_array());
    assert!(!arr.is_byref());
    assert_eq!(arr.base_raw(), 3);
    assert_eq!(arr.base_type(), BaseVarType::I4);

    let byref = VarType::from_raw(0x4008);
    assert!(!byref.is_array());
    assert!(byref.is_byref());
    assert_eq!(byref.base_raw(), 8);
    assert_eq!(byref.base_type(), BaseVarType::Bstr);

    let composite = VarType::from_raw(0x6005);
    assert!(composite.is_array());
    assert!(composite.is_byref());
    assert_eq!(composite.base_raw(), 5);
    assert_eq!(composite.base_type(), BaseVarType::R8);
}

#[test]
fn test_vartype_base_type_decomposition() {
    use crate::types::{BaseVarType, VarType};

    assert_eq!(VarType::from_raw(0).base_type(), BaseVarType::Empty);
    assert_eq!(VarType::from_raw(9).base_type(), BaseVarType::Dispatch);
    assert_eq!(
        VarType::from_raw(13).base_type(),
        BaseVarType::UnknownInterface
    );
    assert_eq!(VarType::from_raw(999).base_type(), BaseVarType::Other(999));
}

#[test]
fn test_vartype_display_formatting() {
    use crate::types::{BaseVarType, VarType};

    assert_eq!(VarType::I4.to_string(), "VT_I4");
    assert_eq!(VarType::BSTR.to_string(), "VT_BSTR");
    assert_eq!(VarType::EMPTY.to_string(), "VT_EMPTY");
    assert_eq!(VarType::from_raw(0x2011).to_string(), "VT_ARRAY | VT_UI1");
    assert_eq!(
        VarType::from_raw(0x6005).to_string(),
        "VT_ARRAY | VT_BYREF | VT_R8"
    );
    assert_eq!(BaseVarType::Bool.to_string(), "VT_BOOL");
    assert_eq!(BaseVarType::Other(0x0FFF).to_string(), "VT_OTHER(0x0FFF)");
    assert_eq!(
        VarType::from_raw(0x2FFF).to_string(),
        "VT_ARRAY | VT_OTHER(0x0FFF)"
    );
}

#[cfg(feature = "opc-da-backend")]
#[test]
fn test_vartype_varenum_roundtrip() {
    use crate::types::VarType;
    use windows::Win32::System::Variant::{
        VARENUM, VT_BOOL, VT_BSTR, VT_DISPATCH, VT_I4, VT_R8, VT_UI1, VT_UNKNOWN,
    };

    assert_eq!(VarType::from(VT_I4), VarType::I4);
    assert_eq!(VarType::from(VT_BSTR), VarType::BSTR);
    assert_eq!(VarType::from(VT_DISPATCH), VarType::DISPATCH);
    assert_eq!(VarType::from(VT_UNKNOWN), VarType::UNKNOWN_INTERFACE);
    assert_eq!(VarType::from(VT_BOOL), VarType::BOOL);
    assert_eq!(VarType::from(VT_UI1), VarType::UI1);
    assert_eq!(VARENUM::from(VarType::R8), VT_R8);
}

#[test]
fn test_tag_value_helpers_success() {
    let tv = TagValue {
        tag_id: "Tag1".to_string(),
        outcome: Ok(OpcValue::Int(42)),
        quality: OpcQuality::GOOD,
        timestamp: Some(SystemTime::UNIX_EPOCH),
    };
    assert!(tv.is_good());
    assert!(!tv.is_error());
    assert_eq!(tv.display_value(), "42");
    assert_eq!(tv.formatted_timestamp(), "N/A"); // UNIX_EPOCH returns "N/A" in helper
}

#[test]
fn test_tag_value_helpers_failure() {
    let tv = TagValue {
        tag_id: "Tag2".to_string(),
        outcome: Err(crate::errors::OpcError::Internal("Comm failed".into())),
        quality: OpcQuality::BAD_COMM_FAILURE,
        timestamp: None,
    };
    assert!(!tv.is_good());
    assert!(tv.is_error());
    assert_eq!(tv.display_value(), "Error");
    assert_eq!(tv.formatted_timestamp(), "N/A");
}

#[test]
fn test_opc_value_display() {
    assert_eq!(OpcValue::String("hello".into()).to_string(), "hello");
    assert_eq!(OpcValue::Int(100).to_string(), "100");
    assert_eq!(OpcValue::Float(12.34).to_string(), "12.34");
    assert_eq!(OpcValue::Bool(true).to_string(), "true");
    assert_eq!(OpcValue::Bool(false).to_string(), "false");
    assert_eq!(OpcValue::Empty.to_string(), "Empty");
    assert_eq!(OpcValue::Null.to_string(), "Null");
}

#[test]
fn test_opc_value_option_ext_some() {
    let val_opt = Some(OpcValue::Int(42));
    assert_eq!(format!("{}", val_opt.display()), "42");
    assert_eq!(format!("{}", val_opt.display_or("Custom")), "42");

    let val_ref = val_opt.as_ref();
    assert_eq!(format!("{}", val_ref.display()), "42");
    assert_eq!(format!("{}", val_ref.display_or("Custom")), "42");
}

#[test]
fn test_opc_value_option_ext_none() {
    let val_opt: Option<OpcValue> = None;
    assert_eq!(format!("{}", val_opt.display()), "Error");
    assert_eq!(format!("{}", val_opt.display_or("Custom")), "Custom");

    let val_ref = val_opt.as_ref();
    assert_eq!(format!("{}", val_ref.display()), "Error");
    assert_eq!(format!("{}", val_ref.display_or("Custom")), "Custom");
}

#[test]
fn test_system_time_option_ext_some() {
    // Non-epoch time (1700000000 = 2023-11-14 22:13:20 UTC)
    let ts = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let ts_opt = Some(ts);
    let expected = "2023-11-14 22:13:20";
    assert_eq!(format!("{}", ts_opt.display()), expected);
    assert_eq!(format!("{}", ts_opt.display_or("Custom")), expected);
}

#[test]
fn test_system_time_option_ext_none_and_epoch() {
    let ts_none: Option<SystemTime> = None;
    assert_eq!(format!("{}", ts_none.display()), "N/A");
    assert_eq!(format!("{}", ts_none.display_or("Custom")), "Custom");

    let ts_epoch = Some(SystemTime::UNIX_EPOCH);
    assert_eq!(format!("{}", ts_epoch.display()), "N/A");
    assert_eq!(format!("{}", ts_epoch.display_or("Custom")), "Custom");
}

#[test]
fn test_tag_value_display() {
    let tv = TagValue {
        tag_id: "Simulation.Item1".to_string(),
        outcome: Ok(OpcValue::Float(99.5)),
        quality: OpcQuality::GOOD,
        timestamp: Some(SystemTime::UNIX_EPOCH),
    };
    assert_eq!(format!("{tv}"), "Simulation.Item1 = 99.5 [Good] @ N/A");
}

#[test]
fn test_tag_value_destructuring_ergonomics() {
    let tv = TagValue {
        tag_id: "Device1.Tag1".to_string(),
        outcome: Ok(OpcValue::String("Active".into())),
        quality: OpcQuality::GOOD,
        timestamp: None,
    };

    // Exact pattern destructuring
    let TagValue {
        tag_id,
        outcome,
        quality,
        timestamp,
    } = tv;

    let formatted = format!(
        "Tag: {:<15} | Value: {:<10} | Quality: {:<6} | Timestamp: {}",
        tag_id,
        outcome.ok().display(),
        quality,
        timestamp.display_or("N/A")
    );

    assert_eq!(
        formatted,
        "Tag: Device1.Tag1    | Value: Active     | Quality: Good   | Timestamp: N/A"
    );
}

#[test]
fn test_tag_collector_lifecycle() {
    let collector = TagCollector::new(5);
    assert_eq!(collector.len(), 0);
    assert!(collector.is_empty());
    assert_eq!(collector.max_tags(), 5);
    assert!(!collector.is_full());
    assert!(!collector.is_cancelled());

    assert!(collector.push("Tag1".into()));
    assert!(collector.push("Tag2".into()));
    assert_eq!(collector.len(), 2);
    assert!(!collector.is_empty());
    assert!(!collector.is_full());

    let snap = collector.snapshot();
    assert_eq!(snap, vec!["Tag1".to_string(), "Tag2".to_string()]);
    assert_eq!(collector.len(), 2);

    let harvested = collector.harvest();
    assert_eq!(harvested, vec!["Tag1".to_string(), "Tag2".to_string()]);
    assert_eq!(collector.len(), 0);
    assert!(collector.is_empty());
}

#[test]
fn test_tag_collector_capacity_cap() {
    let collector = TagCollector::new(2);
    assert!(collector.push("T1".into()));
    assert!(collector.push("T2".into()));
    assert_eq!(collector.len(), 2);
    assert!(collector.is_full());

    // Further pushes must be rejected
    assert!(!collector.push("T3".into()));
    assert_eq!(collector.len(), 2);
    assert_eq!(
        collector.snapshot(),
        vec!["T1".to_string(), "T2".to_string()]
    );
}

#[test]
fn test_tag_collector_unbounded() {
    let collector = TagCollector::unbounded();
    assert_eq!(collector.max_tags(), usize::MAX);
    assert!(!collector.is_full());
    assert!(collector.push("A".into()));
    assert!(!collector.is_full());
}

#[test]
fn test_tag_collector_cancellation() {
    let collector = TagCollector::new(10);
    let c1 = collector.clone();
    let c2 = collector.clone();

    assert!(!c1.is_cancelled());
    assert!(!c2.is_cancelled());

    collector.cancel();
    assert!(c1.is_cancelled());
    assert!(c2.is_cancelled());
    assert!(collector.is_cancelled());

    // Pushes after cancellation must be rejected
    assert!(!collector.push("T1".into()));
    assert_eq!(collector.len(), 0);
}

#[test]
fn test_tag_collector_multithreaded() {
    let collector = TagCollector::new(400);
    let handles: Vec<_> = (0..4)
        .map(|thread_id| {
            let col = collector.clone();
            std::thread::spawn(move || {
                for i in 0..100 {
                    assert!(col.push(format!("T_{thread_id}_{i}")));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(collector.len(), 400);
    assert!(collector.is_full());
    let tags = collector.harvest();
    assert_eq!(tags.len(), 400);
    assert_eq!(collector.len(), 0);
}
