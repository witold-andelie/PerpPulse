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
    }
    Ok(())
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
        "As-of block {} / {} quality {}",
        metrics.as_of_block, pulse.quality.status, pulse.quality.status
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
            wallet.unrealized_pnl,
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
