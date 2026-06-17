type Props = {
  from: string;
  to: string;
  amount: number;
  busy: boolean;
  onFromChange: (v: string) => void;
  onToChange: (v: string) => void;
  onAmountChange: (v: number) => void;
  onSend: () => void;
};

export function TransferForm({ from, to, amount, busy, onFromChange, onToChange, onAmountChange, onSend }: Props) {
  return (
    <div className="card inner" style={{ marginTop: 12 }}>
      <div className="card-h">
        <h2>Transfer generator</h2>
        <div className="kv"><span className="small">Unit is seconds</span></div>
      </div>
      <div className="card-b">
        <div className="formRow">
          <input className="input" value={from} onChange={(e) => onFromChange(e.target.value)} placeholder="from_account" />
          <input className="input" value={to} onChange={(e) => onToChange(e.target.value)} placeholder="to_account" />
          <input className="input" type="number" value={amount} onChange={(e) => onAmountChange(Number(e.target.value))} placeholder="amount (seconds)" />
          <button
            className="btn primary"
            disabled={busy || !from || !to || amount <= 0}
            onClick={onSend}
          >
            Send
          </button>
        </div>
        <div className="small">For higher resolution later, switch to ms without changing ledger semantics.</div>
      </div>
    </div>
  );
}
