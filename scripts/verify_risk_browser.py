"""Browser acceptance for the synthetic open-position risk contract."""
import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:18085")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--screenshot", type=Path)
    parser.add_argument("--expect-mark-event", action="store_true")
    parser.add_argument("--expect-pending-funding-block", type=int)
    parser.add_argument("--fixture", default="fixtures/golden/open-position-as-of.json")
    args = parser.parse_args()
    url = args.url.rstrip("/")
    parsed = urlsplit(url)
    if parsed.scheme != "http" or parsed.hostname != "127.0.0.1" or parsed.username or parsed.password:
        parser.error("Use an unauthenticated loopback fixture service")
    if args.output.exists() or (args.screenshot and args.screenshot.exists()):
        parser.error("Evidence files must not already exist")
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(url, wait_until="networkidle")
        expect(page.locator("#mode")).to_have_text("FIXTURE DEMO")
        page.locator("#wallet-select").select_option("42")
        cards = page.locator("#wallet-summary .card")
        for label in ["Total unrealized PnL", "Unsettled funding"]:
            expect(cards.filter(has=page.get_by_text(label, exact=True)).locator(".card-value")).to_have_text("Unavailable")
        expect(cards.filter(has=page.get_by_text("Price PnL", exact=True)).locator(".card-value")).to_have_text("1,000")
        expect(page.locator("#wallet-note")).to_contain_text("Unsettled funding is unverified")
        expect(page.locator("#positions tr").first.locator("td").nth(7)).to_have_text("Unavailable")
        response = page.request.get(url + "/api/snapshot")
        assert response.status == 200
        snapshot = response.json()
        assert snapshot["mode"] == "fixture"
        wallet = next(w for w in snapshot["wallets"] if w["accountId"] == 42)
        position = wallet["positions"][0]
        for field in ["unrealizedPnl", "unrealizedFunding", "fairMarketValue", "liquidationPrice", "liquidationBuffer"]:
            assert position[field] is None
        assert position["riskStatus"] == "funding-unverified"
        assert position["zeroFundingLiquidationPrice"] is not None
        if args.expect_mark_event:
            assert position["markEventId"]
            page.get_by_role("button", name="Mark evidence", exact=True).click()
            detail = json.loads(page.locator("#detail").inner_text())
            assert detail["abi"] == "MarkUpdated" and detail["eventId"] == position["markEventId"]
            assert detail["markPricePns"] == "710000"
        if args.expect_pending_funding_block:
            assert wallet["marketInputs"][0]["pending"][0]["effectiveBlock"] == args.expect_pending_funding_block
            expect(page.locator("#wallet-note")).to_contain_text("scheduled funding at block " + str(args.expect_pending_funding_block))
        assert page.request.post(url + "/api/wallet/42").status == 405
        if args.screenshot:
            page.screenshot(path=str(args.screenshot), full_page=True)
        page.route("**/api/snapshot", lambda route: route.fulfill(status=503, json={"error": "Fixture source unavailable for acceptance"}))
        page.locator("#refresh").click()
        expect(page.locator("#status")).to_have_text("Source unavailable")
        expect(page.locator("#positions tr")).to_have_count(0)
        expect(page.locator("#export")).to_be_disabled()
        assert not errors, errors
        record = {
            "version": "risk-browser-acceptance-v1", "observedAt": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "mode": "synthetic-fixture", "fixture": args.fixture, "markEvidenceChecked": args.expect_mark_event, "pendingFundingBlockChecked": args.expect_pending_funding_block,
            "accountId": 42, "asOfBlock": snapshot["asOfBlock"],
            "pricePnl": wallet["unrealizedPricePnl"], "actualLiquidationPrice": position["liquidationPrice"],
            "zeroFundingLiquidationPrice": position["zeroFundingLiquidationPrice"],
            "checks": ["price component labeled funding excluded", "total PnL and unsettled funding unavailable",
                       "actual liquidation unavailable", "zero-funding scenario separate in API", "writes rejected with 405",
                       "source failure clears financial rows and disables export", "no JavaScript errors"],
            "sourceArtifacts": {name: "sha256:" + hashlib.sha256((ROOT / name).read_bytes().replace(b"\r\n", b"\n")).hexdigest()
                                for name in ["web/app.js", "web/index.html", "crates/perppulse/src/serve.rs", "scripts/verify_risk_browser.py", args.fixture]},
        }
        if args.expect_mark_event:
            record["checks"].append("canonical mark identity and native price inspected")
        if args.expect_pending_funding_block:
            record["checks"].append("future funding remains pending in API and wallet note")
        with args.output.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(json.dumps(record, indent=2) + "\n")
        print(json.dumps({"status": "passed", "mode": record["mode"], "checks": len(record["checks"])}))
        browser.close()


if __name__ == "__main__":
    main()
