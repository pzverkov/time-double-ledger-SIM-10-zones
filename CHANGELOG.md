# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
