import { useEffect, useMemo, useRef, useState } from "react";
import ZoneMap from "./components/ZoneMap";
import { Modal } from "./components/Modal";
import { ToastHost, ToastMsg } from "./components/Toast";
import { api, getApiBase, setApiBase } from "./lib/api";
import { fmtRfc3339, fmtUnits } from "./lib/time";
import { blastRadius, recommendedControlsFor } from "./lib/risk";
import { ZONES, zoneNumber } from "./zones";

type Zone = { id: string; name: string; status: "OK" | "DEGRADED" | "DOWN"; updated_at: string };

type ZoneControls = {
  zone_id: string;
  writes_blocked: boolean;
  cross_zone_throttle: number;
  spool_enabled: boolean;
  updated_at: string;
};

type SpoolStats = { zone_id: string; pending: number; applied: number; failed: number };

type AuditEntry = {
  id: string;
  actor: string;
  action: string;
  target_type: string;
  target_id: string;
  reason?: string | null;
  details: any;
  created_at: string;
};

type Incident = {
  id: string;
  zone_id: string;
  related_txn_id?: string | null;
  severity: "INFO" | "WARN" | "CRITICAL";
  status: "OPEN" | "ACK" | "RESOLVED" | string;
  title: string;
  details: any;
  detected_at: string;
};

type Balance = { account_id: string; balance_units: number; updated_at: string };

type Txn = {
  id: string;
  request_id: string;
  from_account: string;
  to_account: string;
  amount_units: number;
  zone_id: string;
  created_at: string;
};

type TxnDetail = Txn & { metadata: any; postings: { account_id: string; direction: string; amount_units: number }[] };

function clsStatus(s: string) {
  const t = (s || "").toLowerCase();
  if (t === "ok") return "ok";
  if (t === "degraded") return "degraded";
  return "down";
}

function uuidv4() {
  // good-enough for a sim; browsers also have crypto.randomUUID()
  // eslint-disable-next-line
  // @ts-ignore
  return (crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`);
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

  const [busy, setBusy] = useState(false);
  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const [modal, setModal] = useState<{ title: string; body: any } | null>(null);

  const [actor, setActor] = useState("operator-1");
  const [reason, setReason] = useState("sim action");
  const [adminKey, setAdminKey] = useState("");

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

  async function refreshAll() {
    await Promise.all([
        loadVersion(),
        loadZones(),
      loadBalances(),
      loadTxns(),
      loadAllIncidents(),
      loadZoneDrilldown(selectedZoneId),
    ]);
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
    (async () => {
      try {
        setBusy(true);
        await refreshAll();
      } catch (e: any) {
        toast("Failed to load", String(e?.message || e));
      } finally {
        setBusy(false);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!selectedZoneId) return;
    loadZoneDrilldown(selectedZoneId).catch((e: any) => toast("Zone drilldown failed", String(e?.message || e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedZoneId]);

  useEffect(() => {
    if (!autoTraffic) {
      if (autoRef.current) window.clearInterval(autoRef.current);
      autoRef.current = null;
      return;
    }
    autoRef.current = window.setInterval(() => {
      const z = selectedZoneId;
      const from = `acct-${String.fromCharCode(65 + Math.floor(Math.random() * 6))}`;
      const to = `acct-${String.fromCharCode(65 + Math.floor(Math.random() * 6))}`;
      if (from === to) return;
      const amt = [30, 60, 120, 300, 600, 1200, 3600][Math.floor(Math.random() * 7)];
      createTransfer(from, to, amt, z, { mode: "auto" }).catch(() => {});
    }, 1300);
    return () => { if (autoRef.current) window.clearInterval(autoRef.current); autoRef.current = null; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoTraffic, selectedZoneId]);

  async function setZoneStatus(status: "OK" | "DEGRADED" | "DOWN") {
    if (!selectedZone) return;
    setBusy(true);
    try {
      await api(`/v1/zones/${selectedZone.id}/status`, { method: "POST", body: { status, actor, reason } });
      toast("Zone status updated", `${selectedZone.name} → ${status}`);
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
      actor,
      reason,
    };

    setBusy(true);
    try {
      await api(`/v1/zones/${selectedZone.id}/controls`, { method: "POST", body: payload });
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
        body: { limit: replayLimit, actor, reason },
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
        toast("Transfer spooled", `zone blocked; queued as ${String(res.spool_id).slice(0, 8)}…`);
      } else {
        toast("Transfer applied", `${fmtUnits(amount)} ${from} → ${to}`);
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
        body: {
          action,
          assignee: incidentAssignee,
          note: incidentNote,
          actor,
          reason,
        },
      });
      toast("Incident updated", `${incidentManage.id.slice(0, 8)}… ${action}`);
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
            <div className="sub">Ledger + zones + incidents • idempotent • outbox/inbox • blast radius</div>
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

            <label className="pill" style={{ gap: 8 }}>
              <input type="checkbox" checked={autoTraffic} onChange={(e) => setAutoTraffic(e.target.checked)} />
              <span className="small">Auto traffic</span>
            </label>
          </div>
        </div>
      </div>

      <div className="container">
        <div className="grid">
          <div className="card">
            <div className="card-h">
              <h2>Zones</h2>
              <div className="kv">
                {selectedZone ? (
                  <span className={`badge ${clsStatus(selectedZone.status)}`}>
                    <span className="dot" />
                    {selectedZone.name} (#{zoneNumber(selectedZone.id)}) • {selectedZone.status}
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
                      {controls ? <span className="small mono">spool {String(controls.spool_enabled)} • throttle {controls.cross_zone_throttle}%</span> : <span className="small">…</span>}
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
                      {spool ? <span className="small">pending {spool.pending} • applied {spool.applied} • failed {spool.failed}</span> : <span className="small">…</span>}
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
                        ⚠ Zone still contained. Replay will fail until the zone is OK and unblocked.
                      </div>
                    ) : null}
                  </div>
                </div>
              </div>

              <div className="card inner" style={{ marginTop: 12 }}>
                <div className="card-h">
                  <h2>Incidents in {selectedZone?.name || ""}</h2>
                  <div className="kv"><span className="small">{incidents.length} rows</span></div>
                </div>
                <div className="card-b">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>Time</th>
                        <th>Sev</th>
                        <th>Status</th>
                        <th>Title</th>
                        <th className="mono">Txn</th>
                        <th></th>
                      </tr>
                    </thead>
                    <tbody>
                      {incidents.map((i) => (
                        <tr key={i.id}>
                          <td className="small">{fmtRfc3339(i.detected_at)}</td>
                          <td>
                            <span className={`badge ${i.severity === "CRITICAL" ? "down" : i.severity === "WARN" ? "degraded" : "ok"}`}>
                              <span className="dot" />{i.severity}
                            </span>
                          </td>
                          <td className="small mono">{i.status}</td>
                          <td>{i.title}</td>
                          <td className="mono small">{i.related_txn_id ? i.related_txn_id.slice(0, 8) : ""}</td>
                          <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                            <button className="btn" onClick={() => viewIncident(i.id)}>View</button>
                            <button className="btn" onClick={() => { setIncidentManage(i); setIncidentAssignee(String(i.details?.assignee || "")); setIncidentNote(""); }}>Manage</button>
                          </td>
                        </tr>
                      ))}
                      {incidents.length === 0 ? (
                        <tr><td colSpan={6} className="small">No incidents in this zone.</td></tr>
                      ) : null}
                    </tbody>
                  </table>
                </div>
              </div>

              <div className="card inner" style={{ marginTop: 12 }}>
                <div className="card-h">
                  <h2>Transfer generator</h2>
                  <div className="kv"><span className="small">Unit is seconds</span></div>
                </div>
                <div className="card-b">
                  <div className="formRow">
                    <input className="input" value={transferFrom} onChange={(e) => setTransferFrom(e.target.value)} placeholder="from_account" />
                    <input className="input" value={transferTo} onChange={(e) => setTransferTo(e.target.value)} placeholder="to_account" />
                    <input className="input" type="number" value={transferAmount} onChange={(e) => setTransferAmount(Number(e.target.value))} placeholder="amount (seconds)" />
                    <button
                      className="btn primary"
                      disabled={busy || !transferFrom || !transferTo || transferAmount <= 0}
                      onClick={() => createTransfer(transferFrom, transferTo, transferAmount, selectedZoneId, { mode: "manual" })}
                    >
                      Send
                    </button>
                  </div>
                  <div className="small">For higher resolution later, switch to ms without changing ledger semantics.</div>
                </div>
              </div>

              <div className="card inner" style={{ marginTop: 12 }}>
                <div className="card-h"><h2>Audit trail (zone)</h2><div className="kv"><span className="small">{audit.length} rows</span></div></div>
                <div className="card-b">
                  <table className="table">
                    <thead><tr><th>Time</th><th>Actor</th><th>Action</th><th>Reason</th></tr></thead>
                    <tbody>
                      {audit.map(a => (
                        <tr key={a.id}>
                          <td className="small">{fmtRfc3339(a.created_at)}</td>
                          <td className="small mono">{a.actor}</td>
                          <td className="small">{a.action}</td>
                          <td className="small">{a.reason || ""}</td>
                        </tr>
                      ))}
                      {audit.length === 0 ? <tr><td colSpan={4} className="small">No audit entries yet.</td></tr> : null}
                    </tbody>
                  </table>
                </div>
              </div>

            </div>
          </div>
        </div>

        <div className="grid">
          <div className="card">
            <div className="card-h"><h2>Balances</h2><div className="kv"><span className="small">{balances.length} accounts</span></div></div>
            <div className="card-b">
              <table className="table">
                <thead><tr><th className="mono">Account</th><th>Balance</th><th>Updated</th></tr></thead>
                <tbody>
                  {balances.map(b => (
                    <tr key={b.account_id}>
                      <td className="mono">{b.account_id}</td>
                      <td>{fmtUnits(b.balance_units)}</td>
                      <td className="small">{fmtRfc3339(b.updated_at)}</td>
                    </tr>
                  ))}
                  {balances.length === 0 ? <tr><td colSpan={3} className="small">No balances yet. Create a transfer.</td></tr> : null}
                </tbody>
              </table>
            </div>
          </div>

          <div className="card">
            <div className="card-h"><h2>Recent transactions</h2><div className="kv"><span className="small">{txns.length} rows</span></div></div>
            <div className="card-b">
              <table className="table">
                <thead><tr><th>Time</th><th className="mono">Txn</th><th>From → To</th><th>Amount</th><th>Zone</th><th></th></tr></thead>
                <tbody>
                  {txns.map(t => (
                    <tr key={t.id}>
                      <td className="small">{fmtRfc3339(t.created_at)}</td>
                      <td className="mono">{t.id.slice(0, 8)}</td>
                      <td className="small mono">{t.from_account} → {t.to_account}</td>
                      <td>{fmtUnits(t.amount_units)}</td>
                      <td className="small mono">{t.zone_id}</td>
                      <td style={{ textAlign: "right" }}><button className="btn" onClick={() => viewTxn(t.id)}>View</button></td>
                    </tr>
                  ))}
                  {txns.length === 0 ? <tr><td colSpan={6} className="small">No transactions yet.</td></tr> : null}
                </tbody>
              </table>
            </div>
          </div>
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
        <Modal title={`Manage incident ${incidentManage.id.slice(0, 8)}…`} onClose={() => setIncidentManage(null)}>
          <div className="small" style={{ marginBottom: 10 }}>
            {incidentManage.title} • <span className="mono">{incidentManage.status}</span> • <span className="mono">{incidentManage.zone_id}</span>
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
            Incident actions are audited. This is a sim, so “assignment” just writes metadata.
          </div>
        </Modal>
      ) : null}

      <ToastHost items={toasts} onRemove={(id) => setToasts(t => t.filter(x => x.id !== id))} />
    </div>
  );
}
