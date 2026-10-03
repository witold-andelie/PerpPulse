"""Audit the selected v3 mainnet evidence without credentials or network calls."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs/evidence"
INDEXER_COMMIT = "9e0a3673c8bbfc8c5ccc4a34bdacab4c5ff60269"


def read(name):
    return json.loads((EVIDENCE / name).read_text(encoding="utf-8"))


def digest(raw):
    return "sha256:" + hashlib.sha256(raw.replace(b"\r\n", b"\n")).hexdigest()


def main():
    registry_path = ROOT / "fixtures/protocol/mainnet-registry-2026-10-03.json"
    registry = json.loads(registry_path.read_text())
    specs = {m["perpetual_id"]: m for m in registry["markets"]}
    base = json.loads((ROOT / "fixtures/protocol/mainnet-registry.json").read_text())
    assert all(specs[m["perpetual_id"]] == m for m in base["markets"])
    assert set(specs) - {m["perpetual_id"] for m in base["markets"]} == {70}
    executions = []
    for name, expected in [
        ("sdk-market-execution-2026-10-03.json", (68, 16, 10, 58)),
        ("sdk-market-sol-2026-10-03.json", (12, 0, 2, 32)),
        ("sdk-funding-pending-2026-10-03.json", (18, 12, 2, 33)),
    ]:
        item = read(name)
        assert item["status"] == "matched" and item["asOfLogIndex"] is None
        assert item["chainId"] == 143 and item["sdkCommit"] == "dbb37c59f6aef03e38d0787eb9c968f59f652617"
        groups = ["scorecards", "riskScorecards", "marketScorecards"]
        counts = tuple(sum(len(c["checks"]) for c in item[g]) for g in groups)
        assert counts + (item["rpcGate"]["requests"],) == expected
        assert not item["rpcGate"]["rejections"] and item["rpcGate"]["rpcErrorResponses"] == 0
        assert all(c["status"] == "matched" and all(v["status"] == "matched" for v in c["checks"])
                   for g in groups for c in item[g])
        assert item["registryArtifactHash"] == digest(registry_path.read_bytes())
        for name, expected_hash in item["sourceArtifacts"].items():
            assert name.startswith(("crates/perppulse/src/", "tools/perpl-reference/", "scripts/", "docs/"))
            assert ".." not in Path(name).parts
            assert digest((ROOT/name).read_bytes()) == expected_hash, name
        for market in item["marketObservations"]:
            spec = specs[market["perpetualId"]]
            # Perpl's contract labels the replacement SOL market SOL_v2;
            # the frozen display registry uses SOL for perpetual 31.
            assert market["symbol"] == {31: "SOL_v2"}.get(market["perpetualId"], spec["symbol"])
            assert market["priceDecimals"] == spec["price_decimals"]
            assert market["sizeDecimals"] == spec["size_decimals"]
            assert market["priceMaxAgeSeconds"] * 1000 >= 60000
        assert all(t["positionFundingAmount"] is None for t in item["fundingTimelines"])
        executions.append(item)
    primary, sol, early = executions
    assert primary["asOfBlock"] == sol["asOfBlock"] == 110245407
    assert primary["asOfBlockHash"] == sol["asOfBlockHash"]
    assert {m["perpetualId"] for e in [primary, sol] for m in e["marketObservations"]} == {1, 10, 31, 40, 70, 90}
    active = next(t for t in primary["fundingTimelines"] if t["perpetualId"] == 1)["active"]
    pending = next(t for t in early["fundingTimelines"] if t["perpetualId"] == 1)["pending"]
    assert pending == [active]
    assert active["sourceBlock"] == early["asOfBlock"] == 110240124
    assert early["asOfBlock"] < active["effectiveBlock"] == 110240202 <= primary["asOfBlock"]
    browser = read("live-market-browser-2026-10-03.json")
    manifest = read("live-market-manifest-2026-10-03.json")
    assert len(browser["checks"]) == 10 and len(browser["wallets"]) == 4
    assert browser["mode"] == "live-mainnet-account-slice" and browser["asOfBlock"] == 110247586
    for field in ["asOfBlock", "asOfBlockHash", "asOfLogIndex", "canonicalInputsHash", "marketMarksHash"]:
        assert browser[field] == manifest[field]
    for name, expected_hash in browser["sourceArtifacts"].items():
        raw = (ROOT/name).read_bytes() if name.startswith("fixtures/") else subprocess.check_output(
            ["git", "show", f"{INDEXER_COMMIT}:{name}"], cwd=ROOT)
        assert digest(raw) == expected_hash, name
    positions = [p for w in browser["wallets"] for p in w["openPositions"]]
    assert len(positions) == 4
    assert all(p["markEventId"] and p["unrealizedPricePnl"] is not None
               and p["unrealizedFunding"] is None and p["liquidationPrice"] is None for p in positions)
    source = read("live-market-source-2026-10-03.json")
    assert source["coverage"]["isReady"] and source["coverage"]["eventsProcessed"] == 357925
    assert source["independentHeadLagBlocks"] == 3
    print(json.dumps({"status": "passed", "sameCutoffPositionChecks": 80, "markChecks": 12,
                      "riskChecks": 16, "liveBrowserChecks": 10, "futureFundingPendingToActive": True}))


if __name__ == "__main__":
    main()
