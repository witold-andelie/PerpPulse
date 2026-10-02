use crate::error::{DataQualityError, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://api.nansen.ai/api/v1/profiler/address/labels";
struct Budget {
    remaining: u32,
    cache: BTreeMap<String, (Instant, Value)>,
}

/// Optional context only; explicit process request allowance, five-minute
/// cache, no premium endpoints, no automatic retries or pagination.
pub struct NansenClient {
    key: String,
    endpoint: String,
    agent: ureq::Agent,
    budget: Mutex<Budget>,
}
impl NansenClient {
    pub fn new(key: String, allowed_requests: u32) -> Result<Self> {
        if key.is_empty() || !(1..=10).contains(&allowed_requests) {
            return Err(DataQualityError::msg(
                "Nansen requires a key and an explicit allowance of 1..10 requests",
            ));
        }
        Ok(Self {
            key,
            endpoint: ENDPOINT.into(),
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .build()
                .into(),
            budget: Mutex::new(Budget {
                remaining: allowed_requests,
                cache: BTreeMap::new(),
            }),
        })
    }
    pub fn labels(&self, address: &str, cutoff_ms: i64) -> Value {
        self.try_labels(address,cutoff_ms).unwrap_or_else(|error| json!({"source":"Nansen","status":"unavailable","labels":[],"reason":error.to_string(),"affectsCanonicalFacts":false}))
    }
    fn try_labels(&self, address: &str, cutoff_ms: i64) -> Result<Value> {
        if address.len() != 42
            || !address.starts_with("0x")
            || !address[2..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(DataQualityError::msg(
                "Nansen requires a valid canonical owner address",
            ));
        }
        let address = address.to_ascii_lowercase();
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| DataQualityError::msg("Nansen request budget unavailable"))?;
        if let Some((at, value)) = budget.cache.get(&address) {
            if at.elapsed() < Duration::from_secs(300) {
                let mut cached = value.clone();
                cached["cached"] = json!(true);
                cached["pointInTimeEligible"] = json!(cached["observedAtMs"]
                    .as_i64()
                    .is_some_and(|at| at <= cutoff_ms));
                return Ok(cached);
            }
        }
        if budget.remaining == 0 {
            return Err(DataQualityError::msg(
                "Nansen process request allowance exhausted",
            ));
        }
        budget.remaining -= 1;
        let response: Value = self
            .agent
            .post(&self.endpoint)
            .header("apiKey", &self.key)
            .send_json(
                json!({"address":address,"chain":"monad","pagination":{"page":1,"per_page":100}}),
            )
            .map_err(|_| {
                DataQualityError::msg(
                    "Nansen context request failed; canonical facts remain available",
                )
            })?
            .body_mut()
            .read_json()
            .map_err(|_| DataQualityError::msg("invalid Nansen labels response"))?;
        let rows = response["data"]
            .as_array()
            .ok_or_else(|| DataQualityError::msg("Nansen labels data is missing"))?;
        if rows.len() > 100 {
            return Err(DataQualityError::msg("Nansen response exceeds page bound"));
        }
        let mut labels = Vec::new();
        for row in rows {
            let label = row["label"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or_else(|| DataQualityError::msg("invalid Nansen label"))?;
            let category = match row.get("category") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .filter(|s| s.len() <= 128)
                        .ok_or_else(|| DataQualityError::msg("invalid Nansen category"))?,
                ),
            };
            labels.push(json!({"label":label,"category":category}));
        }
        let complete = response["pagination"]["is_last_page"]
            .as_bool()
            .ok_or_else(|| DataQualityError::msg("Nansen pagination completeness is missing"))?;
        let observed = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| DataQualityError::msg("context clock failed"))?
                .as_millis(),
        )
        .map_err(|_| DataQualityError::msg("context timestamp overflow"))?;
        let value = json!({"source":"Nansen","attribution":"Powered by Nansen API","status":if complete {"available"} else {"partial"},"chain":"monad","address":address,
            "labels":labels,"observedAtMs":observed,"cached":false,"pointInTimeEligible":observed<=cutoff_ms,"affectsCanonicalFacts":false,
            "reason":"Context is observed separately from the ledger cutoff. No premium labels or context-derived financial facts are used."});
        budget
            .cache
            .insert(address, (Instant::now(), value.clone()));
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allowance_and_owner_validation_are_fail_closed() {
        assert!(NansenClient::new("".into(), 1).is_err());
        assert!(NansenClient::new("synthetic-test-key".into(), 0).is_err());
        let client = NansenClient::new("synthetic-test-key".into(), 1).unwrap();
        assert_eq!(client.labels("invalid-owner", 0)["status"], "unavailable");
        assert_eq!(client.budget.lock().unwrap().remaining, 1);
    }

    #[test]
    fn labels_are_cached_budgeted_and_keep_their_own_observation_time() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = NansenClient::new("synthetic-test-key".into(), 1).unwrap();
        client.endpoint = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&stream);
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" {
                    break;
                }
                if let Some((key, value)) = header.split_once(':') {
                    if key.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(request["chain"], "monad");
            assert_eq!(request["pagination"]["per_page"], 100);
            let response=json!({"data":[{"label":"Synthetic label","category":"behavioral"},{"label":"Synthetic uncategorized label"}],"pagination":{"is_last_page":true}}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        });
        let owner = "0x1111111111111111111111111111111111111111";
        let first = client.labels(owner, 0);
        assert_eq!(first["status"], "available");
        assert_eq!(first["labels"].as_array().unwrap().len(), 2);
        assert_eq!(first["labels"][1]["category"], Value::Null);
        assert_eq!(first["pointInTimeEligible"], false);
        let cached = client.labels(owner, i64::MAX);
        assert_eq!(cached["cached"], true);
        assert_eq!(cached["pointInTimeEligible"], true);
        assert_eq!(
            client.labels("0x2222222222222222222222222222222222222222", 0)["status"],
            "unavailable"
        );
        assert_eq!(first["affectsCanonicalFacts"], false);
        worker.join().unwrap();
    }
}
