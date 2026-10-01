CREATE TABLE IF NOT EXISTS perppulse_serving_snapshot (
    source text PRIMARY KEY CHECK (length(source) BETWEEN 1 AND 64),
    as_of_block bigint NOT NULL CHECK (as_of_block >= 0),
    content_hash text NOT NULL CHECK (content_hash LIKE 'sha256:%'),
    snapshot jsonb NOT NULL CHECK (jsonb_typeof(snapshot) = 'object'),
    observed_at timestamptz NOT NULL DEFAULT now(),
    CHECK (octet_length(snapshot::text) <= 1000000)
);
COMMENT ON TABLE perppulse_serving_snapshot IS 'Compact canonical read models only; raw canonical history remains in Envio.';
