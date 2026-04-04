# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
