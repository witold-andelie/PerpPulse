use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{DataQualityError, Result};

pub const SIDE_LONG: u8 = 1;
pub const SIDE_SHORT: u8 = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CollateralSpec {
    pub symbol: String,
    pub address: String,
    pub decimals: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarketSpec {
    pub perpetual_id: u32,
    pub symbol: String,
    pub price_decimals: u32,
    pub size_decimals: u32,
    pub init_margin_frac_hdths: u32,
    pub maint_margin_frac_hdths: u32,
    #[serde(default = "default_listed")]
    pub listed: bool,
    #[serde(default)]
    pub notes: String,
}

fn default_listed() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
struct RegistryFile {
    network: String,
    chain_id: u64,
    chain_name: String,
    exchange_address: String,
    collateral: CollateralSpec,
    deployed_at_block: u64,
    #[serde(default)]
    excluded_perpetuals: Vec<u32>,
    fee_scale_decimals: u32,
    abi_revision: String,
    #[serde(default)]
    sources: Vec<String>,
    retrieved_at: String,
    markets: Vec<MarketSpec>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProtocolRegistry {
    pub network: String,
    pub chain_id: u64,
    pub chain_name: String,
    pub exchange_address: String,
    pub collateral: CollateralSpec,
    pub deployed_at_block: u64,
    pub excluded_perpetuals: Vec<u32>,
    pub fee_scale_decimals: u32,
    pub abi_revision: String,
    pub sources: Vec<String>,
    pub retrieved_at: String,
    pub markets: BTreeMap<u32, MarketSpec>,
}

impl ProtocolRegistry {
    pub fn validate(&self) -> Result<()> {
        if self.chain_id == 0 || self.deployed_at_block == 0 || self.markets.is_empty() {
            return Err(DataQualityError::msg(
                "registry requires a chain, deployment block, and markets",
            ));
        }
        for address in [&self.exchange_address, &self.collateral.address] {
            if address.len() != 42
                || !address.starts_with("0x")
                || !address[2..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(DataQualityError::msg(
                    "registry contains an invalid contract address",
                ));
            }
        }
        if self.collateral.decimals > 18 || self.fee_scale_decimals > 18 {
            return Err(DataQualityError::msg("registry decimal scales exceed 18"));
        }
        let excluded: std::collections::BTreeSet<_> =
            self.excluded_perpetuals.iter().copied().collect();
        if excluded.len() != self.excluded_perpetuals.len() {
            return Err(DataQualityError::msg(
                "registry contains duplicate exclusions",
            ));
        }
        for (id, market) in &self.markets {
            market.validate()?;
            if *id != market.perpetual_id || (excluded.contains(id) && market.listed) {
                return Err(DataQualityError::msg(
                    "registry market identity or listing is inconsistent",
                ));
            }
        }
        Ok(())
    }

    pub fn market(&self, perpetual_id: u32) -> Result<&MarketSpec> {
        let spec = self.markets.get(&perpetual_id).ok_or_else(|| {
            DataQualityError::msg(format!(
                "unknown perpetual_id {perpetual_id} on {}",
                self.network
            ))
        })?;
        spec.validate()?;
        if !spec.listed {
            return Err(DataQualityError::msg(format!(
                "perpetual_id {perpetual_id} ({}) is excluded from the active registry",
                spec.symbol
            )));
        }
        Ok(spec)
    }

    pub fn require_chain(&self, chain_id: u64) -> Result<()> {
        self.validate()?;
        if chain_id != self.chain_id {
            return Err(DataQualityError::msg(format!(
                "chain_id {chain_id} does not match registry {} chain {}",
                self.network, self.chain_id
            )));
        }
        Ok(())
    }
}

impl MarketSpec {
    pub fn validate(&self) -> Result<()> {
        if self.perpetual_id == 0
            || self.symbol.trim().is_empty()
            || self.symbol.len() > 64
            || !self.symbol.is_ascii()
            || self.symbol.chars().any(char::is_control)
        {
            return Err(DataQualityError::msg("registry market identity is invalid"));
        }
        if self.price_decimals > 18 || self.size_decimals > 18 {
            return Err(DataQualityError::msg("market decimal scales exceed 18"));
        }
        if self.init_margin_frac_hdths <= 100
            || self.maint_margin_frac_hdths < self.init_margin_frac_hdths
        {
            return Err(DataQualityError::msg(
                "market margin fractions are inconsistent",
            ));
        }
        Ok(())
    }
}

pub fn load_registry(path: impl AsRef<Path>) -> Result<ProtocolRegistry> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|err| {
        DataQualityError::msg(format!("cannot read registry {}: {err}", path.display()))
    })?;
    let parsed: RegistryFile = serde_json::from_str(&text).map_err(|err| {
        DataQualityError::msg(format!(
            "registry {} is not valid JSON: {err}",
            path.display()
        ))
    })?;
    parse_registry_file(parsed, &path.display().to_string())
}

pub fn parse_registry_value(value: &serde_json::Value, source: &str) -> Result<ProtocolRegistry> {
    let parsed: RegistryFile = serde_json::from_value(value.clone())
        .map_err(|err| DataQualityError::msg(format!("registry {source} is invalid: {err}")))?;
    parse_registry_file(parsed, source)
}

fn parse_registry_file(parsed: RegistryFile, source: &str) -> Result<ProtocolRegistry> {
    if parsed.markets.is_empty() {
        return Err(DataQualityError::msg(format!(
            "protocol registry {source} has no markets"
        )));
    }
    let mut markets = BTreeMap::new();
    for market in parsed.markets {
        if markets.insert(market.perpetual_id, market).is_some() {
            return Err(DataQualityError::msg(format!(
                "duplicate perpetual_id in {source}"
            )));
        }
    }
    let registry = ProtocolRegistry {
        network: parsed.network,
        chain_id: parsed.chain_id,
        chain_name: parsed.chain_name,
        exchange_address: parsed.exchange_address,
        collateral: parsed.collateral,
        deployed_at_block: parsed.deployed_at_block,
        excluded_perpetuals: parsed.excluded_perpetuals,
        fee_scale_decimals: parsed.fee_scale_decimals,
        abi_revision: parsed.abi_revision,
        sources: parsed.sources,
        retrieved_at: parsed.retrieved_at,
        markets,
    };
    registry.validate()?;
    Ok(registry)
}
