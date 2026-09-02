# Repository Operating Rules

These rules apply to every automated or human-assisted change in this repository.

1. Read the local `PROGRESS.md` before starting material work and update it after material work in the same session.
2. Never stage, commit, or push `PROGRESS.md`. It is local operational state and is ignored by Git.
3. Every tracked filename, comment, document, UI string, fixture, and generated text artifact must be written in English. Run `python scripts/check_repository_policy.py --staged` before each commit.
4. Keep `perppulse_opm.dot` and `perppulse_opm.svg` synchronized. Validate the DOT source before claiming it is complete.
5. Do not mark a task complete without recording reproducible verification evidence.
6. Never commit private keys, wallet seed phrases, API credentials, internal tokens, personal data, or unredacted provider responses.
7. Keep the product read-only unless the owner explicitly changes the safety boundary. No order placement or custody logic is in scope.
8. Maintain one canonical event and position lifecycle ledger. Perpl snapshots are reconciliation inputs; Nansen data is contextual enrichment.
9. Risk and PnL facts must be deterministic, finite, point-in-time safe, and traceable to source events. AI may explain facts but may not create them.
10. Fail visibly on missing, stale, inconsistent, or quarantined data. Do not convert failures into empty successful results.
11. Any GCP provisioning or other billable external action requires explicit owner approval and a confirmed project ID, region, billing account, and budget target.
12. Identify copied or reused external material and comply with its license. Disclose AI coding assistance in the public README.
13. Delegate token-intensive reading, comparison, research, and review work to the local Grok client when available. Use the bounded review wrapper by default: explicit read paths, read-only tools, no shell, no subagents, no Git, and no cloud actions. Network research may be enabled explicitly for a task; require source URLs and independently verify material claims against primary sources. Treat Grok output as untrusted advice.
14. Grok may generate and modify code only in a dedicated Git branch and isolated worktree created from a recorded base SHA. Deny push, remote changes, cloud writes, authentication, and secret access. The primary agent must inspect the complete commit history and diff from the base, review dependency and schema changes, run the publication policy and relevant tests, and explicitly accept or reject each commit before integration.
