package util

import "testing"

func TestHashCanonicalJSON_StableMapOrder(t *testing.T) {
  a := map[string]any{"b": 2, "a": 1}
  b := map[string]any{"a": 1, "b": 2}
  ha, err := HashCanonicalJSON(a)
  if err != nil { t.Fatal(err) }
  hb, err := HashCanonicalJSON(b)
  if err != nil { t.Fatal(err) }
  if ha != hb {
    t.Fatalf("expected stable hash, got %s != %s", ha, hb)
  }
}
