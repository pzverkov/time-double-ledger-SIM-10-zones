# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-06-17

### Added
- Swappable messaging broker behind `EventPublisher`/`EventConsumer`/`EventHandler` traits with `OutboxStore`/`FraudStore`/`AnalyticsStore` DB ports; `EVENT_BROKER` selects NATS JetStream (default) or Redpanda (Kafka API, `redpanda` cargo feature). See ADR 0001
- Analytics consumer group (`zone_event_stats`) demonstrating fan-out alongside the fraud consumer
- Dead-letter queue for poison events and an outbox/inbox/DLQ retention job
- Event schema versioning: `schema_version` stamped on every event; consumers reject unsupported versions (see `docs/event-schema.md`)
- Continuous ledger reconciliation job that flags balance-vs-postings drift and unbalanced transactions via Prometheus gauges
- Per-client rate limiting on `POST /v1/transfers` (`RATE_LIMIT_RPS`, `TRUST_PROXY_HEADERS`), returning 429
- OpenTelemetry tracing with W3C trace context propagated from the transfer through the outbox and broker into the consumers
- Readiness probe `/readyz` (dependency-aware), request and statement timeouts, and load shedding
- `outbox_backlog` Prometheus gauge as the pipeline backlog SLI
- Failure-path unit tests and `cargo bench` micro-benchmarks; e2e and load tooling (`scripts/e2e`, `scripts/load`); contributing, security, architecture, parity, event-schema, and backup/DR docs

### Changed
- Outbox publisher is multi-replica safe (`FOR UPDATE SKIP LOCKED`); broker connect retries in the background so a startup race cannot permanently disable messaging
- DB connection pool is bounded and recycling in both backends (verified recycling / lifetime + health-check bounds)
- Admin-guard failures return 401, and the admin key is compared in constant time
- Web `App.tsx` split into shared types and presentational components
- Flyway-convention migration names; handlers unified on `AppError`; internal error detail no longer leaked to clients
- CI: golangci-lint gate, gofmt-simplify, contract-test image-build caching, Schemathesis pinned and scoped to conformance phases

### Security
- BREAKING: operator mutations (zone status/controls, spool replay, incident actions) now require `X-Admin-Key`; the request-body `actor` field is removed and the audit actor is derived from the `X-Actor` header, so an unauthenticated request can no longer write a forged audit record
- Production startup guard refuses a weak or unset `ADMIN_KEY` when `APP_ENV=production`
- Broker authentication via `NATS_CREDS` (JWT/nkey file) or an authenticated/`tls://` `NATS_URL`

### Fixed
- SQL parameter type-inference bugs (uuid binds, `jsonb_build_object` casts) across the handler write paths and the Go fraud consumer
- Omitted transfer `metadata` defaults to an empty object instead of null
- Contract-test flakes: e2e timing, Schemathesis transport retries, and transient registry pull timeouts

## [0.3.1] - 2026-04-28

### Changed
- Routine dependency maintenance across Go, Rust, and web stacks; brought transitive and direct dependencies up to current patch versions.

### Bumped
- Go: `pgx/v5` 5.8.0 to 5.9.2, `nats.go` 1.50.0 to 1.51.0, `otel` and `otel/sdk` 1.42.0 to 1.43.0, `otel/exporters/otlp/otlptrace/otlptracehttp` 1.42.0 to 1.43.0
- Rust: `tokio` 1.50.0 to 1.52.1, `axum` 0.8.8 to 0.8.9, `rand` and `rustls-webpki` to current patch versions
- Web: `react` and `react-dom` 19.2.4 to 19.2.5, `vite` 7.3.1 to 7.3.2, `typescript` 5.9.3 to 6.0.3, `postcss` 8.5.8 to 8.5.12
- CI: `actions/upload-pages-artifact` v4 to v5

## [0.3.0] - 2026-04-05

### Added
- Rust backend: full feature parity with Go (zone controls, spool, audit, incident actions, NATS messaging)
- Rust backend: modular architecture (12 source files mirroring Go's internal/ layout)
- Rust backend: structured JSON error responses via AppError enum
- Rust backend: NATS JetStream outbox publisher and fraud consumer with graceful shutdown
- Rust backend: full snapshot/restore parity (zones, controls, accounts, incidents, spool, audit)
- Schemathesis contract tests in CI validating both backends against OpenAPI spec
- CI test compose (ci/docker-compose.test.yml) for lightweight contract testing
- Comprehensive unit tests: 25 tests across Go (10) and Rust (15)
- Cross-language FNV-1a 32-bit parity tests anchoring deterministic throttle behavior
- AppError tests verifying all HTTP status codes and JSON response bodies
- Committed go.sum (removed from .gitignore)

### Changed
- Rust main.rs reduced from 674 lines to 65 lines (modular extraction)
- Rust transfer path: zone gating with controls check, deterministic throttle, spool support
- Rust transfers return 202 + SpooledResponse when zone is blocked and spool enabled
- OpenAPI spec updated to v0.3.0: fixed envelope wrapping, required fields, VersionInfo schema
- Go Dockerfile now copies go.sum for reproducible builds
- Rust Dockerfile pinned to rust:1.93-bookworm (replaced unstable rust:stable tag)

### Fixed
- FNV hash cross-language parity bug: Rust was using 64-bit FNV (fnv crate) instead of 32-bit FNV-1a matching Go. Replaced with manual implementation. Without this fix, deterministic throttling would produce different results between backends.
- tokio-util and futures incorrectly placed in dev-dependencies instead of dependencies

### Removed
- fnv crate dependency (replaced with manual FNV-1a 32-bit for Go parity)

## [0.2.0] - 2026-04-04

### Added
- Release workflow (manual workflow_dispatch with CHANGELOG extraction)
- CHANGELOG.md following Keep a Changelog format
- `justfile` polyglot task runner replacing Makefile (test, lint, build, infra, dev recipes)
- Dashboard connection health banner (persistent offline/loading indicator)
- Dashboard auto-polling with visibility-aware refresh (10s connected, 5s offline retry)
- Last-updated timestamp in dashboard toolbar
- `rust-toolchain.toml` for reproducible Rust builds
- Committed lockfiles: `web/package-lock.json`, `rust/sim/Cargo.lock`

### Changed
- Rust edition 2021 to 2024
- Rust Dockerfile runtime: `debian:bookworm-slim` to `gcr.io/distroless/cc:nonroot` (nonroot user)
- Axum route syntax: deprecated `:param` to `{param}`
- Zone map dots show gray (unknown) instead of red (DOWN) when backend is unreachable
- Dashboard initial load uses `Promise.allSettled` for partial failure resilience

### Fixed
- RUSTSEC-2026-0049 (rustls-webpki CRL bypass) by bumping async-nats 0.46 to 0.47
- grpc authorization bypass (google.golang.org/grpc 1.79.2 to 1.79.3)

### Removed
- `lazy_static` dependency (unused)
- `Makefile` (replaced by justfile)

### Security
- Bumped: async-nats, grpc, sha2, nats.go, pgx, @types/react, actions/deploy-pages

## [0.1.0] - 2025-04-04

### Added
- Double-entry ledger with integer units (1 unit = 1 second)
- 10 worldwide zones with operator-controlled status (OK/DEGRADED/DOWN)
- Fraud/ops incidents per zone with clickable details
- At-least-once messaging with NATS JetStream + transactional outbox + inbox dedup
- Go backend (primary MVP) with full feature set: transfers, zones, controls, spool, audit
- Rust backend with core ledger endpoints (transfers, zones, balances, transactions)
- React operator console (Vite + TypeScript) with zone map, blast radius visualization
- Observability: structured logs, Prometheus metrics, OpenTelemetry traces (Jaeger)
- Docker Compose dev stack (Postgres, NATS, Flyway, OTel Collector, Jaeger, Prometheus, Grafana)
- CI: build/test for Go, Rust, and web
- Security: CodeQL, govulncheck, cargo-audit, npm audit
- GitHub Pages deployment for dashboard
- OpenAPI 3.0 specification
- Snapshot/restore admin endpoints
