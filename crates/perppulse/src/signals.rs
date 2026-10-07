//! Deterministic, versioned risk signals and stress scenarios.
//!
//! Signals compare facts that the snapshot already serves (canonical position
//! facts, eligible marks, analytics windows) with published thresholds. They
//! never create prices, PnL or funding. Each rule definition is hashed so a
//! signal can be traced to the exact rule, inputs and source events behind it.

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::analytics::{ratio, AnalyticsReport};
use crate::error::{DataQualityError, Result};
use crate::registry::ProtocolRegistry;

pub const SIGNAL_VERSION: &str = "risk-signals-v1";
pub const STRESS_VERSION: &str = "mark-shock-stress-v1";
/// Resolved signals stay visible for this many blocks after they clear.
pub const RESOLVED_RETENTION_BLOCKS: u64 = 1_000;
/// Liquidations count as recent activity inside this window before the cutoff.
pub const ACTIVITY_WINDOW_MS: i64 = 86_400_000;
const MAX_RESOLVED: usize = 50;
const STRESS_SHOCKS: [&str; 6] = ["-0.20", "-0.10", "-0.05", "0.05", "0.10", "0.20"];

struct Rule {
    id: &'static str,
    category: &'static str,
    scope: &'static str,
    metric: &'static str,
    comparator: &'static str,
    watch: Option<&'static str>,
    warning: Option<&'static str>,
    critical: Option<&'static str>,
    requires: &'static str,
}

const RULES: [Rule; 9] = [
    Rule {
        id: "liquidation-distance",
        category: "risk",
        scope: "position",
        metric: "Side-adjusted distance from the eligible mark to the funded liquidation price, divided by the mark",
        comparator: "lte",
        watch: Some("0.25"),
        warning: Some("0.15"),
        critical: Some("0.05"),
        requires: "Proven unsettled funding checkpoint and an eligible as-of mark",
    },
    Rule {
        id: "collateral-drawdown",
        category: "risk",
        scope: "position",
        metric: "Negative unrealized price PnL divided by isolated position collateral; funding excluded",
        comparator: "gte",
        watch: Some("0.25"),
        warning: Some("0.50"),
        critical: Some("0.75"),
        requires: "Eligible as-of mark and positive isolated collateral",
    },
    Rule {
        id: "leverage-utilization",
        category: "risk",
        scope: "position",
        metric: "Mark notional divided by isolated collateral, divided by the registry initial-margin leverage limit",
        comparator: "gte",
        watch: Some("0.50"),
        warning: Some("0.80"),
        critical: Some("1.00"),
        requires: "Eligible as-of mark; zero collateral is critical without a ratio",
    },
    Rule {
        id: "watchlist-crowding",
        category: "concentration",
        scope: "market",
        metric: "Dominant-side share of mark notional among at least two accounts in this snapshot",
        comparator: "gte",
        watch: Some("0.75"),
        warning: Some("0.90"),
        critical: None,
        requires: "At least two accounts with eligible marks in the market; snapshot scope only",
    },
    Rule {
        id: "market-skew",
        category: "concentration",
        scope: "market",
        metric: "Absolute protocol open-interest skew, |long - short| / (long + short)",
        comparator: "gte",
        watch: Some("0.50"),
        warning: Some("0.75"),
        critical: None,
        requires: "Point-in-time protocol state from deployment history with eligible marks",
    },
    Rule {
        id: "liquidation-activity",
        category: "activity",
        scope: "market",
        metric: "Complete 24-hour liquidation notional divided by current market open interest",
        comparator: "gte",
        watch: Some("0"),
        warning: Some("0.05"),
        critical: Some("0.20"),
        requires: "A complete 24-hour window with at least one liquidation; ratio requires market open interest",
    },
    Rule {
        id: "position-liquidated",
        category: "activity",
        scope: "position",
        metric: "Position closed by liquidation or deleveraging within 24 hours of the cutoff",
        comparator: "event",
        watch: None,
        warning: Some("1"),
        critical: None,
        requires: "Canonical closing lifecycle event inside the snapshot",
    },
    Rule {
        id: "risk-input-unavailable",
        category: "data-quality",
        scope: "position",
        metric: "Open position whose mark or funding checkpoint is unavailable",
        comparator: "presence",
        watch: Some("funding-unverified"),
        warning: Some("mark-unavailable"),
        critical: None,
        requires: "None; reports missing inputs instead of inferring them",
    },
    Rule {
        id: "incomplete-history",
        category: "data-quality",
        scope: "account",
        metric: "Account history does not start at deployment or account creation",
        comparator: "presence",
        watch: None,
        warning: Some("replay-ineligible"),
        critical: None,
        requires: "None; replay is blocked rather than approximated",
    },
];

fn rule(id: &str) -> &'static Rule {
    RULES
        .iter()
        .find(|rule| rule.id == id)
        .expect("signal rule is defined")
}

fn rule_definition(rule: &Rule) -> Value {
    json!({
        "id": rule.id, "version": SIGNAL_VERSION, "category": rule.category, "scope": rule.scope,
        "metric": rule.metric, "comparator": rule.comparator,
        "thresholds": {"watch": rule.watch, "warning": rule.warning, "critical": rule.critical},
        "requires": rule.requires,
    })
}

pub fn rule_definitions() -> Result<Vec<Value>> {
    RULES
        .iter()
        .map(|rule| {
            let mut definition = rule_definition(rule);
            definition["ruleHash"] = json!(crate::evidence::digest(&rule_definition(rule))?);
            Ok(definition)
        })
        .collect()
}

fn rule_hash(rule: &Rule) -> Result<String> {
    crate::evidence::digest(&rule_definition(rule))
}

fn threshold(value: Option<&str>) -> Result<Option<Decimal>> {
    value
        .map(|text| {
            text.parse::<Decimal>()
                .map_err(|_| DataQualityError::msg("invalid signal threshold"))
        })
        .transpose()
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 3,
        "warning" => 2,
        "watch" => 1,
        _ => 0,
    }
}

fn category_rank(category: &str) -> u8 {
    match category {
        "risk" => 4,
        "activity" => 3,
        "concentration" => 2,
        _ => 1,
    }
}

/// Grade a numeric metric; returns the severity and the threshold it crossed.
fn grade(rule: &Rule, value: Decimal) -> Result<Option<(&'static str, Decimal, Decimal)>> {
    let levels = [
        ("critical", threshold(rule.critical)?),
        ("warning", threshold(rule.warning)?),
        ("watch", threshold(rule.watch)?),
    ];
    let watch = threshold(rule.watch)?.unwrap_or(Decimal::ZERO);
    for (name, limit) in levels {
        let Some(limit) = limit else { continue };
        let crossed = match rule.comparator {
            "lte" => value <= limit,
            "gte" => value >= limit,
            _ => false,
        };
        if crossed {
            let score = match rule.comparator {
                "lte" => ratio(watch - value, watch).unwrap_or(Decimal::ZERO),
                _ if watch.is_zero() => value,
                _ => ratio(value - watch, watch).unwrap_or(Decimal::ZERO),
            };
            return Ok(Some((name, limit, score)));
        }
    }
    Ok(None)
}

fn checked(value: Option<Decimal>, name: &str) -> Result<Decimal> {
    value.ok_or_else(|| DataQualityError::msg(format!("{name} overflow")))
}

fn percent(value: Decimal) -> String {
    (value * Decimal::ONE_HUNDRED)
        .round_dp_with_strategy(2, RoundingStrategy::ToZero)
        .normalize()
        .to_string()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub signal_id: String,
    pub rule_id: String,
    pub rule_hash: String,
    pub category: String,
    pub severity: String,
    pub scope: String,
    pub account_id: Option<u64>,
    pub perpetual_id: Option<u32>,
    pub symbol: Option<String>,
    pub side: Option<String>,
    pub metric: Option<Decimal>,
    pub threshold: Option<Decimal>,
    pub score: Decimal,
    pub basis: String,
    pub title: String,
    pub explanation: String,
    pub inputs: Value,
    pub evidence: Value,
    pub as_of_block: u64,
    pub first_observed_as_of_block: u64,
    pub severity_since_as_of_block: u64,
    pub change: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSignal {
    pub signal_id: String,
    pub rule_id: String,
    pub last_severity: String,
    pub title: String,
    pub first_observed_as_of_block: u64,
    pub resolved_as_of_block: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StressPosition {
    pub account_id: u64,
    pub perpetual_id: u32,
    pub symbol: String,
    pub side: String,
    pub shocked_mark: Decimal,
    pub equity: Decimal,
    pub maintenance_margin: Decimal,
    pub collateral: Decimal,
    pub basis: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StressScenario {
    pub shock: Decimal,
    pub positions_evaluated: usize,
    pub breached: usize,
    pub breached_collateral: Decimal,
    pub conditional_breaches: usize,
    pub breaches: Vec<StressPosition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StressReport {
    pub version: String,
    pub method: String,
    pub scenarios: Vec<StressScenario>,
    pub excluded: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalReport {
    pub version: String,
    pub scope: String,
    pub as_of_block: u64,
    pub baseline_as_of_block: Option<u64>,
    pub rules: Vec<Value>,
    pub rules_hash: String,
    pub counts: BTreeMap<String, usize>,
    pub top_signal_ids: Vec<String>,
    pub items: Vec<Signal>,
    pub resolved: Vec<ResolvedSignal>,
    pub stress: StressReport,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletView {
    account_id: u64,
    #[serde(default)]
    positions: Vec<PositionView>,
    #[serde(default)]
    replay_eligible: Option<bool>,
    #[serde(default)]
    replay_basis: Option<String>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionView {
    perpetual_id: u32,
    symbol: String,
    side: String,
    status: String,
    size: Decimal,
    entry: Decimal,
    deposit: Decimal,
    realized_pnl: Decimal,
    #[serde(default)]
    mark: Option<Decimal>,
    #[serde(default)]
    mark_event_id: Option<String>,
    #[serde(default)]
    unrealized_price_pnl: Option<Decimal>,
    #[serde(default)]
    unrealized_funding: Option<Decimal>,
    #[serde(default)]
    notional_value: Option<Decimal>,
    #[serde(default)]
    maintenance_margin: Option<Decimal>,
    #[serde(default)]
    liquidation_price: Option<Decimal>,
    #[serde(default)]
    zero_funding_liquidation_price: Option<Decimal>,
    last_event_id: String,
    #[serde(default)]
    funding_checkpoint: Option<Value>,
}

impl PositionView {
    fn is_long(&self) -> bool {
        self.side == "long"
    }
    fn funding_reset(&self) -> Value {
        self.funding_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint["resetEventId"].clone())
            .unwrap_or(Value::Null)
    }
}

pub(crate) fn parse_wallets(wallets: &Value) -> Result<Vec<WalletJson>> {
    let rows = wallets
        .as_array()
        .ok_or_else(|| DataQualityError::msg("wallet snapshot must be an array"))?;
    rows.iter()
        .map(|row| {
            let view: WalletView = serde_json::from_value(row.clone()).map_err(|error| {
                DataQualityError::msg(format!("invalid wallet facts for signals: {error}"))
            })?;
            Ok(WalletJson {
                raw: row.clone(),
                view,
            })
        })
        .collect()
}

pub(crate) struct WalletJson {
    pub raw: Value,
    view: WalletView,
}

impl WalletJson {
    pub fn account_id(&self) -> u64 {
        self.view.account_id
    }
    pub fn eligible(&self) -> bool {
        self.view.replay_eligible != Some(false)
    }
}

struct Draft {
    rule: &'static Rule,
    severity: &'static str,
    account_id: Option<u64>,
    perpetual_id: Option<u32>,
    symbol: Option<String>,
    side: Option<String>,
    metric: Option<Decimal>,
    threshold: Option<Decimal>,
    score: Decimal,
    basis: &'static str,
    title: String,
    explanation: String,
    inputs: Value,
    evidence: Value,
}

fn finish(draft: Draft, as_of_block: u64) -> Result<Signal> {
    let account = draft
        .account_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".into());
    let market = draft
        .perpetual_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".into());
    Ok(Signal {
        signal_id: format!("{}:{account}:{market}", draft.rule.id),
        rule_id: draft.rule.id.to_string(),
        rule_hash: rule_hash(draft.rule)?,
        category: draft.rule.category.to_string(),
        severity: draft.severity.to_string(),
        scope: draft.rule.scope.to_string(),
        account_id: draft.account_id,
        perpetual_id: draft.perpetual_id,
        symbol: draft.symbol,
        side: draft.side,
        metric: draft.metric,
        threshold: draft.threshold,
        score: draft.score,
        basis: draft.basis.to_string(),
        title: draft.title,
        explanation: draft.explanation,
        inputs: draft.inputs,
        evidence: draft.evidence,
        as_of_block,
        first_observed_as_of_block: as_of_block,
        severity_since_as_of_block: as_of_block,
        change: "baseline".to_string(),
    })
}

/// Inputs for one evaluation; every field comes from the same snapshot cutoff.
pub struct SignalInputs<'a> {
    pub scope: &'a str,
    pub as_of_block: u64,
    pub as_of_timestamp_ms: i64,
    pub wallets: &'a Value,
    pub events: &'a Value,
    pub analytics: Option<&'a AnalyticsReport>,
    pub registry: &'a ProtocolRegistry,
}

pub fn evaluate(inputs: &SignalInputs) -> Result<SignalReport> {
    let wallets = parse_wallets(inputs.wallets)?;
    let event_times: BTreeMap<String, i64> = inputs
        .events
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|event| {
            Some((
                event["eventId"].as_str()?.to_string(),
                event["timestampMs"].as_i64()?,
            ))
        })
        .collect();
    let mut drafts = Vec::new();
    for wallet in &wallets {
        let view = &wallet.view;
        if !wallet.eligible() {
            let rule = rule("incomplete-history");
            drafts.push(Draft {
                rule,
                severity: "warning",
                account_id: Some(view.account_id),
                perpetual_id: None,
                symbol: None,
                side: None,
                metric: None,
                threshold: None,
                score: Decimal::ZERO,
                basis: "coverage",
                title: format!("Account {} history is incomplete", view.account_id),
                explanation: format!(
                    "Position replay is blocked because indexed coverage does not include deployment or the account's creation. {}",
                    view.warnings.join(" ")
                ),
                inputs: json!({"replayBasis": view.replay_basis}),
                evidence: json!({"replayBasis": view.replay_basis}),
            });
            continue;
        }
        for position in &view.positions {
            position_signals(view.account_id, position, inputs, &event_times, &mut drafts)?;
        }
    }
    // Protocol skew supersedes snapshot crowding when complete state exists.
    if inputs
        .analytics
        .is_none_or(|report| report.state.status == "unavailable")
    {
        crowding_signals(&wallets, &mut drafts)?;
    }
    if let Some(analytics) = inputs.analytics {
        protocol_signals(analytics, &mut drafts)?;
    }
    let mut items = drafts
        .into_iter()
        .map(|draft| finish(draft, inputs.as_of_block))
        .collect::<Result<Vec<_>>>()?;
    rank(&mut items);
    let ids: BTreeSet<_> = items.iter().map(|item| item.signal_id.clone()).collect();
    if ids.len() != items.len() {
        return Err(DataQualityError::msg("duplicate signal identity"));
    }
    let rules = rule_definitions()?;
    Ok(SignalReport {
        version: SIGNAL_VERSION.to_string(),
        scope: inputs.scope.to_string(),
        as_of_block: inputs.as_of_block,
        baseline_as_of_block: None,
        rules_hash: crate::evidence::digest(&rules)?,
        rules,
        counts: counts(&items),
        top_signal_ids: items
            .iter()
            .take(3)
            .map(|item| item.signal_id.clone())
            .collect(),
        items,
        resolved: Vec::new(),
        stress: stress(&wallets)?,
    })
}

fn counts(items: &[Signal]) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = ["critical", "warning", "watch"]
        .iter()
        .map(|name| (name.to_string(), 0))
        .collect();
    for item in items {
        *counts.entry(item.severity.clone()).or_default() += 1;
    }
    counts
}

fn rank(items: &mut [Signal]) {
    items.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then(category_rank(&b.category).cmp(&category_rank(&a.category)))
            .then(b.score.cmp(&a.score))
            .then(a.signal_id.cmp(&b.signal_id))
    });
}

fn position_signals(
    account: u64,
    p: &PositionView,
    inputs: &SignalInputs,
    event_times: &BTreeMap<String, i64>,
    drafts: &mut Vec<Draft>,
) -> Result<()> {
    let label = format!("Account {account} {} {}", p.symbol, p.side);
    if matches!(p.status.as_str(), "liquidated" | "deleveraged") {
        let closed_at = event_times.get(&p.last_event_id).copied();
        if closed_at.is_some_and(|at| inputs.as_of_timestamp_ms - at <= ACTIVITY_WINDOW_MS) {
            drafts.push(Draft {
                rule: rule("position-liquidated"),
                severity: "warning",
                account_id: Some(account),
                perpetual_id: Some(p.perpetual_id),
                symbol: Some(p.symbol.clone()),
                side: Some(p.side.clone()),
                metric: None,
                threshold: None,
                score: Decimal::ZERO,
                basis: "canonical",
                title: format!("{label} was {} in the last 24 hours", p.status),
                explanation: format!(
                    "The canonical closing event is inside the snapshot. Lifetime realized PnL for this position is {} AUSD.",
                    p.realized_pnl.normalize()
                ),
                inputs: json!({"status": p.status, "realizedPnl": p.realized_pnl, "closedAtMs": closed_at}),
                evidence: json!({"lastEventId": p.last_event_id}),
            });
        }
        return Ok(());
    }
    if p.status != "open" {
        return Ok(());
    }
    let position_evidence = json!({"lastEventId": p.last_event_id, "markEventId": p.mark_event_id, "fundingResetEventId": p.funding_reset()});
    let Some(mark) = p.mark else {
        drafts.push(Draft {
            rule: rule("risk-input-unavailable"),
            severity: "warning",
            account_id: Some(account),
            perpetual_id: Some(p.perpetual_id),
            symbol: Some(p.symbol.clone()),
            side: Some(p.side.clone()),
            metric: None,
            threshold: None,
            score: Decimal::ZERO,
            basis: "coverage",
            title: format!("{label}: as-of mark unavailable"),
            explanation: "No eligible canonical mark exists at the cutoff, so price PnL, drawdown, leverage and liquidation distance are unavailable.".to_string(),
            inputs: json!({"missing": "mark-unavailable"}),
            evidence: position_evidence,
        });
        return Ok(());
    };
    if mark <= Decimal::ZERO {
        return Err(DataQualityError::msg("signal mark must be positive"));
    }
    match p.liquidation_price {
        Some(liquidation) => {
            let gap = if p.is_long() {
                mark.checked_sub(liquidation)
            } else {
                liquidation.checked_sub(mark)
            };
            let distance =
                ratio(checked(gap, "liquidation distance")?, mark).unwrap_or(Decimal::ZERO);
            let rule = rule("liquidation-distance");
            if let Some((severity, limit, score)) = grade(rule, distance)? {
                drafts.push(Draft {
                    rule,
                    severity,
                    account_id: Some(account),
                    perpetual_id: Some(p.perpetual_id),
                    symbol: Some(p.symbol.clone()),
                    side: Some(p.side.clone()),
                    metric: Some(distance),
                    threshold: Some(limit),
                    score,
                    basis: "canonical",
                    title: format!("{label} is {}% from funded liquidation", percent(distance)),
                    explanation: format!(
                        "Mark {} versus funded liquidation price {}; the position is liquidatable when equity reaches entry-based maintenance. Thresholds: watch 25%, warning 15%, critical 5%.",
                        mark.normalize(), liquidation.normalize()
                    ),
                    inputs: json!({"mark": mark, "liquidationPrice": liquidation, "unrealizedFunding": p.unrealized_funding}),
                    evidence: position_evidence.clone(),
                });
            }
        }
        None if p.unrealized_funding.is_none() => {
            let scenario = p.zero_funding_liquidation_price.and_then(|price| {
                let gap = if p.is_long() {
                    mark.checked_sub(price)
                } else {
                    price.checked_sub(mark)
                };
                gap.and_then(|gap| ratio(gap, mark))
            });
            drafts.push(Draft {
                rule: rule("risk-input-unavailable"),
                severity: "watch",
                account_id: Some(account),
                perpetual_id: Some(p.perpetual_id),
                symbol: Some(p.symbol.clone()),
                side: Some(p.side.clone()),
                metric: None,
                threshold: None,
                score: Decimal::ZERO,
                basis: "zero-funding-conditional",
                title: format!("{label}: funded liquidation risk unverified"),
                explanation: "Unsettled funding has no covered checkpoint, so actual equity and liquidation price stay unavailable. The zero-funding distance is a conditional scenario, not a risk fact.".to_string(),
                inputs: json!({"missing": "funding-unverified", "zeroFundingLiquidationPrice": p.zero_funding_liquidation_price,
                    "zeroFundingDistance": scenario}),
                evidence: position_evidence.clone(),
            });
        }
        None => {}
    }
    if let Some(pnl) = p.unrealized_price_pnl {
        if p.deposit > Decimal::ZERO && pnl < Decimal::ZERO {
            let drawdown = ratio(-pnl, p.deposit).unwrap_or(Decimal::ZERO);
            let rule = rule("collateral-drawdown");
            if let Some((severity, limit, score)) = grade(rule, drawdown)? {
                drafts.push(Draft {
                    rule,
                    severity,
                    account_id: Some(account),
                    perpetual_id: Some(p.perpetual_id),
                    symbol: Some(p.symbol.clone()),
                    side: Some(p.side.clone()),
                    metric: Some(drawdown),
                    threshold: Some(limit),
                    score,
                    basis: "canonical",
                    title: format!("{label} price loss is {}% of collateral", percent(drawdown)),
                    explanation: format!(
                        "Unrealized price PnL {} AUSD against isolated collateral {} AUSD; funding is excluded. Thresholds: watch 25%, warning 50%, critical 75%.",
                        pnl.normalize(), p.deposit.normalize()
                    ),
                    inputs: json!({"unrealizedPricePnl": pnl, "collateral": p.deposit, "mark": mark, "entry": p.entry}),
                    evidence: position_evidence.clone(),
                });
            }
        }
    }
    if let Some(notional) = p.notional_value {
        let market = inputs.registry.market(p.perpetual_id)?;
        let limit = Decimal::from(market.init_margin_frac_hdths) / Decimal::ONE_HUNDRED;
        let rule = rule("leverage-utilization");
        if p.deposit <= Decimal::ZERO {
            drafts.push(Draft {
                rule,
                severity: "critical",
                account_id: Some(account),
                perpetual_id: Some(p.perpetual_id),
                symbol: Some(p.symbol.clone()),
                side: Some(p.side.clone()),
                metric: None,
                threshold: threshold(rule.critical)?,
                score: Decimal::ZERO,
                basis: "canonical",
                title: format!("{label} has no isolated collateral"),
                explanation: "An open position with zero isolated collateral has unbounded collateral leverage.".to_string(),
                inputs: json!({"notional": notional, "collateral": p.deposit, "initialLeverageLimit": limit}),
                evidence: position_evidence,
            });
        } else if let Some(leverage) = ratio(notional, p.deposit) {
            let utilization = ratio(leverage, limit).unwrap_or(Decimal::ZERO);
            if let Some((severity, threshold_value, score)) = grade(rule, utilization)? {
                drafts.push(Draft {
                    rule,
                    severity,
                    account_id: Some(account),
                    perpetual_id: Some(p.perpetual_id),
                    symbol: Some(p.symbol.clone()),
                    side: Some(p.side.clone()),
                    metric: Some(utilization),
                    threshold: Some(threshold_value),
                    score,
                    basis: "canonical",
                    title: format!(
                        "{label} uses {}% of the {}x initial leverage limit",
                        percent(utilization),
                        limit.normalize()
                    ),
                    explanation: format!(
                        "Mark notional {} AUSD on isolated collateral {} AUSD is {}x collateral leverage. Thresholds: watch 50%, warning 80%, critical 100%.",
                        notional.normalize(), p.deposit.normalize(), leverage.normalize()
                    ),
                    inputs: json!({"notional": notional, "collateral": p.deposit, "collateralLeverage": leverage, "initialLeverageLimit": limit}),
                    evidence: position_evidence,
                });
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Crowd {
    symbol: String,
    long: Decimal,
    short: Decimal,
    accounts: BTreeSet<u64>,
    positions: Vec<String>,
}

fn crowding_signals(wallets: &[WalletJson], drafts: &mut Vec<Draft>) -> Result<()> {
    let mut markets: BTreeMap<u32, Crowd> = BTreeMap::new();
    for wallet in wallets.iter().filter(|wallet| wallet.eligible()) {
        for p in wallet.view.positions.iter().filter(|p| p.status == "open") {
            let Some(notional) = p.notional_value else {
                continue;
            };
            let crowd = markets.entry(p.perpetual_id).or_default();
            crowd.symbol = p.symbol.clone();
            if p.is_long() {
                crowd.long = checked(crowd.long.checked_add(notional), "crowding notional")?;
            } else {
                crowd.short = checked(crowd.short.checked_add(notional), "crowding notional")?;
            }
            crowd.accounts.insert(wallet.view.account_id);
            crowd.positions.push(p.last_event_id.clone());
        }
    }
    let rule = rule("watchlist-crowding");
    for (id, crowd) in markets {
        if crowd.accounts.len() < 2 {
            continue;
        }
        let total = checked(crowd.long.checked_add(crowd.short), "crowding notional")?;
        let (side, dominant) = if crowd.long >= crowd.short {
            ("long", crowd.long)
        } else {
            ("short", crowd.short)
        };
        let Some(share) = ratio(dominant, total) else {
            continue;
        };
        if let Some((severity, limit, score)) = grade(rule, share)? {
            drafts.push(Draft {
                rule,
                severity,
                account_id: None,
                perpetual_id: Some(id),
                symbol: Some(crowd.symbol.clone()),
                side: Some(side.to_string()),
                metric: Some(share),
                threshold: Some(limit),
                score,
                basis: "snapshot-scope",
                title: format!(
                    "{} is {}% {} across {} snapshot accounts",
                    crowd.symbol,
                    percent(share),
                    side,
                    crowd.accounts.len()
                ),
                explanation: "Dominant-side share of mark notional among accounts in this snapshot only; it is not a protocol-wide positioning fact.".to_string(),
                inputs: json!({"longNotional": crowd.long, "shortNotional": crowd.short, "accounts": crowd.accounts}),
                evidence: json!({"positionLastEventIds": crowd.positions}),
            });
        }
    }
    Ok(())
}

fn protocol_signals(analytics: &AnalyticsReport, drafts: &mut Vec<Draft>) -> Result<()> {
    let skew_rule = rule("market-skew");
    for market in &analytics.state.markets {
        let Some(skew) = market.skew else { continue };
        let magnitude = skew.abs();
        if let Some((severity, limit, score)) = grade(skew_rule, magnitude)? {
            let side = if skew >= Decimal::ZERO {
                "long"
            } else {
                "short"
            };
            drafts.push(Draft {
                rule: skew_rule,
                severity,
                account_id: None,
                perpetual_id: Some(market.perpetual_id),
                symbol: Some(market.symbol.clone()),
                side: Some(side.to_string()),
                metric: Some(magnitude),
                threshold: Some(limit),
                score,
                basis: "canonical",
                title: format!("{} open interest is {}% skewed {side}", market.symbol, percent(magnitude)),
                explanation: format!(
                    "Long {} versus short {} AUSD of mark notional across all open positions from deployment history.",
                    market.long_open_interest.unwrap_or_default().normalize(),
                    market.short_open_interest.unwrap_or_default().normalize()
                ),
                inputs: json!({"longOpenInterest": market.long_open_interest, "shortOpenInterest": market.short_open_interest, "skew": skew}),
                evidence: json!({"markEventId": market.mark_event_id, "analyticsVersion": analytics.version}),
            });
        }
    }
    let Some(day) = analytics.window("24h") else {
        return Ok(());
    };
    if day.status != "complete" {
        return Ok(());
    }
    let liquidation_rule = rule("liquidation-activity");
    for flow in day.markets.iter().filter(|flow| flow.liquidations > 0) {
        let open_interest = analytics
            .state
            .markets
            .iter()
            .find(|market| market.perpetual_id == flow.perpetual_id)
            .and_then(|market| market.open_interest)
            .filter(|value| !value.is_zero());
        let metric = open_interest.and_then(|oi| ratio(flow.liquidation_notional, oi));
        let (severity, limit, score) = match metric {
            Some(value) => grade(liquidation_rule, value)?
                .ok_or_else(|| DataQualityError::msg("liquidation activity grade failed"))?,
            None => ("watch", Decimal::ZERO, Decimal::ZERO),
        };
        drafts.push(Draft {
            rule: liquidation_rule,
            severity,
            account_id: None,
            perpetual_id: Some(flow.perpetual_id),
            symbol: Some(flow.symbol.clone()),
            side: None,
            metric,
            threshold: Some(limit),
            score,
            basis: "canonical",
            title: format!(
                "{} had {} liquidation(s) worth {} AUSD in 24 hours",
                flow.symbol,
                flow.liquidations,
                flow.liquidation_notional.normalize()
            ),
            explanation: match metric {
                Some(value) => format!("Liquidated notional equals {}% of current market open interest. Thresholds: warning 5%, critical 20%.", percent(value)),
                None => "Market open interest is unavailable, so only the liquidation count is reported.".to_string(),
            },
            inputs: json!({"liquidations": flow.liquidations, "liquidationNotional": flow.liquidation_notional, "openInterest": open_interest}),
            evidence: json!({"window": day.id, "windowEventIdsHash": day.event_ids_hash, "lastWindowEventId": day.last_event_id}),
        });
    }
    Ok(())
}

fn stress(wallets: &[WalletJson]) -> Result<StressReport> {
    let mut excluded = Vec::new();
    let mut candidates = Vec::new();
    for wallet in wallets {
        if !wallet.eligible() {
            excluded.push(json!({"accountId": wallet.view.account_id, "perpetualId": null, "reason": "incomplete-history"}));
            continue;
        }
        for p in wallet.view.positions.iter().filter(|p| p.status == "open") {
            match (p.mark, p.maintenance_margin) {
                (Some(mark), Some(maintenance)) => candidates.push((wallet.view.account_id, p, mark, maintenance)),
                _ => excluded.push(json!({"accountId": wallet.view.account_id, "perpetualId": p.perpetual_id, "reason": "mark-unavailable"})),
            }
        }
    }
    let mut scenarios = Vec::new();
    for text in STRESS_SHOCKS {
        let shock: Decimal = text
            .parse()
            .map_err(|_| DataQualityError::msg("invalid stress shock"))?;
        let mut breaches = Vec::new();
        let mut collateral = Decimal::ZERO;
        let mut conditional = 0;
        for (account, p, mark, maintenance) in &candidates {
            let shocked = checked(mark.checked_mul(Decimal::ONE + shock), "stress mark")?;
            let gap = if p.is_long() {
                shocked.checked_sub(p.entry)
            } else {
                p.entry.checked_sub(shocked)
            };
            let move_pnl = checked(
                checked(gap, "stress price move")?.checked_mul(p.size),
                "stress PnL",
            )?;
            let funding = p.unrealized_funding.unwrap_or(Decimal::ZERO);
            let equity = checked(
                p.deposit
                    .checked_add(move_pnl)
                    .and_then(|value| value.checked_add(funding)),
                "stress equity",
            )?;
            if equity <= *maintenance {
                let basis = if p.unrealized_funding.is_some() {
                    "funded"
                } else {
                    "zero-funding-conditional"
                };
                if p.unrealized_funding.is_none() {
                    conditional += 1;
                }
                collateral = checked(collateral.checked_add(p.deposit), "stress collateral")?;
                breaches.push(StressPosition {
                    account_id: *account,
                    perpetual_id: p.perpetual_id,
                    symbol: p.symbol.clone(),
                    side: p.side.clone(),
                    shocked_mark: shocked.normalize(),
                    equity: equity.normalize(),
                    maintenance_margin: *maintenance,
                    collateral: p.deposit,
                    basis: basis.to_string(),
                });
            }
        }
        scenarios.push(StressScenario {
            shock,
            positions_evaluated: candidates.len(),
            breached: breaches.len(),
            breached_collateral: collateral,
            conditional_breaches: conditional,
            breaches,
        });
    }
    Ok(StressReport {
        version: STRESS_VERSION.to_string(),
        method: "Apply each shock to every eligible as-of mark at once. Equity = collateral + side-adjusted (shocked mark - effective entry) * size + proven unsettled funding; a position breaches when equity is at or below entry-based maintenance. Unknown funding uses an explicit zero-funding conditional basis.".to_string(),
        scenarios,
        excluded,
    })
}

/// Carry first-observed and severity-change blocks from the previous accepted
/// snapshot of the same process. Without a previous snapshot every signal is a
/// baseline observation; nothing is inferred about earlier blocks.
pub fn carry_forward(previous: Option<&SignalReport>, current: &mut SignalReport) {
    let Some(previous) = previous.filter(|prior| prior.as_of_block <= current.as_of_block) else {
        return;
    };
    current.baseline_as_of_block = Some(previous.as_of_block);
    let prior: BTreeMap<&str, &Signal> = previous
        .items
        .iter()
        .map(|item| (item.signal_id.as_str(), item))
        .collect();
    for item in &mut current.items {
        match prior.get(item.signal_id.as_str()) {
            None => item.change = "new".to_string(),
            Some(old) => {
                item.first_observed_as_of_block = old.first_observed_as_of_block;
                match severity_rank(&item.severity).cmp(&severity_rank(&old.severity)) {
                    std::cmp::Ordering::Equal => {
                        item.severity_since_as_of_block = old.severity_since_as_of_block;
                        item.change = "unchanged".to_string();
                    }
                    std::cmp::Ordering::Greater => item.change = "escalated".to_string(),
                    std::cmp::Ordering::Less => item.change = "de-escalated".to_string(),
                }
            }
        }
    }
    let active: BTreeSet<&str> = current
        .items
        .iter()
        .map(|item| item.signal_id.as_str())
        .collect();
    let mut resolved: Vec<ResolvedSignal> = previous
        .items
        .iter()
        .filter(|item| !active.contains(item.signal_id.as_str()))
        .map(|item| ResolvedSignal {
            signal_id: item.signal_id.clone(),
            rule_id: item.rule_id.clone(),
            last_severity: item.severity.clone(),
            title: item.title.clone(),
            first_observed_as_of_block: item.first_observed_as_of_block,
            resolved_as_of_block: current.as_of_block,
        })
        .collect();
    let fresh: BTreeSet<String> = resolved.iter().map(|item| item.signal_id.clone()).collect();
    resolved.extend(
        previous
            .resolved
            .iter()
            .filter(|item| {
                !active.contains(item.signal_id.as_str())
                    && !fresh.contains(&item.signal_id)
                    && current.as_of_block - item.resolved_as_of_block <= RESOLVED_RETENTION_BLOCKS
            })
            .cloned(),
    );
    resolved.sort_by(|a, b| {
        b.resolved_as_of_block
            .cmp(&a.resolved_as_of_block)
            .then(a.signal_id.cmp(&b.signal_id))
    });
    resolved.truncate(MAX_RESOLVED);
    current.resolved = resolved;
}
