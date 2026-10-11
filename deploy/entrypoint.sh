#!/bin/sh
set -eu

# Render selects the explicit archive configuration through public environment variables.
case "${PERPPULSE_SERVE_MODE:-fixture}" in
  historical)
    exec perppulse serve-envio-archive \
      --accounts "${PERPPULSE_ACCOUNTS:?Historical accounts are required}" \
      --graphql-url "${PERPPULSE_GRAPHQL_URL:?Public Envio endpoint is required}" \
      --rpc-url "${PERPPULSE_RPC_URL:-https://rpc.monad.xyz}" \
      --registry "${PERPPULSE_REGISTRY:?Historical registry is required}" \
      --registry-proof "${PERPPULSE_REGISTRY_PROOF:?Same-cutoff registry proof is required}" \
      --start-block "${PERPPULSE_START_BLOCK:?Historical start is required}" \
      --end-block "${PERPPULSE_END_BLOCK:?Historical end is required}" \
      --expected-event-count "${PERPPULSE_EVENT_COUNT:?Exact canonical event count is required}" \
      --bind "0.0.0.0:${PORT:-8080}"
    ;;
  fixture) exec perppulse "$@" ;;
  *) printf '%s\n' 'Unsupported PerpPulse serving mode.' >&2; exit 1 ;;
esac
