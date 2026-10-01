use perppulse::envio::EnvioClient;
use perppulse::live::{fetch_snapshot, LiveConfig};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, Ordering},
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
        "accountId":"42","perpetualId":if log==0{None}else{Some(1)},"positionType":if log==0{None}else{Some(1)},"payloadJson":payload.to_string(),
        "schemaVersion":"canonical-event-v4","handlerVersion":"envio-handlers-v4","classifierVersion":"exchange-classifier-v3","ingestionProfile":"risk-hotpath-v2",
        "abiFingerprint":"sha256:16b3a4812e63fd11d543879117f21c48976f8a4ea8c9aa487d7c2ac3fc397482"})
}
fn mock(head: u64, corrupt_subject: bool) -> Mock {
    let created = row(
        "AccountCreated",
        0,
        json!({"id":"42","account":"0x1111111111111111111111111111111111111111"}),
        "ACCOUNT_CREATED",
    );
    let mut opened = row(
        "PositionOpened",
        1,
        json!({"accountId":"42","perpId":"1","positionType":"1","leverageHdths":"700","depositCNS":"10000000000","pricePNS":"700000","lotLNS":"100000","insFeeCNS":"0","protFeeCNS":"69000"}),
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
            json!({"data":{"_meta":[{"chainId":143,"startBlock":"54773010","progressBlock":"54773035","sourceBlock":"54773040","eventsProcessed":"2","isReady":true}],"CanonicalEvent":[point]}})
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
    }
}
#[test]
fn live_snapshot_replays_same_cutoff_without_inventing_marks_or_protocol_totals() {
    let mock = mock(54773040, false);
    let snapshot = fetch_snapshot(&config(&mock)).unwrap();
    assert_eq!(snapshot.mode, "live");
    assert_eq!(snapshot.as_of_block, 54773030);
    assert_eq!(snapshot.wallets[0]["replayEligible"], true);
    assert_eq!(snapshot.wallets[0]["positions"][0]["size"], "1.00000");
    assert!(snapshot.wallets[0]["unrealizedPnl"].is_null());
    assert!(snapshot.protocol["openInterest"].is_null());
    assert_eq!(snapshot.manifest["eventCount"], 2);
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
