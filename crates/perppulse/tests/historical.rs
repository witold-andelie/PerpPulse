use perppulse::envio::{EnvioClient, EnvioCoverage, IndexedPoint, ProtocolEventSlice};
use perppulse::events::{load_fixture, repo_root, CanonicalProvenance};
use perppulse::historical::{build_historical_snapshot, HistoricalConfig};
use perppulse::AsOf;

fn inputs() -> (HistoricalConfig, ProtocolEventSlice, AsOf, AsOf) {
    let mut fixture =
        load_fixture(repo_root().join("fixtures/golden/canonical-market-inputs.json")).unwrap();
    for event in &mut fixture.events {
        event.provenance = Some(CanonicalProvenance {
            envio_id: event.event_id().unwrap().key(),
            parent_hash: "0xparent".to_string(),
            payload_json: "{}".to_string(),
            schema_version: perppulse::envio::MARKET_SCHEMA_VERSION.to_string(),
            handler_version: perppulse::envio::MARKET_HANDLER_VERSION.to_string(),
            classifier_version: perppulse::envio::MARKET_CLASSIFIER_VERSION.to_string(),
            ingestion_profile: perppulse::envio::MARKET_INGESTION_PROFILE.to_string(),
            abi_fingerprint: perppulse::envio::MARKET_ABI_FINGERPRINT.to_string(),
        });
    }
    let first = fixture.events.first().unwrap();
    let last = fixture.events.last().unwrap();
    let start = AsOf::new(
        143,
        first.block_number,
        first.block_hash.clone(),
        first.timestamp_ms,
        None,
    )
    .unwrap();
    let cutoff = fixture.as_of;
    let slice = ProtocolEventSlice {
        coverage: EnvioCoverage {
            evidence: perppulse::coverage::CoverageEvidence {
                chain_id: 143,
                start_block: start.block_number,
                processed_block: cutoff.block_number,
            },
            source_block: cutoff.block_number + 1000,
            events_processed: fixture.events.len() as u64,
            is_ready: false,
            latest_event: IndexedPoint {
                id: last.event_id().unwrap().key(),
                block_number: last.block_number,
                block_hash: last.block_hash.clone(),
                log_index: last.log_index,
                timestamp_ms: last.timestamp_ms,
            },
        },
        events: fixture.events,
    };
    let proof = serde_json::json!({"version":"historical-market-proof-v1", "status":"matched",
        "chainId":143, "asOfBlock":cutoff.block_number, "asOfBlockHash":cutoff.block_hash,
        "asOfTimestampMs":cutoff.timestamp_ms, "registryInputsHash":perppulse::evidence::digest(&fixture.registry).unwrap(),
        "canonicalInputsHash":perppulse::evidence::digest(&slice.events).unwrap(),
        "collateralDecimals":fixture.registry.collateral.decimals,
        "markets":fixture.registry.markets.values().filter(|m| m.listed).map(|m| serde_json::json!({
            "perpetualId":m.perpetual_id,"priceDecimals":m.price_decimals,"sizeDecimals":m.size_decimals,
            "initMarginFracHdths":m.init_margin_frac_hdths,"maintMarginFracHdths":m.maint_margin_frac_hdths
        })).collect::<Vec<_>>()});
    let config = HistoricalConfig {
        client: EnvioClient::new("http://127.0.0.1:1", None, 1000, 50_000).unwrap(),
        registry: fixture.registry,
        accounts: vec![42, 7],
        rpc_url: "http://127.0.0.1:1".to_string(),
        start_block: start.block_number,
        end_block: cutoff.block_number,
        expected_event_count: slice.events.len(),
        registry_proof: proof,
    };
    (config, slice, start, cutoff)
}

#[test]
fn historical_parameters_require_exact_registry_cutoff_and_all_event_markets() {
    let (mut config, slice, start, cutoff) = inputs();
    config.registry_proof["markets"] = serde_json::json!([]);
    assert!(build_historical_snapshot(&config, &slice, &start, &cutoff).is_err());
    let (mut config, slice, start, cutoff) = inputs();
    config.registry_proof["asOfBlockHash"] = serde_json::json!("0xwrong");
    assert!(build_historical_snapshot(&config, &slice, &start, &cutoff).is_err());
    let (mut config, slice, start, cutoff) = inputs();
    config.registry.markets.get_mut(&1).unwrap().price_decimals += 1;
    assert!(build_historical_snapshot(&config, &slice, &start, &cutoff).is_err());
}

#[test]
fn historical_account_birth_without_an_open_position_has_no_empty_funding_scope() {
    let (mut config, mut slice, start, cutoff) = inputs();
    slice.events.retain(|e| e.perpetual_id.is_none());
    config.accounts = vec![42];
    config.expected_event_count = 1;
    slice.coverage.events_processed = 1;
    let last = &slice.events[0];
    slice.coverage.latest_event = IndexedPoint {
        id: last.event_id().unwrap().key(),
        block_number: last.block_number,
        block_hash: last.block_hash.clone(),
        log_index: last.log_index,
        timestamp_ms: last.timestamp_ms,
    };
    config.registry_proof["canonicalInputsHash"] =
        serde_json::json!(perppulse::evidence::digest(&slice.events).unwrap());
    let snapshot = build_historical_snapshot(&config, &slice, &start, &cutoff).unwrap();
    assert_eq!(snapshot.wallets[0]["replayEligible"], true);
    assert!(snapshot.wallets[0]["positions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(snapshot.wallets[0]["freeBalance"].is_null());
}

#[test]
fn historical_flows_are_real_engine_outputs_without_inventing_protocol_or_account_state() {
    let (config, slice, start, cutoff) = inputs();
    let snapshot = build_historical_snapshot(&config, &slice, &start, &cutoff).unwrap();
    assert_eq!(snapshot.mode, "historical");
    assert_eq!(
        snapshot.protocol["takerVolume"]
            .as_str()
            .unwrap()
            .parse::<rust_decimal::Decimal>()
            .unwrap(),
        rust_decimal::Decimal::from(70000)
    );
    assert!(snapshot.protocol["openInterest"].is_null());
    assert!(snapshot.protocol["tvl"].is_null());
    assert_eq!(snapshot.coverage["isLive"], false);
    assert_eq!(snapshot.manifest["protocolEventCount"], slice.events.len());
    let covered = &snapshot.wallets[0];
    assert_eq!(covered["replayEligible"], true);
    assert_eq!(covered["replayBasis"], "account-birth");
    assert!(covered["freeBalance"].is_null());
    assert_eq!(
        covered["positions"][0]["unrealizedPricePnl"]
            .as_str()
            .unwrap()
            .parse::<rust_decimal::Decimal>()
            .unwrap(),
        rust_decimal::Decimal::from(1000)
    );
    assert!(covered["positions"][0]["liquidationPrice"].is_null());
    assert_eq!(snapshot.wallets[1]["replayEligible"], false);
    assert!(snapshot.wallets[1]["realizedPnl"].is_null());
    assert!(snapshot.analytics["windows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["id"] != "coverage")
        .all(|w| w["status"] == "incomplete"));
}

#[test]
fn historical_scope_rejects_changed_count_cutoff_headers_and_duplicate_coordinates() {
    let (mut config, mut slice, start, cutoff) = inputs();
    config.expected_event_count += 1;
    assert!(build_historical_snapshot(&config, &slice, &start, &cutoff).is_err());
    config.expected_event_count -= 1;
    let mut wrong = cutoff.clone();
    wrong.block_hash = "0xdifferent".to_string();
    assert!(build_historical_snapshot(&config, &slice, &start, &wrong).is_err());
    slice.coverage.evidence.processed_block += 1;
    assert!(build_historical_snapshot(&config, &slice, &start, &cutoff).is_err());
    slice.coverage.evidence.processed_block -= 1;
    slice.events[2].log_index = slice.events[1].log_index;
    assert!(build_historical_snapshot(&config, &slice, &start, &cutoff).is_err());
}
