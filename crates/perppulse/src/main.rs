use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use perppulse::accounting::account_wallet;
use perppulse::envio::{AccountEventSlice, EnvioClient};
use perppulse::error::DataQualityError;
use perppulse::events::repo_root;
use perppulse::ledger::replay;
use perppulse::pipeline::run_fixture;
use perppulse::quality::gate_ledger;
use perppulse::registry::load_registry;
use perppulse::serve::{build_snapshot, run_server};

#[derive(Parser)]
#[command(
    name = "perppulse",
    about = "Read-only Perpl protocol-to-wallet risk intelligence"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect sanitized public Perpl metadata; never import REST marks into accounting.
    InspectContext {
        #[arg(long, default_value = "fixtures/protocol/mainnet-registry.json")]
        registry: PathBuf,
        /// Reinspect a locally captured response without making a network call.
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long, requires = "input")]
        observed_at_ms: Option<i64>,
        /// Write a new sanitized observation file; never overwrite prior evidence.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Replay a golden fixture and print protocol -> wallet -> event evidence.
    Demo {
        #[arg(value_name = "FIXTURE")]
        fixture: PathBuf,
        #[arg(long, default_value_t = 0)]
        max_lag_blocks: u64,
    },
    /// Serve a read-only JSON API over one deterministic fixture pulse.
    Serve {
        #[arg(value_name = "FIXTURE")]
        fixture: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8081")]
        bind: String,
        #[arg(long, default_value_t = 0)]
        max_lag_blocks: u64,
    },
    /// Serve the live Envio watchlist with explicit incomplete-data states.
    ServeEnvio {
        #[arg(long, value_delimiter = ',', required = true)]
        accounts: Vec<u64>,
        #[arg(long, default_value = "http://localhost:8080/v1/graphql")]
        graphql_url: String,
        #[arg(long, default_value = "https://rpc.monad.xyz")]
        rpc_url: String,
        #[arg(long, default_value = "fixtures/protocol/mainnet-registry.json")]
        registry: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8081")]
        bind: String,
        #[arg(long, default_value_t = 10)]
        refresh_seconds: u64,
        #[arg(long, default_value_t = 100)]
        max_lag_blocks: u64,
        #[arg(long, default_value_t = 500)]
        page_size: u32,
        #[arg(long, default_value_t = 10_000)]
        max_events: usize,
        /// Publish compact state using PERPPULSE_DATABASE_URL through a local proxy.
        #[arg(long)]
        publish_database: bool,
        /// Explicit owner-approved allowance for optional billable Nansen requests.
        #[arg(long, default_value_t = 0)]
        nansen_max_requests: u32,
        /// Read every canonical v3 event in coverage for protocol analytics,
        /// bounded by this many events (0 disables; at most 1,000,000).
        #[arg(long, default_value_t = 0)]
        protocol_max_events: usize,
    },
    /// Export a reproducible fixture manifest, optionally comparing a Perpl reference.
    Evidence {
        fixture: PathBuf,
        #[arg(long)]
        reference: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Publish a fixture snapshot to local PostgreSQL or a Cloud SQL Auth Proxy.
    Publish {
        fixture: PathBuf,
        #[arg(long, default_value = "fixture-demo")]
        source: String,
    },
    /// Serve compact database state, rejecting missing or stale observations.
    ServeDatabase {
        #[arg(long, default_value = "live-watchlist")]
        source: String,
        #[arg(long, default_value = "127.0.0.1:8081")]
        bind: String,
        #[arg(long, default_value_t = 90)]
        max_age_seconds: u64,
    },
    /// Read a coverage-bounded account slice from Envio GraphQL.
    EnvioAccount {
        #[arg(value_name = "ACCOUNT_ID")]
        account_id: u64,
        #[arg(long, default_value = "http://localhost:8080/v1/graphql")]
        graphql_url: String,
        #[arg(
            long,
            value_name = "REGISTRY",
            default_value = "fixtures/protocol/mainnet-registry.json"
        )]
        registry: PathBuf,
        #[arg(long, default_value_t = 500)]
        page_size: u32,
        #[arg(long, default_value_t = 10_000)]
        max_events: usize,
        /// Return range evidence without attempting financial replay.
        #[arg(long)]
        inspect_only: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("PerpPulse failed visibly: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), DataQualityError> {
    let cli = Cli::parse();
    match cli.command {
        Command::InspectContext {
            registry,
            input,
            observed_at_ms,
            output,
        } => {
            let registry = load_registry(resolve_repo_path(registry))?;
            let observation = if let Some(input) = input {
                let observed = observed_at_ms.ok_or_else(|| {
                    DataQualityError::msg("local context inspection requires --observed-at-ms")
                })?;
                let bytes = std::fs::read(input)
                    .map_err(|_| DataQualityError::msg("cannot read local context"))?;
                if bytes.len() > 1_048_576 {
                    return Err(DataQualityError::msg("local context exceeds 1 MB"));
                }
                let value = serde_json::from_slice(&bytes)
                    .map_err(|_| DataQualityError::msg("invalid local context JSON"))?;
                perppulse::market_inputs::inspect_public_context(&value, &registry, observed)?
            } else {
                perppulse::market_inputs::fetch_public_context(&registry)?
            };
            let text = serde_json::to_string_pretty(&observation)
                .map_err(|_| DataQualityError::msg("cannot serialize market observation"))?;
            if let Some(path) = output {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|_| {
                        DataQualityError::msg("context output must be a new writable path")
                    })?;
                file.write_all(text.as_bytes())
                    .map_err(|_| DataQualityError::msg("cannot write context observation"))?;
            } else {
                println!("{text}");
            }
        }
        Command::Demo {
            fixture,
            max_lag_blocks,
        } => {
            let path = if fixture.is_absolute() {
                fixture
            } else {
                let from_cwd = fixture.clone();
                if from_cwd.exists() {
                    from_cwd
                } else {
                    repo_root().join(fixture)
                }
            };
            let pulse = run_fixture(&path, Some(max_lag_blocks))?;
            print_demo(&pulse)?;
        }
        Command::Serve {
            fixture,
            bind,
            max_lag_blocks,
        } => {
            let path = if fixture.is_absolute() {
                fixture
            } else {
                let from_cwd = fixture.clone();
                if from_cwd.exists() {
                    from_cwd
                } else {
                    repo_root().join(fixture)
                }
            };
            let pulse = run_fixture(&path, Some(max_lag_blocks))?;
            let snapshot = build_snapshot(&pulse)?;
            let addr: std::net::SocketAddr = bind.parse().map_err(|err| {
                DataQualityError::msg(format!("invalid bind address {bind}: {err}"))
            })?;
            run_server(&snapshot, addr)?;
        }
        Command::EnvioAccount {
            account_id,
            graphql_url,
            registry,
            page_size,
            max_events,
            inspect_only,
        } => {
            let registry = load_registry(resolve_repo_path(registry))?;
            let client = EnvioClient::new(
                graphql_url,
                std::env::var("HASURA_GRAPHQL_ADMIN_SECRET").ok(),
                page_size,
                max_events,
            )?;
            let slice = client.fetch_account(registry.chain_id, account_id)?;
            let eligibility = slice.replay_eligibility(&registry)?;
            print_envio_slice(&slice, &eligibility);
            if !inspect_only {
                if !eligibility.eligible {
                    return Err(DataQualityError::msg(format!(
                        "position replay blocked: {}",
                        eligibility.reason
                    )));
                }
                let ledger = replay(&slice.events, &registry, &slice.as_of)?;
                let quality =
                    gate_ledger(&ledger, &slice.as_of, &slice.coverage.evidence, Some(0))?;
                println!(
                    "Position ledger: {} account(s), {} position record(s), quality {}",
                    ledger.accounts.len(),
                    ledger.positions.len(),
                    quality.status
                );
            }
        }
        Command::ServeEnvio {
            accounts,
            graphql_url,
            rpc_url,
            registry,
            bind,
            refresh_seconds,
            max_lag_blocks,
            page_size,
            max_events,
            publish_database,
            nansen_max_requests,
            protocol_max_events,
        } => {
            let config = perppulse::live::LiveConfig {
                client: EnvioClient::new(
                    graphql_url,
                    std::env::var("HASURA_GRAPHQL_ADMIN_SECRET").ok(),
                    page_size,
                    max_events,
                )?,
                registry: load_registry(resolve_repo_path(registry))?,
                accounts,
                rpc_url,
                refresh_seconds,
                max_lag_blocks,
                nansen: if nansen_max_requests > 0 {
                    Some(perppulse::context::NansenClient::new(
                    std::env::var("NANSEN_API_KEY").map_err(|_|DataQualityError::msg("NANSEN_API_KEY is required for the explicit Nansen request allowance"))?,nansen_max_requests)?)
                } else {
                    None
                },
                database_url: if publish_database {
                    Some(database_url()?)
                } else {
                    None
                },
                protocol: if protocol_max_events > 0 {
                    Some(std::sync::Mutex::new(
                        perppulse::live::ProtocolAggregator::new(protocol_max_events)?,
                    ))
                } else {
                    None
                },
            };
            perppulse::live::run_live(config, parse_bind(&bind)?)?;
        }
        Command::Evidence {
            fixture,
            reference,
            output,
        } => {
            let pulse = run_fixture(resolve_repo_path(fixture), Some(0))?;
            let snapshot = build_snapshot(&pulse)?;
            let mut manifest = snapshot.manifest.clone();
            manifest["snapshotHash"] = serde_json::json!(perppulse::evidence::digest(&snapshot)?);
            if let Some(path) = reference {
                let bytes = std::fs::read(path)
                    .map_err(|_| DataQualityError::msg("cannot read Perpl reference"))?;
                let reference = serde_json::from_slice(&bytes)
                    .map_err(|_| DataQualityError::msg("invalid Perpl reference JSON"))?;
                manifest["reconciliation"] = perppulse::evidence::reconcile(&snapshot, &reference)?;
            }
            let text = serde_json::to_string_pretty(&manifest)
                .map_err(|_| DataQualityError::msg("cannot serialize evidence manifest"))?;
            if let Some(path) = output {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)
                    .map_err(|_| DataQualityError::msg("evidence output must be a new writable path; existing evidence is never overwritten"))?;
                file.write_all(text.as_bytes())
                    .map_err(|_| DataQualityError::msg("cannot write evidence manifest"))?;
            } else {
                println!("{text}");
            }
            if manifest["reconciliation"]["status"] == "mismatch" {
                return Err(DataQualityError::msg(
                    "Perpl reconciliation found a mismatch",
                ));
            }
        }
        Command::Publish { fixture, source } => {
            let pulse = run_fixture(resolve_repo_path(fixture), Some(0))?;
            perppulse::publication::publish(&database_url()?, &source, &build_snapshot(&pulse)?)?;
            println!("Compact fixture snapshot published; this is synthetic evidence.");
        }
        Command::ServeDatabase {
            source,
            bind,
            max_age_seconds,
        } => {
            let url = database_url()?;
            perppulse::publication::load(&url, &source, max_age_seconds)?;
            perppulse::serve::run_service(
                move || perppulse::publication::load(&url, &source, max_age_seconds),
                parse_bind(&bind)?,
            )?;
        }
    }
    Ok(())
}

fn database_url() -> Result<String, DataQualityError> {
    std::env::var("PERPPULSE_DATABASE_URL").map_err(|_| DataQualityError::msg("PERPPULSE_DATABASE_URL is required; supply it as process environment, never as a CLI argument"))
}

fn parse_bind(bind: &str) -> Result<std::net::SocketAddr, DataQualityError> {
    bind.parse()
        .map_err(|_| DataQualityError::msg("invalid API bind address"))
}

fn resolve_repo_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() || path.exists() {
        path
    } else {
        repo_root().join(path)
    }
}

fn print_envio_slice(slice: &AccountEventSlice, eligibility: &perppulse::envio::ReplayEligibility) {
    let profile = slice.events[0]
        .provenance
        .as_ref()
        .map(|value| value.ingestion_profile.as_str())
        .unwrap_or("missing");
    println!("PerpPulse Envio account slice: {}", slice.account_id);
    println!(
        "Coverage: blocks {} through {} (source {}, {} total indexed events)",
        slice.coverage.evidence.start_block,
        slice.coverage.evidence.processed_block,
        slice.coverage.source_block,
        slice.coverage.events_processed
    );
    println!(
        "Stable as-of event: block {} log {} {}",
        slice.as_of.block_number,
        slice.as_of.log_index.unwrap_or(0),
        slice.coverage.latest_event.id
    );
    println!(
        "Account rows: {} under ingestion profile {}",
        slice.events.len(),
        profile
    );
    println!(
        "Replay eligible: {} ({})",
        if eligibility.eligible { "yes" } else { "no" },
        eligibility.basis
    );
    println!("Replay evidence: {}", eligibility.reason);
}

fn print_demo(pulse: &perppulse::pipeline::Pulse) -> Result<(), DataQualityError> {
    let metrics = &pulse.metrics;
    println!("PerpPulse demo: {}", pulse.fixture.name);
    println!(
        "Registry: {} chain {} exchange {}",
        pulse.fixture.registry.network,
        pulse.fixture.registry.chain_id,
        pulse.fixture.registry.exchange_address
    );
    println!(
        "As-of block {} / processed block {} / quality {}",
        metrics.as_of_block, pulse.quality.processed_block, pulse.quality.status
    );
    println!();
    println!("1. Protocol Risk Pulse");
    println!(
        "   Taker volume (maker-fill equivalent): {}",
        metrics.taker_volume
    );
    println!("   Open interest: {}", metrics.open_interest);
    println!("   TVL (isolated position collateral): {}", metrics.tvl);
    println!("   Protocol fees: {}", metrics.protocol_fees);
    println!("   Liquidations: {}", metrics.liquidations);
    println!("   Active accounts: {}", metrics.active_accounts);
    println!(
        "   Coverage: processed block {} (lag {} blocks); last event block {} (quiet for {} blocks)",
        pulse.quality.processed_block,
        pulse.quality.coverage_lag_blocks,
        metrics.last_event_block,
        pulse.quality.event_silence_blocks
    );
    for market in &metrics.markets {
        println!(
            "   Market {} {}: volume {} OI {} TVL {} fees {} liqs {}",
            market.perpetual_id,
            market.symbol,
            market.taker_volume,
            market.open_interest,
            market.tvl,
            market.protocol_fees,
            market.liquidations
        );
    }
    if !metrics.warnings.is_empty() {
        println!("   Warnings:");
        for warning in &metrics.warnings {
            println!("   - {warning}");
        }
    }
    println!();
    println!("2. Wallet drill-down");
    let wallets = pulse.wallets(true)?;
    if wallets.is_empty() {
        return Err(DataQualityError::msg("no wallets present after replay"));
    }
    for wallet in &wallets {
        println!(
            "   Account {} owner {} free {} realized {} unrealized {} fees {} funding {}",
            wallet.account_id,
            wallet.owner.as_deref().unwrap_or("unknown"),
            wallet.free_balance,
            wallet.realized_pnl,
            wallet
                .unrealized_pnl
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unavailable".into()),
            wallet.fees,
            wallet.realized_funding
        );
        for position in &wallet.positions {
            println!(
                "     {} {} {} size {} entry {} deposit {} status {} last {}",
                position.symbol,
                position.side,
                position.perpetual_id,
                position.size,
                position.entry,
                position.deposit,
                position.status,
                position.last_event_id
            );
        }
    }
    println!();
    println!("3. Event evidence");
    let last = pulse
        .ledger
        .events
        .last()
        .ok_or_else(|| DataQualityError::msg("ledger has no events to show"))?;
    let event_id = last.event_id()?;
    let stored = pulse.store.get(&event_id.key())?;
    println!("   Event {}", event_id.key());
    println!("   ABI {}", stored.abi_event_name);
    println!("   Kind {:?}", stored.kind);
    println!(
        "   Block {} tx {} log {}",
        stored.block_number, stored.tx_hash, stored.log_index
    );
    if let Some(account_id) = stored.account_id {
        println!("   Account {account_id}");
    }
    if let Some(perpetual_id) = stored.perpetual_id {
        println!("   Market {perpetual_id}");
    }
    println!();
    println!(
        "Selected navigation context preserved: as-of block {}, chain {}.",
        pulse.fixture.as_of.block_number, pulse.fixture.as_of.chain_id
    );
    let _ = account_wallet(
        &pulse.ledger,
        wallets[0].account_id,
        &pulse.fixture.as_of,
        &pulse.fixture.marks,
        true,
    )?;
    Ok(())
}
