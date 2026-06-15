#!/usr/bin/env bash
# Pipeline throughput + drain measurement.
#
# Fires N transfers as fast as the API accepts them, then measures wall-clock
# time until the analytics consumer group has folded all N into zone_event_stats.
# Reports sustained events/sec and the backlog drain time end to end. Run this
# against NATS, then against Redpanda, and compare (see docs/benchmarks.md).
#
# Env:
#   BASE_URL      API base (default http://localhost:8081)
#   COMPOSE_FILE  compose file for psql access (default ci/docker-compose.test.yml)
#   N             number of transfers (default 5000)
#   CONCURRENCY   parallel POSTers (default 50)
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8081}"
COMPOSE_FILE="${COMPOSE_FILE:-ci/docker-compose.test.yml}"
N="${N:-5000}"
CONCURRENCY="${CONCURRENCY:-50}"
ZONE="zone-eu"
RUN="lag-$(date +%s)-$$"

psql_q() {
  docker compose -f "$COMPOSE_FILE" exec -T postgres \
    psql -U postgres -d timeledger -tAc "$1" | tr -d '[:space:]'
}

base_count="$(psql_q "SELECT COALESCE(event_count,0) FROM zone_event_stats WHERE zone_id='$ZONE'")"
base_count="${base_count:-0}"
target=$((base_count + N))

echo "firing $N transfers (concurrency $CONCURRENCY) to $ZONE"
start="$(date +%s.%N)"
seq 1 "$N" | xargs -P "$CONCURRENCY" -I{} \
  curl -fsS -o /dev/null -X POST "$BASE_URL/v1/transfers" \
    -H 'Content-Type: application/json' \
    -d "{\"request_id\":\"$RUN-{}\",\"from_account\":\"a\",\"to_account\":\"b\",\"amount_units\":100,\"zone_id\":\"$ZONE\",\"metadata\":{}}"
post_done="$(date +%s.%N)"

echo "waiting for analytics to drain to event_count>=$target"
while :; do
  cur="$(psql_q "SELECT COALESCE(event_count,0) FROM zone_event_stats WHERE zone_id='$ZONE'")"
  cur="${cur:-0}"
  [ "$cur" -ge "$target" ] && break
  sleep 0.2
done
drained="$(date +%s.%N)"

post_secs="$(echo "$post_done - $start" | bc -l)"
total_secs="$(echo "$drained - $start" | bc -l)"
echo "----"
printf "ingest:   %d transfers in %.2fs -> %.0f req/s\n" "$N" "$post_secs" "$(echo "$N / $post_secs" | bc -l)"
printf "pipeline: drained %d events in %.2fs -> %.0f events/s end-to-end\n" "$N" "$total_secs" "$(echo "$N / $total_secs" | bc -l)"
