import { Incident } from "../types";
import { fmtRfc3339 } from "../lib/time";

type Props = {
  incidents: Incident[];
  zoneName: string;
  onView: (id: string) => void;
  onManage: (incident: Incident) => void;
};

export function IncidentsTable({ incidents, zoneName, onView, onManage }: Props) {
  return (
    <div className="card inner" style={{ marginTop: 12 }}>
      <div className="card-h">
        <h2>Incidents in {zoneName}</h2>
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
                  <button className="btn" onClick={() => onView(i.id)}>View</button>
                  <button className="btn" onClick={() => onManage(i)}>Manage</button>
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
  );
}
