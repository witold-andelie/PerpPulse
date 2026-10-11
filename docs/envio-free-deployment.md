# Bounded Envio Cloud free deployment

The Development Free deployment uses `envio/config.free.yaml`, with root
`envio` and branch `envio-free`. The original `envio/config.yaml` remains the
full-history configuration for an appropriately sized self-hosted or paid
runtime.

## Fixed historical scope

The free configuration indexes Monad chain 143, Exchange proxy
`0x34B6552d57a35a1D042CcAe1951BD1C370112a6F`, from block **89,200,000** through
**89,210,000**. `end_block` stops ingestion at that fixed boundary. Removing
it would allow usage to grow indefinitely and invalidate the free deployment.
The event selection, schema, handlers and canonical provenance are unchanged.
The Rust ledger remains the only position and PnL engine.

On 2026-10-11, a keyset scan of the existing public deployment counted **49,204
canonical events** in this window across 50 pages. Counts and selected public
provenance are recorded in [free-window.json](../envio/free-window.json).
The safety budget is 50,000 events, below the free plan's 100,000-event soft
threshold. Source artifact hashes and tests prevent silently expanding the
scope or reusing count evidence after changing the event pipeline. Verify the
new cloud deployment's actual processed-event total before accepting it.

This is a historical source, not a continuously fresh feed. Consumers must
show both block boundaries and the observed timestamps. A live freshness
check must still fail when this window is stale. Account births or position
baselines outside the window are missing; complete balances, lifetime PnL,
funding or global open interest must remain unavailable unless independently
established under the existing coverage rules. Do not bypass those rules to
make a historical window appear live.

## Verification

From `envio`, using the supported Linux or WSL toolchain:

```sh
pnpm exec envio codegen --config config.free.yaml
pnpm typecheck
pnpm test
```

After deployment, query public `_meta` and keyset-page `CanonicalEvent` over
the configured interval. Confirm the configured start and processed end,
canonical provenance, 49,204 canonical rows and a cloud processed-event count
below 100,000. Repeat the read to confirm processing no longer advances beyond
the fixed end. Count differences, missing coverage or unexpected provenance
are failures, not empty successful observations. Do not delete a previous
deployment until the replacement has passed these checks and the owner has
confirmed the permanent deletion.

## Other free-plan limits

Keeping events below the threshold does not make development hosting
permanent. The current official policy also lists 5GB storage and seven days
without requests as soft limits; 20GB storage and deployment age over 30 days
are hard deletion limits. A soft-limit breach starts a seven-day grace period,
followed by three days of query-only access and deletion. Timings may change.
Do not select a paid plan or automate repeated redeployments to evade these
limits.

References: [Envio deployment and fair-use policy](https://docs.envio.dev/docs/HyperIndex/hosted-service-deployment)
and [end-block configuration](https://docs.envio.dev/docs/HyperIndex/configuration-file#end-block).
