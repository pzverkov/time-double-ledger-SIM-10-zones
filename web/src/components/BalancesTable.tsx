import { Balance } from "../types";
import { fmtRfc3339, fmtUnits } from "../lib/time";

export function BalancesTable({ balances }: { balances: Balance[] }) {
  return (
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
  );
}
