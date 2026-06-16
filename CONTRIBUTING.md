# Contributing

Thanks for contributing. This repo is a monorepo with two backends (Go, Rust), a
React dashboard, and a shared OpenAPI contract.

## Prerequisites

- Go 1.26+
- Rust stable (edition 2024)
- Node 24+
- Docker + Docker Compose
- [`just`](https://github.com/casey/just) (task runner)

## Common tasks

```bash
just            # list all recipes
just test       # Go + Rust + web tests
just lint       # gofmt/go vet + cargo fmt/clippy
just build      # build all backends
just infra-up   # start Postgres, NATS, Flyway, both backends, observability
just infra-down # stop the stack
just migrate    # run DB migrations via Flyway
just dev-web    # Vite dev server
```

## Testing layers

- Unit + in-memory: `cargo test` (Rust), `go test ./...` (Go).
- Redpanda feature: `cargo test --features redpanda`.
- End-to-end pipeline (needs the stack up): `bash scripts/e2e/transfers_e2e.sh`.
- Load / throughput: `scripts/load/transfers.k6.js`, `scripts/load/pipeline_lag.sh`
  (see [docs/benchmarks.md](docs/benchmarks.md)).

## Conventions

- **ASCII only** in all tracked files. No curly quotes, em/en dashes, or other
  Unicode (use `->`, `"`, `...`). A local pre-commit-style guard can enforce this.
- **Formatting/lint are gated in CI**: `cargo fmt --check`, `cargo clippy --all-targets --features redpanda -- -D warnings`,
  `gofmt -s`, `go vet`, and `golangci-lint`. Run `just lint` before pushing.
- The API contract is `api/openapi.yaml`; both backends must conform (CI runs
  Schemathesis against it).
- Keep changes small and focused; split refactors from feature work.

## Commit and PR style

- Commit titles: plain lowercase imperative, no `type:` / `type(scope):` prefix.
  Write `add zone throttle constant`, not `feat(zones): ...`.
- No conventional-commit prefixes, no AI/co-author trailers.
- Keep PR bodies concise: what changed and why. No boilerplate test-plan blocks.
- Reference issues/benchmarks where relevant.

## Project docs

- [Architecture](docs/architecture.md)
- [Go vs Rust parity](docs/parity-matrix.md)
- [Messaging ADR](docs/adr/0001-messaging-broker.md)
- [Benchmarks](docs/benchmarks.md)
- [Security policy](SECURITY.md)
