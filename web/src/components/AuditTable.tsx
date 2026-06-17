import { AuditEntry } from "../types";
import { fmtRfc3339 } from "../lib/time";

export function AuditTable({ audit }: { audit: AuditEntry[] }) {
  return (
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
  );
}
