use std::path::Path;

use crate::accounting::{WalletSnapshot, account_wallet};
use crate::error::Result;
use crate::events::{Fixture, load_fixture};
use crate::ledger::{Ledger, replay};
use crate::metrics::{ProtocolMetrics, protocol_metrics};
use crate::quality::{QualityReport, gate_ledger, gate_metrics};
use crate::store::EventStore;

pub struct Pulse {
    pub fixture: Fixture,
    pub ledger: Ledger,
    pub metrics: ProtocolMetrics,
    pub quality: QualityReport,
    pub store: EventStore,
}

impl Pulse {
    pub fn wallets(&self, require_marks: bool) -> Result<Vec<WalletSnapshot>> {
        let mut wallets = Vec::new();
        for account_id in self.ledger.accounts.keys() {
            wallets.push(account_wallet(
                &self.ledger,
                *account_id,
                &self.fixture.as_of,
                &self.fixture.marks,
                require_marks,
            )?);
        }
        wallets.sort_by_key(|wallet| wallet.account_id);
        Ok(wallets)
    }
}

pub fn run_fixture(path: impl AsRef<Path>, max_lag_blocks: Option<u64>) -> Result<Pulse> {
    let fixture = load_fixture(path)?;
    let store = EventStore::memory()?;
    store.insert_many(&fixture.events)?;
    let stored = store.load_all()?;
    let ledger = replay(&stored, &fixture.registry, &fixture.as_of)?;
    let quality = gate_ledger(&ledger, &fixture.as_of, max_lag_blocks)?;
    let metrics = protocol_metrics(
        &ledger,
        &fixture.as_of,
        &fixture.marks,
        &ledger.events,
        fixture.window_start_ms,
    )?;
    gate_metrics(&metrics)?;
    Ok(Pulse {
        fixture,
        ledger,
        metrics,
        quality,
        store,
    })
}
