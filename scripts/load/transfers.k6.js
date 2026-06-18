// Load test for POST /v1/transfers with pass/fail thresholds (a gate, not a
// vanity number). Run: k6 run scripts/load/transfers.k6.js
//
// Env:
//   BASE_URL   API base (default http://localhost:8081)
//   RATE       target requests/sec (default 200)
//   DURATION   sustained duration (default 30s)
//   LARGE_PCT  percent of transfers above the fraud threshold (default 10)
import http from "k6/http";
import { check } from "k6";

const BASE_URL = __ENV.BASE_URL || "http://localhost:8081";
const RATE = parseInt(__ENV.RATE || "200", 10);
// Distinct account pairs to spread writes across. Default 1 keeps every transfer
// on the same hot pair (a worst-case lock-contention gate); raise it to measure
// throughput without the artificial hot-row serialization.
const ACCTS = parseInt(__ENV.ACCTS || "1", 10);
const DURATION = __ENV.DURATION || "30s";
const LARGE_PCT = parseInt(__ENV.LARGE_PCT || "10", 10);
const ZONES = ["zone-na", "zone-sa", "zone-eu", "zone-af", "zone-ap"];

export const options = {
  scenarios: {
    transfers: {
      executor: "constant-arrival-rate",
      rate: RATE,
      timeUnit: "1s",
      duration: DURATION,
      preAllocatedVUs: Math.max(50, RATE),
      maxVUs: RATE * 4,
    },
  },
  thresholds: {
    // Fail the run on regression.
    http_req_duration: ["p(95)<150"],
    http_req_failed: ["rate<0.01"],
  },
};

export default function () {
  const large = Math.random() * 100 < LARGE_PCT;
  const amount = large ? 4000 + Math.floor(Math.random() * 4000) : 1 + Math.floor(Math.random() * 1000);
  const zone = ZONES[Math.floor(Math.random() * ZONES.length)];
  const pair = ACCTS > 1 ? Math.floor(Math.random() * ACCTS) : "";
  const body = JSON.stringify({
    request_id: `k6-${__VU}-${__ITER}-${Date.now()}`,
    from_account: `acct-src${pair}`,
    to_account: `acct-dst${pair}`,
    amount_units: amount,
    zone_id: zone,
    metadata: {},
  });
  const res = http.post(`${BASE_URL}/v1/transfers`, body, {
    headers: { "Content-Type": "application/json" },
  });
  check(res, { "status 2xx": (r) => r.status >= 200 && r.status < 300 });
}
