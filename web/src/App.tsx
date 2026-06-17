import { useEffect, useMemo, useRef, useState } from "react";
import ZoneMap from "./components/ZoneMap";
import { Modal } from "./components/Modal";
import { ToastHost, ToastMsg } from "./components/Toast";
import { api, getApiBase, setApiBase } from "./lib/api";
import { fmtRfc3339, fmtUnits } from "./lib/time";
import { blastRadius, recommendedControlsFor } from "./lib/risk";
import { ZONES, zoneNumber } from "./zones";
import { Zone, ZoneControls, SpoolStats, AuditEntry, Incident, Balance, Txn, TxnDetail } from "./types";
import { ConnBanner } from "./components/ConnBanner";
import { BalancesTable } from "./components/BalancesTable";
import { TransactionsTable } from "./components/TransactionsTable";
import { IncidentsTable } from "./components/IncidentsTable";
import { AuditTable } from "./components/AuditTable";
import { TransferForm } from "./components/TransferForm";

function clsStatus(s: string) {
  const t = (s || "").toLowerCase();
  if (t === "ok") return "ok";
  if (t === "degraded") return "degraded";
  return "down";
}

function uuidv4(): string {
  return crypto.randomUUID();
}

function secureRandomInt(max: number): number {
  const arr = new Uint32Array(1);
  crypto.getRandomValues(arr);
  return arr[0] % max;
}

export default function App() {
  const [apiBase, setApiBaseState] = useState<string>(getApiBase());
  const [apiVersion, setApiVersion] = useState<any | null>(null);

  const [zones, setZones] = useState<Zone[]>([]);
  const [selectedZoneId, setSelectedZoneId] = useState<string>(ZONES[0].id);

  const [controls, setControls] = useState<ZoneControls | null>(null);
  const [spool, setSpool] = useState<SpoolStats | null>(null);
  const [audit, setAudit] = useState<AuditEntry[]>([]);

  const [incidents, setIncidents] = useState<Incident[]>([]);
  const [allIncidents, setAllIncidents] = useState<Incident[]>([]);

  const [balances, setBalances] = useState<Balance[]>([]);
  const [txns, setTxns] = useState<Txn[]>([]);

  const [connStatus, setConnStatus] = useState<"loading" | "connected" | "offline">("loading");
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  const [busy, setBusy] = useState(false);
  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const [modal, setModal] = useState<{ title: string; body: any } | null>(null);

  const [actor, setActor] = useState("operator-1");
  const [reason, setReason] = useState("sim action");
  // Defaults to the local dev key (infra/docker-compose.yml) so the demo's
  // operator actions work one-click; override for any other backend.
  const [adminKey, setAdminKey] = useState("dev-admin-key");

  const [transferFrom, setTransferFrom] = useState("acct-A");
  const [transferTo, setTransferTo] = useState("acct-B");
  const [transferAmount, setTransferAmount] = useState(120);

  const [autoTraffic, setAutoTraffic] = useState(false);
  const autoRef = useRef<number | null>(null);

  const [replayLimit, setReplayLimit] = useState(50);

  const [incidentManage, setIncidentManage] = useState<Incident | null>(null);
  const [incidentAssignee, setIncidentAssignee] = useState("");
  const [incidentNote, setIncidentNote] = useState("");

  const selectedZone = useMemo(() => zones.find(z => z.id === selectedZoneId) || null, [zones, selectedZoneId]);

  function toast(title: string, message?: string) {
    setToasts((t) => [{ id: uuidv4(), title, message }, ...t].slice(0, 6));
  }

  // Auth + actor headers for operator mutations. The server records the audit
  // actor from X-Actor (trusted only because X-Admin-Key validated), not the body.
  function opsHeaders(): Record<string, string> {
    return { "X-Admin-Key": adminKey, "X-Actor": actor };
  }

  async function refreshAll() {
    const results = await Promise.allSettled([
      loadVersion(),
      loadZones(),
      loadBalances(),
      loadTxns(),
      loadAllIncidents(),
      loadZoneDrilldown(selectedZoneId),
    ]);
    const allFailed = results.every(r => r.status === "rejected");
    if (allFailed) {
      setConnStatus("offline");
    } else {
      setConnStatus("connected");
      setLastUpdated(new Date());
    }
  }

  async function loadVersion() {
    try {
      const v = await api<any>("/v1/version");
      setApiVersion(v);
    } catch {
      setApiVersion(null);
    }
  }

  async function loadZones() {
    const res = await api<{ zones: Zone[] }>("/v1/zones");
    setZones(res.zones);
  }

  async function loadZoneDrilldown(zoneId: string) {
    await Promise.all([
      loadIncidents(zoneId),
      loadControls(zoneId),
      loadSpool(zoneId),
      loadAudit(zoneId),
    ]);
  }

  async function loadControls(zoneId: string) {
    const res = await api<ZoneControls>(`/v1/zones/${zoneId}/controls`);
    setControls(res);
  }

  async function loadSpool(zoneId: string) {
    const res = await api<SpoolStats>(`/v1/zones/${zoneId}/spool`);
    setSpool(res);
  }

  async function loadAudit(zoneId: string) {
    const res = await api<{ audit: AuditEntry[] }>(`/v1/zones/${zoneId}/audit?limit=80`);
    setAudit(res.audit);
  }

  async function loadIncidents(zoneId: string) {
    const res = await api<{ incidents: Incident[] }>(`/v1/zones/${zoneId}/incidents`);
    setIncidents(res.incidents);
  }

  async function loadAllIncidents() {
    const res = await api<{ incidents: Incident[] }>(`/v1/incidents?limit=2000`);
    setAllIncidents(res.incidents);
  }

  async function loadBalances() {
    const res = await api<{ balances: Balance[] }>("/v1/balances");
    setBalances(res.balances);
  }

  async function loadTxns() {
    const res = await api<{ transactions: Txn[] }>("/v1/transactions");
    setTxns(res.transactions);
  }

  useEffect(() => {
    setApiBase(apiBase);
  }, [apiBase]);

  useEffect(() => {
    refreshAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!selectedZoneId) return;
    loadZoneDrilldown(selectedZoneId).catch((e: any) => toast("Zone drilldown failed", String(e?.message || e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedZoneId]);

  useEffect(() => {
    let id: number;
    function schedule() {
      id = window.setTimeout(async () => {
        if (!document.hidden) {
          await refreshAll().catch(() => {});
        }
        schedule();
      }, connStatus === "offline" ? 5000 : 10000);
    }
    schedule();
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connStatus, selectedZoneId]);

  useEffect(() => {
    if (!autoTraffic) {
      if (autoRef.current) window.clearInterval(autoRef.current);
      autoRef.current = null;
      return;
    }
    autoRef.current = window.setInterval(() => {
      const z = selectedZoneId;
      const from = `acct-${String.fromCharCode(65 + secureRandomInt(6))}`;
      const to = `acct-${String.fromCharCode(65 + secureRandomInt(6))}`;
      if (from === to) return;
      const amt = [30, 60, 120, 300, 600, 1200, 3600][secureRandomInt(7)];
      createTransfer(from, to, amt, z, { mode: "auto" }).catch(() => {});
    }, 1300);
    return () => { if (autoRef.current) window.clearInterval(autoRef.current); autoRef.current = null; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoTraffic, selectedZoneId]);

  async function setZoneStatus(status: "OK" | "DEGRADED" | "DOWN") {
    if (!selectedZone) return;
    setBusy(true);
    try {
      await api(`/v1/zones/${selectedZone.id}/status`, { method: "POST", headers: opsHeaders(), body: { status, reason } });
      toast("Zone status updated", `${selectedZone.name} -> ${status}`);
      await refreshAll();
    } catch (e: any) {
      toast("Zone status failed", String(e?.message || e));
    } finally {
      setBusy(false);
    }
  }

  async function saveControls(next: Partial<ZoneControls>) {
    if (!selectedZone) return;
    const current = controls;
    if (!current) return;

    const payload = {
      writes_blocked: next.writes_blocked ?? current.writes_blocked,
      cross_zone_throttle: next.cross_zone_throttle ?? current.cross_zone_throttle,
      spool_enabled: next.spool_enabled ?? current.spool_enabled,
      reason,
    };

    setBusy(true);
    try {
      await api(`/v1/zones/${selectedZone.id}/controls`, { method: "POST", headers: opsHeaders(), body: payload });
      toast("Controls updated", `${selectedZone.id}`);
      await loadZoneDrilldown(selectedZone.id);
    } catch (e: any) {
      toast("Controls update failed", String(e?.message || e));
    } finally {
      setBusy(false);
    }
  }

  async function applyRecommendedControls() {
    if (!selectedZone) return;
    const rec = recommendedControlsFor(selectedZone.status);
    toast("Playbook suggestion", rec.rationale);
    await saveControls({
      writes_blocked: rec.writes_blocked,
      cross_zone_throttle: rec.cross_zone_throttle,
      spool_enabled: rec.spool_enabled,
    });
  }

  async function replaySpool() {
    if (!selectedZone) return;
    setBusy(true);
    try {
      const res = await api<any>(`/v1/zones/${selectedZone.id}/spool/replay`, {
        method: "POST",
        headers: opsHeaders(),
        body: { limit: replayLimit, reason },
      });
      toast("Spool replayed", `Applied ${res.applied}, failed ${res.failed}`);
      await loadZoneDrilldown(selectedZone.id);
      await Promise.all([loadBalances(), loadTxns()]);
    } catch (e: any) {
      toast("Replay failed", String(e?.message || e));
    } finally {
      setBusy(false);
    }
  }

  async function createTransfer(from: string, to: string, amount: number, zoneId: string, metadata?: any) {
    try {
      const res = await api<any>(`/v1/transfers`, {
        method: "POST",
        body: {
          request_id: uuidv4(),
          from_account: from,
          to_account: to,
          amount_units: amount,
          zone_id: zoneId,
          metadata: metadata || {},
        },
      });

      if (res?.status === "SPOOLED") {
        toast("Transfer spooled", `zone blocked; queued as ${String(res.spool_id).slice(0, 8)}...`);
      } else {
        toast("Transfer applied", `${fmtUnits(amount)} ${from} -> ${to}`);
      }

      await Promise.all([loadBalances(), loadTxns(), loadAllIncidents(), loadZoneDrilldown(zoneId)]);
    } catch (e: any) {
      toast("Transfer failed", String(e?.message || e));
      throw e;
    }
  }

  async function viewTxn(id: string) {
    try {
      const res = await api<TxnDetail>(`/v1/transactions/${id}`);
      setModal({ title: `Transaction ${id}`, body: res });
    } catch (e: any) {
      toast("Failed to load transaction", String(e?.message || e));
    }
  }

  async function viewIncident(id: string) {
    try {
      const res = await api<Incident>(`/v1/incidents/${id}`);
      setModal({ title: `Incident ${id}`, body: res });
    } catch (e: any) {
      toast("Failed to load incident", String(e?.message || e));
    }
  }

  async function incidentAction(action: "ACK" | "ASSIGN" | "RESOLVE") {
    if (!incidentManage) return;
    setBusy(true);
    try {
      await api(`/v1/incidents/${incidentManage.id}/action`, {
        method: "POST",
        headers: opsHeaders(),
        body: {
          action,
          assignee: incidentAssignee,
          note: incidentNote,
          reason,
        },
      });
      toast("Incident updated", `${incidentManage.id.slice(0, 8)}... ${action}`);
      setIncidentManage(null);
      setIncidentAssignee("");
      setIncidentNote("");
      await Promise.all([loadAllIncidents(), loadZoneDrilldown(selectedZoneId)]);
    } catch (e: any) {
      toast("Incident action failed", String(e?.message || e));
    } finally {
      setBusy(false);
    }
  }

  async function exportSnapshot() {
    if (!adminKey) {
      toast("Admin key required", "Set Admin Key to export snapshot.");
      return;
    }
    try {
      const snap = await api<any>(`/v1/sim/snapshot`, { method: "POST", headers: { "X-Admin-Key": adminKey } });
      const blob = new Blob([JSON.stringify(snap, null, 2)], { type: "application/json" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `tlsim-snapshot-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
      a.click();
      URL.revokeObjectURL(a.href);
      toast("Snapshot exported");
    } catch (e: any) {
      toast("Snapshot export failed", String(e?.message || e));
    }
  }

  async function importSnapshot(file: File) {
    if (!adminKey) {
      toast("Admin key required", "Set Admin Key to restore snapshot.");
      return;
    }
    try {
      const txt = await file.text();
      const snap = JSON.parse(txt);
      await api(`/v1/sim/restore`, { method: "POST", headers: { "X-Admin-Key": adminKey }, body: snap });
      toast("Snapshot restored");
      await refreshAll();
    } catch (e: any) {
      toast("Snapshot restore failed", String(e?.message || e));
    }
  }

  const zoneIncidentCounts = useMemo(() => {
    const map = new Map<string, { info: number; warn: number; crit: number }>();
    for (const z of ZONES) map.set(z.id, { info: 0, warn: 0, crit: 0 });
    for (const inc of allIncidents) {
      const c = map.get(inc.zone_id) || { info: 0, warn: 0, crit: 0 };
      if (inc.severity === "CRITICAL") c.crit += 1;
      else if (inc.severity === "WARN") c.warn += 1;
      else c.info += 1;
      map.set(inc.zone_id, c);
    }
    return map;
  }, [allIncidents]);

  const highlightIds = useMemo(() => {
    const st = selectedZone?.status || "OK";
    const c = controls;
    const isContained = c?.writes_blocked || (c?.cross_zone_throttle ?? 100) === 0;
    if (st === "DOWN" || st === "DEGRADED" || isContained) {
      return blastRadius(selectedZoneId);
    }
    return [];
  }, [selectedZoneId, selectedZone?.status, controls]);

  return (
    <div>
      <div className="header">
        <div className="container" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
          <div className="hgroup">
            <h1>Time Ledger Operator Console</h1>
            <div className="sub">Ledger + zones + incidents | idempotent | outbox/inbox | blast radius</div>
          </div>

          <div className="toolbar">
            <div className="pill">
              <span className="small">API</span>
              <input
                type="text"
                value={apiBase}
                onChange={(e) => setApiBaseState(e.target.value)}
                placeholder="(empty = dev proxy)"
                aria-label="API base URL"
              />
              <button className="btn" onClick={() => { setApiBase(apiBase); toast("API base saved", apiBase || "(proxy)"); }}>
                Save
              </button>
            </div>

<div className="pill" title="Backend build info">
  <span className="small">Backend</span>
  <span className="mono">
    {apiVersion ? `${apiVersion.language}@${apiVersion.version}${apiVersion.revision ? " (" + apiVersion.revision + ")" : ""}` : "unknown"}
  </span>
</div>

            <button className="btn primary" disabled={busy} onClick={() => refreshAll().catch(() => {})}>Refresh</button>

            {lastUpdated && connStatus === "connected" && (
              <span className="small" style={{ whiteSpace: "nowrap" }}>
                Updated {lastUpdated.toLocaleTimeString()}
              </span>
            )}

            <label className="pill" style={{ gap: 8 }}>
              <input type="checkbox" checked={autoTraffic} onChange={(e) => setAutoTraffic(e.target.checked)} />
              <span className="small">Auto traffic</span>
            </label>
          </div>
        </div>
      </div>

      <ConnBanner connStatus={connStatus} lastUpdated={lastUpdated} />

      <div className="container">
        <div className="grid">
          <div className="card">
            <div className="card-h">
              <h2>Zones</h2>
              <div className="kv">
                {selectedZone ? (
                  <span className={`badge ${clsStatus(selectedZone.status)}`}>
                    <span className="dot" />
                    {selectedZone.name} (#{zoneNumber(selectedZone.id)}) | {selectedZone.status}
                  </span>
                ) : <span className="small">Select a zone</span>}
              </div>
            </div>
            <div className="card-b">
              <ZoneMap zones={zones} selectedId={selectedZoneId} highlightIds={highlightIds} onSelect={setSelectedZoneId} />
              <div className="zoneGrid">
                {ZONES.map(meta => {
                  const z = zones.find(zz => zz.id === meta.id);
                  const selected = selectedZoneId === meta.id;
                  const st = z?.status || "DOWN";
                  const counts = zoneIncidentCounts.get(meta.id) || { info: 0, warn: 0, crit: 0 };
                  const inRadius = highlightIds.includes(meta.id);
                  return (
                    <div
                      key={meta.id}
                      className={`zoneTile ${selected ? "selected" : ""} ${inRadius ? "radius" : ""}`}
                      onClick={() => setSelectedZoneId(meta.id)}
                      role="button"
                      aria-label={`Select ${meta.id}`}
                    >
                      <div className="t">
                        <div className="name">{meta.label} <span className="small">#{meta.n}</span></div>
                        <span className={`badge ${clsStatus(st)}`}><span className="dot" />{st}</span>
                      </div>
                      <div className="meta">
                        <span className="small mono">{meta.id}</span>
                        <span className="small">Inc: {counts.crit}/{counts.warn}/{counts.info}</span>
                      </div>
                    </div>
                  );
                })}
              </div>

              <div className="formRow" style={{ marginTop: 12 }}>
                <input className="input" value={actor} onChange={(e) => setActor(e.target.value)} placeholder="actor" />
                <input className="input" value={reason} onChange={(e) => setReason(e.target.value)} placeholder="reason" />
                <button className="btn" disabled={busy || !selectedZone} onClick={() => setZoneStatus("OK")}>OK</button>
                <button className="btn" disabled={busy || !selectedZone} onClick={() => setZoneStatus("DEGRADED")}>DEGRADED</button>
                <button className="btn" disabled={busy || !selectedZone} onClick={() => setZoneStatus("DOWN")}>DOWN</button>
              </div>

              <div className="formRow">
                <input className="input" value={adminKey} onChange={(e) => setAdminKey(e.target.value)} placeholder="Admin key (snapshots)" />
                <button className="btn" onClick={() => exportSnapshot()}>Export snapshot</button>
                <label className="btn" style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                  Import snapshot
                  <input
                    type="file"
                    accept="application/json"
                    style={{ display: "none" }}
                    onChange={(e) => {
                      const f = e.target.files?.[0];
                      if (f) importSnapshot(f);
                      e.currentTarget.value = "";
                    }}
                  />
                </label>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="card-h">
              <h2>Ops & incidents</h2>
              <div className="kv">
                {selectedZone ? <span className="small">Updated {fmtRfc3339(selectedZone.updated_at)}</span> : null}
              </div>
            </div>
            <div className="card-b">
              <div className="grid2">
                <div className="card inner">
                  <div className="card-h">
                    <h2>Controls</h2>
                    <div className="kv">
                      {controls ? <span className="small mono">spool {String(controls.spool_enabled)} | throttle {controls.cross_zone_throttle}%</span> : <span className="small">...</span>}
                    </div>
                  </div>
                  <div className="card-b">
                    <div className="small" style={{ marginBottom: 8 }}>
                      Use these toggles to contain blast radius during outages. The map highlights dependent zones.
                    </div>

                    <div className="formRow" style={{ justifyContent: "space-between" }}>
                      <label className="pill" style={{ gap: 8 }}>
                        <input
                          type="checkbox"
                          checked={!!controls?.writes_blocked}
                          onChange={(e) => saveControls({ writes_blocked: e.target.checked })}
                        />
                        <span className="small">Writes blocked</span>
                      </label>

                      <label className="pill" style={{ gap: 8 }}>
                        <input
                          type="checkbox"
                          checked={!!controls?.spool_enabled}
                          onChange={(e) => saveControls({ spool_enabled: e.target.checked })}
                        />
                        <span className="small">Spool enabled</span>
                      </label>
                    </div>

                    <div className="formRow" style={{ alignItems: "center" }}>
                      <div className="small" style={{ minWidth: 120 }}>Cross-zone throttle</div>
                      <input
                        className="input"
                        type="range"
                        min={0}
                        max={100}
                        value={controls?.cross_zone_throttle ?? 100}
                        onChange={(e) => saveControls({ cross_zone_throttle: Number(e.target.value) })}
                      />
                      <div className="small mono" style={{ width: 52, textAlign: "right" }}>{controls?.cross_zone_throttle ?? 100}%</div>
                    </div>

                    <div className="formRow" style={{ justifyContent: "space-between" }}>
                      <button className="btn" disabled={busy || !selectedZone} onClick={() => applyRecommendedControls()}>
                        Apply playbook
                      </button>
                      <div className="small">Status-based defaults ({selectedZone?.status || "OK"})</div>
                    </div>
                  </div>
                </div>

                <div className="card inner">
                  <div className="card-h">
                    <h2>Spool</h2>
                    <div className="kv">
                      {spool ? <span className="small">pending {spool.pending} | applied {spool.applied} | failed {spool.failed}</span> : <span className="small">...</span>}
                    </div>
                  </div>
                  <div className="card-b">
                    <div className="small" style={{ marginBottom: 8 }}>
                      When a zone is DOWN/contained, writes can be queued and replayed later (bypassing gating but keeping idempotency).
                    </div>
                    <div className="formRow">
                      <input
                        className="input"
                        type="number"
                        value={replayLimit}
                        min={1}
                        max={500}
                        onChange={(e) => setReplayLimit(Number(e.target.value))}
                        placeholder="limit"
                      />
                      <button className="btn primary" disabled={busy || !spool || spool.pending <= 0} onClick={() => replaySpool()}>
                        Replay
                      </button>
                      <button className="btn" disabled={busy} onClick={() => loadZoneDrilldown(selectedZoneId)}>
                        Refresh
                      </button>
                    </div>
                    {spool && spool.pending > 0 && (selectedZone?.status === "DOWN" || controls?.writes_blocked || (controls?.cross_zone_throttle ?? 100) === 0) ? (
                      <div className="small" style={{ marginTop: 8 }}>
                        (!) Zone still contained. Replay will fail until the zone is OK and unblocked.
                      </div>
                    ) : null}
                  </div>
                </div>
              </div>

              <IncidentsTable
                incidents={incidents}
                zoneName={selectedZone?.name || ""}
                onView={viewIncident}
                onManage={(i) => { setIncidentManage(i); setIncidentAssignee(String(i.details?.assignee || "")); setIncidentNote(""); }}
              />

              <TransferForm
                from={transferFrom}
                to={transferTo}
                amount={transferAmount}
                busy={busy}
                onFromChange={setTransferFrom}
                onToChange={setTransferTo}
                onAmountChange={setTransferAmount}
                onSend={() => createTransfer(transferFrom, transferTo, transferAmount, selectedZoneId, { mode: "manual" })}
              />

              <AuditTable audit={audit} />

            </div>
          </div>
        </div>

        <div className="grid">
          <BalancesTable balances={balances} />
          <TransactionsTable txns={txns} onView={viewTxn} />
        </div>

        <div className="small" style={{ marginTop: 14 }}>
          Hosting note: GitHub Pages can host the UI only. Run the backend elsewhere (Docker/VPS/K8s) and set CORS_ALLOW_ORIGINS to your Pages URL.
        </div>
      </div>

      {modal ? (
        <Modal title={modal.title} onClose={() => setModal(null)}>
          <pre className="mono" style={{ whiteSpace: "pre-wrap", margin: 0 }}>
            {JSON.stringify(modal.body, null, 2)}
          </pre>
        </Modal>
      ) : null}

      {incidentManage ? (
        <Modal title={`Manage incident ${incidentManage.id.slice(0, 8)}...`} onClose={() => setIncidentManage(null)}>
          <div className="small" style={{ marginBottom: 10 }}>
            {incidentManage.title} | <span className="mono">{incidentManage.status}</span> | <span className="mono">{incidentManage.zone_id}</span>
          </div>

          <div className="formRow" style={{ marginBottom: 10 }}>
            <input className="input" value={incidentAssignee} onChange={(e) => setIncidentAssignee(e.target.value)} placeholder="assignee (optional)" />
            <input className="input" value={incidentNote} onChange={(e) => setIncidentNote(e.target.value)} placeholder="note (optional)" />
          </div>

          <div className="formRow" style={{ justifyContent: "flex-end" }}>
            <button className="btn" disabled={busy} onClick={() => incidentAction("ACK")}>ACK</button>
            <button className="btn" disabled={busy} onClick={() => incidentAction("ASSIGN")}>Assign</button>
            <button className="btn primary" disabled={busy} onClick={() => incidentAction("RESOLVE")}>Resolve</button>
          </div>

          <div className="small" style={{ marginTop: 10 }}>
            Incident actions are audited. This is a sim, so "assignment" just writes metadata.
          </div>
        </Modal>
      ) : null}

      <ToastHost items={toasts} onRemove={(id) => setToasts(t => t.filter(x => x.id !== id))} />
    </div>
  );
}
