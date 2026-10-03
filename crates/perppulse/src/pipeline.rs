use std::path::Path;

use crate::accounting::{account_wallet, WalletSnapshot};
use crate::error::Result;
use crate::events::{load_fixture, Fixture};
use crate::ledger::{replay, Ledger};
use crate::metrics::{protocol_metrics, ProtocolMetrics};
use crate::quality::{gate_ledger, gate_metrics, QualityReport};
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
    let mut fixture = load_fixture(path)?;
    let store = EventStore::memory()?;
    store.insert_many(&fixture.events)?;
    let stored = store.load_all()?;
    let ledger = replay(&stored, &fixture.registry, &fixture.as_of)?;
    if !ledger.market_marks.is_empty() {
        if !fixture.marks.is_empty()
            && fixture
                .marks
                .iter()
                .any(|mark| ledger.market_marks.get(&mark.perpetual_id) != Some(mark))
        {
            return Err(crate::DataQualityError::msg(
                "fixture market state conflicts with canonical mark events",
            ));
        }
        fixture.marks = ledger.market_marks.values().cloned().collect();
    }
    let quality = gate_ledger(&ledger, &fixture.as_of, &fixture.coverage, max_lag_blocks)?;
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
