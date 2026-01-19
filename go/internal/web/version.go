package web

import (
  "net/http"
  "runtime"
)

// These can be set at build time using -ldflags, e.g.
// -X 'time-ledger-sim/go/internal/web.buildVersion=v1.2.3' -X '...buildCommit=abc' -X '...buildDate=2026-01-19'
var (
  buildVersion = "dev"
  buildCommit  = "unknown"
  buildDate    = "unknown"
)

func (a *API) handleVersion(w http.ResponseWriter, r *http.Request) {
  writeJSON(w, 200, map[string]any{
    "service":  "time-ledger-sim",
    "version":  buildVersion,
    "commit":   buildCommit,
    "buildDate": buildDate,
    "go":       runtime.Version(),
  })
}
