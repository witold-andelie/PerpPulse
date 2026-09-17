use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};

use serde_json::{json, Value};

use crate::error::{DataQualityError, Result};
use crate::pipeline::Pulse;

const MAX_EVENTS_IN_SNAPSHOT: usize = 5_000;

/// Read-only API snapshot built once from a deterministic fixture pulse.
/// The server never mutates ledger state and never computes new financial
/// facts per request; it only serializes the pre-gated pulse.
#[derive(Clone, Debug)]
pub struct ApiSnapshot {
    pub fixture_name: String,
    pub chain_id: u64,
    pub exchange_address: String,
    pub as_of_block: u64,
    pub as_of_timestamp_ms: i64,
    pub start_block: u64,
    pub processed_block: u64,
    pub source_note: String,
    pub protocol: Value,
    pub coverage: Value,
    pub wallets: Value,
    pub events: Value,
}

pub fn build_snapshot(pulse: &Pulse) -> Result<ApiSnapshot> {
    let wallets = pulse.wallets(true)?;
    if wallets.is_empty() {
        return Err(DataQualityError::msg(
            "serve snapshot rejected a pulse with no wallets",
        ));
    }
    if pulse.ledger.events.len() > MAX_EVENTS_IN_SNAPSHOT {
        return Err(DataQualityError::msg(format!(
            "serve snapshot rejected {} events above the {} event bound",
            pulse.ledger.events.len(),
            MAX_EVENTS_IN_SNAPSHOT
        )));
    }

    let metrics = &pulse.metrics;
    let protocol = json!({
        "fixture": pulse.fixture.name,
        "chainId": pulse.fixture.registry.chain_id,
        "exchange": pulse.fixture.registry.exchange_address,
        "asOfBlock": metrics.as_of_block,
        "asOfTimestampMs": metrics.as_of_timestamp_ms,
        "lastEventBlock": metrics.last_event_block,
        "lastEventTimestampMs": metrics.last_event_timestamp_ms,
        "takerVolume": metrics.taker_volume.to_string(),
        "openInterest": metrics.open_interest.to_string(),
        "tvl": metrics.tvl.to_string(),
        "protocolFees": metrics.protocol_fees.to_string(),
        "liquidations": metrics.liquidations,
        "activeAccounts": metrics.active_accounts,
        "quality": pulse.quality.status,
        "processedBlock": pulse.quality.processed_block,
        "coverageLagBlocks": pulse.quality.coverage_lag_blocks,
        "eventSilenceBlocks": pulse.quality.event_silence_blocks,
        "markets": metrics.markets.iter().map(|market| json!({
            "perpetualId": market.perpetual_id,
            "symbol": market.symbol,
            "takerVolume": market.taker_volume.to_string(),
            "openInterest": market.open_interest.to_string(),
            "tvl": market.tvl.to_string(),
            "protocolFees": market.protocol_fees.to_string(),
            "insuranceFees": market.insurance_fees.to_string(),
            "fillFees": market.fill_fees.to_string(),
            "liquidations": market.liquidations,
            "liquidationNotional": market.liquidation_notional.to_string(),
            "activeAccounts": market.active_accounts,
            "warnings": market.warnings,
        })).collect::<Vec<_>>(),
        "warnings": metrics.warnings,
    });

    let coverage = json!({
        "chainId": pulse.fixture.coverage.chain_id,
        "startBlock": pulse.fixture.coverage.start_block,
        "processedBlock": pulse.fixture.coverage.processed_block,
        "asOfBlock": pulse.fixture.as_of.block_number,
        "asOfTimestampMs": pulse.fixture.as_of.timestamp_ms,
        "quality": pulse.quality.status,
        "coverageLagBlocks": pulse.quality.coverage_lag_blocks,
        "eventSilenceBlocks": pulse.quality.event_silence_blocks,
        "notes": pulse.quality.notes,
        "rangeNote": "Time ranges are bounded by startBlock; a quick window must not be presented as complete history.",
    });

    let wallets_json = json!(wallets
        .iter()
        .map(|wallet| json!({
            "accountId": wallet.account_id,
            "owner": wallet.owner,
            "freeBalance": wallet.free_balance.to_string(),
            "realizedPnl": wallet.realized_pnl.to_string(),
            "unrealizedPnl": wallet.unrealized_pnl.to_string(),
            "fees": wallet.fees.to_string(),
            "realizedFunding": wallet.realized_funding.to_string(),
            "warnings": wallet.warnings,
            "positions": wallet.positions.iter().map(|position| json!({
                "perpetualId": position.perpetual_id,
                "symbol": position.symbol,
                "side": position.side,
                "status": position.status,
                "size": position.size.to_string(),
                "entry": position.entry.to_string(),
                "mark": position.mark.map(|value| value.to_string()),
                "deposit": position.deposit.to_string(),
                "leverage": position.leverage.map(|value| value.to_string()),
                "unrealizedPnl": position.unrealized_pnl.map(|value| value.to_string()),
                "realizedPnl": position.realized_pnl.to_string(),
                "realizedFunding": position.realized_funding.to_string(),
                "fees": position.fees.to_string(),
                "notionalValue": position.notional_value.map(|value| value.to_string()),
                "liquidationPrice": position.liquidation_price.map(|value| value.to_string()),
                "liquidationBuffer": position.liquidation_buffer.map(|value| value.to_string()),
                "lastEventId": position.last_event_id,
                "warnings": position.warnings,
            })).collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>());

    let mut evidence = Vec::with_capacity(pulse.ledger.events.len());
    for event in &pulse.ledger.events {
        let event_id = event.event_id()?;
        let stored = pulse.store.get(&event_id.key())?;
        evidence.push(json!({
            "eventId": event_id.key(),
            "abi": stored.abi_event_name,
            "kind": format!("{:?}", stored.kind),
            "blockNumber": stored.block_number,
            "blockHash": stored.block_hash,
            "txHash": stored.tx_hash,
            "logIndex": stored.log_index,
            "timestampMs": stored.timestamp_ms,
            "accountId": stored.account_id,
            "perpetualId": stored.perpetual_id,
        }));
    }

    Ok(ApiSnapshot {
        fixture_name: pulse.fixture.name.clone(),
        chain_id: pulse.fixture.registry.chain_id,
        exchange_address: pulse.fixture.registry.exchange_address.clone(),
        as_of_block: pulse.fixture.as_of.block_number,
        as_of_timestamp_ms: pulse.fixture.as_of.timestamp_ms,
        start_block: pulse.fixture.coverage.start_block,
        processed_block: pulse.fixture.coverage.processed_block,
        source_note: "Envio canonical events; Perpl snapshot is a verifier; Nansen is not included.".to_string(),
        protocol,
        coverage: coverage.clone(),
        wallets: wallets_json,
        events: Value::Array(evidence),
    })
}

/// Pure router used by the std HTTP loop and by unit tests.
/// Only GET is served; every other method fails visibly with 405.
pub fn route(snapshot: &ApiSnapshot, method: &str, target: &str) -> (u16, Value) {
    if method != "GET" {
        return (
            405,
            json!({"error": "only GET is supported; the API is read-only"}),
        );
    }
    let path = target.split('?').next().unwrap_or(target);
    let normalized = normalize_path(path);
    if normalized == "/" || normalized == "/health" {
        return (
            200,
            json!({
                "status": "ok",
                "product": "PerpPulse",
                "fixture": snapshot.fixture_name,
                "chainId": snapshot.chain_id,
                "asOfBlock": snapshot.as_of_block,
                "processedBlock": snapshot.processed_block,
            }),
        );
    }
    if normalized == "/api/protocol" {
        return (200, snapshot.protocol.clone());
    }
    if normalized == "/api/coverage" {
        return (200, snapshot.coverage.clone());
    }
    if normalized == "/api/wallets" {
        return (200, snapshot.wallets.clone());
    }
    if let Some(rest) = normalized.strip_prefix("/api/wallet/") {
        if rest.is_empty() || rest.contains('/') {
            return (404, json!({"error": "unknown wallet path"}));
        }
        let account_id: u64 = match rest.parse() {
            Ok(value) => value,
            Err(_) => {
                return (
                    400,
                    json!({"error": "wallet account id must be a non-negative integer"}),
                )
            }
        };
        let wallets = snapshot.wallets.as_array().cloned().unwrap_or_default();
        for wallet in wallets {
            if wallet.get("accountId").and_then(Value::as_u64) == Some(account_id) {
                return (200, wallet);
            }
        }
        return (
            404,
            json!({"error": format!("account {account_id} is not present in the snapshot")}),
        );
    }
    if normalized == "/api/events" {
        return (200, snapshot.events.clone());
    }
    if let Some(rest) = normalized.strip_prefix("/api/event/") {
        if rest.is_empty() {
            return (404, json!({"error": "unknown event path"}));
        }
        let key = percent_decode(rest);
        let events = snapshot.events.as_array().cloned().unwrap_or_default();
        for event in events {
            if event.get("eventId").and_then(Value::as_str) == Some(key.as_str()) {
                return (200, event);
            }
        }
        return (
            404,
            json!({"error": "event id is not present in the snapshot"}),
        );
    }
    (
        404,
        json!({"error": "unknown path; see /health, /api/protocol, /api/wallets, /api/wallet/<id>, /api/events, /api/event/<id>, /api/coverage"}),
    )
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    let mut out = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

fn percent_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                hex_value(bytes[index + 1]),
                hex_value(bytes[index + 2]),
            ) {
                out.push((high * 16 + low) as char);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    }
}

/// Blocking single-threaded HTTP/1.1 loop over the immutable snapshot.
/// Each connection is answered and closed; request bodies are ignored.
pub fn run_server(snapshot: &ApiSnapshot, bind: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(bind).map_err(|err| {
        DataQualityError::msg(format!("cannot bind read-only API to {bind}: {err}"))
    })?;
    println!(
        "PerpPulse read-only API: fixture {} chain {} as-of block {} coverage {} through {} on http://{bind}",
        snapshot.fixture_name,
        snapshot.chain_id,
        snapshot.as_of_block,
        snapshot.start_block,
        snapshot.processed_block
    );
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(err) => {
                eprintln!("PerpPulse API accept failed visibly: {err}");
                continue;
            }
        };
        let (method, target) = match read_request_line(&stream) {
            Ok(line) => line,
            Err(err) => {
                let body = serde_json::to_string(&json!({"error": err.to_string()}))
                    .unwrap_or_else(|_| "{\"error\":\"bad request\"}".to_string());
                let _ = write_response(&mut stream, 400, &body);
                continue;
            }
        };
        let (status, value) = route(snapshot, &method, &target);
        let body = serde_json::to_string(&value)
            .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string());
        if let Err(err) = write_response(&mut stream, status, &body) {
            eprintln!("PerpPulse API write failed visibly: {err}");
        }
    }
    Ok(())
}

fn read_request_line(stream: &std::net::TcpStream) -> Result<(String, String)> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|err| {
        DataQualityError::msg(format!("cannot read HTTP request line: {err}"))
    })?;
    let line = line.trim_end().to_string();
    if line.is_empty() {
        return Err(DataQualityError::msg("empty HTTP request line"));
    }
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| DataQualityError::msg("HTTP request is missing a method"))?;
    let target = parts
        .next()
        .ok_or_else(|| DataQualityError::msg("HTTP request is missing a target"))?;
    Ok((method.to_string(), target.to_string()))
}

fn write_response(stream: &mut std::net::TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\ncache-control: no-store\r\n\r\n",
        reason(status),
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::repo_root;
    use crate::pipeline::run_fixture;

    fn snapshot() -> ApiSnapshot {
        let pulse = run_fixture(
            repo_root().join("fixtures/golden/open-increase-reduce-close.json"),
            Some(0),
        )
        .expect("pulse");
        build_snapshot(&pulse).expect("snapshot")
    }

    #[test]
    fn protocol_wallet_event_and_coverage_routes_are_bounded() {
        let snapshot = snapshot();
        let (status, protocol) = route(&snapshot, "GET", "/api/protocol");
        assert_eq!(status, 200);
        assert_eq!(
            protocol.get("asOfBlock").and_then(Value::as_u64),
            Some(snapshot.as_of_block)
        );

        let (status, wallets) = route(&snapshot, "GET", "/api/wallets");
        assert_eq!(status, 200);
        assert!(wallets.as_array().is_some_and(|rows| !rows.is_empty()));

        let (status, wallet) = route(&snapshot, "GET", "/api/wallet/42");
        assert_eq!(status, 200);
        assert_eq!(wallet.get("accountId").and_then(Value::as_u64), Some(42));

        let (status, missing) = route(&snapshot, "GET", "/api/wallet/999999");
        assert_eq!(status, 404);
        assert!(missing.get("error").is_some());

        let (status, bad_value) = route(&snapshot, "GET", "/api/wallet/not-a-number");
        assert_eq!(status, 400);
        assert!(bad_value.get("error").is_some());

        let (status, events) = route(&snapshot, "GET", "/api/events");
        assert_eq!(status, 200);
        let first = events
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(|row| row.get("eventId"))
            .and_then(Value::as_str)
            .expect("event id")
            .to_string();
        let encoded = first.replace(':', "%3A");
        let (status, event) = route(&snapshot, "GET", &format!("/api/event/{encoded}"));
        assert_eq!(status, 200);
        assert_eq!(
            event.get("eventId").and_then(Value::as_str),
            Some(first.as_str())
        );

        let (status, coverage) = route(&snapshot, "GET", "/api/coverage");
        assert_eq!(status, 200);
        assert!(coverage.get("startBlock").and_then(Value::as_u64).is_some());
    }

    #[test]
    fn unknown_paths_and_methods_fail_visibly() {
        let snapshot = snapshot();
        let (status, _) = route(&snapshot, "GET", "/api/unknown");
        assert_eq!(status, 404);
        let (status, _) = route(&snapshot, "POST", "/api/protocol");
        assert_eq!(status, 405);
    }
}
