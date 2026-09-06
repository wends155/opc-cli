use opc_da_client::{
    MockOpcProvider, OpcProvider, OpcQuality, OpcValue, TagBatch, TagCollector, TagValue,
    TagValues, WriteResult,
};

#[tokio::test]
async fn test_mock_opc_provider_full_contract_stability() {
    let mut mock = MockOpcProvider::new();

    // 1. list_servers expectation
    mock.expect_list_servers()
        .with(mockall::predicate::eq("localhost"))
        .returning(|_| Ok(vec!["Matrikon.OPC.Simulation.1".into()]));

    // 2. browse_tags expectation
    mock.expect_browse_tags().returning(|server, collector| {
        assert_eq!(server, "Matrikon.OPC.Simulation.1");
        let _ = collector.push("Random.Int4".into());
        let _ = collector.push("Random.Real8".into());
        Ok(collector.snapshot())
    });

    // 3. read_tag_values expectation
    mock.expect_read_tag_values().returning(|server, batch| {
        assert_eq!(server, "Matrikon.OPC.Simulation.1");
        let items: Vec<TagValue> = batch
            .iter()
            .map(|tag| TagValue::new(tag, Some(OpcValue::Float(99.5)), OpcQuality::GOOD, None))
            .collect();
        Ok(TagValues::new(items))
    });

    // 4. read_tag_value expectation
    mock.expect_read_tag_value()
        .with(
            mockall::predicate::eq("Matrikon.OPC.Simulation.1"),
            mockall::predicate::eq("Random.Int4"),
        )
        .returning(|_, tag| {
            Ok(TagValue::new(
                tag,
                Some(OpcValue::Int(42)),
                OpcQuality::GOOD,
                None,
            ))
        });

    // 5. write_tag_values expectation (ensures &[ (String, OpcValue) ] slice contract is preserved)
    mock.expect_write_tag_values().returning(|server, writes| {
        assert_eq!(server, "Matrikon.OPC.Simulation.1");
        let results = writes
            .iter()
            .map(|(tag, _)| WriteResult::success(tag))
            .collect();
        Ok(results)
    });

    // 6. write_tag_value expectation
    mock.expect_write_tag_value()
        .returning(|_, tag, _| Ok(WriteResult::success(tag)));

    // Verify dynamic dispatch via &dyn OpcProvider
    let provider: &dyn OpcProvider = &mock;

    // Test list_servers
    let servers = provider.list_servers("localhost").await.unwrap();
    assert_eq!(servers, vec!["Matrikon.OPC.Simulation.1"]);

    // Test browse_tags
    let collector = TagCollector::new(100);
    let tags = provider
        .browse_tags("Matrikon.OPC.Simulation.1", collector)
        .await
        .unwrap();
    assert_eq!(tags.len(), 2);

    // Test read_tag_values
    let batch = TagBatch::from(vec!["Random.Real8".into()]);
    let values = provider
        .read_tag_values("Matrikon.OPC.Simulation.1", batch)
        .await
        .unwrap();
    assert_eq!(values.len(), 1);
    assert!((values.get_f64("random.real8").unwrap() - 99.5).abs() < f64::EPSILON);

    // Test read_tag_value
    let single_val = provider
        .read_tag_value("Matrikon.OPC.Simulation.1", "Random.Int4")
        .await
        .unwrap();
    assert_eq!(single_val.value(), Some(&OpcValue::Int(42)));

    // Test write_tag_values
    let writes = vec![("Random.Int4".to_string(), OpcValue::Int(123))];
    let write_results = provider
        .write_tag_values("Matrikon.OPC.Simulation.1", &writes)
        .await
        .unwrap();
    assert_eq!(write_results.len(), 1);
    assert!(write_results[0].is_success());

    // Test write_tag_value
    let single_write = provider
        .write_tag_value(
            "Matrikon.OPC.Simulation.1",
            "Random.Int4",
            OpcValue::Int(456),
        )
        .await
        .unwrap();
    assert!(single_write.is_success());
}
