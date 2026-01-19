package web

import (
  "net/http"
  "strings"
)

func CORSMiddleware(corsAllowOrigins string) func(http.Handler) http.Handler {
  allowed := []string{}
  for _, o := range strings.Split(corsAllowOrigins, ",") {
    t := strings.TrimSpace(o)
    if t != "" { allowed = append(allowed, t) }
  }

  allowAny := false
  for _, a := range allowed {
    if a == "*" { allowAny = true; break }
  }

  return func(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
      origin := r.Header.Get("Origin")
      if origin != "" {
        if allowAny {
          w.Header().Set("Access-Control-Allow-Origin", origin)
        } else {
          for _, a := range allowed {
            if origin == a {
              w.Header().Set("Access-Control-Allow-Origin", origin)
              break
            }
          }
        }
        w.Header().Set("Vary", "Origin")
        w.Header().Set("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
        w.Header().Set("Access-Control-Allow-Headers", "Content-Type,X-Admin-Key")
      }

      if r.Method == http.MethodOptions {
        w.WriteHeader(http.StatusNoContent)
        return
      }

      next.ServeHTTP(w, r)
    })
  }
}
