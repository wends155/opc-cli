//! Domain pipeline integration tests.
//!
//! Exercises the full pure-Rust domain data pipeline without COM or mock dependencies.
//!
//! Run with:
//!   cargo test -p opc-da-client --test domain_pipeline_test --no-default-features

use opc_da_client::{
    BaseVarType, IntoTags, IntoWriteBatch, OpcError, OpcQuality, OpcValue, QualityLimit,
    QualityMajor, QualitySubstatus, TagBatch, TagValue, TagValues, VarType, WriteBatch,
    WriteResult,
};

#[test]
fn test_domain_pipeline_full_roundtrip() {
    // Stage 1: TagBatch construction
    let batch: TagBatch = ["Pump.Speed", "Valve.Position", "Tank.Level"].into_tag_batch();
    assert_eq!(batch.len(), 3);

    // Stage 2: TagValues from read response simulation
    let items = vec![
        TagValue::new(
            "Pump.Speed",
            Some(OpcValue::Float(1450.0)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Valve.Position",
            Some(OpcValue::Bool(true)),
            OpcQuality::GOOD,
            None,
        ),
        TagValue::new(
            "Tank.Level",
            Some(OpcValue::UInt(85)),
            OpcQuality::GOOD,
            None,
        ),
    ];
    let values = TagValues::new(items);
    assert_eq!(values.len(), 3);

    // Stage 3: TagValue facade
    let speed = values.get("Pump.Speed").unwrap();
    assert!(speed.is_good());
    assert_eq!(speed.value(), Some(&OpcValue::Float(1450.0)));

    // Stage 4: OpcQuality field access (quality is pub field, NOT a method)
    let q = speed.quality;
    assert_eq!(q.major(), QualityMajor::Good);
    assert_eq!(q.substatus(), QualitySubstatus::NonSpecific);
    assert_eq!(q.limit(), QualityLimit::NotLimited);
    assert!(q.is_good());

    // Stage 5: Typed extraction
    let speed_f64 = values.get_f64("pump.speed").unwrap();
    assert!((speed_f64 - 1450.0).abs() < f64::EPSILON);
    let valve_bool = values.get_bool("valve.position").unwrap();
    assert!(valve_bool);

    // Stage 6: WriteBatch and WriteResult
    let write_items: Vec<(String, OpcValue)> = vec![
        ("Pump.Speed".to_string(), OpcValue::Float(1500.0)),
        ("Valve.Position".to_string(), OpcValue::Bool(false)),
    ];
    let write_batch: WriteBatch = write_items.into_write_batch();
    let write_results: Vec<WriteResult> = write_batch
        .iter()
        .map(|(tag, _val)| WriteResult::success(tag))
        .collect();
    assert_eq!(write_results.len(), 2);
    assert!(write_results[0].is_success());
    assert_eq!(write_results[0].tag_id, "Pump.Speed");

    // Stage 7: VarType automation type enforcement (canonical constant names, no VT_ prefix)
    let f32_var = VarType::R4;
    let f64_var = VarType::R8;
    let i4_var = VarType::I4;
    let bool_var = VarType::BOOL;
    assert_eq!(f32_var.base_type(), BaseVarType::R4);
    assert_eq!(f64_var.base_type(), BaseVarType::R8);
    assert_eq!(i4_var.base_type(), BaseVarType::I4);
    assert_eq!(bool_var.base_type(), BaseVarType::Bool);
    assert!(!f64_var.is_array());
    assert!(!f64_var.is_byref());
}

#[test]
fn test_domain_pipeline_degraded_quality_and_error_propagation() {
    let items = vec![
        TagValue::new(
            "Healthy.Tag",
            Some(OpcValue::Int(42)),
            OpcQuality::GOOD,
            None,
        ),
        // with_error: outcome: Err(...), value() returns None, is_error() returns true
        TagValue::with_error(
            "Comm.Failed",
            OpcQuality::BAD_COMM_FAILURE,
            OpcError::Internal("device offline".into()),
        ),
        TagValue::new(
            "Uncertain.Tag",
            Some(OpcValue::Float(3.5)),
            OpcQuality::UNCERTAIN,
            None,
        ),
    ];
    let values = TagValues::new(items);

    let healthy = values.get("Healthy.Tag").unwrap();
    assert!(healthy.is_good());
    assert_eq!(healthy.value(), Some(&OpcValue::Int(42)));

    // with_error: value() == None, is_error() == true
    let comm_failed = values.get("Comm.Failed").unwrap();
    assert!(comm_failed.is_bad());
    assert!(!comm_failed.is_good());
    assert_eq!(comm_failed.value(), None);
    assert!(comm_failed.is_error());
    // .quality is a public field, NOT a method
    let q_comm = comm_failed.quality;
    assert_eq!(q_comm.major(), QualityMajor::Bad);
    assert_eq!(q_comm.substatus(), QualitySubstatus::CommFailure);

    let uncertain = values.get("Uncertain.Tag").unwrap();
    assert!(uncertain.is_uncertain());
    assert_eq!(uncertain.value(), Some(&OpcValue::Float(3.5)));

    // WriteResult failure propagation
    let write_fail =
        WriteResult::failure("Comm.Failed", OpcError::Internal("write rejected".into()));
    assert!(!write_fail.is_success());
    assert_eq!(write_fail.tag_id, "Comm.Failed");
    assert!(matches!(write_fail.error(), Some(OpcError::Internal(_))));
}

#[test]
fn test_domain_pipeline_vartype_automation_enforcement() {
    // Use VarType::from_raw with VT_ARRAY_FLAG (BitOr is not impl'd on VarType)
    let vt_array_r8 = VarType::from_raw(VarType::VT_ARRAY_FLAG | VarType::R8.raw());
    assert!(vt_array_r8.is_array());
    assert!(!vt_array_r8.is_byref());
    assert_eq!(vt_array_r8.base_type(), BaseVarType::R8);

    let vt_byref_i4 = VarType::from_raw(VarType::VT_BYREF_FLAG | VarType::I4.raw());
    assert!(vt_byref_i4.is_byref());
    assert!(!vt_byref_i4.is_array());
    assert_eq!(vt_byref_i4.base_type(), BaseVarType::I4);

    // Display formatting (constants use no VT_ prefix; Display output uses VT_ prefix)
    assert_eq!(format!("{}", VarType::R8), "VT_R8");
    assert_eq!(format!("{}", VarType::EMPTY), "VT_EMPTY");
    assert_eq!(
        format!("{}", VarType::from_raw(0x2011)),
        "VT_ARRAY | VT_UI1"
    );

    // BaseVarType roundtrip via From impl
    let base = BaseVarType::R8;
    let vt: VarType = base.into();
    assert_eq!(vt.base_type(), BaseVarType::R8);
    assert!(!vt.is_array());

    // Batch verification of OPC primitive types
    let opc_types = [
        VarType::BOOL,
        VarType::I1,
        VarType::I2,
        VarType::I4,
        VarType::I8,
        VarType::R4,
        VarType::R8,
        VarType::BSTR,
    ];
    for vt in opc_types {
        assert!(!vt.is_array());
        assert!(!vt.is_byref());
        let array_vt = VarType::from_raw(VarType::VT_ARRAY_FLAG | vt.raw());
        assert!(array_vt.is_array());
        assert_eq!(array_vt.base_type(), vt.base_type());
    }
}
