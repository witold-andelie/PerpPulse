use serde::{Deserialize, Serialize};

use crate::error::{DataQualityError, Result};

/// Transactional chain coverage supplied by the indexer, never inferred from
/// the timestamp or block number of the last matching business event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoverageEvidence {
    pub chain_id: u64,
    pub start_block: u64,
    pub processed_block: u64,
}

impl CoverageEvidence {
    pub fn validate(&self) -> Result<()> {
        if self.chain_id == 0 {
            return Err(DataQualityError::msg("coverage chain_id must be positive"));
        }
        if self.processed_block < self.start_block {
            return Err(DataQualityError::msg(format!(
                "coverage processed block {} precedes start block {}",
                self.processed_block, self.start_block
            )));
        }
        Ok(())
    }
}
