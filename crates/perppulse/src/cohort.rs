//! Snapshot-scoped wallet comparison and participant context.
//!
//! Statistics are normalized from facts already served for each wallet at one
//! cutoff. Percentiles rank only replay-eligible wallets with a known value;
//! incomplete histories are listed as excluded instead of being ranked. Nansen
//! labels group participants for discovery and never change a number.

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::analytics::ratio;
use crate::error::{DataQualityError, Result};
use crate::signals::parse_wallets;

pub const COHORT_VERSION: &str = "snapshot-cohort-v1";
pub const PERCENTILE_STATISTICS: [&str; 6] = [
    "realizedPnl",
    "unrealizedPricePnl",
    "priceReturnOnCollateral",
    "openNotional",
    "collateralLeverage",
    "fees",
];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exposure {
    pub perpetual_id: u32,
    pub symbol: String,
    pub side: String,
    pub notional: Option<Decimal>,
    pub collateral: Decimal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletStats {
    pub account_id: u64,
    pub open_positions: usize,
    pub closed_positions: usize,
    pub open_notional: Option<Decimal>,
    pub open_collateral: Decimal,
    pub collateral_leverage: Option<Decimal>,
    pub realized_pnl: Option<Decimal>,
    pub unrealized_price_pnl: Option<Decimal>,
    pub price_return_on_collateral: Option<Decimal>,
    pub fees: Option<Decimal>,
    pub exposures: Vec<Exposure>,
    pub percentiles: BTreeMap<String, Option<Decimal>>,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedMarket {
    pub perpetual_id: u32,
    pub symbol: String,
    pub side_a: String,
    pub side_b: String,
    pub same_side: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairOverlap {
    pub account_a: u64,
    pub account_b: u64,
    pub shared_markets: Vec<SharedMarket>,
    pub union_markets: usize,
    pub jaccard: Option<Decimal>,
    pub same_side_markets: usize,
    pub opposing_markets: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelMember {
    pub account_id: u64,
    pub point_in_time_eligible: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelGroup {
    pub label: String,
    pub category: Option<String>,
    pub members: Vec<LabelMember>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelIndex {
    pub source: String,
    pub status: String,
    pub attribution: Option<String>,
    pub groups: Vec<LabelGroup>,
    pub unlabeled_accounts: Vec<u64>,
    pub unavailable_accounts: Vec<u64>,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CohortReport {
    pub version: String,
    pub scope: String,
    pub as_of_block: u64,
    pub members: usize,
    pub statistics: Vec<String>,
    pub percentile_method: String,
    pub wallets: Vec<WalletStats>,
    pub excluded: Vec<Value>,
    pub overlap: Vec<PairOverlap>,
    pub labels: LabelIndex,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletFacts {
    #[serde(default)]
    realized_pnl: Option<Decimal>,
    #[serde(default)]
    unrealized_price_pnl: Option<Decimal>,
    #[serde(default)]
    fees: Option<Decimal>,
    #[serde(default)]
    positions: Vec<PositionFacts>,
    #[serde(default)]
    replay_basis: Option<String>,
    #[serde(default)]
    context: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionFacts {
    perpetual_id: u32,
    symbol: String,
    side: String,
    status: String,
    deposit: Decimal,
    #[serde(default)]
    notional_value: Option<Decimal>,
}

fn checked(value: Option<Decimal>, name: &str) -> Result<Decimal> {
    value.ok_or_else(|| DataQualityError::msg(format!("{name} overflow")))
}

pub fn evaluate(scope: &str, as_of_block: u64, wallets: &Value) -> Result<CohortReport> {
    let parsed = parse_wallets(wallets)?;
    let mut stats = Vec::new();
    let mut excluded = Vec::new();
    let mut contexts = Vec::new();
    for wallet in &parsed {
        let facts: WalletFacts = serde_json::from_value(wallet.raw.clone()).map_err(|error| {
            DataQualityError::msg(format!("invalid wallet facts for comparison: {error}"))
        })?;
        contexts.push((wallet.account_id(), facts.context.clone()));
        if !wallet.eligible() {
            excluded.push(json!({"accountId": wallet.account_id(), "reason": "incomplete-history", "replayBasis": facts.replay_basis}));
            continue;
        }
        stats.push(wallet_stats(wallet.account_id(), &facts)?);
    }
    for name in PERCENTILE_STATISTICS {
        let values: Vec<Option<Decimal>> =
            stats.iter().map(|wallet| statistic(wallet, name)).collect();
        let known: Vec<Decimal> = values.iter().flatten().copied().collect();
        for (wallet, value) in stats.iter_mut().zip(values) {
            wallet
                .percentiles
                .insert(name.to_string(), value.map(|value| midrank(value, &known)));
        }
    }
    let labels = label_index(&contexts);
    for wallet in &mut stats {
        wallet.labels = labels
            .groups
            .iter()
            .filter(|group| {
                group
                    .members
                    .iter()
                    .any(|member| member.account_id == wallet.account_id)
            })
            .map(|group| group.label.clone())
            .collect();
    }
    let overlap = overlaps(&stats)?;
    Ok(CohortReport {
        version: COHORT_VERSION.to_string(),
        scope: scope.to_string(),
        as_of_block,
        members: stats.len(),
        statistics: PERCENTILE_STATISTICS.iter().map(|name| name.to_string()).collect(),
        percentile_method: "Midrank: (values below + half of equal values) / known values * 100, truncated to two decimals. Wallets with an unknown statistic or incomplete history are not ranked for it.".to_string(),
        wallets: stats,
        excluded,
        overlap,
        labels,
    })
}

fn wallet_stats(account_id: u64, facts: &WalletFacts) -> Result<WalletStats> {
    let mut open_notional = Some(Decimal::ZERO);
    let mut open_collateral = Decimal::ZERO;
    let mut exposures = Vec::new();
    let mut open_positions = 0;
    let mut closed_positions = 0;
    for position in &facts.positions {
        if position.status != "open" {
            closed_positions += 1;
            continue;
        }
        open_positions += 1;
        open_collateral = checked(
            open_collateral.checked_add(position.deposit),
            "open collateral",
        )?;
        open_notional = match (open_notional, position.notional_value) {
            (Some(sum), Some(value)) => Some(checked(sum.checked_add(value), "open notional")?),
            _ => None,
        };
        exposures.push(Exposure {
            perpetual_id: position.perpetual_id,
            symbol: position.symbol.clone(),
            side: position.side.clone(),
            notional: position.notional_value,
            collateral: position.deposit,
        });
    }
    let collateral_leverage = open_notional.and_then(|value| ratio(value, open_collateral));
    let price_return_on_collateral = if open_positions == 0 {
        None
    } else {
        facts
            .unrealized_price_pnl
            .and_then(|value| ratio(value, open_collateral))
    };
    Ok(WalletStats {
        account_id,
        open_positions,
        closed_positions,
        open_notional,
        open_collateral,
        collateral_leverage,
        realized_pnl: facts.realized_pnl,
        unrealized_price_pnl: facts.unrealized_price_pnl,
        price_return_on_collateral,
        fees: facts.fees,
        exposures,
        percentiles: BTreeMap::new(),
        labels: Vec::new(),
    })
}

fn statistic(wallet: &WalletStats, name: &str) -> Option<Decimal> {
    match name {
        "realizedPnl" => wallet.realized_pnl,
        "unrealizedPricePnl" => wallet.unrealized_price_pnl,
        "priceReturnOnCollateral" => wallet.price_return_on_collateral,
        "openNotional" => wallet.open_notional,
        "collateralLeverage" => wallet.collateral_leverage,
        "fees" => wallet.fees,
        _ => None,
    }
}

fn midrank(value: Decimal, known: &[Decimal]) -> Decimal {
    let below = known.iter().filter(|other| **other < value).count() as u64;
    let equal = known.iter().filter(|other| **other == value).count() as u64;
    let numerator = Decimal::from(2 * below + equal) * Decimal::ONE_HUNDRED;
    let denominator = Decimal::from(2 * known.len() as u64);
    (numerator / denominator)
        .round_dp_with_strategy(2, RoundingStrategy::ToZero)
        .normalize()
}

fn overlaps(stats: &[WalletStats]) -> Result<Vec<PairOverlap>> {
    let holders: Vec<&WalletStats> = stats
        .iter()
        .filter(|wallet| !wallet.exposures.is_empty())
        .collect();
    let mut pairs = Vec::new();
    for (index, a) in holders.iter().enumerate() {
        for b in &holders[index + 1..] {
            let a_markets: BTreeMap<u32, &Exposure> =
                a.exposures.iter().map(|e| (e.perpetual_id, e)).collect();
            let b_markets: BTreeMap<u32, &Exposure> =
                b.exposures.iter().map(|e| (e.perpetual_id, e)).collect();
            let union: BTreeSet<u32> = a_markets.keys().chain(b_markets.keys()).copied().collect();
            let shared: Vec<SharedMarket> = a_markets
                .iter()
                .filter_map(|(id, left)| {
                    b_markets.get(id).map(|right| SharedMarket {
                        perpetual_id: *id,
                        symbol: left.symbol.clone(),
                        side_a: left.side.clone(),
                        side_b: right.side.clone(),
                        same_side: left.side == right.side,
                    })
                })
                .collect();
            let same = shared.iter().filter(|market| market.same_side).count();
            pairs.push(PairOverlap {
                account_a: a.account_id,
                account_b: b.account_id,
                union_markets: union.len(),
                jaccard: ratio(
                    Decimal::from(shared.len() as u64),
                    Decimal::from(union.len() as u64),
                ),
                same_side_markets: same,
                opposing_markets: shared.len() - same,
                shared_markets: shared,
            });
        }
    }
    Ok(pairs)
}

fn label_index(contexts: &[(u64, Option<Value>)]) -> LabelIndex {
    let mut groups: BTreeMap<(String, Option<String>), Vec<LabelMember>> = BTreeMap::new();
    let mut unlabeled = Vec::new();
    let mut unavailable = Vec::new();
    let mut attribution = None;
    for (account_id, context) in contexts {
        let usable = context
            .as_ref()
            .filter(|value| matches!(value["status"].as_str(), Some("available" | "partial")));
        let Some(context) = usable else {
            unavailable.push(*account_id);
            continue;
        };
        if attribution.is_none() {
            attribution = context["attribution"].as_str().map(str::to_string);
        }
        let point_in_time = context["pointInTimeEligible"].as_bool() == Some(true);
        let labels = context["labels"].as_array().cloned().unwrap_or_default();
        if labels.is_empty() {
            unlabeled.push(*account_id);
        }
        for label in labels {
            let Some(name) = label["label"].as_str() else {
                continue;
            };
            groups
                .entry((
                    name.to_string(),
                    label["category"].as_str().map(str::to_string),
                ))
                .or_default()
                .push(LabelMember {
                    account_id: *account_id,
                    point_in_time_eligible: point_in_time,
                });
        }
    }
    let status = if groups.is_empty() && unlabeled.is_empty() {
        "unavailable"
    } else if unavailable.is_empty() {
        "available"
    } else {
        "partial"
    };
    LabelIndex {
        source: "Nansen".to_string(),
        status: status.to_string(),
        attribution,
        groups: groups
            .into_iter()
            .map(|((label, category), mut members)| {
                members.sort_by_key(|member| member.account_id);
                members.dedup_by_key(|member| member.account_id);
                LabelGroup { label, category, members }
            })
            .collect(),
        unlabeled_accounts: unlabeled,
        unavailable_accounts: unavailable,
        note: "Labels describe who is in this snapshot and are observed separately from the ledger cutoff. They never alter canonical numbers and do not establish protocol-wide cohorts.".to_string(),
    }
}
