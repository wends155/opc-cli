//! Unit tests for domain types, quality conversions, and tag collections.

use super::*;
use crate::errors::OpcError;
use std::sync::Arc;

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

    let direct_guid = windows::core::GUID::from_u128(0x28E6_8F9A_8D75_11D1_8DC3_3C30_2A00_0000);
    let from_guid = ServerIdentifier::from(direct_guid);
    assert!(from_guid.is_clsid());
}

#[test]
fn test_opc_server_info_display_name_and_endpoint() {
    let info_with_user_type = OpcServerInfo {
        prog_id: "Matrikon.OPC.Simulation.1".into(),
        clsid: windows::core::GUID::zeroed(),
        user_type: Some("Matrikon Simulation Server".into()),
        host: None,
    };
    assert_eq!(
        info_with_user_type.display_name(),
        "Matrikon Simulation Server"
    );
    assert_eq!(
        info_with_user_type.endpoint().identifier,
        ServerIdentifier::ProgId("Matrikon.OPC.Simulation.1".into())
    );

    let info_without_user_type = OpcServerInfo {
        prog_id: "Kepware.KEPServerEX.V6".into(),
        clsid: windows::core::GUID::zeroed(),
        user_type: None,
        host: Some("192.168.1.10".into()),
    };
    assert_eq!(
        info_without_user_type.display_name(),
        "Kepware.KEPServerEX.V6"
    );
    assert_eq!(
        info_without_user_type.endpoint().host.as_deref(),
        Some("192.168.1.10")
    );
}

#[test]
fn test_format_guid_bracketed() {
    let guid = windows::core::GUID::zeroed();
    assert_eq!(
        format_guid_bracketed(&guid),
        "{00000000-0000-0000-0000-000000000000}"
    );
    assert_eq!(
        format_guid_bracketed(&guid),
        ServerIdentifier::Clsid(guid).to_string()
    );

    let custom_guid = windows::core::GUID::from_u128(0x01234567_89AB_CDEF_0123_456789ABCDEF);
    assert_eq!(
        format_guid_bracketed(&custom_guid),
        ServerIdentifier::Clsid(custom_guid).to_string()
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
    let owned_batch = vec!["TagA".to_string(), "TagB".to_string()].into_tag_batch();
    let shareable = owned_batch.into_shareable();
    assert!(matches!(shareable, TagBatch::Shared(_)));
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
    assert!(matches!(err, OpcError::Conversion(_)));
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
fn test_tag_result_decomposition_and_conversions() {
    let success = TagSuccess::new(
        "Sensor.Temperature",
        OpcValue::Float(23.5),
        OpcQuality::GOOD,
        None,
    );
    let tv: TagValue = success.into();
    assert!(tv.is_good());
    assert_eq!(tv.tag_id, "Sensor.Temperature");

    let result = tv.to_result();
    assert!(result.is_ok());
    let unwrapped = result.unwrap();
    assert_eq!(unwrapped.tag_id, "Sensor.Temperature");
    assert_eq!(unwrapped.value, OpcValue::Float(23.5));

    let failure = TagFailure::new(
        "Sensor.Faulty",
        OpcQuality::BAD_COMM_FAILURE,
        OpcError::Connection("Device unplugged".into()),
    );
    let tv_fail: TagValue = failure.into();
    assert!(tv_fail.is_error());

    let res_fail = tv_fail.clone().into_result();
    assert!(res_fail.is_err());
    let unwrapped_fail = res_fail.unwrap_err();
    assert_eq!(unwrapped_fail.tag_id, "Sensor.Faulty");
    assert_eq!(unwrapped_fail.quality, OpcQuality::BAD_COMM_FAILURE);

    let tvs = TagValues::new(vec![tv, tv_fail]);
    let results: Vec<TagResult> = tvs.iter_results().collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
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

    let converted = failure.into_result();
    assert!(converted.is_err());
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
        windows::core::GUID::zeroed(),
        Some("Test Title".to_string()),
        Some("localhost".to_string()),
    );
    assert_eq!(info.host, None);
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
    assert!(q1.into_result().is_ok());

    // Quadrant 2: UNCERTAIN quality + Ok(val) (The Bug Quadrant)
    let q2 = TagValue::new("Q2", Some(OpcValue::Int(42)), OpcQuality::UNCERTAIN, None);
    assert!(!q2.is_good());
    assert!(q2.is_uncertain());
    assert!(!q2.is_error(), "Outcome is Ok, so is_error() must be false");
    assert!(!q2.is_bad());
    assert_eq!(q2.error(), None);
    assert!(q2.into_result().is_ok());

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
    assert!(q3.into_result().is_ok());

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
    assert!(q4.into_result().is_err());
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
    assert_eq!(ep1.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(ep1.identifier.to_string(), "Matrikon.OPC.Simulation");
    assert!(ep1.is_remote());
    assert_eq!(ep1.to_string(), r"\\192.168.1.50\Matrikon.OPC.Simulation");

    // Unix-style forward slash path
    let ep2 = OpcServerEndpoint::from_str("//192.168.1.50/Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep2.host.as_deref(), Some("192.168.1.50"));
    assert_eq!(ep2.identifier.to_string(), "Matrikon.OPC.Simulation");
    assert!(ep2.is_remote());

    // Localhost UNC normalized to local
    let ep_local = OpcServerEndpoint::from_str(r"\\localhost\Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep_local.host, None);
    assert!(!ep_local.is_remote());
    assert_eq!(ep_local.to_string(), "Matrikon.OPC.Simulation");

    // Plain server name without host
    let ep_plain = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation").unwrap();
    assert_eq!(ep_plain.host, None);
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
