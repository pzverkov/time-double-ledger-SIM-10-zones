package web

import (
  "encoding/json"
  "net/http"
  "strconv"
  "time"

  "github.com/go-chi/chi/v5"
  "log/slog"

    "time-ledger-sim/go/internal/ledger"
  "time-ledger-sim/go/internal/util"
)

type API struct {
  adminKey string
  led *ledger.Ledger
  log *slog.Logger
}

func NewAPI(adminKey string, led *ledger.Ledger, log *slog.Logger) *API {
  return &API{adminKey: adminKey, led: led, log: log}
}

func (a *API) RegisterRoutes(r chi.Router) {
  r.Get("/v1/version", a.handleVersion)

  r.Get("/v1/zones", a.handleListZones)

  r.Post("/v1/transfers", a.handleCreateTransfer)

  r.Get("/v1/balances", a.handleListBalances)
  r.Get("/v1/transactions", a.handleListTransactions)
  r.Get("/v1/transactions/{transaction_id}", a.handleGetTransaction)

  r.Post("/v1/zones/{zone_id}/status", a.handleSetZoneStatus)

  // incidents
  r.Get("/v1/zones/{zone_id}/incidents", a.handleListIncidentsByZone)
  r.Get("/v1/incidents", a.handleListRecentIncidents)
  r.Get("/v1/incidents/{incident_id}", a.handleGetIncident)
  r.Post("/v1/incidents/{incident_id}/action", a.handleIncidentAction)

  // ops controls + spool + audit
  r.Get("/v1/zones/{zone_id}/controls", a.handleGetZoneControls)
  r.Post("/v1/zones/{zone_id}/controls", a.handleSetZoneControls)

  r.Get("/v1/zones/{zone_id}/spool", a.handleGetSpoolStats)
  r.Post("/v1/zones/{zone_id}/spool/replay", a.handleReplaySpool)

  r.Get("/v1/zones/{zone_id}/audit", a.handleListAudit)

  // sim admin (snapshots)
  r.Post("/v1/sim/snapshot", a.admin(a.handleSnapshot))
  r.Post("/v1/sim/restore", a.admin(a.handleRestore))
}

func (a *API) admin(next http.HandlerFunc) http.HandlerFunc {
  return func(w http.ResponseWriter, r *http.Request) {
    if a.adminKey == "" {
      http.Error(w, "admin disabled", http.StatusForbidden)
      return
    }
    if r.Header.Get("X-Admin-Key") != a.adminKey {
      http.Error(w, "forbidden", http.StatusForbidden)
      return
    }
    next(w, r)
  }
}

func writeJSON(w http.ResponseWriter, status int, v any) {
  w.Header().Set("content-type", "application/json")
  w.WriteHeader(status)
  _ = json.NewEncoder(w).Encode(v)
}

func (a *API) handleListZones(w http.ResponseWriter, r *http.Request) {
  zones, err := a.led.ListZones(r.Context())
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"zones": zones})
}

type CreateTransferRequest struct {
  RequestID string        `json:"request_id"`
  FromAccount string      `json:"from_account"`
  ToAccount string        `json:"to_account"`
  AmountUnits int64       `json:"amount_units"`
  ZoneID string           `json:"zone_id"`
  Metadata map[string]any `json:"metadata"`
}

type TransferAppliedResponse struct {
  Status string    `json:"status"` // APPLIED
  TransactionID string `json:"transaction_id"`
  RequestID string `json:"request_id"`
  CreatedAt time.Time `json:"created_at"`
}

type TransferSpooledResponse struct {
  Status string `json:"status"` // SPOOLED
  SpoolID string `json:"spool_id"`
  RequestID string `json:"request_id"`
}

func (a *API) handleCreateTransfer(w http.ResponseWriter, r *http.Request) {
  var req CreateTransferRequest
  if err := json.NewDecoder(r.Body).Decode(&req); err != nil { http.Error(w, "bad json", 400); return }
  if req.RequestID == "" || req.FromAccount == "" || req.ToAccount == "" || req.ZoneID == "" || req.AmountUnits <= 0 {
    http.Error(w, "missing/invalid fields", 400); return
  }
  if req.Metadata == nil { req.Metadata = map[string]any{} }

  payloadHash, err := util.HashCanonicalJSON(req)
  if err != nil { http.Error(w, "hash error", 500); return }

  txn, spoolID, err := a.led.CreateTransfer(r.Context(), ledger.CreateTransferInput{
    RequestID: req.RequestID,
    PayloadHash: payloadHash,
    FromAccount: req.FromAccount,
    ToAccount: req.ToAccount,
    AmountUnits: req.AmountUnits,
    ZoneID: req.ZoneID,
    Metadata: req.Metadata,
  })
  if err != nil {
    if ledger.IsIdempotencyConflict(err) {
      http.Error(w, "idempotency conflict", http.StatusConflict)
      return
    }
    if ledger.IsZoneDown(err) {
      http.Error(w, "zone down", http.StatusServiceUnavailable)
      return
    }
    if ledger.IsZoneBlocked(err) {
      http.Error(w, "zone blocked", http.StatusServiceUnavailable)
      return
    }
    http.Error(w, err.Error(), 500)
    return
  }

  if spoolID != nil {
    writeJSON(w, http.StatusAccepted, TransferSpooledResponse{Status: "SPOOLED", SpoolID: *spoolID, RequestID: req.RequestID})
    return
  }
  writeJSON(w, 200, TransferAppliedResponse{Status: "APPLIED", TransactionID: txn.ID, RequestID: txn.RequestID, CreatedAt: txn.CreatedAt})
}

func (a *API) handleListBalances(w http.ResponseWriter, r *http.Request) {
  limit := 100
  if q := r.URL.Query().Get("limit"); q != "" {
    if n, err := strconv.Atoi(q); err == nil { limit = n }
  }
  rows, err := a.led.ListBalances(r.Context(), limit)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"balances": rows})
}

func (a *API) handleListTransactions(w http.ResponseWriter, r *http.Request) {
  limit := 100
  if q := r.URL.Query().Get("limit"); q != "" {
    if n, err := strconv.Atoi(q); err == nil { limit = n }
  }
  rows, err := a.led.ListTransactions(r.Context(), limit)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"transactions": rows})
}

func (a *API) handleGetTransaction(w http.ResponseWriter, r *http.Request) {
  id := chi.URLParam(r, "transaction_id")
  t, err := a.led.GetTransaction(r.Context(), id)
  if err != nil { http.Error(w, err.Error(), 404); return }
  writeJSON(w, 200, t)
}

type SetZoneStatusRequest struct {
  Status string `json:"status"`
  Actor string `json:"actor"`
  Reason string `json:"reason"`
}

func (a *API) handleSetZoneStatus(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  var req SetZoneStatusRequest
  if err := json.NewDecoder(r.Body).Decode(&req); err != nil { http.Error(w, "bad json", 400); return }
  if zoneID == "" || req.Status == "" || req.Actor == "" { http.Error(w, "missing fields", 400); return }
  z, err := a.led.SetZoneStatus(r.Context(), zoneID, req.Status, req.Actor, req.Reason)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, z)
}

func (a *API) handleListIncidentsByZone(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  inc, err := a.led.ListIncidentsByZone(r.Context(), zoneID)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"incidents": inc})
}

func (a *API) handleListRecentIncidents(w http.ResponseWriter, r *http.Request) {
  limit := 500
  if q := r.URL.Query().Get("limit"); q != "" {
    if n, err := strconv.Atoi(q); err == nil { limit = n }
  }
  inc, err := a.led.ListRecentIncidents(r.Context(), limit)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"incidents": inc})
}

func (a *API) handleGetIncident(w http.ResponseWriter, r *http.Request) {
  id := chi.URLParam(r, "incident_id")
  inc, err := a.led.GetIncident(r.Context(), id)
  if err != nil { http.Error(w, err.Error(), 404); return }
  writeJSON(w, 200, inc)
}

// --- ops: controls + spool + audit + incident actions ---

func (a *API) handleGetZoneControls(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  c, err := a.led.GetZoneControls(r.Context(), zoneID)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, c)
}

type SetZoneControlsRequest struct {
  WritesBlocked bool `json:"writes_blocked"`
  CrossZoneThrottle int `json:"cross_zone_throttle"`
  SpoolEnabled bool `json:"spool_enabled"`
  Actor string `json:"actor"`
  Reason string `json:"reason"`
}

func (a *API) handleSetZoneControls(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  var req SetZoneControlsRequest
  if err := json.NewDecoder(r.Body).Decode(&req); err != nil { http.Error(w, "bad json", 400); return }
  if zoneID == "" || req.Actor == "" { http.Error(w, "missing fields", 400); return }
  c, err := a.led.SetZoneControls(r.Context(), zoneID, req.WritesBlocked, req.CrossZoneThrottle, req.SpoolEnabled, req.Actor, req.Reason)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, c)
}

func (a *API) handleGetSpoolStats(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  s, err := a.led.GetSpoolStats(r.Context(), zoneID)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, s)
}

type ReplaySpoolRequest struct {
  Limit int `json:"limit"`
  Actor string `json:"actor"`
  Reason string `json:"reason"`
}

func (a *API) handleReplaySpool(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  var req ReplaySpoolRequest
  if err := json.NewDecoder(r.Body).Decode(&req); err != nil { http.Error(w, "bad json", 400); return }
  if zoneID == "" || req.Actor == "" { http.Error(w, "missing fields", 400); return }
  res, err := a.led.ReplaySpool(r.Context(), zoneID, req.Limit, req.Actor, req.Reason)
  if err != nil { http.Error(w, err.Error(), 409); return }
  writeJSON(w, 200, res)
}

func (a *API) handleListAudit(w http.ResponseWriter, r *http.Request) {
  zoneID := chi.URLParam(r, "zone_id")
  limit := 100
  if q := r.URL.Query().Get("limit"); q != "" {
    if n, err := strconv.Atoi(q); err == nil { limit = n }
  }
  entries, err := a.led.ListAuditForZone(r.Context(), zoneID, limit)
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"audit": entries})
}

type IncidentActionRequest struct {
  Action string `json:"action"` // ACK|ASSIGN|RESOLVE
  Assignee string `json:"assignee"`
  Note string `json:"note"`
  Actor string `json:"actor"`
  Reason string `json:"reason"`
}

func (a *API) handleIncidentAction(w http.ResponseWriter, r *http.Request) {
  id := chi.URLParam(r, "incident_id")
  var req IncidentActionRequest
  if err := json.NewDecoder(r.Body).Decode(&req); err != nil { http.Error(w, "bad json", 400); return }
  if id == "" || req.Actor == "" || req.Action == "" { http.Error(w, "missing fields", 400); return }

  out, err := a.led.ApplyIncidentAction(r.Context(), id, ledger.IncidentAction{
    Action: req.Action,
    Assignee: req.Assignee,
    Note: req.Note,
    Actor: req.Actor,
    Reason: req.Reason,
  })
  if err != nil { http.Error(w, err.Error(), 409); return }
  writeJSON(w, 200, out)
}

func (a *API) handleSnapshot(w http.ResponseWriter, r *http.Request) {
  snap, err := a.led.Snapshot(r.Context())
  if err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, snap)
}

func (a *API) handleRestore(w http.ResponseWriter, r *http.Request) {
  var snap map[string]any
  if err := json.NewDecoder(r.Body).Decode(&snap); err != nil { http.Error(w, "bad json", 400); return }
  if err := a.led.Restore(r.Context(), snap); err != nil { http.Error(w, err.Error(), 500); return }
  writeJSON(w, 200, map[string]any{"status":"ok"})
}
