package util

import (
  "bytes"
  "crypto/sha256"
  "encoding/hex"
  "encoding/json"
  "sort"
)

// HashCanonicalJSON: canonicalizes JSON-like structs by encoding with stable map key order.
// For MVP we do a pragmatic approach: marshal to interface{}, recursively sort maps, then marshal again.
func HashCanonicalJSON(v any) (string, error) {
  raw, err := json.Marshal(v)
  if err != nil { return "", err }
  var x any
  if err := json.Unmarshal(raw, &x); err != nil { return "", err }
  canon := canonicalize(x)
  canonBytes, err := json.Marshal(canon)
  if err != nil { return "", err }
  sum := sha256.Sum256(canonBytes)
  return hex.EncodeToString(sum[:]), nil
}

func canonicalize(v any) any {
  switch t := v.(type) {
  case map[string]any:
    keys := make([]string, 0, len(t))
    for k := range t { keys = append(keys, k) }
    sort.Strings(keys)
    out := make(map[string]any, len(t))
    for _, k := range keys {
      out[k] = canonicalize(t[k])
    }
    return out
  case []any:
    out := make([]any, 0, len(t))
    for _, it := range t { out = append(out, canonicalize(it)) }
    return out
  default:
    return t
  }
}

// Useful for logs/testing
func PrettyJSON(v any) string {
  b, _ := json.Marshal(v)
  var out bytes.Buffer
  _ = json.Indent(&out, b, "", "  ")
  return out.String()
}
