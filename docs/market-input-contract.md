# Market input observation and accounting eligibility

Version: market-input-contract-v1. This contract adds local validation and a
public metadata observation path. It does not establish live mark verification,
complete wallet history, or protocol-wide coverage.

## Public context observation

The source is the official public
[context endpoint](https://app.perpl.xyz/api/v1/pub/context), interpreted against
the [official API types](https://github.com/PerplFoundation/api-docs/blob/main/types.md).
`perppulse inspect-context` makes one GET with a ten-second global timeout,
a 1 MB decoded-body bound, and at most 100 rows per subject collection. It uses
no authentication and no automatic retry. It exports selected public protocol
fields, not the raw response. Location, rewards, competition, user, and other
unrelated fields are omitted.

The export records source URL, observation time, chain, Exchange, collateral,
per-market decimals and margin fractions, configuration/state block and time,
contract version, trading status, candidate mark native value, and differences
from the frozen registry. `fieldsHash` covers exactly the normalized `fields`
object; time-dependent `stateQuality` is separate. Sorting by perpetual ID
makes provider row ordering irrelevant to that hash.

Chain, Exchange and collateral must agree with the expected registry. Missing
subjects, duplicate IDs, unknown instance references, future observations,
state preceding configuration, zero prices, invalid version components,
and unsafe scales or margin parameters are errors. State at least 60 seconds
older than observation is explicitly `stale-state`. A recently observed state
does not make the underlying mark accounting-eligible.

The current REST `state.at` is a composite state timestamp. It has neither a
block hash nor a mark-specific log cutoff or update time. Inferring a fresh
mark from that timestamp could accept an old mark after an unrelated state
change. Therefore `accountingEligible`, `markAccountingEligible`, and
`canonicalRegistryUpdated` remain false. REST OI, TVL, fees and volume are not
copied into the canonical ledger.

## Accounting marks

The existing `MarketMark` contract now supports optional `block_hash` and
`log_index`. For an open position, accounting requires one positive finite mark
per market with a nonnegative timestamp, no future block or timestamp, and an
age strictly below 60,000 ms relative to the ledger cutoff. A supplied oracle
price must also be positive and finite. Duplicate marks are rejected.

When the mark is in the selected cutoff block and the cutoff selects a log,
the mark must include the matching block hash and a log index at or before the
cutoff. A block-end cutoff can consume a same-block fixture mark without a log;
an explicitly supplied conflicting block hash is always rejected. Earlier-block
inputs still need source verification before use in live accounting.

The 60-second freshness limit is an application quality rule. It does not claim
to be the live Perpl `refPriceMaxAgeSec`. Native fixture marks remain synthetic.
The live watchlist still supplies no marks and retains null mark-derived facts.
Supplying malformed or stale marks fails the accounting request visibly.

Wallet aggregates, price PnL, fair market value, maintenance margin, liquidation
buffer and liquidation price use checked decimal arithmetic. Values outside
the supported finite decimal range return errors instead of wrapping or
panicking. Registry validation rejects invalid addresses, decimal scales above
18, inconsistent exclusions, and margin fractions below the supported domain.

## Remaining live verification

Before accepting live marks or registry updates, capture an independent
onchain source at the common canonical chain, block hash, log cutoff and
timestamp; verify mark-specific update time and protocol age limit; prove
metadata did not change after that selected cutoff; and attach a sanitized
source manifest. An end-of-block SDK/RPC snapshot must not be used for a
mid-block log cutoff without proving that intervening relevant logs are absent.
Then compare eligible open-position facts against an independently obtained
Perpl SDK state. These requirements remain open.
