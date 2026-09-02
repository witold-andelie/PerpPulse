# Grok Operator Guide

This guide defines how to use the local Grok CLI for PerpPulse without allowing a fast sub-agent to bypass repository, security, cost, or review boundaries.

## Division of responsibility

- Grok handles token-intensive reading, comparison, research, scaffolding, and implementation in a bounded context.
- The primary Codex agent is the integration gatekeeper. It verifies sources, reviews the complete Git history and diff, runs tests and policy checks, and decides what can enter the main branch.
- The project owner authorizes login, external writes, cloud provisioning, remote pushes, and licensing decisions.

Grok output is advice or a candidate patch, never a source of truth by itself.

## Hard rules

1. Never use `--always-approve`, `--permission-mode bypassPermissions`, or an unrestricted `--tools` list.
2. Keep `--no-subagents` enabled. One bounded Grok process is easier to audit than a hidden tree of agents.
3. Use English in every prompt that can produce repository content.
4. Never let Grok stage or include `PROGRESS.md`.
5. Never expose `.env` files, API keys, wallet keys, seed phrases, browser cookies, OAuth tokens, cloud credentials, or billing details.
6. Grok may write code only in an isolated worktree and task branch created from a recorded base SHA.
7. Grok must not push, change remotes, deploy, create cloud resources, rotate credentials, or change billing.
8. Network access is task-specific. Require direct source URLs and verify important claims independently.
9. Stop the process if it requests broader access than the task needs.

## 1. Login and inspect

Use device authentication to avoid embedded-browser login problems:

```powershell
grok login --device-auth
```

Confirm the discovered project instructions and current client configuration:

```powershell
Set-Location 'D:\AI_Models\hackson\monad'
grok --version
grok inspect --json
```

The project instructions should include `AGENTS.md`. Do not paste credentials into a prompt or terminal transcript.

## 2. Offline read-only review

Use the checked-in wrapper. It exposes only `Read`, `Glob`, and `Grep`, disables web access, disables nested agents, and prevents edits or shell use.

```powershell
Set-Location 'D:\AI_Models\hackson\monad'

$reviewTask = @'
Audit the GCP deployment plan against the reference implementation.
Return English only. Order findings by severity, cite exact files and symbols,
state uncertainty, and do not implement changes.
'@

.\scripts\run_bounded_grok_review.ps1 `
  -Task $reviewTask `
  -AllowedPath @(
    'D:\AI_Models\hackson\monad\docs\gcp-deployment-plan.md',
    'D:\AI_Models\hackson\agri\deploy\deploy.ps1'
  ) `
  -MaxTurns 10
```

If a large review appears idle, split the file set and question into smaller batches. Do not solve a timeout by adding tools.

## 3. Read-only web research

Add `-AllowWeb` only when external research is part of the task. The wrapper enables web search and fetch while keeping files read-only and shell, Git, authentication, subagents, and cloud APIs forbidden.

```powershell
$researchTask = @'
Research the current official Cloud Run worker-pool and Cloud SQL pricing and limits.
Use primary Google Cloud sources only. Return direct URLs, retrieval dates,
region assumptions, and any uncertainty. Do not modify files.
'@

.\scripts\run_bounded_grok_review.ps1 `
  -Task $researchTask `
  -AllowedPath @(
    'D:\AI_Models\hackson\monad\docs\gcp-deployment-plan.md'
  ) `
  -AllowWeb `
  -MaxTurns 12
```

The primary agent must reopen and verify every source used for a material cost, rules, API, or security claim.

## 4. Isolated code generation and modification

Do not begin write mode until the main repository has a reviewed baseline commit and a clean worktree.

### Create a task worktree

Use an explicit sibling path and record the exact base commit:

```powershell
$repoPath = 'D:\AI_Models\hackson\monad'
$taskName = 'gcp-scaffold'
$taskBranch = "grok/$taskName"
$taskWorktree = "D:\AI_Models\hackson\monad-grok-$taskName"

git -C $repoPath status --short
$baseSha = git -C $repoPath rev-parse HEAD
git -C $repoPath worktree add -b $taskBranch $taskWorktree $baseSha

Write-Output "BASE_SHA=$baseSha"
Write-Output "BRANCH=$taskBranch"
Write-Output "WORKTREE=$taskWorktree"
```

If the main status is not clean, stop and ask the primary agent to preserve or commit the current work before creating the worktree.

### Launch Grok in edit-only mode

This mode allows file reads and edits but no shell, Git, web, or cloud calls. Ask Grok to report test commands instead of executing them.

```powershell
$implementationPrompt = @'
Implement only the requested vertical slice in this isolated worktree.

Scope:
- Write English-only source, tests, comments, fixtures, and documentation.
- Never read secrets or PROGRESS.md.
- Never run Git, shell commands, tests, package installation, network calls, or cloud APIs.
- Never deploy or push.
- Keep changes small and list every modified file.
- Preserve the read-only product boundary and canonical-ledger rules in AGENTS.md.
- At the end, report assumptions, risks, and exact test commands for the operator.

Task:
<replace with one concrete, bounded implementation task and acceptance criteria>
'@

grok `
  --cwd $taskWorktree `
  --permission-mode acceptEdits `
  --tools 'Read,Glob,Grep,Edit,Write' `
  --disable-web-search `
  --no-subagents `
  --no-plan `
  --no-alt-screen `
  $implementationPrompt
```

If implementation needs current documentation, add `WebSearch,WebFetch` to the tool list and remove `--disable-web-search`. Keep shell, Git, authentication, and cloud actions unavailable.

### Review and checkpoint each slice

Run these commands yourself or hand the worktree back to Codex. Stage named files rather than staging everything blindly.

```powershell
git -C $taskWorktree status --short
git -C $taskWorktree diff --check
git -C $taskWorktree diff

py -3 "$taskWorktree\scripts\check_repository_policy.py" --tracked

git -C $taskWorktree add <explicit-file-list>
py -3 "$taskWorktree\scripts\check_repository_policy.py" --staged
git -C $taskWorktree diff --cached --check
git -C $taskWorktree diff --cached

# Run the relevant project tests here before committing.
git -C $taskWorktree commit -m "<short English commit message>"
```

Create one coherent commit per reviewed slice. Do not let an unrelated formatting rewrite hide functional changes.

## 5. Hand the work back to Codex

Provide the primary agent with:

- repository path;
- worktree path;
- task branch;
- recorded base SHA;
- intended outcome and acceptance criteria;
- commands and tests already run;
- known failures or unresolved assumptions.

The primary audit includes:

```powershell
git -C $taskWorktree status --short
git -C $taskWorktree log --oneline --decorate "$baseSha..HEAD"
git -C $taskWorktree diff --stat "$baseSha...HEAD"
git -C $taskWorktree diff --check "$baseSha...HEAD"
git -C $taskWorktree diff "$baseSha...HEAD"
py -3 "$taskWorktree\scripts\check_repository_policy.py" --tracked
```

Codex also reviews dependency locks, migrations, generated artifacts, security boundaries, source attribution, and relevant test output. A clean status or passing test alone is not approval.

## 6. Cloud and Git boundaries

Grok may generate deployment code or infrastructure definitions in the isolated worktree. It may not execute them. Actual provisioning requires the owner-confirmed project ID, billing account, region, USD 400 ceiling, working budget, and stop conditions. Project `project-5e761e8c-65aa-4033-8cb` and the USD 400 ceiling are recorded; billing account and region are still required.

Grok may not push. After the primary agent accepts the work, the owner or primary agent performs the integration and any remote push under the repository's English-only and local-progress exclusion rules.

## 7. Stop and recover safely

Use `Ctrl+C` if Grok loops, requests unnecessary authority, or leaves the task scope. Then inspect before changing anything:

```powershell
git -C $taskWorktree status --short
git -C $taskWorktree diff --stat
git -C $taskWorktree diff
```

Do not use `git reset --hard`, `git clean`, or recursive deletion to recover. Preserve the worktree and ask Codex to review or selectively revert the candidate changes.
