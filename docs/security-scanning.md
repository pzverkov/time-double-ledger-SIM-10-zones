# Security scanning

This repo includes lightweight security gates in CI:

- **CodeQL** for Go and TypeScript (static analysis)
- **Go** `govulncheck` (known vuln database scanning)
- **Rust** `cargo audit` (RustSec advisory scanning)
- **Web** `npm audit --audit-level=high` + build

Notes:
- For deterministic installs, commit lockfiles:
  - `go/go.sum`
  - `web/package-lock.json`
  - `rust/sim/Cargo.lock`

CI will still run without them, but results will be more reproducible with lockfiles.
