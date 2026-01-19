package util

import (
  "net/http"
  "strconv"
)

func QueryInt(r *http.Request, key string, def int) int {
  v := r.URL.Query().Get(key)
  if v == "" { return def }
  n, err := strconv.Atoi(v)
  if err != nil { return def }
  return n
}
