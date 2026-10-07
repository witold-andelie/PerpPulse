use perppulse::envio::EnvioClient;
use perppulse::live::{fetch_snapshot, LiveConfig};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

struct Mock {
    endpoint: String,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Mock {
    fn new(handler: impl Fn(Value) -> Value + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body = handler(serde_json::from_slice(&body).unwrap()).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        Self {
            endpoint,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let result = self.worker.take().unwrap().join();
        if !std::thread::panicking() {
            assert!(result.is_ok(), "mock server failed");
        }
    }
}

fn row(name: &str, log: u32, payload: Value, kind: &str) -> Value {
    json!({"id":format!("143:0xblock:0xtx:{log}"),"chainId":143,"blockNumber":"54773030","blockHash":"0xblock","parentHash":"0xparent","txHash":"0xtx",
        "logIndex":log,"timestampMs":"1770000600000","srcAddress":"0x34B6552d57a35a1D042CcAe1951BD1C370112a6F","abiEventName":name,"kind":kind,
        "accountId":"42","perpetualId":if log==0{None}else{Some(1)},"positionType":payload.get("positionType").and_then(Value::as_str).map(|v|v.parse::<u8>().unwrap()),"payloadJson":payload.to_string(),
        "schemaVersion":"canonical-event-v4","handlerVersion":"envio-handlers-v4","classifierVersion":"exchange-classifier-v3","ingestionProfile":"risk-hotpath-v2",
        "abiFingerprint":"sha256:b98e14a49e4201d71feeae380261784fc8872aa45b201d193194c6c5d56adbf1"})
}

fn archived_mock(corrupt_header: bool, regress: bool) -> Mock {
    let reads = AtomicUsize::new(0);
    let mut created = row(
        "AccountCreated",
        0,
        json!({"id":"42","account":"0x1111111111111111111111111111111111111111"}),
        "ACCOUNT_CREATED",
    );
    if corrupt_header {
        created["blockHash"] = json!("0xdifferent");
    }
    let point = json!({"id":"143:0xlater:0xtx:1", "blockNumber":"54773035",
        "blockHash":"0xlater", "logIndex":1, "timestampMs":"1770000605000"});
    Mock::new(move |body| {
        let query = body["query"].as_str().unwrap();
        if query.contains("PerpPulseRustCoverage") {
            let read = reads.fetch_add(1, Ordering::SeqCst);
            let progress = if regress && read > 0 {
                "54773035"
            } else {
                "54773036"
            };
            json!({"data":{"_meta":[{"chainId":143,"startBlock":"54773010",
                "progressBlock":progress,"sourceBlock":"54773035","eventsProcessed":"2",
                "isReady":false}],"CanonicalEvent":[point]}})
        } else if query.contains("PerpPulseRustVerifyEvent") {
            json!({"data":{"CanonicalEvent":[point]}})
        } else {
            assert_eq!(body["variables"]["endBlock"], "54773030");
            json!({"data":{"CanonicalEvent":[created]}})
        }
    })
}

#[test]
fn explicit_archive_accepts_retained_whole_block_but_live_still_rejects_it() {
    let server = archived_mock(false, false);
    let client = EnvioClient::new(&server.endpoint, None, 500, 100).unwrap();
    let cutoff = perppulse::AsOf::new(143, 54773030, "0xblock", 1770000600000, None).unwrap();
    let slice = client.fetch_archived_account_at(42, &cutoff).unwrap();
    assert_eq!(slice.as_of, cutoff);
    assert!(!slice.coverage.is_ready);
    assert_eq!(slice.coverage.evidence.processed_block, 54773036);
    assert_eq!(slice.coverage.source_block, 54773035);
    assert_eq!(slice.events.len(), 1);
    assert!(client.fetch_account_at(42, &slice.coverage).is_err());
    let mut partial = cutoff.clone();
    partial.log_index = Some(0);
    assert!(client.fetch_archived_account_at(42, &partial).is_err());
    let beyond_source =
        perppulse::AsOf::new(143, 54773036, "0xblock", 1770000600000, None).unwrap();
    assert!(client
        .fetch_archived_account_at(42, &beyond_source)
        .is_err());
}

#[test]
fn archive_rejects_wrong_header_and_regressing_retained_coverage() {
    let cutoff = perppulse::AsOf::new(143, 54773030, "0xblock", 1770000600000, None).unwrap();
    for (wrong_header, regress) in [(true, false), (false, true)] {
        let server = archived_mock(wrong_header, regress);
        let client = EnvioClient::new(&server.endpoint, None, 500, 100).unwrap();
        assert!(client.fetch_archived_account_at(42, &cutoff).is_err());
    }
    let server = archived_mock(false, false);
    let client = EnvioClient::new(&server.endpoint, None, 500, 100).unwrap();
    let wrong_time = perppulse::AsOf::new(143, 54773030, "0xblock", 1770000600001, None).unwrap();
    assert!(client.fetch_archived_account_at(42, &wrong_time).is_err());
}
fn mock(head: u64, corrupt_subject: bool) -> Mock {
    mock_side(head, corrupt_subject, 0)
}
fn mock_side(head: u64, corrupt_subject: bool, wire_side: u8) -> Mock {
    mock_metadata(head, corrupt_subject, wire_side, 0)
}
fn mock_metadata(
    head: u64,
    corrupt_subject: bool,
    wire_side: u8,
    inconsistent_reads: usize,
) -> Mock {
    mock_watermarks(head, corrupt_subject, wire_side, inconsistent_reads, false)
}
fn mock_watermarks(
    head: u64,
    corrupt_subject: bool,
    wire_side: u8,
    inconsistent_reads: usize,
    moving: bool,
) -> Mock {
    mock_precision(
        head,
        corrupt_subject,
        wire_side,
        inconsistent_reads,
        moving,
        None,
    )
}
fn mock_precision(
    head: u64,
    corrupt_subject: bool,
    wire_side: u8,
    inconsistent_reads: usize,
    moving: bool,
    residue: Option<&'static str>,
) -> Mock {
    let coverage_reads = AtomicUsize::new(0);
    let created = row(
        "AccountCreated",
        0,
        json!({"id":"42","account":"0x1111111111111111111111111111111111111111"}),
        "ACCOUNT_CREATED",
    );
    let mut payload = json!({"accountId":"42","perpId":"1","positionType":wire_side.to_string(),"leverageHdths":"700","depositCNS":"10000000000","pricePNS":"700000","lotLNS":"100000","insFeeCNS":"0","protFeeCNS":"69000"});
    if let Some(value) = residue {
        if value != "missing" {
            payload["priceResiduePNSQ16"] = json!(value);
        }
    }
    let mut opened = row(
        if residue.is_some() {
            "PositionOpenedV2"
        } else {
            "PositionOpened"
        },
        1,
        payload,
        "POSITION_OPENED",
    );
    if corrupt_subject {
        opened["accountId"] = json!("99");
    }
    let point = json!({"id":"143:0xblock:0xtx:1","blockNumber":"54773030","blockHash":"0xblock","logIndex":1,"timestampMs":"1770000600000"});
    Mock::new(move |body| {
        if body["method"] == "eth_chainId" {
            return json!({"jsonrpc":"2.0","id":2,"result":"0x8f"});
        }
        if body["method"] == "eth_blockNumber" {
            return json!({"jsonrpc":"2.0","id":1,"result":format!("0x{head:x}")});
        }
        let query = body["query"].as_str().unwrap();
        if query.contains("PerpPulseRustCoverage") {
            let read = coverage_reads.fetch_add(1, Ordering::SeqCst);
            let source = if moving {
                (54773034 + read).to_string()
            } else if read < inconsistent_reads {
                "54773034".into()
            } else {
                "54773040".into()
            };
            let progress = if moving { 54773035 + read } else { 54773035 };
            json!({"data":{"_meta":[{"chainId":143,"startBlock":"54773010","progressBlock":progress.to_string(),"sourceBlock":source,"eventsProcessed":"2","isReady":true}],"CanonicalEvent":[point]}})
        } else if query.contains("PerpPulseRustVerifyEvent") {
            json!({"data":{"CanonicalEvent":[point]}})
        } else {
            assert_eq!(body["variables"]["endBlock"], "54773030");
            json!({"data":{"CanonicalEvent":[created,opened]}})
        }
    })
}
fn config(mock: &Mock) -> LiveConfig {
    LiveConfig {
        client: EnvioClient::new(&mock.endpoint, None, 500, 100).unwrap(),
        registry: perppulse::load_registry(
            perppulse::events::repo_root().join("fixtures/protocol/mainnet-registry.json"),
        )
        .unwrap(),
        accounts: vec![42],
        rpc_url: mock.endpoint.clone(),
        refresh_seconds: 10,
        max_lag_blocks: 100,
        database_url: None,
        nansen: None,
        protocol: None,
    }
}

fn v3_row(mut value: Value) -> Value {
    value["schemaVersion"] = json!(perppulse::envio::MARKET_SCHEMA_VERSION);
    value["handlerVersion"] = json!(perppulse::envio::MARKET_HANDLER_VERSION);
    value["classifierVersion"] = json!(perppulse::envio::MARKET_CLASSIFIER_VERSION);
    value["ingestionProfile"] = json!(perppulse::envio::MARKET_INGESTION_PROFILE);
    value["abiFingerprint"] = json!(perppulse::envio::MARKET_ABI_FINGERPRINT);
    value
}

fn market_mock(failure: &'static str) -> Mock {
    market_mock_with(failure, Arc::new(std::sync::Mutex::new(Vec::new())))
}

type Cursors = Arc<std::sync::Mutex<Vec<(Value, Value)>>>;

fn market_mock_with(failure: &'static str, protocol_cursors: Cursors) -> Mock {
    let created = v3_row(row(
        "AccountCreated",
        0,
        json!({"id":"42","account":"0x1111111111111111111111111111111111111111"}),
        "ACCOUNT_CREATED",
    ));
    let opened = v3_row(row(
        "PositionOpened",
        1,
        json!({"accountId":"42","perpId":"1","positionType":"0","leverageHdths":"700","depositCNS":"10000000000","pricePNS":"700000","lotLNS":"100000","insFeeCNS":"0","protFeeCNS":"69000"}),
        "POSITION_OPENED",
    ));
    let mut mark = v3_row(row(
        "MarkUpdated",
        2,
        json!({"perpId":"1","pricePNS":"710000"}),
        "MARK_UPDATED",
    ));
    mark["accountId"] = Value::Null;
    mark["positionType"] = Value::Null;
    let mut funding = v3_row(row(
        "FundingEventCompleted",
        3,
        json!({"perpId":"1","fundingEventBlock":"54773040","actualRatePct100k":"1","fundingPricePNS":"700000","fundingPaymentPNS":"1","fundingSumPNS":"5","allowOverwrite":false}),
        "MARKET_FUNDING",
    ));
    funding["accountId"] = Value::Null;
    funding["positionType"] = Value::Null;
    if failure == "funding-covered" {
        funding["blockNumber"] = json!("54773020");
        funding["blockHash"] = json!("0xearlier");
        funding["timestampMs"] = json!("1770000500000");
        funding["id"] = json!("143:0xearlier:0xtx:3");
        let mut payload: Value =
            serde_json::from_str(funding["payloadJson"].as_str().unwrap()).unwrap();
        payload["fundingEventBlock"] = json!("54773025");
        funding["payloadJson"] = json!(payload.to_string());
    }
    let point = json!({"id":"143:0xblock:0xtx:3","blockNumber":"54773030","blockHash":"0xblock","logIndex":3,"timestampMs":"1770000600000"});
    if failure == "future-log" {
        mark["logIndex"] = json!(4);
        mark["id"] = json!("143:0xblock:0xtx:4");
    }
    if failure == "foreign-market" {
        mark["perpetualId"] = json!(10);
    }
    if failure == "legacy-mark" {
        mark["schemaVersion"] = json!("canonical-event-v4");
    }
    if failure == "stale" {
        mark["blockNumber"] = json!("54773020");
        mark["timestampMs"] = json!("1770000540000");
    }
    if failure == "malformed-funding" {
        let mut p: Value = serde_json::from_str(funding["payloadJson"].as_str().unwrap()).unwrap();
        p["allowOverwrite"] = json!("false");
        funding["payloadJson"] = json!(p.to_string());
    }
    let mut fill = v3_row(row(
        "MakerOrderFilled",
        0,
        json!({"accountId":"7","perpId":"1","pricePNS":"700000","lotLNS":"100000","feeCNS":"9000","amountCNS":"0","balanceCNS":"0"}),
        "MAKER_FILL",
    ));
    fill["accountId"] = json!("7");
    fill["blockNumber"] = json!("54773025");
    fill["blockHash"] = json!("0xfill");
    fill["timestampMs"] = json!("1770000550000");
    fill["id"] = json!("143:0xfill:0xtx:0");
    fill["positionType"] = Value::Null;
    fill["perpetualId"] = json!(1);
    let mut protocol_rows = [
        fill,
        created.clone(),
        opened.clone(),
        mark.clone(),
        funding.clone(),
    ];
    if failure == "protocol-legacy" {
        protocol_rows[0]["ingestionProfile"] = json!("risk-hotpath-v2");
    }
    let verifies = AtomicUsize::new(0);
    let market_reads = AtomicUsize::new(0);
    Mock::new(move |body| {
        if body["method"] == "eth_chainId" {
            return json!({"result":"0x8f"});
        }
        if body["method"] == "eth_getBlockByNumber" {
            assert_eq!(body["params"][0], "0x343c51c");
            return json!({"result":{"number":"0x343c51c","timestamp":format!("0x{:x}", 1770000500u64)}});
        }
        let query = body["query"].as_str().unwrap_or_default();
        if query.contains("PerpPulseRustProtocolEvents") {
            protocol_cursors.lock().unwrap().push((
                body["variables"]["cursorBlock"].clone(),
                body["variables"]["cursorLog"].clone(),
            ));
            assert_eq!(body["variables"]["endBlock"], "54773030");
            assert_eq!(body["variables"]["endLog"], 3);
            let cursor = (
                body["variables"]["cursorBlock"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap(),
                body["variables"]["cursorLog"].as_i64().unwrap(),
            );
            let limit = body["variables"]["limit"].as_u64().unwrap() as usize;
            let rows: Vec<Value> = protocol_rows
                .iter()
                .filter(|row| {
                    let position = (
                        row["blockNumber"].as_str().unwrap().parse::<u64>().unwrap(),
                        row["logIndex"].as_i64().unwrap(),
                    );
                    position > cursor && position <= (54773030, 3)
                })
                .take(limit)
                .cloned()
                .collect();
            return json!({"data":{"CanonicalEvent":rows}});
        }
        if query.contains("PerpPulseRustVerifyEvent") && failure == "protocol-rewrite" {
            // The as-of point verifies, but a later reread of the last ingested
            // event observes a rewritten block hash.
            // Second refresh: account (3), market (4), then the aggregator's
            // continuity check of its last ingested event (5).
            if verifies.fetch_add(1, Ordering::SeqCst) == 5 {
                return json!({"data":{"CanonicalEvent":[{"id":"143:0xblock:0xtx:3","blockNumber":"54773030","blockHash":"0xrewritten","logIndex":3,"timestampMs":"1770000600000"}]}});
            }
        }
        if body["method"] == "eth_blockNumber" {
            return json!({"result":format!("0x{:x}",54773040)});
        }
        if query.contains("PerpPulseRustCoverage") {
            let progress = if matches!(failure, "regression" | "archive-regression")
                && market_reads.load(Ordering::SeqCst) > 1
            {
                54773034
            } else {
                54773035
            };
            let source = match failure {
                "stopped" => "54773034",
                "archive-lookahead" => "54773029",
                _ => "54773040",
            };
            let start = if matches!(failure, "bounded-start" | "protocol-bounded") {
                "54773020"
            } else {
                "54773010"
            };
            json!({"data":{"_meta":[{"chainId":143,"startBlock":start,"progressBlock":progress.to_string(),"sourceBlock":source,"eventsProcessed":"4","isReady":failure!="stopped"}],"CanonicalEvent":[point]}})
        } else if query.contains("PerpPulseRustVerifyEvent") {
            json!({"data":{"CanonicalEvent":[point]}})
        } else if query.contains("PerpPulseRustMarketEvents") {
            market_reads.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                body["variables"]["endLog"],
                if matches!(
                    failure,
                    "stopped" | "archive-lookahead" | "archive-regression"
                ) {
                    i32::MAX
                } else {
                    3
                }
            );
            assert_eq!(body["variables"]["endBlock"], "54773030");
            if body["variables"]["abiNames"][0] == "MarkUpdated" {
                assert!(query.contains("logIndex: desc"));
                json!({"data":{"CanonicalEvent":if failure=="missing" {vec![]} else {vec![mark.clone()]}}})
            } else {
                json!({"data":{"CanonicalEvent":[funding]}})
            }
        } else {
            json!({"data":{"CanonicalEvent":[created,opened]}})
        }
    })
}

#[test]
fn archived_market_inputs_accept_retained_coverage_but_never_bypass_live_gates() {
    let server = market_mock("stopped");
    let cfg = config(&server);
    let cutoff = perppulse::AsOf::new(143, 54773030, "0xblock", 1770000600000, None).unwrap();
    assert!(fetch_snapshot(&cfg).is_err());
    let archived = cfg
        .client
        .fetch_archived_market_inputs_at(&[1], &cutoff, &cfg.registry)
        .unwrap();
    assert_eq!(archived.events.len(), 2);
    let timeline = perppulse::funding::timeline(&archived.events, 1, &cutoff).unwrap();
    assert!(timeline["active"].is_null());
    assert_eq!(timeline["pending"][0]["effectiveBlock"], 54773040);
    assert!(fetch_snapshot(&cfg).is_err());
    let outside = market_mock("archive-lookahead");
    let cfg = config(&outside);
    assert!(cfg
        .client
        .fetch_archived_market_inputs_at(&[1], &cutoff, &cfg.registry)
        .is_err());
    let inside_block =
        perppulse::AsOf::new(143, 54773030, "0xblock", 1770000600000, Some(3)).unwrap();
    assert!(cfg
        .client
        .fetch_archived_market_inputs_at(&[1], &inside_block, &cfg.registry)
        .is_err());
    let regressed = market_mock("archive-regression");
    let cfg = config(&regressed);
    assert!(cfg
        .client
        .fetch_archived_market_inputs_at(&[1], &cutoff, &cfg.registry)
        .is_err());
}

#[test]
fn v3_live_marks_drive_price_pnl_and_source_evidence_while_funding_stays_unknown() {
    // Coverage after deployment cannot see a schedule published before it.
    let server = market_mock("bounded-start");
    let snapshot = fetch_snapshot(&config(&server)).unwrap();
    let wallet = &snapshot.wallets[0];
    let p = &wallet["positions"][0];
    assert_eq!(
        rust_decimal::Decimal::from_str_exact(p["unrealizedPricePnl"].as_str().unwrap()).unwrap(),
        rust_decimal::Decimal::from(1000)
    );
    assert_eq!(p["markEventId"], "143:0xblock:0xtx:2");
    assert!(p["unrealizedPnl"].is_null());
    assert!(p["liquidationPrice"].is_null());
    assert_eq!(
        wallet["marketInputs"][0]["pending"][0]["effectiveBlock"],
        54773040
    );
    assert!(wallet["marketInputs"][0]["active"].is_null());
    assert!(snapshot.manifest["marketMarksHash"].is_string());
    assert_eq!(snapshot.events.as_array().unwrap().len(), 4);
}

#[test]
fn deployment_coverage_proves_a_reset_before_the_first_effective_schedule() {
    let server = market_mock("");
    let snapshot = fetch_snapshot(&config(&server)).unwrap();
    let p = &snapshot.wallets[0]["positions"][0];
    assert_eq!(p["riskStatus"], "canonical-funding-covered");
    assert_eq!(p["unrealizedFunding"], "0");
    assert!(p["liquidationPrice"].is_string());
    assert!(p["fundingCheckpoint"]["reason"].is_null());
    assert_eq!(p["fundingCheckpoint"]["coverageStartBlock"], 54773010);
    assert_eq!(
        snapshot.wallets[0]["marketInputs"][0]["pending"][0]["effectiveBlock"],
        54773040
    );
}

#[test]
fn missing_v3_mark_is_visible_without_suppressing_canonical_history() {
    let server = market_mock("missing");
    let snapshot = fetch_snapshot(&config(&server)).unwrap();
    assert_eq!(snapshot.wallets[0]["quality"], "marks-unavailable");
    assert!(snapshot.wallets[0]["unrealizedPricePnl"].is_null());
    assert_eq!(snapshot.wallets[0]["positions"][0]["status"], "open");
}

#[test]
fn complete_v3_market_pages_prove_a_postbaseline_reset_and_hash_its_checkpoint() {
    let server = market_mock("funding-covered");
    let snapshot = fetch_snapshot(&config(&server)).unwrap();
    let wallet = &snapshot.wallets[0];
    let p = &wallet["positions"][0];
    assert_eq!(wallet["quality"], "canonical-funding-covered");
    assert_eq!(p["riskStatus"], "canonical-funding-covered");
    assert_eq!(p["unrealizedFunding"], "0");
    assert_eq!(p["unrealizedPnl"], p["unrealizedPricePnl"]);
    assert!(p["liquidationPrice"].is_string());
    assert_eq!(p["fundingCheckpoint"]["baselineEffectiveBlock"], 54773025);
    assert_eq!(p["fundingCheckpoint"]["throughBlock"], snapshot.as_of_block);
    assert!(snapshot.manifest["fundingCheckpointsHash"].is_string());
    let baseline = p["fundingCheckpoint"]["baselineEventId"].as_str().unwrap();
    assert!(snapshot
        .events
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["eventId"] == baseline));
}

#[test]
fn market_inputs_reject_lookahead_wrong_scope_malformed_funding_and_regression() {
    for failure in [
        "future-log",
        "foreign-market",
        "legacy-mark",
        "stale",
        "malformed-funding",
        "regression",
    ] {
        let server = market_mock(failure);
        assert!(fetch_snapshot(&config(&server)).is_err(), "{failure}");
    }
}

#[test]
fn market_input_limit_and_duplicate_market_scope_never_return_partial_success() {
    let server = market_mock("");
    let mut config = config(&server);
    config.client = EnvioClient::new(&server.endpoint, None, 1, 1).unwrap();
    let coverage = config.client.fetch_coverage(143).unwrap();
    let cutoff = perppulse::AsOf::new(143, 54773030, "0xblock", 1770000600000, Some(3)).unwrap();
    assert!(config
        .client
        .fetch_market_inputs_at(&[1, 1], &coverage, &cutoff, &config.registry)
        .is_err());
    assert!(config
        .client
        .fetch_market_inputs_at(&[1], &coverage, &cutoff, &config.registry)
        .is_err());
}
#[test]
fn live_snapshot_replays_same_cutoff_without_inventing_marks_or_protocol_totals() {
    let mock = mock(54773040, false);
    let snapshot = fetch_snapshot(&config(&mock)).unwrap();
    assert_eq!(snapshot.mode, "live");
    assert_eq!(snapshot.as_of_block, 54773030);
    assert_eq!(snapshot.wallets[0]["replayEligible"], true);
    assert_eq!(snapshot.wallets[0]["positions"][0]["size"], "1.00000");
    assert_eq!(snapshot.wallets[0]["positions"][0]["side"], "long");
    assert!(snapshot.wallets[0]["unrealizedPnl"].is_null());
    assert!(snapshot.protocol["openInterest"].is_null());
    assert_eq!(snapshot.manifest["eventCount"], 2);
}
#[test]
fn live_wire_short_is_normalized_and_unknown_side_is_rejected() {
    let short = mock_side(54773040, false, 1);
    let snapshot = fetch_snapshot(&config(&short)).unwrap();
    assert_eq!(snapshot.wallets[0]["positions"][0]["side"], "short");
    let unknown = mock_side(54773040, false, 2);
    let error = fetch_snapshot(&config(&unknown)).unwrap_err();
    assert!(error
        .to_string()
        .contains("unsupported Perpl positionType 2"));
}
#[test]
fn v2_effective_entry_preserves_long_and_short_q16_rounding() {
    for (wire, expected) in [(0, "69999.95"), (1, "70000.05")] {
        let source = mock_precision(54773040, false, wire, 0, false, Some("32768"));
        let snapshot = fetch_snapshot(&config(&source)).unwrap();
        assert_eq!(snapshot.wallets[0]["positions"][0]["entry"], expected);
    }
    for residue in ["65536", "missing"] {
        let source = mock_precision(54773040, false, 0, 0, false, Some(residue));
        assert!(fetch_snapshot(&config(&source)).is_err());
    }
}
#[test]
fn transient_source_height_race_is_retried_but_persistent_inconsistency_fails() {
    let recovered = mock_metadata(54773040, false, 0, 1);
    assert!(fetch_snapshot(&config(&recovered)).is_ok());
    let inconsistent = mock_metadata(54773040, false, 0, usize::MAX);
    let error = fetch_snapshot(&config(&inconsistent)).unwrap_err();
    assert!(error.to_string().contains("exceeds Envio source block"));
}
#[test]
fn source_height_witness_covers_the_fixed_cutoff_without_chasing_new_progress() {
    let moving = mock_watermarks(54773040, false, 0, 0, true);
    let snapshot = fetch_snapshot(&config(&moving)).unwrap();
    assert_eq!(snapshot.processed_block, 54773035);
}
#[test]
fn stale_coverage_wrong_subject_and_event_limit_never_return_partial_success() {
    let stale = mock(54774040, false);
    assert!(fetch_snapshot(&config(&stale)).is_err());
    let corrupt = mock(54773040, true);
    assert!(fetch_snapshot(&config(&corrupt)).is_err());
    let limited = mock(54773040, false);
    let mut configuration = config(&limited);
    configuration.client = EnvioClient::new(&limited.endpoint, None, 500, 1).unwrap();
    assert!(fetch_snapshot(&configuration).is_err());
}

fn protocol_config(server: &Mock, max_events: usize) -> LiveConfig {
    let mut cfg = config(server);
    cfg.protocol = Some(std::sync::Mutex::new(
        perppulse::live::ProtocolAggregator::new(max_events).unwrap(),
    ));
    cfg
}

#[test]
fn global_reader_serves_deployment_history_totals_and_resumes_incrementally() {
    let cursors: Cursors = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = market_mock_with("", cursors.clone());
    let cfg = protocol_config(&server, 100);
    let snapshot = fetch_snapshot(&cfg).unwrap();
    assert_eq!(snapshot.protocol["scope"], "protocol");
    assert_eq!(snapshot.protocol["quality"], "deployment-history");
    assert_eq!(snapshot.protocol["takerVolume"], "70000.000000");
    assert_eq!(snapshot.protocol["openInterest"], "71000.000000");
    assert_eq!(snapshot.protocol["tvl"], "10000.000000");
    assert_eq!(snapshot.protocol["activeAccounts"], 2);
    assert_eq!(
        perppulse::serve::route(&snapshot, "GET", "/api/protocol").0,
        200
    );
    assert_eq!(
        perppulse::serve::route(&snapshot, "GET", "/api/analytics").0,
        200
    );
    assert_eq!(snapshot.analytics["state"]["status"], "complete");
    assert_eq!(snapshot.analytics["windows"][0]["status"], "complete");
    assert_eq!(snapshot.manifest["protocolEventCount"], 5);
    assert!(snapshot.manifest["protocolEventIdsHash"].is_string());
    assert_eq!(snapshot.signals["scope"], "protocol-and-watchlist");
    // Raw global events are not mirrored: served events remain the watchlist plus market inputs.
    assert!(snapshot
        .events
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["accountId"] != 7));

    let again = fetch_snapshot(&cfg).unwrap();
    assert_eq!(
        again.manifest["protocolEventIdsHash"],
        snapshot.manifest["protocolEventIdsHash"]
    );
    assert_eq!(
        cfg.protocol.as_ref().unwrap().lock().unwrap().event_count(),
        5
    );
    let seen = cursors.lock().unwrap().clone();
    assert_eq!(seen[0], (json!("54773010"), json!(-1)));
    assert_eq!(seen.last().unwrap(), &(json!("54773030"), json!(3)));
}

#[test]
fn global_reader_bound_profile_and_rewrite_fail_without_partial_commit() {
    let server = market_mock("");
    let cfg = protocol_config(&server, 2);
    let error = fetch_snapshot(&cfg).unwrap_err().to_string();
    assert!(error.contains("protocol event bound exceeded"), "{error}");
    assert_eq!(
        cfg.protocol.as_ref().unwrap().lock().unwrap().event_count(),
        0
    );

    let legacy = market_mock("protocol-legacy");
    assert!(fetch_snapshot(&protocol_config(&legacy, 100)).is_err());

    let rewritten = market_mock("protocol-rewrite");
    let cfg = protocol_config(&rewritten, 100);
    fetch_snapshot(&cfg).unwrap();
    let error = fetch_snapshot(&cfg).unwrap_err().to_string();
    assert!(error.contains("changed"), "{error}");
    assert_eq!(
        cfg.protocol.as_ref().unwrap().lock().unwrap().event_count(),
        5
    );
}

#[test]
fn bounded_global_coverage_proves_only_windows_after_its_observed_start() {
    let server = market_mock("protocol-bounded");
    let snapshot = fetch_snapshot(&protocol_config(&server, 100)).unwrap();
    assert_eq!(snapshot.protocol["quality"], "bounded-coverage");
    assert!(snapshot.protocol["openInterest"].is_null());
    assert_eq!(
        snapshot.analytics["coverageStartTimestampMs"],
        1770000500000i64
    );
    assert_eq!(snapshot.analytics["state"]["status"], "unavailable");
    let day = &snapshot.analytics["windows"][0];
    assert_eq!(day["status"], "incomplete");
    assert_eq!(day["basis"], "coverage-after-window-start");
    assert!(day["totals"].is_null());
    assert_eq!(snapshot.analytics["windows"][3]["status"], "complete");
    // Only the market input rows at or after the bounded start are counted.
    assert_eq!(snapshot.manifest["protocolEventCount"], 5);
}
