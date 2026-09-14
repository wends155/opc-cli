use opc_da_client::{
    MockOpcProvider, OpcProvider, OpcQuality, OpcValue, ServerDiscovery, TagBatch, TagBrowser,
    TagCollector, TagReader, TagValue, TagValues, TagWriter, WriteResult,
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

    // 5. write_tag_batch expectation
    mock.expect_write_tag_batch().returning(|server, writes| {
        assert_eq!(server, "Matrikon.OPC.Simulation.1");
        let results = writes
            .iter()
            .map(|(tag, _)| WriteResult::success(tag))
            .collect();
        Ok(results)
    });

    // 5b. write_tag_values expectation (ensures deprecated slice contract is preserved)
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

    // Verify static dispatch via OpcProvider
    fn assert_provider<P: OpcProvider>(_p: &P) {}
    assert_provider(&mock);
    let provider = &mock;

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

    // Test write_tag_batch
    let writes = vec![("Random.Int4".to_string(), OpcValue::Int(123))];
    let write_results = provider
        .write_tag_batch("Matrikon.OPC.Simulation.1", writes.clone().into())
        .await
        .unwrap();
    assert_eq!(write_results.len(), 1);
    assert!(write_results[0].is_success());

    // Test deprecated write_tag_values backward compatibility
    #[allow(deprecated)]
    let deprecated_results = provider
        .write_tag_values("Matrikon.OPC.Simulation.1", &writes)
        .await
        .unwrap();
    assert_eq!(deprecated_results.len(), 1);
    assert!(deprecated_results[0].is_success());

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

#[tokio::test]
async fn test_standalone_role_mocks() {
    use opc_da_client::{
        MockServerDiscovery, MockTagBrowser, MockTagReader, MockTagWriter, OpcQuality,
        OpcServerInfo, OpcValue, ServerDiscovery, TagBrowser, TagCollector, TagReader, TagValue,
        TagValues, TagWriter, WriteResult,
    };

    // 1. Verify MockServerDiscovery in complete isolation
    let mut discovery_mock = MockServerDiscovery::new();
    discovery_mock
        .expect_list_servers()
        .with(mockall::predicate::eq("127.0.0.1"))
        .returning(|_| Ok(vec!["Isolated.Server.1".into()]));
    discovery_mock
        .expect_list_server_details()
        .returning(|host| {
            Ok(vec![OpcServerInfo::new(
                "Isolated.Server.1",
                opc_da_client::Clsid::zeroed(),
                Some("Isolated Server".into()),
                Some(host.to_string()),
            )])
        });
    let servers = discovery_mock.list_servers("127.0.0.1").await.unwrap();
    assert_eq!(servers, vec!["Isolated.Server.1"]);
    let details = discovery_mock
        .list_server_details("127.0.0.1")
        .await
        .unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].prog_id(), "Isolated.Server.1");
    assert_eq!(details[0].clsid(), opc_da_client::Clsid::zeroed());
    assert_eq!(details[0].user_type(), Some("Isolated Server"));
    assert_eq!(details[0].host(), None);

    // 2. Verify MockTagBrowser in complete isolation
    let mut browser_mock = MockTagBrowser::new();
    browser_mock
        .expect_browse_tags()
        .returning(|_server, collector| {
            let _ = collector.push("Isolated.Tag1".into());
            let _ = collector.push("Isolated.Tag2".into());
            Ok(collector.snapshot())
        });
    let collector = TagCollector::new(10);
    let tags = browser_mock
        .browse_tags("Isolated.Server.1", collector)
        .await
        .unwrap();
    assert_eq!(tags, vec!["Isolated.Tag1", "Isolated.Tag2"]);

    // 3. Verify MockTagReader in complete isolation
    let mut reader_mock = MockTagReader::new();
    reader_mock
        .expect_read_tag_values()
        .returning(|_server, batch| {
            let items = batch
                .iter_str()
                .map(|t| TagValue::new(t, Some(OpcValue::Int(101)), OpcQuality::GOOD, None))
                .collect();
            Ok(TagValues::new(items))
        });
    reader_mock
        .expect_read_tag_value()
        .returning(|_server, tag| {
            Ok(TagValue::new(
                tag,
                Some(OpcValue::Int(101)),
                OpcQuality::GOOD,
                None,
            ))
        });
    let val = reader_mock
        .read_tag_value("Isolated.Server.1", "Isolated.Tag1")
        .await
        .unwrap();
    assert_eq!(val.tag_id, "Isolated.Tag1");
    assert_eq!(val.value(), Some(&OpcValue::Int(101)));

    // 4. Verify MockTagWriter in complete isolation
    let mut writer_mock = MockTagWriter::new();
    writer_mock
        .expect_write_tag_value()
        .returning(|_server, tag, _val| Ok(WriteResult::success(tag)));
    writer_mock
        .expect_write_tag_batch()
        .returning(|_server, writes| {
            Ok(writes
                .iter()
                .map(|(t, _)| WriteResult::success(t))
                .collect())
        });
    let res = writer_mock
        .write_tag_value("Isolated.Server.1", "Isolated.Tag1", OpcValue::Int(202))
        .await
        .unwrap();
    assert!(res.is_success());
    assert_eq!(res.tag_id, "Isolated.Tag1");
}
