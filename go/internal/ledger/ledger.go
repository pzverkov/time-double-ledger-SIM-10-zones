package ledger

import (
  "context"
  "encoding/json"
  "errors"
  "fmt"
  "hash/fnv"
  "time"

  "github.com/jackc/pgx/v5"
  "github.com/jackc/pgx/v5/pgxpool"
  "log/slog"
)

type Ledger struct {
  db *pgxpool.Pool
  log *slog.Logger
}

func New(db *pgxpool.Pool, log *slog.Logger) *Ledger {
  return &Ledger{db: db, log: log}
}

type Zone struct {
  ID string `json:"id"`
  Name string `json:"name"`
  Status string `json:"status"`
  UpdatedAt time.Time `json:"updated_at"`
}

type Transaction struct {
  ID string
  RequestID string
  CreatedAt time.Time
}

type CreateTransferInput struct {
  RequestID string
  PayloadHash string
  FromAccount string
  ToAccount string
  AmountUnits int64
  ZoneID string
  Metadata map[string]any
}

var (
  ErrIdempotencyConflict = errors.New("idempotency conflict")
  ErrZoneDown = errors.New("zone down")
  ErrZoneBlocked = errors.New("zone blocked")
)

func IsIdempotencyConflict(err error) bool { return errors.Is(err, ErrIdempotencyConflict) }
func IsZoneDown(err error) bool { return errors.Is(err, ErrZoneDown) }
func IsZoneBlocked(err error) bool { return errors.Is(err, ErrZoneBlocked) }

func (l *Ledger) ListZones(ctx context.Context) ([]Zone, error) {
  rows, err := l.db.Query(ctx, `SELECT id,name,status,updated_at FROM zones ORDER BY id`)
  if err != nil { return nil, err }
  defer rows.Close()
  out := []Zone{}
  for rows.Next() {
    var z Zone
    if err := rows.Scan(&z.ID, &z.Name, &z.Status, &z.UpdatedAt); err != nil { return nil, err }
    out = append(out, z)
  }
  return out, rows.Err()
}

func (l *Ledger) getZoneStatusTx(ctx context.Context, tx pgx.Tx, zoneID string) (string, error) {
  var status string
  err := tx.QueryRow(ctx, `SELECT status FROM zones WHERE id=$1`, zoneID).Scan(&status)
  if err != nil { return "", err }
  return status, nil
}

func (l *Ledger) ensureAccount(ctx context.Context, tx pgx.Tx, accountID, zoneID string) error {
  // Insert if missing
  _, err := tx.Exec(ctx, `INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT (id) DO NOTHING`, accountID, zoneID)
  return err
}

func (l *Ledger) CreateTransfer(ctx context.Context, in CreateTransferInput) (*Transaction, *string, error) {
  // serialize metadata
  metaBytes, err := json.Marshal(in.Metadata)
  if err != nil { return nil, nil, err }

  tx, err := l.db.BeginTx(ctx, pgx.TxOptions{})
  if err != nil { return nil, nil, err }
  defer func() { _ = tx.Rollback(ctx) }()

  // zone gate + controls
  status, err := l.getZoneStatusTx(ctx, tx, in.ZoneID)
  if err != nil { return nil, nil, err }

  controls, err := l.getZoneControlsTx(ctx, tx, in.ZoneID)
  if err != nil { return nil, nil, err }

  blockedReason := ""
  if status == "DOWN" {
    blockedReason = "zone down"
  } else if controls.WritesBlocked {
    blockedReason = "writes blocked"
  } else {
    // deterministic throttle (good for demos + reproducibility)
    thr := controls.CrossZoneThrottle
    if thr < 100 {
      if thr <= 0 {
        blockedReason = "throttled"
      } else {
        if l.hashPercent(in.RequestID) >= thr {
          blockedReason = "throttled"
        }
      }
    }
  }

  // idempotency check (applies to both applied and spooled cases)
  var existingID string
  var existingHash string
  var createdAt time.Time
  err = tx.QueryRow(ctx, `SELECT id::text,payload_hash,created_at FROM transactions WHERE request_id=$1`, in.RequestID).
    Scan(&existingID, &existingHash, &createdAt)
  if err == nil {
    if existingHash != in.PayloadHash {
      return nil, nil, ErrIdempotencyConflict
    }
    _ = tx.Commit(ctx)
    return &Transaction{ID: existingID, RequestID: in.RequestID, CreatedAt: createdAt}, nil, nil
  }
  if err != nil && !errors.Is(err, pgx.ErrNoRows) {
    return nil, nil, err
  }

  // idempotency check for previously spooled transfer
  var existingSpoolID string
  var existingSpoolHash string
  err = tx.QueryRow(ctx, `SELECT id::text,payload_hash FROM spooled_transfers WHERE request_id=$1`, in.RequestID).
    Scan(&existingSpoolID, &existingSpoolHash)
  if err == nil {
    if existingSpoolHash != in.PayloadHash {
      return nil, nil, ErrIdempotencyConflict
    }
    _ = tx.Commit(ctx)
    return nil, &existingSpoolID, nil
  }
  if err != nil && !errors.Is(err, pgx.ErrNoRows) {
    return nil, nil, err
  }

  // blocked? -> spool if enabled
  if blockedReason != "" {
    if controls.SpoolEnabled {
      spoolID, err := l.spoolTransferTx(ctx, tx, in, metaBytes, blockedReason)
      if err != nil { return nil, nil, err }
      if err := tx.Commit(ctx); err != nil { return nil, nil, err }
      return nil, &spoolID, nil
    }
    // no spooling
    if status == "DOWN" {
      return nil, nil, ErrZoneDown
    }
    return nil, nil, ErrZoneBlocked
  }

  // ensure accounts exist (simulation simplification: all accounts live in initiating zone)
  if err := l.ensureAccount(ctx, tx, in.FromAccount, in.ZoneID); err != nil { return nil, nil, err }
  if err := l.ensureAccount(ctx, tx, in.ToAccount, in.ZoneID); err != nil { return nil, nil, err }

  txnID, createdAt, err := l.applyTransferTx(ctx, tx, in, metaBytes)
  if err != nil { return nil, nil, err }

  if err := tx.Commit(ctx); err != nil { return nil, nil, err }
  return &Transaction{ID: txnID, RequestID: in.RequestID, CreatedAt: createdAt}, nil, nil
}

func (l *Ledger) SetZoneStatus(ctx context.Context, zoneID, status, actor, reason string) (*Zone, error) {
  if status != "OK" && status != "DEGRADED" && status != "DOWN" {
    return nil, fmt.Errorf("invalid status")
  }
  tx, err := l.db.BeginTx(ctx, pgx.TxOptions{})
  if err != nil { return nil, err }
  defer func(){ _ = tx.Rollback(ctx) }()

  var z Zone
  err = tx.QueryRow(ctx, `
    UPDATE zones SET status=$2, updated_at=now() WHERE id=$1
    RETURNING id,name,status,updated_at
  `, zoneID, status).Scan(&z.ID, &z.Name, &z.Status, &z.UpdatedAt)
  if err != nil { return nil, err }

  _, err = tx.Exec(ctx, `
    INSERT INTO audit_log(actor,action,target_type,target_id,reason,details)
    VALUES($1,'SET_ZONE_STATUS','zone',$2,$3, jsonb_build_object('status',$4))
  `, actor, zoneID, reason, status)
  if err != nil { return nil, err }

  if status == "DOWN" {
    _, _ = tx.Exec(ctx, `
      INSERT INTO incidents(zone_id,severity,title,details)
      VALUES($1,'CRITICAL','Zone marked DOWN', jsonb_build_object('reason',$2,'actor',$3))
    `, zoneID, reason, actor)
  }

  if err := tx.Commit(ctx); err != nil { return nil, err }
  return &z, nil
}

type Incident struct {
  ID string `json:"id"`
  ZoneID string `json:"zone_id"`
  RelatedTxnID *string `json:"related_txn_id"`
  Severity string `json:"severity"`
  Status string `json:"status"`
  Title string `json:"title"`
  Details map[string]any `json:"details"`
  DetectedAt time.Time `json:"detected_at"`
}


func (l *Ledger) ListRecentIncidents(ctx context.Context, limit int) ([]Incident, error) {
  if limit <= 0 || limit > 2000 { limit = 500 }
  rows, err := l.db.Query(ctx, `
    SELECT id::text, zone_id, related_txn_id::text, severity, status, title, details, detected_at
    FROM incidents
    ORDER BY detected_at DESC
    LIMIT $1
  `, limit)
  if err != nil { return nil, err }
  defer rows.Close()

  out := []Incident{}
  for rows.Next() {
    var inc Incident
    var related *string
    var detailsBytes []byte
    if err := rows.Scan(&inc.ID, &inc.ZoneID, &related, &inc.Severity, &inc.Status, &inc.Title, &detailsBytes, &inc.DetectedAt); err != nil { return nil, err }
    inc.RelatedTxnID = related
    _ = json.Unmarshal(detailsBytes, &inc.Details)
    out = append(out, inc)
  }
  return out, rows.Err()
}

func (l *Ledger) ListIncidentsByZone(ctx context.Context, zoneID string) ([]Incident, error) {
  rows, err := l.db.Query(ctx, `
    SELECT id::text, zone_id, related_txn_id::text, severity, status, title, details, detected_at
    FROM incidents WHERE zone_id=$1 ORDER BY detected_at DESC LIMIT 200
  `, zoneID)
  if err != nil { return nil, err }
  defer rows.Close()

  out := []Incident{}
  for rows.Next() {
    var inc Incident
    var related *string
    var detailsBytes []byte
    if err := rows.Scan(&inc.ID, &inc.ZoneID, &related, &inc.Severity, &inc.Status, &inc.Title, &detailsBytes, &inc.DetectedAt); err != nil { return nil, err }
    inc.RelatedTxnID = related
    _ = json.Unmarshal(detailsBytes, &inc.Details)
    out = append(out, inc)
  }
  return out, rows.Err()
}

func (l *Ledger) GetIncident(ctx context.Context, id string) (*Incident, error) {
  var inc Incident
  var related *string
  var detailsBytes []byte
  err := l.db.QueryRow(ctx, `
    SELECT id::text, zone_id, related_txn_id::text, severity, status, title, details, detected_at
    FROM incidents WHERE id=$1::uuid
  `, id).Scan(&inc.ID, &inc.ZoneID, &related, &inc.Severity, &inc.Status, &inc.Title, &detailsBytes, &inc.DetectedAt)
  if err != nil { return nil, err }
  inc.RelatedTxnID = related
  _ = json.Unmarshal(detailsBytes, &inc.Details)
  return &inc, nil
}

func (l *Ledger) Snapshot(ctx context.Context) (map[string]any, error) {
  snap := map[string]any{
    "version": "v2",
    "created_at": time.Now().UTC().Format(time.RFC3339Nano),
    "note": "Restore resets transaction history; balances/incidents/controls/spool/audit are restored.",
  }

  zones, err := l.ListZones(ctx)
  if err != nil { return nil, err }
  snap["zones"] = zones

  // zone controls
  rows, err := l.db.Query(ctx, `SELECT zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at FROM zone_controls ORDER BY zone_id`)
  if err != nil { return nil, err }
  defer rows.Close()
  ctrls := []map[string]any{}
  for rows.Next() {
    var zid string
    var wb bool
    var thr int
    var sp bool
    var ua time.Time
    if err := rows.Scan(&zid, &wb, &thr, &sp, &ua); err != nil { return nil, err }
    ctrls = append(ctrls, map[string]any{
      "zone_id": zid,
      "writes_blocked": wb,
      "cross_zone_throttle": thr,
      "spool_enabled": sp,
      "updated_at": ua.UTC().Format(time.RFC3339Nano),
    })
  }
  snap["zone_controls"] = ctrls

  // accounts + balances (joined)
  abRows, err := l.db.Query(ctx, `
    SELECT a.id, a.zone_id, COALESCE(b.balance_units,0) as balance_units
    FROM accounts a
    LEFT JOIN balances b ON b.account_id=a.id
    ORDER BY a.id
    LIMIT 20000
  `)
  if err != nil { return nil, err }
  defer abRows.Close()
  accts := []map[string]any{}
  for abRows.Next() {
    var id, zid string
    var bal int64
    if err := abRows.Scan(&id, &zid, &bal); err != nil { return nil, err }
    accts = append(accts, map[string]any{"id": id, "zone_id": zid, "balance_units": bal})
  }
  snap["accounts"] = accts

  // incidents
  incRows, err := l.db.Query(ctx, `
    SELECT id::text, zone_id, related_txn_id::text, severity, status, title, details, detected_at
    FROM incidents
    ORDER BY detected_at DESC
    LIMIT 5000
  `)
  if err != nil { return nil, err }
  defer incRows.Close()
  incs := []map[string]any{}
  for incRows.Next() {
    var id, zid, sev, st, title string
    var related *string
    var detailsBytes []byte
    var dt time.Time
    if err := incRows.Scan(&id, &zid, &related, &sev, &st, &title, &detailsBytes, &dt); err != nil { return nil, err }
    var d any
    _ = json.Unmarshal(detailsBytes, &d)
    m := map[string]any{
      "id": id,
      "zone_id": zid,
      "related_txn_id": related,
      "severity": sev,
      "status": st,
      "title": title,
      "details": d,
      "detected_at": dt.UTC().Format(time.RFC3339Nano),
    }
    incs = append(incs, m)
  }
  snap["incidents"] = incs

  // spool (cap)
  spRows, err := l.db.Query(ctx, `
    SELECT id::text, request_id, payload_hash, from_account, to_account, amount_units, zone_id, metadata, status, fail_reason, created_at, updated_at, applied_at
    FROM spooled_transfers
    ORDER BY created_at DESC
    LIMIT 5000
  `)
  if err != nil { return nil, err }
  defer spRows.Close()
  spools := []map[string]any{}
  for spRows.Next() {
    var id, req, ph, from, to, zid, st string
    var amt int64
    var meta []byte
    var fail *string
    var ca, ua time.Time
    var aa *time.Time
    if err := spRows.Scan(&id, &req, &ph, &from, &to, &amt, &zid, &meta, &st, &fail, &ca, &ua, &aa); err != nil { return nil, err }
    var m any
    _ = json.Unmarshal(meta, &m)
    item := map[string]any{
      "id": id,
      "request_id": req,
      "payload_hash": ph,
      "from_account": from,
      "to_account": to,
      "amount_units": amt,
      "zone_id": zid,
      "metadata": m,
      "status": st,
      "fail_reason": fail,
      "created_at": ca.UTC().Format(time.RFC3339Nano),
      "updated_at": ua.UTC().Format(time.RFC3339Nano),
      "applied_at": nil,
    }
    if aa != nil { item["applied_at"] = aa.UTC().Format(time.RFC3339Nano) }
    spools = append(spools, item)
  }
  snap["spooled_transfers"] = spools

  // audit tail
  aRows, err := l.db.Query(ctx, `
    SELECT id::text, actor, action, target_type, target_id, reason, details, created_at
    FROM audit_log
    ORDER BY created_at DESC
    LIMIT 2000
  `)
  if err != nil { return nil, err }
  defer aRows.Close()
  audits := []map[string]any{}
  for aRows.Next() {
    var id, actor, action, tt, tid string
    var reason *string
    var details []byte
    var ca time.Time
    if err := aRows.Scan(&id, &actor, &action, &tt, &tid, &reason, &details, &ca); err != nil { return nil, err }
    var d any
    _ = json.Unmarshal(details, &d)
    audits = append(audits, map[string]any{
      "id": id,
      "actor": actor,
      "action": action,
      "target_type": tt,
      "target_id": tid,
      "reason": reason,
      "details": d,
      "created_at": ca.UTC().Format(time.RFC3339Nano),
    })
  }
  snap["audit_log"] = audits

  return snap, nil
}

func (l *Ledger) Restore(ctx context.Context, snap map[string]any) error {
  tx, err := l.db.BeginTx(ctx, pgx.TxOptions{})
  if err != nil { return err }
  defer func(){ _ = tx.Rollback(ctx) }()

  // Hard reset mutable state for a consistent restore.
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE postings RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE transactions RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE balances RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE accounts RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE incidents RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE outbox_events RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE inbox_events RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE audit_log RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE spooled_transfers RESTART IDENTITY CASCADE`)
  _, _ = tx.Exec(ctx, `TRUNCATE TABLE zone_controls RESTART IDENTITY CASCADE`)

  // zones: update statuses only
  if zs, ok := snap["zones"].([]any); ok {
    for _, it := range zs {
      m, _ := it.(map[string]any)
      id, _ := m["id"].(string)
      status, _ := m["status"].(string)
      if id != "" && (status=="OK"||status=="DEGRADED"||status=="DOWN") {
        _, _ = tx.Exec(ctx, `UPDATE zones SET status=$2, updated_at=now() WHERE id=$1`, id, status)
      }
    }
  }

  // zone controls
  if cs, ok := snap["zone_controls"].([]any); ok {
    for _, it := range cs {
      m, _ := it.(map[string]any)
      zid, _ := m["zone_id"].(string)
      if zid == "" { continue }
      wb, _ := m["writes_blocked"].(bool)
      thrF, _ := m["cross_zone_throttle"].(float64)
      thr := int(thrF)
      sp, _ := m["spool_enabled"].(bool)
      _, _ = tx.Exec(ctx, `
        INSERT INTO zone_controls(zone_id,writes_blocked,cross_zone_throttle,spool_enabled,updated_at)
        VALUES($1,$2,$3,$4,now())
        ON CONFLICT (zone_id) DO UPDATE
          SET writes_blocked=EXCLUDED.writes_blocked,
              cross_zone_throttle=EXCLUDED.cross_zone_throttle,
              spool_enabled=EXCLUDED.spool_enabled,
              updated_at=now()
      `, zid, wb, thr, sp)
    }
  } else {
    // seed defaults if absent
    _, _ = tx.Exec(ctx, `INSERT INTO zone_controls(zone_id) SELECT id FROM zones ON CONFLICT DO NOTHING`)
  }

  // accounts + balances
  if acs, ok := snap["accounts"].([]any); ok {
    for _, it := range acs {
      m, _ := it.(map[string]any)
      id, _ := m["id"].(string)
      zid, _ := m["zone_id"].(string)
      if id == "" { continue }
      if zid == "" { zid = "zone-eu" }
      _, _ = tx.Exec(ctx, `INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING`, id, zid)

      balF, _ := m["balance_units"].(float64)
      bal := int64(balF)
      _, _ = tx.Exec(ctx, `INSERT INTO balances(account_id,balance_units,updated_at) VALUES($1,$2,now()) ON CONFLICT (account_id) DO UPDATE SET balance_units=EXCLUDED.balance_units, updated_at=now()`, id, bal)
    }
  }

  // incidents
  if ins, ok := snap["incidents"].([]any); ok {
    for _, it := range ins {
      m, _ := it.(map[string]any)
      zid, _ := m["zone_id"].(string)
      sev, _ := m["severity"].(string)
      st, _ := m["status"].(string)
      title, _ := m["title"].(string)
      relAny := m["related_txn_id"]
      var rel *string
      if relAny != nil {
        if rs, ok := relAny.(string); ok && rs != "" { rel = &rs }
      }
      details := m["details"]
      if zid=="" || title=="" { continue }
      if sev=="" { sev="INFO" }
      if st=="" { st="OPEN" }
      b, _ := json.Marshal(details)
      if rel != nil {
        _, _ = tx.Exec(ctx, `INSERT INTO incidents(zone_id,related_txn_id,severity,status,title,details) VALUES($1,$2::uuid,$3,$4,$5,$6::jsonb)`,
          zid, *rel, sev, st, title, string(b))
      } else {
        _, _ = tx.Exec(ctx, `INSERT INTO incidents(zone_id,severity,status,title,details) VALUES($1,$2,$3,$4,$5::jsonb)`,
          zid, sev, st, title, string(b))
      }
    }
  }

  // spooled transfers
  if sp, ok := snap["spooled_transfers"].([]any); ok {
    for _, it := range sp {
      m, _ := it.(map[string]any)
      req, _ := m["request_id"].(string)
      if req == "" { continue }
      ph, _ := m["payload_hash"].(string)
      from, _ := m["from_account"].(string)
      to, _ := m["to_account"].(string)
      zid, _ := m["zone_id"].(string)
      amtF, _ := m["amount_units"].(float64)
      amt := int64(amtF)
      st, _ := m["status"].(string)
      if st == "" { st = "PENDING" }
      failAny := m["fail_reason"]
      var fail *string
      if fs, ok := failAny.(string); ok && fs != "" { fail = &fs }
      meta := m["metadata"]
      mb, _ := json.Marshal(meta)

      if fail != nil {
        _, _ = tx.Exec(ctx, `
          INSERT INTO spooled_transfers(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata,status,fail_reason,updated_at)
          VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9,now())
          ON CONFLICT (request_id) DO NOTHING
        `, req, ph, from, to, amt, zid, string(mb), st, *fail)
      } else {
        _, _ = tx.Exec(ctx, `
          INSERT INTO spooled_transfers(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata,status,updated_at)
          VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8,now())
          ON CONFLICT (request_id) DO NOTHING
        `, req, ph, from, to, amt, zid, string(mb), st)
      }
    }
  }

  // audit tail
  if al, ok := snap["audit_log"].([]any); ok {
    for _, it := range al {
      m, _ := it.(map[string]any)
      actor, _ := m["actor"].(string)
      action, _ := m["action"].(string)
      tt, _ := m["target_type"].(string)
      tid, _ := m["target_id"].(string)
      if actor=="" || action=="" || tt=="" || tid=="" { continue }
      reasonAny := m["reason"]
      var reason *string
      if rs, ok := reasonAny.(string); ok && rs != "" { reason = &rs }
      details := m["details"]
      db, _ := json.Marshal(details)
      if reason != nil {
        _, _ = tx.Exec(ctx, `INSERT INTO audit_log(actor,action,target_type,target_id,reason,details,created_at) VALUES($1,$2,$3,$4,$5,$6::jsonb,now())`,
          actor, action, tt, tid, *reason, string(db))
      } else {
        _, _ = tx.Exec(ctx, `INSERT INTO audit_log(actor,action,target_type,target_id,details,created_at) VALUES($1,$2,$3,$4,$5::jsonb,now())`,
          actor, action, tt, tid, string(db))
      }
    }
  }

  return tx.Commit(ctx)
}


type BalanceRow struct {
  AccountID string    `json:"account_id"`
  BalanceUnits int64  `json:"balance_units"`
  UpdatedAt time.Time `json:"updated_at"`
}

func (l *Ledger) ListBalances(ctx context.Context, limit int) ([]BalanceRow, error) {
  if limit <= 0 || limit > 500 { limit = 100 }
  rows, err := l.db.Query(ctx, `
    SELECT account_id, balance_units, updated_at
    FROM balances
    ORDER BY updated_at DESC
    LIMIT $1
  `, limit)
  if err != nil { return nil, err }
  defer rows.Close()

  out := []BalanceRow{}
  for rows.Next() {
    var b BalanceRow
    if err := rows.Scan(&b.AccountID, &b.BalanceUnits, &b.UpdatedAt); err != nil { return nil, err }
    out = append(out, b)
  }
  return out, nil
}

type TransactionRow struct {
  ID string `json:"id"`
  RequestID string `json:"request_id"`
  FromAccount string `json:"from_account"`
  ToAccount string `json:"to_account"`
  AmountUnits int64 `json:"amount_units"`
  ZoneID string `json:"zone_id"`
  CreatedAt time.Time `json:"created_at"`
}

type PostingRow struct {
  AccountID string `json:"account_id"`
  Direction string `json:"direction"`
  AmountUnits int64 `json:"amount_units"`
}

type TransactionDetail struct {
  TransactionRow
  Metadata map[string]any `json:"metadata"`
  Postings []PostingRow `json:"postings"`
}

func (l *Ledger) ListTransactions(ctx context.Context, limit int) ([]TransactionRow, error) {
  if limit <= 0 || limit > 500 { limit = 100 }
  rows, err := l.db.Query(ctx, `
    SELECT id::text, request_id, from_account, to_account, amount_units, zone_id, created_at
    FROM transactions
    ORDER BY created_at DESC
    LIMIT $1
  `, limit)
  if err != nil { return nil, err }
  defer rows.Close()

  out := []TransactionRow{}
  for rows.Next() {
    var t TransactionRow
    if err := rows.Scan(&t.ID, &t.RequestID, &t.FromAccount, &t.ToAccount, &t.AmountUnits, &t.ZoneID, &t.CreatedAt); err != nil { return nil, err }
    out = append(out, t)
  }
  return out, nil
}

func (l *Ledger) GetTransaction(ctx context.Context, id string) (*TransactionDetail, error) {
  var t TransactionDetail
  var metaBytes []byte
  err := l.db.QueryRow(ctx, `
    SELECT id::text, request_id, from_account, to_account, amount_units, zone_id, created_at, metadata
    FROM transactions
    WHERE id::text = $1
  `, id).Scan(&t.ID, &t.RequestID, &t.FromAccount, &t.ToAccount, &t.AmountUnits, &t.ZoneID, &t.CreatedAt, &metaBytes)
  if err != nil { return nil, err }
  _ = json.Unmarshal(metaBytes, &t.Metadata)

  rows, err := l.db.Query(ctx, `
    SELECT account_id, direction, amount_units
    FROM postings
    WHERE txn_id::text = $1
    ORDER BY direction ASC
  `, id)
  if err != nil { return nil, err }
  defer rows.Close()

  posts := []PostingRow{}
  for rows.Next() {
    var p PostingRow
    if err := rows.Scan(&p.AccountID, &p.Direction, &p.AmountUnits); err != nil { return nil, err }
    posts = append(posts, p)
  }
  t.Postings = posts
  return &t, nil
}


// --- internal helpers for transfer application and spooling ---

func (l *Ledger) hashPercent(s string) int {
  h := fnv.New32a()
  _, _ = h.Write([]byte(s))
  return int(h.Sum32() % 100)
}

func (l *Ledger) getZoneControlsTx(ctx context.Context, tx pgx.Tx, zoneID string) (*ZoneControls, error) {
  // ensure row exists
  _, _ = tx.Exec(ctx, `INSERT INTO zone_controls(zone_id) VALUES($1) ON CONFLICT DO NOTHING`, zoneID)
  var c ZoneControls
  err := tx.QueryRow(ctx, `
    SELECT zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at
    FROM zone_controls
    WHERE zone_id=$1
  `, zoneID).Scan(&c.ZoneID, &c.WritesBlocked, &c.CrossZoneThrottle, &c.SpoolEnabled, &c.UpdatedAt)
  if err != nil {
    return nil, err
  }
  return &c, nil
}

func (l *Ledger) spoolTransferTx(ctx context.Context, tx pgx.Tx, in CreateTransferInput, metaBytes []byte, failReason string) (string, error) {
  // idempotency within spool table
  var existingID string
  var existingHash string
  err := tx.QueryRow(ctx, `SELECT id::text, payload_hash FROM spooled_transfers WHERE request_id=$1`, in.RequestID).
    Scan(&existingID, &existingHash)
  if err == nil {
    if existingHash != in.PayloadHash {
      return "", ErrIdempotencyConflict
    }
    return existingID, nil
  }
  if err != nil && !errors.Is(err, pgx.ErrNoRows) {
    return "", err
  }

  var id string
  err = tx.QueryRow(ctx, `
    INSERT INTO spooled_transfers(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata,status,fail_reason,updated_at)
    VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,'PENDING',$8,now())
    RETURNING id::text
  `, in.RequestID, in.PayloadHash, in.FromAccount, in.ToAccount, in.AmountUnits, in.ZoneID, string(metaBytes), failReason).Scan(&id)
  if err != nil { return "", err }

  _, _ = tx.Exec(ctx, `
    INSERT INTO audit_log(actor,action,target_type,target_id,reason,details)
    VALUES('system','SPOOL_TRANSFER','zone',$1,$2, jsonb_build_object('request_id',$3,'spool_id',$4))
  `, in.ZoneID, failReason, in.RequestID, id)

  return id, nil
}

func (l *Ledger) applyTransferTx(ctx context.Context, tx pgx.Tx, in CreateTransferInput, metaBytes []byte) (string, time.Time, error) {
  var txnID string
  var createdAt time.Time
  err := tx.QueryRow(ctx, `
    INSERT INTO transactions(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata)
    VALUES($1,$2,$3,$4,$5,$6,$7::jsonb)
    RETURNING id::text, created_at
  `, in.RequestID, in.PayloadHash, in.FromAccount, in.ToAccount, in.AmountUnits, in.ZoneID, string(metaBytes)).Scan(&txnID, &createdAt)
  if err != nil { return "", time.Time{}, err }

  // postings
  _, err = tx.Exec(ctx, `
    INSERT INTO postings(txn_id,account_id,direction,amount_units)
    VALUES($1::uuid,$2,'DEBIT',$3),
          ($1::uuid,$4,'CREDIT',$3)
  `, txnID, in.FromAccount, in.AmountUnits, in.ToAccount)
  if err != nil { return "", time.Time{}, err }

  // balance projection (allow negative; this is a sim)
  _, err = tx.Exec(ctx, `
    INSERT INTO balances(account_id,balance_units,updated_at)
    VALUES($1,$2,now())
    ON CONFLICT (account_id) DO UPDATE
      SET balance_units = balances.balance_units + EXCLUDED.balance_units,
          updated_at = now()
  `, in.FromAccount, -in.AmountUnits)
  if err != nil { return "", time.Time{}, err }

  _, err = tx.Exec(ctx, `
    INSERT INTO balances(account_id,balance_units,updated_at)
    VALUES($1,$2,now())
    ON CONFLICT (account_id) DO UPDATE
      SET balance_units = balances.balance_units + EXCLUDED.balance_units,
          updated_at = now()
  `, in.ToAccount, in.AmountUnits)
  if err != nil { return "", time.Time{}, err }

  // transactional outbox event => JetStream => fraud consumer
  payload := map[string]any{
    "event_id": "generated_by_db",
    "transaction_id": txnID,
    "zone_id": in.ZoneID,
    "amount_units": in.AmountUnits,
    "created_at": createdAt.UTC().Format(time.RFC3339Nano),
  }
  pb, _ := json.Marshal(payload)

  _, err = tx.Exec(ctx, `
    INSERT INTO outbox_events(event_type,aggregate_type,aggregate_id,payload)
    VALUES('TRANSFER_POSTED','transaction',$1,$2::jsonb)
  `, txnID, string(pb))
  if err != nil { return "", time.Time{}, err }

  return txnID, createdAt, nil
}

// ApplyTransferBypass applies a transfer without zone gating (used for spool replay).
// Idempotency is still enforced by request_id + payload_hash.
func (l *Ledger) ApplyTransferBypass(ctx context.Context, in CreateTransferInput) (*Transaction, error) {
  metaBytes, err := json.Marshal(in.Metadata)
  if err != nil { return nil, err }

  tx, err := l.db.BeginTx(ctx, pgx.TxOptions{})
  if err != nil { return nil, err }
  defer func() { _ = tx.Rollback(ctx) }()

  // idempotency
  var existingID string
  var existingHash string
  var createdAt time.Time
  err = tx.QueryRow(ctx, `SELECT id::text,payload_hash,created_at FROM transactions WHERE request_id=$1`, in.RequestID).
    Scan(&existingID, &existingHash, &createdAt)
  if err == nil {
    if existingHash != in.PayloadHash {
      return nil, ErrIdempotencyConflict
    }
    _ = tx.Commit(ctx)
    return &Transaction{ID: existingID, RequestID: in.RequestID, CreatedAt: createdAt}, nil
  }
  if err != nil && !errors.Is(err, pgx.ErrNoRows) {
    return nil, err
  }

  if err := l.ensureAccount(ctx, tx, in.FromAccount, in.ZoneID); err != nil { return nil, err }
  if err := l.ensureAccount(ctx, tx, in.ToAccount, in.ZoneID); err != nil { return nil, err }

  txnID, createdAt, err := l.applyTransferTx(ctx, tx, in, metaBytes)
  if err != nil { return nil, err }

  if err := tx.Commit(ctx); err != nil { return nil, err }
  return &Transaction{ID: txnID, RequestID: in.RequestID, CreatedAt: createdAt}, nil
}
