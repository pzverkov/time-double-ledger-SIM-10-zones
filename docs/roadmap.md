# Time Ledger Sim - Roadmap

This project is a finance-flavored simulation game: double-entry ledger, idempotency, at-least-once messaging, incident ops, and an operator console. We optimize for **correctness first**, then **cost**, then **comfort**.

---

## Guiding Principles

- **Ledger truth is boring on purpose**: ACID, constraints, immutability, audit trail.
- **Everything else can fail**: retries + idempotency + outbox/inbox.
- **Ship fast, avoid drift**: one source of truth for config; no copy/paste regions.
- **Pay for state, not for idle**: Cloud Run scales to zero; only DB + messaging are always-on.

---

## Phases

### Phase 0 - Safe Public Repo
**Goal:** publish code without publishing danger.  
**What we do:**
- No secrets in repo.
- Operator endpoints (controls, snapshot/restore) require `ADMIN_KEY`.
- Logs avoid raw user metadata / accidental PII.
**Trade-off:** a bit of setup, a lot less regret.

Done when:
- CI runs on PRs, secrets are only in Secret Manager, and “admin” actions are gated.

---

### Phase 1 - EU Main Demo (Cheapest, Closest)
**Goal:** live public demo with minimal monthly spend.  
**What we do:**
- Deploy **EU only** (`europe-west1`).
- No custom domain.
- No load balancer.
- Dashboard on GitHub Pages calls the Cloud Run `*.run.app` URL via CORS allowlist.
**Trade-off:** URLs are ugly, costs stay tiny.

Done when:
- EU backend is live, UI is live, and basic incident + controls flows are demoable.

---

### Phase 2 - Optional Regional Stacks (AU, South America, optional Africa)
**Goal:** one-click deploy additional regions with zero drift.  
**What we do:**
- Add region config files only (`deploy/regions/*.yaml`).
- Same workflow, manual toggles to deploy.
- Each region is an independent “game instance” (no global ledger).
**Trade-off:** each region adds baseline cost for state (DB + NATS VM), but compute stays cheap.

Done when:
- You can deploy AU/SA/AF by checking boxes in workflow_dispatch without editing code.

---

### Phase 3 - Domain + Global External HTTP(S) Load Balancer
**Goal:** one clean hostname and multi-region front door.  
**What we do:**
- Buy a domain.
- Create global external HTTPS load balancer with serverless NEGs to Cloud Run services.
- Route policy: “closest region” or “EU primary + failover.”
**Trade-off:** introduces a baseline monthly cost even at low traffic.

Done when:
- `api.example.com` works, TLS is automated, and EU/AU/SA can be attached without drift.

---

### Phase 4 - Mature Finance Posture
**Goal:** “not just correct, but resilient under stress.”  
**What we do (later):**
- HA DB, multi-node messaging, stronger auth (OIDC), tighter IAM, richer audit export.
- SLOs, alerts, and incident response playbooks.
**Trade-off:** cost + complexity rise. Only worth it when usage demands it.

Done when:
- You can safely run production traffic with clear SLOs and enforced access controls.

---

## “Split Repos Later” Plan (No Pain Version)

We start as a monorepo for speed. Later, we split without rewriting.

### Keep these rules now (to make later easy)
1) Web talks to backend only via HTTP (`VITE_API_BASE`).
2) All deploy config lives in `deploy/` (no hidden scripts elsewhere).
3) Secrets live only in Secret Manager.
4) `api/openapi.yaml` stays the contract (later published as an artifact).

### Split steps (when ready)
1) Create repo: `time-ledger-sim-dashboard`
2) Move `web/` + Pages workflow there
3) Set `VITE_API_BASE` to backend URL/domain
4) Tighten backend `CORS_ALLOW_ORIGINS` to the new Pages domain
5) Keep backend deploy workflow in backend repo
6) Optional: publish OpenAPI as a versioned release artifact

---

## Improvement Backlog (High ROI First)

### Immediate
- Lockfiles committed (go.sum, package-lock, Cargo.lock)
- CI: govulncheck + cargo audit + npm audit gating
- `/v1/version` displayed in UI (already present in hardened plan)

### Next
- Add dependency graph editor in UI (instead of hardcoded deps)
- Add “runbook checklist” per incident (stored in incident.details JSON)
- Add reconciliation job (ledger invariant checks) + report panel

### Later
- Digest-pinned container images
- SBOM generation (CycloneDX) + artifact upload
- Regional failover policy (LB) + chaos toggles for the sim

---

## Decisions We’ve Made (So We Don’t Re-decide Them Every Week)

- EU is the main demo region first; AU/SA/AF are optional.
- “10 zones” are logical game zones, not tied to physical regions.
- Load balancer + custom domain happens only after the demo is stable.
- We optimize for finance correctness even if it’s “a bit boring.”
