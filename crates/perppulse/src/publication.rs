use crate::error::{DataQualityError, Result};
use crate::serve::ApiSnapshot;
use postgres::{Client, Config, NoTls};

pub const SCHEMA: &str = include_str!("../../../deploy/serving-schema.sql");

fn connect(url: &str) -> Result<Client> {
    let mut config: Config = url
        .parse()
        .map_err(|_| DataQualityError::msg("invalid serving database configuration"))?;
    // NoTls is safe only behind a loopback Cloud SQL Auth Proxy or local
    // PostgreSQL. The proxy performs encrypted, authenticated cloud transport.
    if config.get_hosts().is_empty()
        || config.get_hosts().iter().any(|host| match host {
            postgres::config::Host::Tcp(host) => {
                !["127.0.0.1", "localhost", "::1"].contains(&host.as_str())
            }
            #[cfg(unix)]
            postgres::config::Host::Unix(_) => true,
        })
    {
        return Err(DataQualityError::msg(
            "serving database requires a loopback PostgreSQL or Cloud SQL Auth Proxy endpoint",
        ));
    }
    config.connect_timeout(std::time::Duration::from_secs(10));
    let mut client = config
        .connect(NoTls)
        .map_err(|_| DataQualityError::msg("cannot connect to compact serving database"))?;
    client
        .batch_execute("SET statement_timeout=5000; SET lock_timeout=2000")
        .map_err(|_| DataQualityError::msg("cannot apply bounded serving database timeouts"))?;
    Ok(client)
}

/// One atomic compact snapshot per source; raw events remain in Envio.
/// Parameterized SQL never interpolates provider data or credentials.
pub fn publish(url: &str, source: &str, snapshot: &ApiSnapshot) -> Result<()> {
    if source.is_empty() || source.len() > 64 {
        return Err(DataQualityError::msg(
            "publication source must be 1..64 bytes",
        ));
    }
    let mut compact = snapshot.clone();
    compact.events = serde_json::json!([]);
    compact.events_available = false;
    let payload = serde_json::to_string(&compact)
        .map_err(|_| DataQualityError::msg("cannot serialize compact snapshot"))?;
    if payload.len() > 1_000_000 {
        return Err(DataQualityError::msg("compact snapshot exceeds 1 MB bound"));
    }
    let hash = crate::evidence::digest(&compact)?;
    let cutoff = i64::try_from(snapshot.as_of_block)
        .map_err(|_| DataQualityError::msg("snapshot block exceeds database range"))?;
    let mut client = connect(url)?;
    let mut transaction = client
        .transaction()
        .map_err(|_| DataQualityError::msg("cannot begin snapshot transaction"))?;
    transaction
        .batch_execute(SCHEMA)
        .map_err(|_| DataQualityError::msg("cannot apply compact serving schema"))?;
    let changed = transaction.execute(
        "INSERT INTO perppulse_serving_snapshot(source, as_of_block, content_hash, snapshot) VALUES ($1, $2, $3, $4::text::jsonb)
         ON CONFLICT (source) DO UPDATE SET as_of_block=EXCLUDED.as_of_block, content_hash=EXCLUDED.content_hash,
         snapshot=EXCLUDED.snapshot, observed_at=now()
         WHERE (perppulse_serving_snapshot.as_of_block, COALESCE((perppulse_serving_snapshot.snapshot#>>'{manifest,asOfLogIndex}')::bigint, 4294967295))
                 <= (EXCLUDED.as_of_block, COALESCE((EXCLUDED.snapshot#>>'{manifest,asOfLogIndex}')::bigint, 4294967295))
           AND (perppulse_serving_snapshot.snapshot->>'processedBlock')::bigint <= (EXCLUDED.snapshot->>'processedBlock')::bigint
           AND (perppulse_serving_snapshot.as_of_block != EXCLUDED.as_of_block
                OR (perppulse_serving_snapshot.snapshot#>>'{manifest,asOfLogIndex}') IS DISTINCT FROM (EXCLUDED.snapshot#>>'{manifest,asOfLogIndex}')
                OR ((perppulse_serving_snapshot.snapshot#>>'{manifest,canonicalInputsHash}') = (EXCLUDED.snapshot#>>'{manifest,canonicalInputsHash}')
                    AND (perppulse_serving_snapshot.snapshot#>>'{manifest,asOfBlockHash}') = (EXCLUDED.snapshot#>>'{manifest,asOfBlockHash}')
                    AND (perppulse_serving_snapshot.snapshot#>>'{manifest,registryInputsHash}') IS NOT DISTINCT FROM (EXCLUDED.snapshot#>>'{manifest,registryInputsHash}')
                    AND (perppulse_serving_snapshot.snapshot#>>'{manifest,marketMarksHash}') IS NOT DISTINCT FROM (EXCLUDED.snapshot#>>'{manifest,marketMarksHash}')))",
        &[&source, &cutoff, &hash, &payload]).map_err(|_| DataQualityError::msg("compact snapshot publication failed"))?;
    if changed != 1 {
        return Err(DataQualityError::msg(
            "compact publication rejected regressing coverage or inconsistent same-cutoff facts",
        ));
    }
    transaction
        .commit()
        .map_err(|_| DataQualityError::msg("compact snapshot commit failed"))?;
    Ok(())
}

pub fn load(url: &str, source: &str, max_age_seconds: u64) -> Result<ApiSnapshot> {
    if max_age_seconds == 0 || max_age_seconds > 86400 {
        return Err(DataQualityError::msg(
            "database snapshot age must be 1..86400 seconds",
        ));
    }
    let mut client = connect(url)?;
    let row = client.query_opt("SELECT snapshot::text, content_hash, extract(epoch from (now()-observed_at))::float8 FROM perppulse_serving_snapshot WHERE source=$1", &[&source])
        .map_err(|_| DataQualityError::msg("compact snapshot read failed"))?
        .ok_or_else(|| DataQualityError::msg("compact snapshot is missing"))?;
    let age: f64 = row.get(2);
    if !age.is_finite() || age < 0.0 || age > max_age_seconds as f64 {
        return Err(DataQualityError::msg(
            "compact snapshot observation is stale or inconsistent",
        ));
    }
    let payload: String = row.get(0);
    let snapshot: ApiSnapshot = serde_json::from_str(&payload)
        .map_err(|_| DataQualityError::msg("invalid compact snapshot"))?;
    let expected: String = row.get(1);
    if crate::evidence::digest(&snapshot)? != expected {
        return Err(DataQualityError::msg(
            "compact snapshot content hash mismatch",
        ));
    }
    Ok(snapshot)
}
