"""Run the pinned SDK behind a bounded read-only historical RPC gate."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
EXCHANGE = "0x34b6552d57a35a1d042ccae1951bd1c370112a6f"
MULTICALL = "0xca11bde05977b3631167028862be2a173976ca11"
MAX_BODY = 65_536
MAX_RESPONSE = 4 * 1024 * 1024


class Gate:
    def __init__(self, block: int, allowance: int, endpoint: str):
        self.block = hex(block)
        self.allowance = allowance
        self.endpoint = endpoint
        self.lock = threading.Lock()
        self.methods = Counter()
        self.rejections = Counter()
        self.failed = 0

    def validate(self, item: dict) -> None:
        if not isinstance(item, dict) or item.get("jsonrpc") != "2.0" or "id" not in item:
            raise ValueError("invalid_envelope")
        method, params = item.get("method"), item.get("params", [])
        if not isinstance(params, list):
            raise ValueError("invalid_params")
        if method == "eth_chainId" and params == []:
            return
        if method == "eth_getBlockByNumber" and params == [self.block, False]:
            return
        if method == "eth_getCode" and len(params) == 2:
            if str(params[0]).lower() in {EXCHANGE, MULTICALL} and params[1] == self.block:
                return
        if method == "eth_call" and len(params) == 2 and isinstance(params[0], dict):
            call = params[0]
            data = call.get("input", call.get("data", ""))
            if (params[1] == self.block and str(call.get("to")).lower() in {EXCHANGE, MULTICALL}
                    and call.get("value", "0x0") in {"0x0", "0x00"}
                    and isinstance(data, str) and re.fullmatch(r"0x(?:[0-9a-fA-F]{2})+", data)):
                return
        raise ValueError("method_target_or_cutoff_denied")

    def forward(self, body: bytes) -> bytes:
        try:
            value = json.loads(body)
            items = value if isinstance(value, list) else [value]
            if not 1 <= len(items) <= 20:
                raise ValueError("batch_limit")
            for item in items:
                self.validate(item)
            with self.lock:
                if sum(self.methods.values()) + len(items) > self.allowance:
                    raise ValueError("request_allowance_exhausted")
                self.methods.update(item["method"] for item in items)
        except (ValueError, TypeError, KeyError) as error:
            reason = str(error) if str(error) in {
                "invalid_envelope", "invalid_params", "batch_limit",
                "method_target_or_cutoff_denied", "request_allowance_exhausted"
            } else "invalid_json"
            with self.lock:
                self.rejections[reason] += 1
            raise ValueError(reason) from None
        try:
            request = urllib.request.Request(self.endpoint, data=body, headers={
                "Content-Type": "application/json", "User-Agent": "PerpPulse-ReadOnly-Reference/1"
            })
            with urllib.request.urlopen(request, timeout=15) as response:
                result = response.read(MAX_RESPONSE + 1)
            if len(result) > MAX_RESPONSE:
                raise ValueError("response_size_limit")
            envelope = json.loads(result)
            responses = envelope if isinstance(envelope, list) else [envelope]
            if len(responses) != len(items) or any(
                not isinstance(row, dict) or row.get("jsonrpc") != "2.0"
                or ("result" not in row and "error" not in row) for row in responses
            ):
                raise ValueError("invalid_rpc_response")
            with self.lock:
                self.failed += sum("error" in row for row in responses)
            return result
        except (OSError, ValueError, urllib.error.URLError):
            with self.lock:
                self.failed += len(items)
            raise ValueError("rpc_unavailable") from None


def handler(gate: Gate):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 1 <= length <= MAX_BODY:
                    raise ValueError("request_size_limit")
                body = self.rfile.read(length)
                result = gate.forward(body)
                status = 200
            except ValueError:
                result = b'{"error":"Read-only RPC gate rejected or failed the request"}'
                status = 502
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(result)))
            self.end_headers()
            try:
                self.wfile.write(result)
            except OSError:
                pass

    return Handler


def ids(value: str) -> list[int]:
    try:
        values = [int(part) for part in value.split(",")]
        if not values or any(n <= 0 or n > 2**32 - 1 for n in values) or len(set(values)) != len(values):
            raise ValueError
        return values
    except ValueError:
        raise argparse.ArgumentTypeError("IDs must be distinct positive u32 integers") from None


def failure_reason(stderr: bytes) -> str:
    """Export only fixed local failure codes, never arbitrary child/provider text."""
    known = {
        "funding targets regress or overlap before effectiveness": "funding_schedule_overlap",
        "funding cumulative sums are discontinuous": "funding_sum_discontinuity",
        "funding overwrite is unauthorized or changes its prior cumulative sum": "funding_overwrite_invalid",
        "as-of mark is stale (60-second application limit)": "canonical_mark_stale",
        "invalid market scope, cutoff or unready coverage": "market_coverage_invalid",
        "market input coverage changed or regressed": "market_coverage_regressed",
        "processed block ": "processed_source_inconsistent",
        "Envio coverage changed incompatibly during source-height verification": "source_witness_invalid",
        "Envio GraphQL request failed; check endpoint and authentication": "graphql_request_failed",
        "SDK snapshot failed; no reference was accepted": "sdk_snapshot_failed",
        "Canonical price PnL missing": "canonical_price_pnl_missing",
    }
    if len(stderr) > MAX_BODY:
        return "unclassified_local_failure"
    message = stderr.decode("utf-8", errors="replace")
    for phrase, code in known.items():
        if message.startswith("Reference verification failed: ") and phrase in message:
            return code
    return "unclassified_local_failure"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--block", type=int, required=True)
    parser.add_argument("--block-hash", required=True)
    parser.add_argument("--accounts", type=ids, required=True)
    parser.add_argument("--markets", type=ids, required=True)
    parser.add_argument("--graphql", default="http://127.0.0.1:8080/v1/graphql")
    parser.add_argument("--registry", type=Path, default=ROOT / "fixtures/protocol/mainnet-registry.json")
    parser.add_argument("--allowance", type=int, default=96)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--risk-diagnostics", action="store_true")
    parser.add_argument("--market-diagnostics", action="store_true")
    args = parser.parse_args()
    if (not 1 <= args.allowance <= 256 or not 0 < args.block <= 2**64 - 1
            or len(args.accounts) > 20 or len(args.markets) > 5
            or not re.fullmatch(r"0x[0-9a-f]{64}", args.block_hash)):
        parser.error("Invalid cutoff, scope or allowance (maximum 256 RPC requests)")
    if args.output.exists():
        parser.error("Output already exists; select a new evidence path")
    endpoint = os.environ.get("MONAD_RPC_URL", "https://rpc.monad.xyz")
    if not endpoint.startswith("https://"):
        parser.error("Remote RPC requires HTTPS")
    gate = Gate(args.block, args.allowance, endpoint)
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler(gate))
    server.timeout = 1
    server.daemon_threads = True
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    config = {"rpcUrl": f"http://127.0.0.1:{server.server_port}", "block": args.block,
              "blockHash": args.block_hash, "accountIds": args.accounts, "marketIds": args.markets,
              "graphqlUrl": args.graphql, "registryPath": str(args.registry.resolve()),
              "riskDiagnostics": args.risk_diagnostics, "marketDiagnostics": args.market_diagnostics}
    try:
        result = subprocess.run([str(args.binary.resolve())], input=json.dumps(config).encode(),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180, check=False)
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)
    stats = {"allowance": args.allowance, "requests": sum(gate.methods.values()),
             "methods": dict(gate.methods), "rejections": dict(gate.rejections),
             "rpcErrorResponses": gate.failed, "responseLimitBytes": MAX_RESPONSE,
             "requestLimitBytes": MAX_BODY, "overallTimeoutSeconds": 180}
    if result.returncode not in {0, 2} or gate.rejections:
        print(json.dumps({"status": "failed", "stage": "sdk_reference",
                          "reason": failure_reason(result.stderr), "rpcGate": stats}), file=sys.stderr)
        return 1
    if len(result.stdout) > MAX_RESPONSE:
        raise ValueError("Selected reference output exceeds size limit")
    selected = json.loads(result.stdout)
    if (selected.get("version") != "sdk-reference-execution-v1"
            or selected.get("status") not in {"matched", "mismatch"}
            or (selected["status"] == "matched") != (result.returncode == 0)):
        raise ValueError("Invalid SDK reference result")
    selected["observedAt"] = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    selected["rpcGate"] = stats
    artifacts = ["crates/perppulse/src/envio.rs", "crates/perppulse/src/evidence.rs",
                 "crates/perppulse/src/accounting.rs", "crates/perppulse/src/serve.rs", "docs/methodology.json",
                 "crates/perppulse/src/funding.rs", "crates/perppulse/src/ledger.rs",
                 "tools/perpl-reference/src/main.rs", "tools/perpl-reference/Cargo.toml",
                 "tools/perpl-reference/Cargo.lock", "scripts/run_sdk_reference.py"]
    # Git normalizes tracked text to LF; hash that publication representation.
    selected["sourceArtifacts"] = {
        name: "sha256:" + hashlib.sha256((ROOT / name).read_bytes().replace(b"\r\n", b"\n")).hexdigest()
        for name in artifacts
    }
    selected["operatorBinaryHash"] = "sha256:" + hashlib.sha256(args.binary.read_bytes()).hexdigest()
    selected["registryArtifactName"] = args.registry.name
    selected["registryArtifactHash"] = "sha256:" + hashlib.sha256(args.registry.read_bytes().replace(b"\r\n", b"\n")).hexdigest()
    # No raw provider responses, owner addresses, endpoint or credentials are exported.
    with args.output.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(selected, output, indent=2, ensure_ascii=True)
        output.write("\n")
    print(json.dumps({"status": selected["status"], "block": args.block,
                      "accountCount": len(args.accounts), "rpcRequests": stats["requests"]}))
    return result.returncode


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.TimeoutExpired):
        print("Reference execution failed; no successful evidence was exported.", file=sys.stderr)
        raise SystemExit(1) from None
