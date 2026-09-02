use crate::error::{DataQualityError, Result};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EventId {
    pub chain_id: u64,
    pub block_hash: String,
    pub tx_hash: String,
    pub log_index: u32,
}

impl EventId {
    pub fn new(chain_id: u64, block_hash: impl Into<String>, tx_hash: impl Into<String>, log_index: u32) -> Result<Self> {
        if chain_id == 0 {
            return Err(DataQualityError::msg("chain_id must be positive"));
        }
        let block_hash = require_text(block_hash.into(), "block_hash")?;
        let tx_hash = require_text(tx_hash.into(), "tx_hash")?;
        Ok(Self { chain_id, block_hash, tx_hash, log_index })
    }

    pub fn key(&self) -> String {
        format!("{}:{}:{}:{}", self.chain_id, self.block_hash, self.tx_hash, self.log_index)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PositionId {
    pub chain_id: u64,
    pub account_id: u64,
    pub perpetual_id: u32,
}

impl PositionId {
    pub fn new(chain_id: u64, account_id: u64, perpetual_id: u32) -> Result<Self> {
        if chain_id == 0 {
            return Err(DataQualityError::msg("chain_id must be positive"));
        }
        Ok(Self { chain_id, account_id, perpetual_id })
    }

    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.chain_id, self.account_id, self.perpetual_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsOf {
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: String,
    pub timestamp_ms: i64,
    pub log_index: Option<u32>,
}

impl AsOf {
    pub fn new(
        chain_id: u64,
        block_number: u64,
        block_hash: impl Into<String>,
        timestamp_ms: i64,
        log_index: Option<u32>,
    ) -> Result<Self> {
        if chain_id == 0 {
            return Err(DataQualityError::msg("chain_id must be positive"));
        }
        if timestamp_ms < 0 {
            return Err(DataQualityError::msg("timestamp_ms must be >= 0"));
        }
        Ok(Self {
            chain_id,
            block_number,
            block_hash: require_text(block_hash.into(), "block_hash")?,
            timestamp_ms,
            log_index,
        })
    }

    pub fn includes(&self, block_number: u64, log_index: u32) -> bool {
        if block_number < self.block_number {
            return true;
        }
        if block_number > self.block_number {
            return false;
        }
        match self.log_index {
            None => true,
            Some(cutoff) => log_index <= cutoff,
        }
    }
}

fn require_text(value: String, name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DataQualityError::msg(format!("{name} is required")));
    }
    Ok(trimmed.to_string())
}
