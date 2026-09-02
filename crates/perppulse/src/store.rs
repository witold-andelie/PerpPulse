use rusqlite::{Connection, OptionalExtension};

use crate::error::{DataQualityError, Result};
use crate::events::CanonicalEvent;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS canonical_event (
    event_id TEXT PRIMARY KEY,
    chain_id INTEGER NOT NULL,
    block_number INTEGER NOT NULL,
    block_hash TEXT NOT NULL,
    tx_hash TEXT NOT NULL,
    log_index INTEGER NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    contract_address TEXT NOT NULL,
    abi_event_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    account_id INTEGER,
    perpetual_id INTEGER,
    payload_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS canonical_event_account ON canonical_event(account_id, block_number, log_index);
CREATE INDEX IF NOT EXISTS canonical_event_market ON canonical_event(perpetual_id, block_number, log_index);
";

pub struct EventStore {
    connection: Connection,
}

impl EventStore {
    pub fn memory() -> Result<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|err| DataQualityError::msg(format!("cannot open in-memory event store: {err}")))?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|err| DataQualityError::msg(format!("cannot create event store schema: {err}")))?;
        Ok(Self { connection })
    }

    pub fn insert(&self, event: &CanonicalEvent) -> Result<()> {
        let event_id = event.event_id()?.key();
        let kind = format!("{:?}", event.kind);
        let payload = serde_json::to_string(event)
            .map_err(|err| DataQualityError::msg(format!("cannot serialize event {event_id}: {err}")))?;
        self.connection
            .execute(
                "INSERT INTO canonical_event (
                    event_id, chain_id, block_number, block_hash, tx_hash, log_index,
                    timestamp_ms, contract_address, abi_event_name, kind, account_id,
                    perpetual_id, payload_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    event_id,
                    event.chain_id as i64,
                    event.block_number as i64,
                    event.block_hash,
                    event.tx_hash,
                    event.log_index as i64,
                    event.timestamp_ms,
                    event.contract_address,
                    event.abi_event_name,
                    kind,
                    event.account_id.map(|value| value as i64),
                    event.perpetual_id.map(|value| value as i64),
                    payload,
                ],
            )
            .map_err(|err| DataQualityError::msg(format!("cannot insert event {event_id}: {err}")))?;
        Ok(())
    }

    pub fn insert_many(&self, events: &[CanonicalEvent]) -> Result<()> {
        for event in events {
            self.insert(event)?;
        }
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<CanonicalEvent>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json FROM canonical_event ORDER BY block_number, log_index, event_id")
            .map_err(|err| DataQualityError::msg(format!("cannot query event store: {err}")))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| DataQualityError::msg(format!("cannot read event store: {err}")))?;
        let mut events = Vec::new();
        for row in rows {
            let payload = row.map_err(|err| DataQualityError::msg(format!("event store row failed: {err}")))?;
            let event: CanonicalEvent = serde_json::from_str(&payload)
                .map_err(|err| DataQualityError::msg(format!("stored event is not valid JSON: {err}")))?;
            event.validate()?;
            events.push(event);
        }
        if events.is_empty() {
            return Err(DataQualityError::msg("event store contains no canonical events"));
        }
        Ok(events)
    }

    pub fn get(&self, event_id: &str) -> Result<CanonicalEvent> {
        let payload: Option<String> = self
            .connection
            .query_row(
                "SELECT payload_json FROM canonical_event WHERE event_id = ?1",
                [event_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| DataQualityError::msg(format!("cannot fetch event {event_id}: {err}")))?;
        let payload = payload.ok_or_else(|| DataQualityError::msg(format!("event {event_id} is not in the store")))?;
        let event: CanonicalEvent = serde_json::from_str(&payload)
            .map_err(|err| DataQualityError::msg(format!("stored event {event_id} is not valid JSON: {err}")))?;
        event.validate()?;
        Ok(event)
    }
}
