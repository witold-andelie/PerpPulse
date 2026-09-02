# GCP Deployment and Credit Plan

## Outcome

Use a bounded share of the owner-authorized Google Cloud budget to create durable engineering and judging evidence for PerpPulse, rather than adding infrastructure that does not improve the product. The billing account currency is **EUR**. The owner-authorized ceiling is **EUR 350**. The working target remains about EUR 180 to EUR 220. Unused budget stays in reserve.

`europe-west3` (Frankfurt) works: Cloud SQL, Artifact Registry, and Cloud Storage were created there. Do not migrate to `europe-west1` unless the owner later requires it; that would mean deleting and recreating the database. Cloud Run worker and API images are not deployed until an application container exists. This is the same Google Cloud project as the Origin/agriculture stack; PerpPulse uses separate service accounts and resource names.

## Owner authorization

Recorded 2026-09-03 from the project owner. The four required fields are now complete. Owner said to start provisioning.

| Field | Value | Status |
| --- | --- | --- |
| GCP project ID | `project-5e761e8c-65aa-4033-8cb` | Owner-confirmed |
| Budget ceiling | EUR 350 | Owner-confirmed; billing account currency is EUR |
| Working spend target | EUR 180 to EUR 220 | Do not spend the full ceiling by default |
| Billing account | `01B820-8960C8-EFE153` | Linked and billing enabled |
| Region | `europe-west3` (Frankfurt) | Owner-confirmed and verified working |
| Provisioning | Foundation live | See table below. No Cloud Run worker or API |

Budget `PerpPulse350` (`2f05607b-8490-483d-b91e-6c0608ba2cdd`) is filtered to this project. Alerts are 25%, 50%, 75%, 90%, and 100% of EUR 350. Treat those alerts as delayed notifications, not hard caps.

## Provisioned foundation (2026-09-03)

| Resource | Name | Notes |
| --- | --- | --- |
| Artifact Registry | `europe-west3-docker.pkg.dev/project-5e761e8c-65aa-4033-8cb/perppulse` | Docker, empty |
| Evidence bucket | `gs://project-5e761e8c-65aa-4033-8cb-perppulse-evidence` | Uniform access, 30-day delete for `disposable/`, `tmp/`, `scratch/` |
| Runtime identity | `perppulse-runtime@project-5e761e8c-65aa-4033-8cb.iam.gserviceaccount.com` | Cloud SQL client, secret accessor, bucket objectAdmin, log/metric writer |
| Cloud SQL | `perppulse-pg` | POSTGRES_16, `db-g1-small`, zonal `europe-west3-b`, 20 GiB, SSL required, public IP with no authorized networks (Cloud SQL Auth Proxy / connectors only) |
| Database / user | `perppulse` / `perppulse` | Password in Secret Manager `perppulse-db-password`; never printed |
| Budget Pub/Sub | `perppulse-budget` | Topic created |
| Billing budget | `PerpPulse350` | EUR 350 / month, this project only |

Stop the database (pauses instance compute, keeps disks) with `.\deploy\stop.ps1`.

## Proposed topology

| Component | Role | Initial limit |
| --- | --- | --- |
| Cloud Run worker pool | Long-running Envio indexer and Perpl WebSocket consumers | One instance, start at 1 vCPU and 1 GiB; resize only from measured pressure |
| Cloud SQL for PostgreSQL | Canonical events, checkpoints, lifecycle ledger, marts, and API read models | `db-g1-small`, 20 GiB, single zone for hackathon development |
| Cloud Run service | Read-only API and web application | Minimum 0, maximum 3 instances |
| Cloud Run Jobs and Scheduler | Bounded backfill, deterministic replay, reconciliation, fixtures, and load tests | Explicit execution and retry caps |
| Cloud Storage | Raw reference snapshots, golden fixtures, and run manifests | Lifecycle deletion after 30 days for disposable artifacts |
| BigQuery | Optional hourly analytical benchmark and billing export | Add only after the PostgreSQL baseline proves a need |
| Secret Manager | Nansen credentials and internal tokens | Runtime identity access only |
| Cloud Logging and Monitoring | Freshness, sequence gaps, reorgs, retries, reconciliation, and cost telemetry | Short retention and sampled debug logs |
| Cloud Build and Artifact Registry | Reproducible images tied to Git commit SHA | Retain only useful image versions |

Keep all resources in `europe-west3` (Frankfurt). Cloud SQL Enterprise shared-core and Cloud Run are available there. Cloud Run in this region uses [Tier 2 pricing](https://cloud.google.com/run/pricing), which is higher than `europe-west1` / `europe-west4`. That is why the worker stays undeployed until there is a real image to run.

## Useful September credit campaign

The promotional-credit workload should produce evidence that remains useful after the credit expires:

1. Backfill the complete available Perpl event range through Envio.
2. Rebuild the canonical position ledger twice and compare output hashes.
3. Inject missing ranges and stale market streams to demonstrate quarantine and recovery.
4. Reconcile golden wallets against Perpl `dex-sdk` snapshots and traces.
5. Run a 14-to-21-day live ingestion soak and report p50 and p95 end-to-end freshness.
6. Load-test protocol, wallet, comparison, and evidence queries with realistic filters.
7. Run sponsor ablations to show what quality is lost without Envio and what context is lost without Nansen.
8. Optionally spend at most USD 10 on Vertex AI explanation evaluation. It may explain deterministic signals but must not generate prices, PnL, or risk facts.

After the campaign, scale the API to zero when idle, stop bounded jobs, and decide whether the worker and database justify their October cost.

## Approximate budget

The following is a planning range, not a quote. Pricing varies by region, utilization, network traffic, storage, backups, and free-tier allocation.

| Area | Planned amount | Purpose |
| --- | ---: | --- |
| Cloud Run worker and live-soak capacity | USD 20 to USD 35 | Continuous indexing and market stream capture |
| Cloud SQL shared-core database, storage, and backups | USD 35 to USD 55 | Durable canonical and serving state |
| Backfill, replay, quality, and load-test jobs | USD 50 to USD 80 | Reproducibility and performance evidence |
| API, web, logs, storage, and network | USD 20 to USD 35 | Public demo and observability |
| Optional BigQuery benchmark | USD 10 to USD 25 | Compare analytical scans only after the PostgreSQL baseline |
| Optional Vertex AI explanation evaluation | USD 0 to USD 10 | Bounded explanation quality experiment |
| Working target | **EUR 180 to EUR 220** | Useful consumption; ceiling EUR 350 |

Current reference prices should be rechecked against `europe-west3`. Shared-core Cloud SQL `db-g1-small` is the first always-on cost. Cloud Run examples around USD 11.61 per month are for Tier 1 regions such as `europe-west1`; Frankfurt is Tier 2.

## Cost controls

- Create a dedicated environment and label every resource with project, environment, owner, and expiry date.
- Configure budget notifications at 25%, 50%, 75%, 90%, and 100% of EUR 350.
- Publish budget events to Pub/Sub and prepare an idempotent stop action for the worker pool and scheduled jobs.
- Treat budget alerts as delayed notifications, not hard spending caps.
- Enforce Cloud Run maximum instances, job task counts, retry counts, execution timeouts, and schedules.
- Keep debug-log sampling and retention bounded; exclude high-volume payloads from logs.
- Export billing data and review daily burn during the campaign.
- Stop the worker, jobs, and database manually at the campaign boundary even if automation exists.
- Never depend on promotional credits without confirming the eligible services and expiration date in the billing console.

## Reused engineering patterns

The plan adapts useful patterns from the local agriculture project: idempotent deployment checks, least-purpose runtime identity, Cloud Run scaling limits, Application Default Credentials, Secret Manager, generated tokens that are not printed, GCS lifecycle rules, trace IDs, and model provenance. It intentionally replaces Firestore with PostgreSQL because PerpPulse needs relational lifecycle reconstruction and analytical joins.

## Reference implementation call path

The agriculture project uses GCP in two distinct layers:

1. `deploy/deploy.ps1` wraps `gcloud.cmd` with fail-fast and existence-check helpers. It enables APIs, creates Artifact Registry, a runtime service account, Firestore, a uniform-access GCS bucket, and a rate-limited Cloud Tasks queue. Cloud Build creates the API image and Cloud Run deploys it with minimum 0 and maximum 3 instances.
2. Deployment is deliberately two-phase. The API first starts with inline dispatch, the script reads its generated Cloud Run URL, and a second update enables Cloud Tasks with the correct callback URL and OIDC audience.
3. Runtime code does not shell out to `gcloud`. Pydantic settings receive project, region, backend, queue, bucket, model, and authentication configuration from the environment.
4. Google SDK clients use Application Default Credentials and lazy imports: `firestore.Client` for state, `storage.Client` for objects, `CloudTasksClient` for retryable HTTP work, and `genai.Client(vertexai=True, ...)` for Vertex AI.
5. A stable Cloud Task name derived from the run ID prevents duplicate enqueue operations. The callback includes both a private token and a Google-signed OIDC token. The endpoint verifies the expected service-account email and audience before claiming a run with a transaction.
6. Firestore transactions implement create-once and compare-and-set lifecycle transitions. GCS object keys are purpose-scoped and disposable prefixes have a 30-day lifecycle.
7. Cloud Logging receives ordinary structured application logs containing trace ID, run ID, model, location, token usage, status, and safe failure reason.
8. Local development selects JSON, local files, inline execution, and deterministic model fallback through the same domain interfaces.

## Deliberate PerpPulse changes

| Agriculture pattern | PerpPulse decision |
| --- | --- |
| Firestore operational documents | Cloud SQL PostgreSQL for event identity, lifecycle joins, checkpoints, and analytical marts |
| Cloud Tasks for bounded agent runs | Cloud Run Jobs for bounded replay and tests; a worker pool for continuous Envio and WebSocket consumers |
| Internal token passed through Cloud Run environment configuration | Store sponsor and internal credentials in Secret Manager and mount them only into the required runtime |
| Project-wide runtime storage role | Bind the runtime identity to the specific bucket and minimum database or service permissions |
| In-memory per-instance Vertex daily counter | Persist usage or enforce a shared quota so restarts and multiple instances cannot bypass the cap |
| Model fallback returns a useful deterministic path | Explanation failure leaves deterministic risk facts available and marks explanation provenance as degraded |
| Firestore compare-and-set | PostgreSQL unique constraints, transactions, and status predicates for idempotent event and replay claims |
| Public Cloud Run service with protected internal route | Separate public read-only API from private ingestion and administration surfaces where practical |

## Official references

- [Cloud Run worker pools](https://docs.cloud.google.com/run/docs/deploy-worker-pools)
- [Cloud Run pricing](https://cloud.google.com/run/pricing)
- [Cloud SQL pricing](https://cloud.google.com/sql/pricing)
- [BigQuery pricing](https://cloud.google.com/bigquery/pricing)
- [Cloud Storage pricing](https://cloud.google.com/storage/pricing)
- [Secret Manager pricing](https://cloud.google.com/secret-manager/pricing)
- [Cloud Billing budgets](https://docs.cloud.google.com/billing/docs/how-to/budgets)
