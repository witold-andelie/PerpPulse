use perppulse::{funding::timeline, AsOf, CanonicalEvent};
use serde_json::json;

const START: u64 = 54773010;
fn event(block: u64, target: u64, payment: i128, sum: i128, overwrite: bool) -> CanonicalEvent {
    serde_json::from_value(json!({"chain_id":143,"block_hash":format!("0x{block:x}"),"tx_hash":format!("0x{block:x}"),
        "block_number":block,"log_index":0,"timestamp_ms":block as i64 * 1000,
        "contract_address":"0x34B6552d57a35a1D042CcAe1951BD1C370112a6F","abi_event_name":"FundingEventCompleted",
        "kind":"market_funding","perpetual_id":1,"funding_rate_pct100k":1,
        "funding_event_block":target,"funding_allow_overwrite":overwrite,
        "funding_payment_pns":payment,"funding_sum_pns":sum})).unwrap()
}
fn cutoff(block: u64) -> AsOf {
    AsOf::new(
        143,
        block,
        format!("0x{block:x}"),
        block as i64 * 1000,
        None,
    )
    .unwrap()
}

#[test]
fn published_funding_is_pending_until_its_effective_block() {
    let event = event(START + 1, START + 10, 3, 100, false);
    let before = timeline(std::slice::from_ref(&event), 1, &cutoff(START + 9)).unwrap();
    assert!(before["active"].is_null());
    assert_eq!(before["pending"].as_array().unwrap().len(), 1);
    let at = timeline(&[event], 1, &cutoff(START + 10)).unwrap();
    assert_eq!(at["active"]["effectiveBlock"], START + 10);
    assert!(at["pending"].as_array().unwrap().is_empty());
    assert!(at["positionFundingAmount"].is_null());
}

#[test]
fn overwritten_payment_keeps_its_prior_sum_and_does_not_apply_twice() {
    let first = event(START + 1, START + 10, 3, 100, false);
    let replacement = event(START + 2, START + 10, -5, 92, true);
    let next = event(START + 11, START + 20, 2, 94, false);
    let view = timeline(
        &[first.clone(), replacement.clone(), next.clone()],
        1,
        &cutoff(START + 20),
    )
    .unwrap();
    assert_eq!(view["replacementCount"], 1);
    assert_eq!(view["active"]["sumPns"], "94");
    let mut unauthorized = replacement.clone();
    unauthorized.funding_allow_overwrite = Some(false);
    assert!(timeline(&[first.clone(), unauthorized], 1, &cutoff(START + 20)).is_err());
    let mut inconsistent = replacement;
    inconsistent.funding_sum_pns = Some(93);
    assert!(timeline(&[first, inconsistent, next], 1, &cutoff(START + 20)).is_err());
}

#[test]
fn discontinuity_late_scheduling_and_native_range_fail_visibly() {
    let first = event(START + 1, START + 10, 3, 100, false);
    let skipped = event(START + 11, START + 20, 2, 103, false);
    assert!(timeline(&[first, skipped], 1, &cutoff(START + 20)).is_err());
    for invalid in [
        event(START + 10, START + 10, 1, 1, false),
        event(START + 1, START + 10, 1i128 << 47, 0, false),
    ] {
        assert!(timeline(&[invalid], 1, &cutoff(START + 20)).is_err());
    }
}

#[test]
fn later_publications_do_not_leak_into_an_earlier_cutoff_or_fake_zero() {
    let later = event(START + 11, START + 20, 2, 103, false);
    let earlier = timeline(&[later], 1, &cutoff(START + 10)).unwrap();
    assert_eq!(earlier["status"], "unavailable");
    assert!(earlier["positionFundingAmount"].is_null());
    assert!(earlier["active"].is_null());
}

#[test]
fn foreign_chain_and_wrong_cutoff_hash_are_rejected() {
    let mut foreign = event(START + 1, START + 10, 3, 100, false);
    foreign.chain_id = 1;
    assert!(timeline(&[foreign], 1, &cutoff(START + 10)).is_err());
    let mut altered = event(START + 1, START + 10, 3, 100, false);
    altered.block_hash = "0xdifferent".into();
    assert!(timeline(&[altered], 1, &cutoff(START + 1)).is_err());
}
