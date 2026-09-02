use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use perppulse::accounting::account_wallet;
use perppulse::error::DataQualityError;
use perppulse::events::repo_root;
use perppulse::pipeline::run_fixture;

#[derive(Parser)]
#[command(name = "perppulse", about = "Read-only Perpl protocol-to-wallet risk intelligence")]
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
        Command::Demo { fixture, max_lag_blocks } => {
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
    }
    Ok(())
}

fn print_demo(pulse: &perppulse::pipeline::Pulse) -> Result<(), DataQualityError> {
    let metrics = &pulse.metrics;
    println!("PerpPulse demo: {}", pulse.fixture.name);
    println!("Registry: {} chain {} exchange {}", pulse.fixture.registry.network, pulse.fixture.registry.chain_id, pulse.fixture.registry.exchange_address);
    println!("As-of block {} / {} quality {}", metrics.as_of_block, pulse.quality.status, pulse.quality.status);
    println!();
    println!("1. Protocol Risk Pulse");
    println!("   Taker volume (maker-fill equivalent): {}", metrics.taker_volume);
    println!("   Open interest: {}", metrics.open_interest);
    println!("   TVL (isolated position collateral): {}", metrics.tvl);
    println!("   Protocol fees: {}", metrics.protocol_fees);
    println!("   Liquidations: {}", metrics.liquidations);
    println!("   Active accounts: {}", metrics.active_accounts);
    println!("   Freshness: last event block {} (lag {} blocks)", metrics.last_event_block, pulse.quality.lag_blocks);
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
    println!("   Block {} tx {} log {}", stored.block_number, stored.tx_hash, stored.log_index);
    if let Some(account_id) = stored.account_id {
        println!("   Account {account_id}");
    }
    if let Some(perpetual_id) = stored.perpetual_id {
        println!("   Market {perpetual_id}");
    }
    println!();
    println!("Selected navigation context preserved: as-of block {}, chain {}.", pulse.fixture.as_of.block_number, pulse.fixture.as_of.chain_id);
    let _ = account_wallet(&pulse.ledger, wallets[0].account_id, &pulse.fixture.as_of, &pulse.fixture.marks, true)?;
    Ok(())
}
