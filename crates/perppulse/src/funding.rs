//! Effective-dated funding observations from the canonical ledger.
//! This timeline proves scheduling, not a position's funding checkpoint or amount.
use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::{AsOf, CanonicalEvent, DataQualityError, LifecycleKind, Result};

pub fn timeline(events: &[CanonicalEvent], market: u32, as_of: &AsOf) -> Result<Value> {
    let mut ordered: Vec<_> = events
        .iter()
        .filter(|e| {
            e.perpetual_id == Some(market)
                && matches!(
                    e.kind,
                    LifecycleKind::MarketFunding | LifecycleKind::FundingScaleUpdated
                )
                && as_of.includes(e.block_number, e.log_index)
        })
        .collect();
    ordered.sort_by_key(|e| (e.block_number, e.log_index));
    let mut schedules: BTreeMap<u64, &CanonicalEvent> = BTreeMap::new();
    let mut scaling_event = None;
    let mut replacements = 0;
    for event in &ordered {
        event.validate()?;
        if event.chain_id != as_of.chain_id
            || event.timestamp_ms > as_of.timestamp_ms
            || (event.block_number == as_of.block_number && event.block_hash != as_of.block_hash)
        {
            return Err(DataQualityError::msg(
                "funding observation differs from the selected cutoff",
            ));
        }
        if event.kind == LifecycleKind::FundingScaleUpdated {
            scaling_event = Some(*event);
            continue;
        }
        let target = event
            .funding_event_block
            .ok_or_else(|| DataQualityError::msg("funding effective block is unverified"))?;
        let payment = event
            .funding_payment_pns
            .ok_or_else(|| DataQualityError::msg("funding payment is unverified"))?;
        let sum = event
            .funding_sum_pns
            .ok_or_else(|| DataQualityError::msg("funding sum is unverified"))?;
        let base = sum
            .checked_sub(payment)
            .ok_or_else(|| DataQualityError::msg("funding base overflow"))?;
        if let Some(previous) = schedules.get(&target) {
            if event.funding_allow_overwrite != Some(true)
                || previous
                    .funding_sum_pns
                    .and_then(|s| s.checked_sub(previous.funding_payment_pns?))
                    != Some(base)
            {
                return Err(DataQualityError::msg(
                    "funding overwrite is unauthorized or changes its prior cumulative sum",
                ));
            }
            replacements += 1;
        } else if let Some((prior_target, previous)) = schedules.last_key_value() {
            if target <= *prior_target || event.block_number < *prior_target {
                return Err(DataQualityError::msg(
                    "funding targets regress or overlap before effectiveness",
                ));
            }
            // A scale update changes the native representation. Never compare
            // differently scaled sums or infer a funding amount across it.
            if scaling_event.is_none_or(|scale| {
                (scale.block_number, scale.log_index) <= (previous.block_number, previous.log_index)
            }) && previous.funding_sum_pns != Some(base)
            {
                return Err(DataQualityError::msg(
                    "funding cumulative sums are discontinuous",
                ));
            }
        }
        schedules.insert(target, event);
    }
    let observation = |event: &&CanonicalEvent| -> Result<Value> {
        Ok(json!({
            "sourceEventId": event.event_id()?.key(), "sourceBlock": event.block_number,
            "effectiveBlock": event.funding_event_block, "paymentPns": event.funding_payment_pns.map(|v| v.to_string()),
            "sumPns": event.funding_sum_pns.map(|v| v.to_string()),
            "role": "raw native-unit funding observation; not a position funding amount"
        }))
    };
    let active = schedules
        .range(..=as_of.block_number)
        .next_back()
        .map(|(_, e)| observation(e))
        .transpose()?;
    let pending = schedules
        .range((
            std::ops::Bound::Excluded(as_of.block_number),
            std::ops::Bound::Unbounded,
        ))
        .map(|(_, e)| observation(e))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"perpetualId": market,
        "status": if schedules.is_empty() {"unavailable"} else {"observed-checkpoint-unverified"},
        "observationCount": ordered.len(), "replacementCount": replacements,
        "active": active, "pending": pending,
        "scalingExponent": scaling_event.and_then(|e| e.funding_scaling_exp),
        "scalingSourceEventId": scaling_event.map(|e| e.event_id().map(|id| id.key())).transpose()?,
        "positionFundingAmount": Value::Null,
        "reason": "Position funding checkpoints and coverage before the first observed funding schedule remain unverified; raw sums are never assumed to be collateral units."
    }))
}
