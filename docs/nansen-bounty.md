# Nansen bounty: withdrawn

On 2026-10-11 the owner decided not to use Nansen in PerpPulse. The Best Use of
Nansen bounty is not pursued.

- No Nansen API, MCP tool, CLI, key or data is used by the product, tests, demo
  or CI.
- The optional common-label adapter, the `--nansen-max-requests` option, the
  `/api/context` route and the comparison label filter were removed. Wallet
  comparison is now `snapshot-cohort-v2`, introduced in methodology v9; its
  statistics, percentiles and overlap are unchanged.
- No live Nansen request was ever made by this project. Earlier verification
  records that mention the adapter describe mock-only checks at their own
  revisions.
- Compact snapshots published before the removal keep their content hash: the
  retired `context` field is still accepted on read and omitted from new
  snapshots.

The 2026-10-02 requirement text and the
[catalog record](evidence/bounty-sources-2026-10-03.json) remain historical
inputs only. See the [audit record](verification-2026-10-11-audit.md).
