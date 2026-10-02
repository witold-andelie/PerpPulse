use perppulse::accounting::{account_wallet, MAX_MARK_AGE_MS};
use perppulse::events::{load_fixture, repo_root};
use perppulse::ledger::replay;
use perppulse::market_inputs::inspect_public_context;
use perppulse::registry::{load_registry, parse_registry_value};
use serde_json::{json, Value};

fn registry() -> perppulse::ProtocolRegistry {
    load_registry(repo_root().join("fixtures/protocol/mainnet-registry.json")).unwrap()
}

fn context() -> Value {
    json!({"chain":{"chain_id":143},
        "instances":[{"id":1,"address":"0x34b6552d57a35a1d042ccae1951bd1c370112a6f","collateral_token_id":1}],
        "tokens":[{"id":1,"address":"0x00000000efe302beaa2b3e6e1b18d08d69a9012a","symbol":"AUSD","decimals":6}],
        "markets":[{"id":1,"perpetual_id":1,"instance_id":1,"symbol":"","name":"BTC",
            "config":{"at":{"b":54773020,"t":1770000000000_i64},"is_open":true,
                "price_decimals":1,"size_decimals":5,"initial_margin":1500,"maintenance_margin":2500,"contract_version":[1,7,5]},
            "state":{"at":{"b":54773030,"t":1770000001000_i64},"mrk":710000,"oi":999999,"tvl":"999999"}}],
        "geo_block":"synthetic-private-location","unrelated_provider_field":"must-not-be-exported"})
}

#[test]
fn context_export_preserves_observation_and_keeps_rest_values_out_of_accounting() {
    let value = inspect_public_context(&context(), &registry(), 1770000002000).unwrap();
    assert_eq!(value["fields"]["markets"][0]["market"]["symbol"], "BTC");
    assert_eq!(value["fields"]["markets"][0]["observedMarkPns"], "710000");
    assert_eq!(value["accountingEligible"], false);
    assert_eq!(value["canonicalRegistryUpdated"], false);
    assert_eq!(
        value["fields"]["markets"][0]["markAccountingEligible"],
        false
    );
    assert_eq!(value["observedAtMs"], 1770000002000_i64);
    let serialized = value.to_string();
    for denied in [
        "synthetic-private-location",
        "unrelated_provider_field",
        "must-not-be-exported",
        "999999",
    ] {
        assert!(!serialized.contains(denied));
    }
    let reordered = inspect_public_context(&context(), &registry(), 1770000003000).unwrap();
    assert_eq!(value["fieldsHash"], reordered["fieldsHash"]);
    let stale = inspect_public_context(&context(), &registry(), 1770000061000).unwrap();
    assert_eq!(stale["stateQuality"][0]["status"], "stale-state");
    assert_eq!(stale["accountingEligible"], false);
    assert_eq!(value["fieldsHash"], stale["fieldsHash"]);
}

#[test]
fn context_detects_new_and_changed_markets_without_updating_the_ledger_registry() {
    let mut input = context();
    let mut new = input["markets"][0].clone();
    new["perpetual_id"] = json!(70);
    new["symbol"] = json!("VVV");
    new["name"] = json!("VVV");
    input["markets"].as_array_mut().unwrap().push(new);
    input["markets"][0]["config"]["maintenance_margin"] = json!(3000);
    let base = registry();
    let value = inspect_public_context(&input, &base, 1770000002000).unwrap();
    let changes = value["registryDifferences"].as_array().unwrap();
    assert!(changes
        .iter()
        .any(|row| row["perpetualId"] == 70 && row["kind"] == "new-market"));
    assert!(changes
        .iter()
        .any(|row| row["perpetualId"] == 1 && row["kind"] == "metadata-changed"));
    assert!(!base.markets.contains_key(&70));
    assert_eq!(base.markets[&1].maint_margin_frac_hdths, 2500);
}

#[test]
fn malformed_inconsistent_or_future_context_never_becomes_an_empty_success() {
    for pointer in [
        "/chain/chain_id",
        "/instances/0/address",
        "/tokens/0/decimals",
        "/markets/0/config/price_decimals",
        "/markets/0/config/initial_margin",
        "/markets/0/config/at/t",
        "/markets/0/state/mrk",
        "/markets/0/instance_id",
        "/markets/0/config/is_open",
        "/markets/0/config/contract_version",
        "/markets/0/symbol",
    ] {
        let mut value = context();
        *value.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(
            inspect_public_context(&value, &registry(), 1770000002000).is_err(),
            "{pointer}"
        );
    }
    for (pointer, replacement) in [
        ("/chain/chain_id", json!(10143)),
        ("/tokens/0/decimals", json!(18)),
        ("/markets/0/config/price_decimals", json!(19)),
        ("/markets/0/config/maintenance_margin", json!(100)),
        ("/markets/0/config/at/t", json!(1770000003000_i64)),
        ("/markets/0/state/at/t", json!(1770000003000_i64)),
        ("/markets/0/state/mrk", json!(0)),
        ("/markets/0/name", json!("Bitcoin market")),
        ("/markets", json!([])),
    ] {
        let mut value = context();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            inspect_public_context(&value, &registry(), 1770000002000).is_err(),
            "{pointer}"
        );
    }
    let mut duplicate = context();
    let row = duplicate["markets"][0].clone();
    duplicate["markets"].as_array_mut().unwrap().push(row);
    assert!(inspect_public_context(&duplicate, &registry(), 1770000002000).is_err());
}

#[test]
fn registry_rejects_unsafe_scales_addresses_exclusions_and_margin_parameters() {
    let base: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("fixtures/protocol/mainnet-registry.json"))
            .unwrap(),
    )
    .unwrap();
    for (pointer, replacement) in [
        ("/markets/0/price_decimals", json!(19)),
        ("/markets/0/size_decimals", json!(19)),
        ("/markets/0/init_margin_frac_hdths", json!(100)),
        ("/markets/0/maint_margin_frac_hdths", json!(1400)),
        ("/markets/0/symbol", json!("")),
        ("/collateral/decimals", json!(19)),
        ("/exchange_address", json!("0xinvalid")),
        ("/excluded_perpetuals", json!([1])),
    ] {
        let mut value = base.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            parse_registry_value(&value, "synthetic invalid registry").is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn marks_reject_staleness_nonpositive_values_duplicates_and_log_lookahead() {
    let fixture =
        load_fixture(repo_root().join("fixtures/golden/open-position-as-of.json")).unwrap();
    let ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let check = |marks: &[perppulse::events::MarketMark], as_of: &perppulse::AsOf| {
        account_wallet(&ledger, 42, as_of, marks, true)
    };
    let mut fresh = fixture.marks.clone();
    fresh[0].timestamp_ms = fixture.as_of.timestamp_ms - MAX_MARK_AGE_MS + 1;
    assert!(check(&fresh, &fixture.as_of).is_ok());
    fresh[0].timestamp_ms -= 1;
    assert!(check(&fresh, &fixture.as_of)
        .unwrap_err()
        .to_string()
        .contains("stale"));
    for invalid_price in [0, -1, i128::MAX] {
        let mut marks = fixture.marks.clone();
        marks[0].mark_pns = invalid_price;
        assert!(check(&marks, &fixture.as_of).is_err());
    }
    let mut duplicate = fixture.marks.clone();
    duplicate.push(duplicate[0].clone());
    assert!(check(&duplicate, &fixture.as_of).is_err());
    let mut cutoff = fixture.as_of.clone();
    cutoff.log_index = Some(5);
    assert!(check(&fixture.marks, &cutoff).is_err());
    let mut proven = fixture.marks.clone();
    proven[0].block_hash = Some(cutoff.block_hash.clone());
    proven[0].log_index = Some(5);
    assert!(check(&proven, &cutoff).is_ok());
    proven[0].log_index = Some(6);
    assert!(check(&proven, &cutoff).is_err());
    proven[0].log_index = Some(5);
    proven[0].block_hash = Some("0xother".into());
    assert!(check(&proven, &cutoff).is_err());
}

#[test]
fn oversized_accounting_inputs_return_errors_without_decimal_panics() {
    let fixture =
        load_fixture(repo_root().join("fixtures/golden/open-position-as-of.json")).unwrap();
    let mut ledger = replay(&fixture.events, &fixture.registry, &fixture.as_of).unwrap();
    let market = ledger.registry.markets.get_mut(&1).unwrap();
    market.price_decimals = 0;
    market.size_decimals = 0;
    ledger.registry.collateral.decimals = 0;
    let position = ledger.positions.values_mut().next().unwrap();
    position.entry_pns = 70_000;
    position.lot_lns = 1;
    position.deposit_cns = rust_decimal::Decimal::MAX.mantissa();
    let mut marks = fixture.marks.clone();
    marks[0].mark_pns = 71_000;
    let result =
        std::panic::catch_unwind(|| account_wallet(&ledger, 42, &fixture.as_of, &marks, true));
    assert!(result.is_ok());
    assert!(result.unwrap().is_err());
}
