use std::fmt;

/// Visible failure for missing, stale, inconsistent, or non-finite data.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DataQualityError(pub String);

impl DataQualityError {
    pub fn msg(message: impl fmt::Display) -> Self {
        Self(message.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DataQualityError>;
