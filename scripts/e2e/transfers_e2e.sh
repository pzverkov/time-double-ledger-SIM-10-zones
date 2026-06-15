#!/usr/bin/env bash
# End-to-end check of the event pipeline against a running stack.
#
# Posts a large transfer and asserts BOTH consumer groups acted on the event:
#   - fraud      -> an incidents row
#   - analytics  -> a zone_event_stats row
# Then posts a duplicate (same request_id) and asserts no double-processing.
#
# Works for either broker: bring the stack up on NATS (default) or Redpanda
# (EVENT_BROKER=redpanda + --profile redpanda) and run this unchanged.
#
# Env:
#   BASE_URL      API base (default http://localhost:8081)
#   COMPOSE_FILE  compose file for psql access (default ci/docker-compose.test.yml)
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8081}"
COMPOSE_FILE="${COMPOSE_FILE:-ci/docker-compose.test.yml}"
ZONE="zone-eu"
REQ_ID="e2e-$(date +%s)-$$"

psql_q() {
  docker compose -f "$COMPOSE_FILE" exec -T postgres \
    psql -U postgres -d timeledger -tAc "$1" | tr -d '[:space:]'
}

post_transfer() {
  curl -fsS -X POST "$BASE_URL/v1/transfers" \
    -H 'Content-Type: application/json' \
    -d "{\"request_id\":\"$REQ_ID\",\"from_account\":\"a\",\"to_account\":\"b\",\"amount_units\":5000,\"zone_id\":\"$ZONE\",\"metadata\":{}}"
}

wait_for() { # <description> <sql returning a count> <expected>
  local desc="$1" sql="$2" want="$3" got
  for _ in $(seq 1 40); do
    got="$(psql_q "$sql")"
    if [ "$got" = "$want" ]; then echo "ok: $desc ($got)"; return 0; fi
    sleep 0.5
  done
  echo "FAIL: $desc - got '$got', want '$want'" >&2
  exit 1
}

echo "posting transfer $REQ_ID (amount 5000, $ZONE)"
TXN_ID="$(post_transfer | python3 -c 'import sys,json;print(json.load(sys.stdin)["transaction_id"])')"
echo "transaction_id=$TXN_ID"

# fraud consumer -> incident
wait_for "fraud incident recorded" \
  "SELECT count(*) FROM incidents WHERE related_txn_id='$TXN_ID'::uuid AND title='Large time transfer'" 1

# analytics consumer -> zone aggregate (>=1 event for the zone)
wait_for "analytics stats present for $ZONE" \
  "SELECT (event_count>0)::int FROM zone_event_stats WHERE zone_id='$ZONE'" 1

# idempotent re-post must not double-process
echo "re-posting duplicate request_id"
post_transfer >/dev/null
sleep 2
wait_for "no duplicate incident" \
  "SELECT count(*) FROM incidents WHERE related_txn_id='$TXN_ID'::uuid AND title='Large time transfer'" 1

echo "e2e PASSED"
