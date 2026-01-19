package ledger

import (
  "context"
  "encoding/json"
  "errors"
  "fmt"
  "time"

  "github.com/jackc/pgx/v5"
)

type ZoneControls struct {
  ZoneID string    `json:"zone_id"`
  WritesBlocked bool `json:"writes_blocked"`
  CrossZoneThrottle int `json:"cross_zone_throttle"`
  SpoolEnabled bool `json:"spool_enabled"`
  UpdatedAt time.Time `json:"updated_at"`
}

func (l *Ledger) GetZoneControls(ctx context.Context, zoneID string) (*ZoneControls, error) {
  var c ZoneControls
  err := l.db.QueryRow(ctx, `
    SELECT zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at
    FROM zone_controls WHERE zone_id=$1
  `, zoneID).Scan(&c.ZoneID, &c.WritesBlocked, &c.CrossZoneThrottle, &c.SpoolEnabled, &c.UpdatedAt)
  if err == nil {
    return &c, nil
  }
  if !errors.Is(err, pgx.ErrNoRows) {
    return nil, err
  }
  // default row
  _, err = l.db.Exec(ctx, `INSERT INTO zone_controls(zone_id) VALUES($1) ON CONFLICT DO NOTHING`, zoneID)
  if err != nil { return nil, err }
  return l.GetZoneControls(ctx, zoneID)
}

func (l *Ledger) SetZoneControls(ctx context.Context, zoneID string, writesBlocked bool, crossZoneThrottle int, spoolEnabled bool, actor, reason string) (*ZoneControls, error) {
  if crossZoneThrottle < 0 || crossZoneThrottle > 100 {
    return nil, fmt.Errorf("invalid cross_zone_throttle")
  }

  tx, err := l.db.BeginTx(ctx, pgx.TxOptions{})
  if err != nil { return nil, err }
  defer func() { _ = tx.Rollback(ctx) }()

  // ensure row exists
  _, _ = tx.Exec(ctx, `INSERT INTO zone_controls(zone_id) VALUES($1) ON CONFLICT DO NOTHING`, zoneID)

  var c ZoneControls
  err = tx.QueryRow(ctx, `
    UPDATE zone_controls
    SET writes_blocked=$2, cross_zone_throttle=$3, spool_enabled=$4, updated_at=now()
    WHERE zone_id=$1
    RETURNING zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at
  `, zoneID, writesBlocked, crossZoneThrottle, spoolEnabled).Scan(&c.ZoneID, &c.WritesBlocked, &c.CrossZoneThrottle, &c.SpoolEnabled, &c.UpdatedAt)
  if err != nil { return nil, err }

  _, err = tx.Exec(ctx, `
    INSERT INTO audit_log(actor,action,target_type,target_id,reason,details)
    VALUES($1,'SET_ZONE_CONTROLS','zone',$2,$3,
      jsonb_build_object('writes_blocked',$4,'cross_zone_throttle',$5,'spool_enabled',$6)
    )
  `, actor, zoneID, reason, writesBlocked, crossZoneThrottle, spoolEnabled)
  if err != nil { return nil, err }

  // Optional incident for strong containment
  if writesBlocked || crossZoneThrottle == 0 {
    sev := "WARN"
    title := "Zone controls tightened"
    if writesBlocked { sev = "CRITICAL"; title = "Writes blocked by operator" }
    _, _ = tx.Exec(ctx, `
      INSERT INTO incidents(zone_id,severity,title,details)
      VALUES($1,$2,$3, jsonb_build_object('reason',$4,'actor',$5,'writes_blocked',$6,'cross_zone_throttle',$7,'spool_enabled',$8))
    `, zoneID, sev, title, reason, actor, writesBlocked, crossZoneThrottle, spoolEnabled)
  }

  if err := tx.Commit(ctx); err != nil { return nil, err }
  return &c, nil
}

type SpoolStats struct {
  ZoneID string `json:"zone_id"`
  Pending int64 `json:"pending"`
  Applied int64 `json:"applied"`
  Failed int64 `json:"failed"`
}

func (l *Ledger) GetSpoolStats(ctx context.Context, zoneID string) (*SpoolStats, error) {
  var p, a, f int64
  err := l.db.QueryRow(ctx, `
    SELECT
      COUNT(*) FILTER (WHERE status='PENDING') as pending,
      COUNT(*) FILTER (WHERE status='APPLIED') as applied,
      COUNT(*) FILTER (WHERE status='FAILED') as failed
    FROM spooled_transfers
    WHERE zone_id=$1
  `, zoneID).Scan(&p, &a, &f)
  if err != nil { return nil, err }
  return &SpoolStats{ZoneID: zoneID, Pending: p, Applied: a, Failed: f}, nil
}

type ReplayResult struct {
  ZoneID string `json:"zone_id"`
  Applied int `json:"applied"`
  Failed int `json:"failed"`
}

func (l *Ledger) ReplaySpool(ctx context.Context, zoneID string, limit int, actor, reason string) (*ReplayResult, error) {
  if limit <= 0 || limit > 500 { limit = 50 }
  // Do not replay if zone is still blocked/down.
  var status string
  err := l.db.QueryRow(ctx, `SELECT status FROM zones WHERE id=$1`, zoneID).Scan(&status)
  if err != nil { return nil, err }
  c, err := l.GetZoneControls(ctx, zoneID)
  if err != nil { return nil, err }
  if status == "DOWN" || c.WritesBlocked || c.CrossZoneThrottle == 0 {
    return nil, fmt.Errorf("zone not ready for replay")
  }

  rows, err := l.db.Query(ctx, `
    SELECT id::text, request_id, payload_hash, from_account, to_account, amount_units, zone_id, metadata
    FROM spooled_transfers
    WHERE zone_id=$1 AND status='PENDING'
    ORDER BY created_at ASC
    LIMIT $2
  `, zoneID, limit)
  if err != nil { return nil, err }
  defer rows.Close()

  res := &ReplayResult{ZoneID: zoneID}

  type spoolRow struct {
    ID string
    Req string
    Hash string
    From string
    To string
    Amt int64
    Zone string
    Meta []byte
  }
  list := []spoolRow{}
  for rows.Next() {
    var r spoolRow
    if err := rows.Scan(&r.ID, &r.Req, &r.Hash, &r.From, &r.To, &r.Amt, &r.Zone, &r.Meta); err != nil { return nil, err }
    list = append(list, r)
  }
  if err := rows.Err(); err != nil { return nil, err }

  for _, s := range list {
    meta := map[string]any{}
    _ = json.Unmarshal(s.Meta, &meta)

    // Apply bypassing gating; idempotency still enforced.
    _, err := l.ApplyTransferBypass(ctx, CreateTransferInput{
      RequestID: s.Req,
      PayloadHash: s.Hash,
      FromAccount: s.From,
      ToAccount: s.To,
      AmountUnits: s.Amt,
      ZoneID: s.Zone,
      Metadata: meta,
    })

    if err == nil {
      res.Applied++
      _, _ = l.db.Exec(ctx, `UPDATE spooled_transfers SET status='APPLIED', updated_at=now(), applied_at=now(), fail_reason=NULL WHERE id=$1::uuid`, s.ID)
      continue
    }

    res.Failed++
    _, _ = l.db.Exec(ctx, `UPDATE spooled_transfers SET status='FAILED', updated_at=now(), fail_reason=$2 WHERE id=$1::uuid`, s.ID, err.Error())
  }

  // Audit summary
  _, _ = l.db.Exec(ctx, `
    INSERT INTO audit_log(actor,action,target_type,target_id,reason,details)
    VALUES($1,'REPLAY_SPOOL','zone',$2,$3, jsonb_build_object('applied',$4,'failed',$5,'limit',$6))
  `, actor, zoneID, reason, res.Applied, res.Failed, limit)

  return res, nil
}

type AuditEntry struct {
  ID string `json:"id"`
  Actor string `json:"actor"`
  Action string `json:"action"`
  TargetType string `json:"target_type"`
  TargetID string `json:"target_id"`
  Reason *string `json:"reason"`
  Details map[string]any `json:"details"`
  CreatedAt time.Time `json:"created_at"`
}

func (l *Ledger) ListAuditForZone(ctx context.Context, zoneID string, limit int) ([]AuditEntry, error) {
  if limit <= 0 || limit > 500 { limit = 100 }
  rows, err := l.db.Query(ctx, `
    (SELECT a.id::text, a.actor, a.action, a.target_type, a.target_id, a.reason, a.details, a.created_at
     FROM audit_log a
     WHERE a.target_type='zone' AND a.target_id=$1
     ORDER BY a.created_at DESC
     LIMIT $2)
    UNION ALL
    (SELECT a.id::text, a.actor, a.action, a.target_type, a.target_id, a.reason, a.details, a.created_at
     FROM audit_log a
     WHERE a.target_type='incident' AND a.target_id IN (
       SELECT id::text FROM incidents WHERE zone_id=$1
     )
     ORDER BY a.created_at DESC
     LIMIT $2)
    ORDER BY created_at DESC
    LIMIT $2
  `, zoneID, limit)
  if err != nil { return nil, err }
  defer rows.Close()

  out := []AuditEntry{}
  for rows.Next() {
    var e AuditEntry
    var reason *string
    var detailsBytes []byte
    if err := rows.Scan(&e.ID, &e.Actor, &e.Action, &e.TargetType, &e.TargetID, &reason, &detailsBytes, &e.CreatedAt); err != nil { return nil, err }
    e.Reason = reason
    _ = json.Unmarshal(detailsBytes, &e.Details)
    out = append(out, e)
  }
  return out, rows.Err()
}

type IncidentAction struct {
  Action string `json:"action"` // ACK|ASSIGN|RESOLVE
  Assignee string `json:"assignee"`
  Note string `json:"note"`
  Actor string `json:"actor"`
  Reason string `json:"reason"`
}

func (l *Ledger) ApplyIncidentAction(ctx context.Context, incidentID string, in IncidentAction) (*Incident, error) {
  if in.Actor == "" { return nil, fmt.Errorf("actor required") }
  if in.Action != "ACK" && in.Action != "ASSIGN" && in.Action != "RESOLVE" {
    return nil, fmt.Errorf("invalid action")
  }
  if in.Action == "ASSIGN" && in.Assignee == "" {
    return nil, fmt.Errorf("assignee required")
  }

  tx, err := l.db.BeginTx(ctx, pgx.TxOptions{})
  if err != nil { return nil, err }
  defer func() { _ = tx.Rollback(ctx) }()

  inc, err := l.GetIncident(ctx, incidentID)
  if err != nil { return nil, err }

  // mutate details
  d := map[string]any{}
  for k, v := range inc.Details { d[k] = v }
  if in.Action == "ASSIGN" {
    d["assignee"] = in.Assignee
  }
  if in.Note != "" {
    // append note
    notes, _ := d["notes"].([]any)
    entry := map[string]any{"at": time.Now().UTC().Format(time.RFC3339Nano), "actor": in.Actor, "note": in.Note, "action": in.Action}
    d["notes"] = append(notes, entry)
  }
  detailsBytes, _ := json.Marshal(d)

  newStatus := inc.Status
  if in.Action == "ACK" {
    newStatus = "ACK"
  } else if in.Action == "RESOLVE" {
    newStatus = "RESOLVED"
  }

  var out Incident
  var related *string
  var dbDetails []byte
  err = tx.QueryRow(ctx, `
    UPDATE incidents
    SET status=$2, details=$3::jsonb
    WHERE id=$1::uuid
    RETURNING id::text, zone_id, related_txn_id::text, severity, status, title, details, detected_at
  `, incidentID, newStatus, string(detailsBytes)).Scan(&out.ID, &out.ZoneID, &related, &out.Severity, &out.Status, &out.Title, &dbDetails, &out.DetectedAt)
  if err != nil { return nil, err }
  out.RelatedTxnID = related
  _ = json.Unmarshal(dbDetails, &out.Details)

  _, err = tx.Exec(ctx, `
    INSERT INTO audit_log(actor,action,target_type,target_id,reason,details)
    VALUES($1,$2,'incident',$3,$4, jsonb_build_object('assignee',$5,'note',$6,'status',$7))
  `, in.Actor, "INCIDENT_"+in.Action, incidentID, in.Reason, in.Assignee, in.Note, newStatus)
  if err != nil { return nil, err }

  if err := tx.Commit(ctx); err != nil { return nil, err }
  return &out, nil
}
