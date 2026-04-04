package util

import "testing"

func TestHashCanonicalJSON_StableMapOrder(t *testing.T) {
	a := map[string]any{"b": 2, "a": 1}
	b := map[string]any{"a": 1, "b": 2}
	ha, err := HashCanonicalJSON(a)
	if err != nil {
		t.Fatal(err)
	}
	hb, err := HashCanonicalJSON(b)
	if err != nil {
		t.Fatal(err)
	}
	if ha != hb {
		t.Fatalf("expected stable hash, got %s != %s", ha, hb)
	}
}

func TestHashCanonicalJSON_DifferentInput(t *testing.T) {
	a := map[string]any{"a": 1}
	b := map[string]any{"a": 2}
	ha, _ := HashCanonicalJSON(a)
	hb, _ := HashCanonicalJSON(b)
	if ha == hb {
		t.Fatal("different inputs should produce different hashes")
	}
}

func TestHashCanonicalJSON_Nested(t *testing.T) {
	a := map[string]any{"z": map[string]any{"b": 1, "a": 2}, "a": []any{3, 2, 1}}
	b := map[string]any{"a": []any{3, 2, 1}, "z": map[string]any{"a": 2, "b": 1}}
	ha, _ := HashCanonicalJSON(a)
	hb, _ := HashCanonicalJSON(b)
	if ha != hb {
		t.Fatalf("nested maps with same data should have same hash: %s != %s", ha, hb)
	}
}

func TestHashCanonicalJSON_EmptyMap(t *testing.T) {
	h, err := HashCanonicalJSON(map[string]any{})
	if err != nil {
		t.Fatal(err)
	}
	if h == "" {
		t.Fatal("expected non-empty hash for empty map")
	}
}
