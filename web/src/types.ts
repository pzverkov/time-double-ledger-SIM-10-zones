// Shared API/domain types used by App and the panel components.

export type Zone = { id: string; name: string; status: "OK" | "DEGRADED" | "DOWN"; updated_at: string };

export type ZoneControls = {
  zone_id: string;
  writes_blocked: boolean;
  cross_zone_throttle: number;
  spool_enabled: boolean;
  updated_at: string;
};

export type SpoolStats = { zone_id: string; pending: number; applied: number; failed: number };

export type AuditEntry = {
  id: string;
  actor: string;
  action: string;
  target_type: string;
  target_id: string;
  reason?: string | null;
  details: any;
  created_at: string;
};

export type Incident = {
  id: string;
  zone_id: string;
  related_txn_id?: string | null;
  severity: "INFO" | "WARN" | "CRITICAL";
  status: "OPEN" | "ACK" | "RESOLVED" | string;
  title: string;
  details: any;
  detected_at: string;
};

export type Balance = { account_id: string; balance_units: number; updated_at: string };

export type Txn = {
  id: string;
  request_id: string;
  from_account: string;
  to_account: string;
  amount_units: number;
  zone_id: string;
  created_at: string;
};

export type TxnDetail = Txn & {
  metadata: any;
  postings: { account_id: string; direction: string; amount_units: number }[];
};
