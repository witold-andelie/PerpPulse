"use strict";
let snapshot = null;
let compareSelection = new Set();
const byId = (id) => document.getElementById(id);
const text = (value) => value === null || value === undefined ? "Unavailable" : String(value);
function amount(value) { if (typeof value !== "string" || !/^-?\d+(\.\d+)?$/.test(value)) return text(value); const [integer,fraction] = value.split("."); const clean = (fraction || "").replace(/0+$/, ""); return integer.replace(/\B(?=(\d{3})+(?!\d))/g,",") + (clean ? `.${clean}` : ""); }
// Display-only: shift a served decimal ratio two places without floating-point arithmetic.
function percent(value) { if (typeof value !== "string" || !/^-?\d+(\.\d+)?$/.test(value)) return text(value); const negative = value.startsWith("-"); const [integer,fraction = ""] = value.replace("-","").split("."); const padded = fraction.padEnd(2,"0"); const whole = (integer + padded.slice(0,2)).replace(/^0+(?=\d)/,""); const rest = padded.slice(2).replace(/0+$/,""); return `${negative ? "-" : ""}${whole}${rest ? "." + rest.slice(0,2) : ""}%`; }
function cell(value, className) { const node = document.createElement("td"); node.textContent = text(value); if (className) node.className = className; return node; }
function empty(body, message, columns) { const row = document.createElement("tr"); const node = cell(message); node.colSpan = columns; node.className = "empty"; row.append(node); body.append(row); }
function cards(id, values) { const area = byId(id); area.replaceChildren(); for (const [label, value, note] of values) { const card = document.createElement("div"); card.className = "card"; for (const [className, content] of [["card-label",label],["card-value",amount(value)],["card-note",note || ""]]) { const node = document.createElement("div"); node.className = className; node.textContent = content; card.append(node); } area.append(card); } }
function option(select, value, label) { const node = document.createElement("option"); node.value = String(value); node.textContent = label; select.append(node); }
function button(label, handler) { const node = document.createElement("button"); node.type = "button"; node.textContent = label; node.addEventListener("click", handler); return node; }
function showDetail(value) { byId("detail").textContent = typeof value === "string" ? value : JSON.stringify(value,null,2); byId("event-detail").open = true; byId("event-detail").scrollIntoView({behavior:"smooth",block:"center"}); }
const severityRank = {critical:3, warning:2, watch:1};
function failure(error) { byId("error").textContent = error.message; byId("error").hidden = false; byId("status").textContent = "Source unavailable"; byId("mode").textContent = "SOURCE UNAVAILABLE"; byId("mode").className = "badge"; byId("scope").textContent = "Source unavailable"; snapshot = null; byId("export").disabled = true; for (const id of ["metrics","markets","wallet-summary","positions","events","coverage","top-signals","windows","signals","stress","resolved","compare","compare-head","overlap","compare-pick"]) byId(id).replaceChildren(); for (const id of ["cutoff","source-note","wallet-note","context","event-count","detail","signal-summary","state-note","signal-count","compare-note","label-note"]) byId(id).textContent = "Data unavailable. Refresh after the source recovers."; }
async function refresh() {
  byId("refresh").disabled = true;
  try {
    const response = await fetch("/api/snapshot", {cache:"no-store", signal:AbortSignal.timeout(15000)});
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || "Snapshot request failed");
    if (!Array.isArray(data.wallets) || !Array.isArray(data.events)) throw new Error("Invalid snapshot contract");
    snapshot = data; byId("error").hidden = true;
    byId("status").textContent = data.mode === "fixture" ? "Synthetic fixture / deterministic replay" : (data.protocol && data.protocol.scope === "protocol" ? "Live coverage / protocol reader" : "Live coverage / limited account scope");
    byId("mode").textContent = data.mode === "fixture" ? "FIXTURE DEMO" : (data.protocol && data.protocol.scope === "protocol" ? "LIVE PROTOCOL + WATCHLIST" : "LIVE ACCOUNT SLICE");
    byId("mode").className = data.mode === "live" ? "badge live" : "badge";
    byId("cutoff").textContent = `As of ${data.asOfBlock} · ${new Date(data.asOfTimestampMs).toISOString()} · coverage ${data.startBlock}–${data.processedBlock}`;
    byId("scope").textContent = data.mode === "fixture" ? "Synthetic fixture scope" : (data.protocol && data.protocol.scope === "protocol" ? "Protocol coverage and selected accounts" : "Selected account scope");
    const p = data.protocol;
    cards("metrics", [["Executed volume",p.takerVolume,"Maker fills / AUSD"],["Open interest",p.openInterest,"Mark-derived / AUSD"],["Position collateral",p.tvl,"Isolated deposits / AUSD"],["Protocol fees",p.protocolFees,"Insurance + protocol / AUSD"],["Liquidations",p.liquidations,"Covered events"],["Active accounts",p.activeAccounts,"Covered metric scope"]]);
    byId("source-note").textContent = `${data.sourceNote} ${(p.warnings || []).join(" ")}`;
    renderAnalytics(data);
    const selected = byId("wallet-select").value; byId("wallet-select").replaceChildren(); for (const wallet of data.wallets) option(byId("wallet-select"),wallet.accountId,`Account ${wallet.accountId}`);
    if (data.wallets.some((w) => String(w.accountId) === selected)) byId("wallet-select").value = selected;
    else { const withPosition = data.wallets.find((w) => w.positions && w.positions.some((p) => p.status === "open")) || data.wallets.find((w) => w.positions && w.positions.length); if (withPosition) byId("wallet-select").value = String(withPosition.accountId); }
    byId("market-select").replaceChildren(); option(byId("market-select"),"","All markets");
    const marketIds = new Set();
    for (const market of p.markets || []) { option(byId("market-select"),market.perpetualId,market.symbol); marketIds.add(market.perpetualId); }
    for (const event of data.events) if (event.perpetualId !== null && !marketIds.has(event.perpetualId)) { option(byId("market-select"),event.perpetualId,`Market ${event.perpetualId}`); marketIds.add(event.perpetualId); }
    byId("from-block").value = data.startBlock; byId("to-block").value = data.asOfBlock;
    const coverage = byId("coverage"); coverage.replaceChildren();
    for (const [label,value] of [["Quality",data.coverage.quality],["Processed block",data.processedBlock],["Coverage lag",data.coverage.coverageLagBlocks],["Quiet interval (blocks)",data.coverage.eventSilenceBlocks],["Canonical events",data.manifest.eventCount],["Reconciliation",data.manifest.reconciliation.status],["Signal rules",data.signals ? data.signals.version : null],["Analytics",data.analytics ? data.analytics.version : null]]) { const dt = document.createElement("dt"); dt.textContent = label; const dd = document.createElement("dd"); dd.textContent = text(value); coverage.append(dt,dd); }
    byId("context").textContent = `${data.context.source}: ${data.context.status}. ${data.context.reason}`;
    byId("export").disabled = false; byId("detail").textContent = "Choose an event or signal to inspect its evidence.";
    renderSignals(); renderWallet(); renderCompareOptions(); renderCompare(); renderEvents();
  } catch (error) { failure(error); } finally { byId("refresh").disabled = false; }
}
function renderAnalytics(data) {
  const analytics = data.analytics || {};
  const windows = byId("windows"); windows.replaceChildren();
  byId("analytics-scope").textContent = analytics.status === "unavailable" || !analytics.windows ? "Analytics unavailable" : `${analytics.version} · ${analytics.historyBasis === "deployment-history" ? "history from deployment" : "bounded coverage from block " + analytics.coverageStartBlock}`;
  for (const w of analytics.windows || []) {
    const row = document.createElement("tr"); const t = w.totals || {};
    const status = cell(w.status === "complete" ? "Complete" : "Incomplete", w.status === "complete" ? "ok" : "warn"); status.title = w.reason;
    row.append(cell(w.id === "coverage" ? "Covered range" : w.id), status);
    for (const value of w.totals ? [amount(t.takerVolume), t.trades, amount(t.protocolFees), `${t.liquidations} / ${amount(t.liquidationNotional)}`, amount(t.netCollateralFlow), t.activeAccounts] : [null,null,null,null,null,null]) row.append(cell(value));
    const action = cell(""); action.append(button("Window ↗", () => showDetail({window:w.id, status:w.status, basis:w.basis, reason:w.reason, startTimestampMs:w.startTimestampMs, endBlock:w.endBlock, endLogIndex:w.endLogIndex, eventCount:w.eventCount, firstEventId:w.firstEventId, lastEventId:w.lastEventId, eventIdsHash:w.eventIdsHash}))); row.append(action);
    windows.append(row);
  }
  if (!windows.children.length) empty(windows, analytics.reason || "Protocol windows are unavailable for this source scope.", 9);
  const state = analytics.state;
  byId("state-note").textContent = state ? `Point-in-time state: ${state.status}. ${state.reason}` : (analytics.reason || "Point-in-time protocol state is unavailable.");
  const stateMarkets = new Map(((state && state.markets) || []).map((m) => [m.perpetualId, m]));
  const markets = byId("markets"); markets.replaceChildren();
  for (const market of (data.protocol.markets || [])) {
    const s = stateMarkets.get(market.perpetualId);
    const row = document.createElement("tr");
    const skew = s ? s.skew : market.skew;
    for (const value of [market.symbol, amount(market.takerVolume), amount(market.openInterest), s ? `${amount(s.longOpenInterest)} / ${amount(s.shortOpenInterest)}` : null, skew === null || skew === undefined ? null : percent(skew), amount(market.tvl), amount(market.protocolFees), market.liquidations]) row.append(cell(value));
    markets.append(row);
  }
  if (!markets.children.length) empty(markets,"Global market totals are unavailable for this source scope.",8);
}
function selectedAccount() { return byId("wallet-select").value; }
function renderSignals() {
  const report = snapshot && snapshot.signals;
  const top = byId("top-signals"); top.replaceChildren();
  const body = byId("signals"); body.replaceChildren();
  const stress = byId("stress"); stress.replaceChildren();
  const resolved = byId("resolved"); resolved.replaceChildren();
  if (!report) { byId("signal-summary").textContent = "Signals are unavailable for this snapshot."; empty(body,"Signals are unavailable.",7); empty(stress,"Stress scenarios are unavailable.",6); return; }
  byId("rules-hash").textContent = `${report.version} · rules ${report.rulesHash.slice(0,19)}…`;
  const items = report.items || [];
  for (const id of report.topSignalIds || []) {
    const s = items.find((item) => item.signalId === id); if (!s) continue;
    const card = document.createElement("article"); card.className = `signal-card ${s.severity}`;
    const head = document.createElement("div"); head.className = "signal-head"; head.textContent = `${s.severity.toUpperCase()} · ${s.category}${s.change && s.change !== "baseline" && s.change !== "unchanged" ? " · " + s.change : ""}`;
    const title = document.createElement("div"); title.className = "signal-title"; title.textContent = s.title;
    const note = document.createElement("div"); note.className = "card-note"; note.textContent = `Rule ${s.ruleId} · since block ${s.severitySinceAsOfBlock} · ${s.basis}`;
    const actions = document.createElement("div"); actions.className = "signal-actions";
    actions.append(button("Inspect evidence ↗", () => inspectSignal(s)));
    card.append(head, title, note, actions);
    if (s.accountId !== null && snapshot.wallets.some((w) => w.accountId === s.accountId)) actions.append(button("Open account", () => { byId("wallet-select").value = String(s.accountId); renderWallet(); renderSignalRows(); renderEvents(); byId("wallet-title").scrollIntoView({behavior:"smooth"}); }));
    top.append(card);
  }
  if (!top.children.length) { const none = document.createElement("p"); none.className = "source-note"; none.textContent = "No rule crossed a threshold at this cutoff."; top.append(none); }
  const counts = report.counts || {};
  byId("signal-summary").textContent = `${counts.critical || 0} critical · ${counts.warning || 0} warning · ${counts.watch || 0} watch at block ${report.asOfBlock}${report.baselineAsOfBlock ? " · changes since block " + report.baselineAsOfBlock : " · first observation in this process"}. Signals compare served facts with hashed rules; they never create prices or PnL.`;
  renderSignalRows();
  for (const r of report.resolved || []) { const li = document.createElement("li"); li.textContent = `${r.title} (last ${r.lastSeverity}, resolved at block ${r.resolvedAsOfBlock})`; resolved.append(li); }
  if (!resolved.children.length) { const li = document.createElement("li"); li.textContent = "No signal resolved in the retained window."; resolved.append(li); }
  byId("stress-method").textContent = report.stress.method;
  for (const scenario of report.stress.scenarios || []) {
    const row = document.createElement("tr");
    for (const value of [percent(scenario.shock), scenario.positionsEvaluated, scenario.breached, amount(scenario.breachedCollateral), scenario.conditionalBreaches, (scenario.breaches || []).map((b) => `${b.accountId} ${b.symbol} ${b.side}${b.basis === "funded" ? "" : " (conditional)"}`).join(", ") || "None"]) row.append(cell(value, scenario.breached ? "warn" : ""));
    stress.append(row);
  }
  if (!stress.children.length) empty(stress,"No eligible open positions to stress.",6);
  if ((report.stress.excluded || []).length) { const row = document.createElement("tr"); const node = cell(`Excluded: ${report.stress.excluded.map((e) => `${e.accountId}${e.perpetualId === null ? "" : "/" + e.perpetualId} (${e.reason})`).join(", ")}`); node.colSpan = 6; node.className = "empty"; row.append(node); stress.append(row); }
}
function renderSignalRows() {
  const report = snapshot && snapshot.signals; if (!report) return;
  const body = byId("signals"); body.replaceChildren();
  const minimum = severityRank[byId("signal-severity").value] || 0; const category = byId("signal-category").value; const walletOnly = byId("signal-wallet").checked;
  const rows = (report.items || []).filter((s) => (severityRank[s.severity] || 0) >= minimum && (!category || s.category === category) && (!walletOnly || String(s.accountId) === selectedAccount()));
  for (const s of rows) {
    const row = document.createElement("tr");
    row.append(cell(s.severity, `sev ${s.severity}`), cell(s.title, "wrap"), cell(s.metric === null ? null : percent(s.metric)), cell(s.change), cell(s.basis), cell(`${s.ruleId} ${s.ruleHash.slice(7,15)}`));
    const action = cell(""); action.append(button("Evidence ↗", () => inspectSignal(s))); row.append(action); body.append(row);
  }
  if (!rows.length) empty(body, (report.items || []).length ? "No signals match these filters." : "No rule crossed a threshold at this cutoff.", 7);
  byId("signal-count").textContent = `Showing ${rows.length} of ${(report.items || []).length} signals · shared as-of block ${report.asOfBlock}.`;
}
function inspectSignal(s) {
  const linked = {};
  for (const [key, id] of Object.entries(s.evidence || {})) if (typeof id === "string") { const event = snapshot.events.find((e) => e.eventId === id); if (event) linked[key] = {blockNumber:event.blockNumber, logIndex:event.logIndex, txHash:event.txHash, abi:event.abi}; }
  const rule = (snapshot.signals.rules || []).find((r) => r.id === s.ruleId);
  showDetail({signal:s, rule, linkedEvents:linked});
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
    byId("wallet-note").textContent += ` Market ${market.perpetualId}: ${market.active ? "last observed funding effective at block " + market.active.effectiveBlock : "no effective funding observed in coverage"}${pending ? "; scheduled funding at block " + pending : ""}.`;
  }
  const positions = byId("positions"); positions.replaceChildren();
  for (const p of wallet.positions || []) { const row = document.createElement("tr"); for (const value of [`${p.symbol} / ${p.side}`,p.status,amount(p.size),amount(p.entry),amount(p.deposit),amount(p.realizedPnl),amount(p.unrealizedPricePnl),amount(p.liquidationPrice)]) row.append(cell(value)); const action = cell(""); action.append(button("Evidence ↗",() => inspect(p.lastEventId))); if (p.markEventId) action.append(button("Mark evidence", () => inspect(p.markEventId))); if (p.fundingCheckpoint?.resetEventId) action.append(button("Funding checkpoint", () => showDetail(p.fundingCheckpoint))); row.append(action); positions.append(row); }
  if (!positions.children.length) empty(positions,wallet.replayEligible === false ? "Position replay is blocked by incomplete history." : "No position records in this account snapshot.",9);
}
function renderCompareOptions() {
  const cohort = snapshot && snapshot.cohort; const select = byId("label-filter"); const previous = select.value;
  select.replaceChildren(); option(select,"","All snapshot accounts");
  if (!cohort) return;
  for (const group of cohort.labels.groups || []) option(select, group.label, `${group.label}${group.category ? " (" + group.category + ")" : ""} · ${group.members.length}`);
  if ([...select.options].some((o) => o.value === previous)) select.value = previous;
  const labels = cohort.labels;
  byId("label-note").textContent = labels.status === "unavailable" ? "Nansen labels are unavailable for this snapshot; comparison uses canonical facts only." : `${labels.attribution || "Nansen"}: ${labels.status}. ${labels.note}`;
  const holders = (cohort.wallets || []).filter((w) => w.openPositions > 0).map((w) => w.accountId);
  compareSelection = new Set([...compareSelection].filter((id) => cohort.wallets.some((w) => w.accountId === id)));
  if (!compareSelection.size) for (const id of holders.slice(0,3)) compareSelection.add(id);
}
function renderCompare() {
  const cohort = snapshot && snapshot.cohort; const pick = byId("compare-pick"); pick.replaceChildren();
  const head = byId("compare-head"); head.replaceChildren(); const body = byId("compare"); body.replaceChildren(); const overlap = byId("overlap"); overlap.replaceChildren();
  if (!cohort) { empty(body,"Comparison is unavailable.",1); return; }
  const label = byId("label-filter").value;
  const group = (cohort.labels.groups || []).find((g) => g.label === label);
  const allowed = new Set(group ? group.members.map((m) => m.accountId) : cohort.wallets.map((w) => w.accountId));
  for (const wallet of cohort.wallets.filter((w) => allowed.has(w.accountId))) {
    const chip = document.createElement("label"); chip.className = "chip";
    const box = document.createElement("input"); box.type = "checkbox"; box.checked = compareSelection.has(wallet.accountId);
    box.addEventListener("change", () => { if (box.checked) compareSelection.add(wallet.accountId); else compareSelection.delete(wallet.accountId); renderCompare(); });
    chip.append(box, document.createTextNode(` ${wallet.accountId}${wallet.labels.length ? " · " + wallet.labels.join(", ") : ""}`)); pick.append(chip);
  }
  const chosen = cohort.wallets.filter((w) => allowed.has(w.accountId) && compareSelection.has(w.accountId));
  const header = document.createElement("tr"); header.append(Object.assign(document.createElement("th"), {textContent:"Statistic"}));
  for (const w of chosen) { const th = document.createElement("th"); th.append(button(`Account ${w.accountId}`, () => { byId("wallet-select").value = String(w.accountId); renderWallet(); renderSignalRows(); renderEvents(); byId("wallet-title").scrollIntoView({behavior:"smooth"}); })); header.append(th); }
  head.append(header);
  const stats = [["Open positions","openPositions",false,false],["Open notional","openNotional",true,false],["Open collateral","openCollateral",false,false],["Collateral leverage","collateralLeverage",true,false],["Realized PnL","realizedPnl",true,false],["Unrealized price PnL","unrealizedPricePnl",true,false],["Price return on collateral","priceReturnOnCollateral",true,true],["Fees","fees",true,false]];
  for (const [title,key,ranked,isRatio] of stats) {
    const row = document.createElement("tr"); row.append(cell(title));
    for (const w of chosen) { const value = w[key]; const pct = ranked ? w.percentiles[key] : undefined; row.append(cell(`${isRatio ? percent(value) : amount(value)}${pct === undefined ? "" : pct === null ? " · unranked" : " · P" + pct}`)); }
    body.append(row);
  }
  if (!chosen.length) empty(body,"Select accounts to compare.",1);
  byId("compare-note").textContent = `${cohort.version}: ${cohort.members} ranked accounts at block ${cohort.asOfBlock}. ${cohort.percentileMethod}${(cohort.excluded || []).length ? " Excluded: " + cohort.excluded.map((e) => e.accountId).join(", ") + " (incomplete history)." : ""}`;
  const ids = new Set(chosen.map((w) => w.accountId));
  for (const pair of cohort.overlap || []) {
    if (!ids.has(pair.accountA) || !ids.has(pair.accountB)) continue;
    const row = document.createElement("tr");
    for (const value of [`${pair.accountA} / ${pair.accountB}`, pair.sharedMarkets.map((m) => `${m.symbol} ${m.sideA}/${m.sideB}`).join(", ") || "None", pair.jaccard, pair.sameSideMarkets, pair.opposingMarkets]) row.append(cell(value));
    overlap.append(row);
  }
  if (!overlap.children.length) empty(overlap,"Select at least two accounts with open positions.",5);
}
function inspect(id) { if (!snapshot) return; const event = snapshot.events.find((e) => e.eventId === id); showDetail(event || "Event body is unavailable in compact serving mode. Resolve its ID against the canonical Envio source."); }
function renderEvents() {
  if (!snapshot) return;
  const fromText = byId("from-block").value, toText = byId("to-block").value;
  if (!/^\d+$/.test(fromText) || !/^\d+$/.test(toText)) { byId("error").textContent = "Block bounds must be non-negative integers."; byId("error").hidden = false; return; }
  const from = BigInt(fromText), to = BigInt(toText);
  if (from < BigInt(snapshot.startBlock) || to > BigInt(snapshot.asOfBlock) || from > to) { byId("error").textContent = "The selected range must stay inside the snapshot coverage and as-of cutoff."; byId("error").hidden = false; return; }
  byId("error").hidden = true;
  const rows = snapshot.events.filter((e) => BigInt(e.blockNumber) >= from && BigInt(e.blockNumber) <= to && (!byId("market-select").value || String(e.perpetualId) === byId("market-select").value) && (!byId("wallet-filter").checked || String(e.accountId) === byId("wallet-select").value));
  const body = byId("events"); body.replaceChildren();
  for (const e of rows.slice(0,100)) { const row = document.createElement("tr"); for (const value of [`${e.blockNumber} / ${e.logIndex}`,e.abi,e.accountId,e.perpetualId,`${e.txHash.slice(0,10)}…${e.txHash.slice(-6)}`]) row.append(cell(value)); const action = cell(""); action.append(button("Inspect ↗",() => inspect(e.eventId))); row.append(action); body.append(row); }
  if (!rows.length) empty(body,snapshot.eventsAvailable === false ? "Raw events remain in Envio; compact serving carries manifest references only." : "No matching events in this covered range.",6);
  byId("event-count").textContent = `Showing ${Math.min(rows.length,100)} of ${rows.length} matching events · shared as-of block ${snapshot.asOfBlock}.`;
}
byId("refresh").addEventListener("click",refresh);
byId("wallet-select").addEventListener("change",() => {renderWallet();renderSignalRows();renderEvents();});
byId("apply").addEventListener("click",renderEvents);
byId("wallet-filter").addEventListener("change",renderEvents);
for (const id of ["signal-severity","signal-category","signal-wallet"]) byId(id).addEventListener("change",renderSignalRows);
byId("label-filter").addEventListener("change",renderCompare);
byId("export").addEventListener("click",() => { if (!snapshot) return; const url = URL.createObjectURL(new Blob([JSON.stringify(snapshot.manifest,null,2)],{type:"application/json"})); const link = document.createElement("a"); link.href = url; link.download = `perppulse-manifest-${snapshot.asOfBlock}.json`; link.click(); URL.revokeObjectURL(url); });
refresh();
