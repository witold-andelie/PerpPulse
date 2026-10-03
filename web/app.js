"use strict";
let snapshot = null;
const byId = (id) => document.getElementById(id);
const text = (value) => value === null || value === undefined ? "Unavailable" : String(value);
function amount(value) { if (typeof value !== "string" || !/^-?\d+(\.\d+)?$/.test(value)) return text(value); const [integer,fraction] = value.split("."); const clean = (fraction || "").replace(/0+$/, ""); return integer.replace(/\B(?=(\d{3})+(?!\d))/g,",") + (clean ? `.${clean}` : ""); }
function cell(value) { const node = document.createElement("td"); node.textContent = text(value); return node; }
function empty(body, message, columns) { const row = document.createElement("tr"); const node = cell(message); node.colSpan = columns; node.className = "empty"; row.append(node); body.append(row); }
function cards(id, values) { const area = byId(id); area.replaceChildren(); for (const [label, value, note] of values) { const card = document.createElement("div"); card.className = "card"; for (const [className, content] of [["card-label",label],["card-value",amount(value)],["card-note",note || ""]]) { const node = document.createElement("div"); node.className = className; node.textContent = content; card.append(node); } area.append(card); } }
function option(select, value, label) { const node = document.createElement("option"); node.value = String(value); node.textContent = label; select.append(node); }
function failure(error) { byId("error").textContent = error.message; byId("error").hidden = false; byId("status").textContent = "Source unavailable"; byId("mode").textContent = "SOURCE UNAVAILABLE"; byId("mode").className = "badge"; byId("scope").textContent = "Source unavailable"; snapshot = null; byId("export").disabled = true; for (const id of ["metrics","markets","wallet-summary","positions","events","coverage"]) byId(id).replaceChildren(); for (const id of ["cutoff","source-note","wallet-note","context","event-count","detail"]) byId(id).textContent = "Data unavailable. Refresh after the source recovers."; }
async function refresh() {
  byId("refresh").disabled = true;
  try {
    const response = await fetch("/api/snapshot", {cache:"no-store", signal:AbortSignal.timeout(15000)});
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || "Snapshot request failed");
    if (!Array.isArray(data.wallets) || !Array.isArray(data.events)) throw new Error("Invalid snapshot contract");
    snapshot = data; byId("error").hidden = true;
    byId("status").textContent = data.mode === "fixture" ? "Synthetic fixture / deterministic replay" : "Live coverage / limited account scope";
    byId("mode").textContent = data.mode === "fixture" ? "FIXTURE DEMO" : "LIVE ACCOUNT SLICE";
    byId("mode").className = data.mode === "live" ? "badge live" : "badge";
    byId("cutoff").textContent = `As of ${data.asOfBlock} · ${new Date(data.asOfTimestampMs).toISOString()} · coverage ${data.startBlock}–${data.processedBlock}`;
    byId("scope").textContent = data.mode === "fixture" ? "Synthetic fixture scope" : "Selected account scope";
    const p = data.protocol;
    cards("metrics", [["Executed volume",p.takerVolume,"Maker fills / AUSD"],["Open interest",p.openInterest,"Mark-derived / AUSD"],["Position collateral",p.tvl,"Isolated deposits / AUSD"],["Protocol fees",p.protocolFees,"Insurance + protocol / AUSD"],["Liquidations",p.liquidations,"Covered events"],["Active accounts",p.activeAccounts,"Covered metric scope"]]);
    byId("source-note").textContent = `${data.sourceNote} ${(p.warnings || []).join(" ")}`;
    const markets = byId("markets"); markets.replaceChildren(); byId("market-select").replaceChildren(); option(byId("market-select"),"","All markets");
    for (const market of p.markets || []) { const row = document.createElement("tr"); for (const value of [market.symbol,market.takerVolume,market.openInterest,market.tvl,market.protocolFees,market.liquidations]) row.append(cell(value)); markets.append(row); option(byId("market-select"),market.perpetualId,market.symbol); }
    if (!markets.children.length) empty(markets,"Global market totals are unavailable for this source scope.",6);
    const selected = byId("wallet-select").value; byId("wallet-select").replaceChildren(); for (const wallet of data.wallets) option(byId("wallet-select"),wallet.accountId,`Account ${wallet.accountId}`);
    if (data.wallets.some((w) => String(w.accountId) === selected)) byId("wallet-select").value = selected;
    else { const withPosition = data.wallets.find((w) => w.positions && w.positions.length); if (withPosition) byId("wallet-select").value = String(withPosition.accountId); }
    const marketIds = new Set((p.markets || []).map((m) => m.perpetualId));
    for (const event of data.events) if (event.perpetualId !== null && !marketIds.has(event.perpetualId)) { option(byId("market-select"),event.perpetualId,`Market ${event.perpetualId}`); marketIds.add(event.perpetualId); }
    byId("from-block").value = data.startBlock; byId("to-block").value = data.asOfBlock;
    const coverage = byId("coverage"); coverage.replaceChildren();
    for (const [label,value] of [["Quality",data.coverage.quality],["Processed block",data.processedBlock],["Coverage lag",data.coverage.coverageLagBlocks],["Quiet interval (blocks)",data.coverage.eventSilenceBlocks],["Canonical events",data.manifest.eventCount],["Reconciliation",data.manifest.reconciliation.status]]) { const dt = document.createElement("dt"); dt.textContent = label; const dd = document.createElement("dd"); dd.textContent = text(value); coverage.append(dt,dd); }
    byId("context").textContent = `${data.context.source}: ${data.context.status}. ${data.context.reason}`;
    byId("export").disabled = false; byId("detail").textContent = "Choose an event to inspect its evidence."; renderWallet(); renderEvents();
  } catch (error) { failure(error); } finally { byId("refresh").disabled = false; }
}
function renderWallet() {
  if (!snapshot) return;
  const wallet = snapshot.wallets.find((w) => String(w.accountId) === byId("wallet-select").value);
  if (!wallet) return;
  cards("wallet-summary",[["Realized PnL",wallet.realizedPnl,"AUSD / lifecycle facts"],["Price PnL",wallet.unrealizedPricePnl,"AUSD / funding excluded"],["Total unrealized PnL",wallet.unrealizedPnl,"AUSD / verified funding required"],["Unsettled funding",wallet.unrealizedFunding,"AUSD / verification required"],["Settled funding",wallet.realizedFunding,"AUSD / lifecycle settlements"],["Free balance",wallet.freeBalance,"AUSD / completeness required"]]);
  byId("wallet-note").textContent = [wallet.quality,wallet.balanceNote,...(wallet.warnings || [])].filter(Boolean).join(" ");
  if (wallet.context) byId("wallet-note").textContent += ` Powered by Nansen API: ${wallet.context.status}. ${(wallet.context.labels || []).map((item) => item.label).join(", ")} ${wallet.context.pointInTimeEligible === false ? "Context was observed after the ledger cutoff." : ""}`;
  for (const market of wallet.marketInputs || []) {
    const pending = (market.pending || []).map((p) => p.effectiveBlock).join(", ");
    byId("wallet-note").textContent += ` Market ${market.perpetualId}: ${market.active ? "last observed funding effective at block " + market.active.effectiveBlock : "no effective funding observed in coverage"}${pending ? "; scheduled funding at block " + pending : ""}. Position funding amount remains unverified.`;
  }
  const positions = byId("positions"); positions.replaceChildren();
  for (const p of wallet.positions || []) { const row = document.createElement("tr"); for (const value of [`${p.symbol} / ${p.side}`,p.status,p.size,p.entry,p.deposit,p.realizedPnl,p.unrealizedPricePnl,p.liquidationPrice]) row.append(cell(value)); const action = cell(""); const button = document.createElement("button"); button.textContent = "Evidence ↗"; button.addEventListener("click",() => inspect(p.lastEventId)); action.append(button); if (p.markEventId) { const mark = document.createElement("button"); mark.textContent = "Mark evidence"; mark.addEventListener("click", () => inspect(p.markEventId)); action.append(mark); } row.append(action); positions.append(row); }
  if (!positions.children.length) empty(positions,wallet.replayEligible === false ? "Position replay is blocked by incomplete history." : "No position records in this account snapshot.",9);
}
function inspect(id) { if (!snapshot) return; const event = snapshot.events.find((e) => e.eventId === id); byId("detail").textContent = event ? JSON.stringify(event,null,2) : "Event body is unavailable in compact serving mode. Resolve its ID against the canonical Envio source."; byId("event-detail").open = true; byId("event-detail").scrollIntoView({behavior:"smooth",block:"center"}); }
function renderEvents() {
  if (!snapshot) return;
  const fromText = byId("from-block").value, toText = byId("to-block").value;
  if (!/^\d+$/.test(fromText) || !/^\d+$/.test(toText)) { byId("error").textContent = "Block bounds must be non-negative integers."; byId("error").hidden = false; return; }
  const from = BigInt(fromText), to = BigInt(toText);
  if (from < BigInt(snapshot.startBlock) || to > BigInt(snapshot.asOfBlock) || from > to) { byId("error").textContent = "The selected range must stay inside the snapshot coverage and as-of cutoff."; byId("error").hidden = false; return; }
  byId("error").hidden = true;
  const rows = snapshot.events.filter((e) => BigInt(e.blockNumber) >= from && BigInt(e.blockNumber) <= to && (!byId("market-select").value || String(e.perpetualId) === byId("market-select").value) && (!byId("wallet-filter").checked || String(e.accountId) === byId("wallet-select").value));
  const body = byId("events"); body.replaceChildren();
  for (const e of rows.slice(0,100)) { const row = document.createElement("tr"); for (const value of [`${e.blockNumber} / ${e.logIndex}`,e.abi,e.accountId,e.perpetualId,`${e.txHash.slice(0,10)}…${e.txHash.slice(-6)}`]) row.append(cell(value)); const action = cell(""); const button = document.createElement("button"); button.textContent = "Inspect ↗"; button.addEventListener("click",() => inspect(e.eventId)); action.append(button); row.append(action); body.append(row); }
  if (!rows.length) empty(body,snapshot.eventsAvailable === false ? "Raw events remain in Envio; compact serving carries manifest references only." : "No matching events in this covered range.",6);
  byId("event-count").textContent = `Showing ${Math.min(rows.length,100)} of ${rows.length} matching events · shared as-of block ${snapshot.asOfBlock}.`;
}
byId("refresh").addEventListener("click",refresh);
byId("wallet-select").addEventListener("change",() => {renderWallet();renderEvents();});
byId("apply").addEventListener("click",renderEvents);
byId("wallet-filter").addEventListener("change",renderEvents);
byId("export").addEventListener("click",() => { if (!snapshot) return; const url = URL.createObjectURL(new Blob([JSON.stringify(snapshot.manifest,null,2)],{type:"application/json"})); const link = document.createElement("a"); link.href = url; link.download = `perppulse-manifest-${snapshot.asOfBlock}.json`; link.click(); URL.revokeObjectURL(url); });
refresh();
