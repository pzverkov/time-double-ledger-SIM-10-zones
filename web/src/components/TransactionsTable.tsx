import { Txn } from "../types";
import { fmtRfc3339, fmtUnits } from "../lib/time";

export function TransactionsTable({ txns, onView }: { txns: Txn[]; onView: (id: string) => void }) {
  return (
    <div className="card">
      <div className="card-h"><h2>Recent transactions</h2><div className="kv"><span className="small">{txns.length} rows</span></div></div>
      <div className="card-b">
        <table className="table">
          <thead><tr><th>Time</th><th className="mono">Txn</th><th>From {"->"} To</th><th>Amount</th><th>Zone</th><th></th></tr></thead>
          <tbody>
            {txns.map(t => (
              <tr key={t.id}>
                <td className="small">{fmtRfc3339(t.created_at)}</td>
                <td className="mono">{t.id.slice(0, 8)}</td>
                <td className="small mono">{t.from_account} {"->"} {t.to_account}</td>
                <td>{fmtUnits(t.amount_units)}</td>
                <td className="small mono">{t.zone_id}</td>
                <td style={{ textAlign: "right" }}><button className="btn" onClick={() => onView(t.id)}>View</button></td>
              </tr>
            ))}
            {txns.length === 0 ? <tr><td colSpan={6} className="small">No transactions yet.</td></tr> : null}
          </tbody>
        </table>
      </div>
    </div>
  );
}
