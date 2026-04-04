# Time Ledger Sim: polyglot task runner
# Install: brew install just

# Default: list recipes
default:
    @just --list

# -- Testing -------------------------------------------------------

# Run all tests
test: test-go test-rust test-web

# Run Go tests
test-go:
    cd go && go test ./...

# Run Go tests with coverage
cover-go:
    cd go && go test ./... -coverprofile=cover.out && go tool cover -func=cover.out

# Run Rust tests
test-rust:
    cd rust/sim && cargo test

# Build web (type-check + bundle)
test-web:
    cd web && npm run build

# -- Linting -------------------------------------------------------

# Lint all
lint: lint-go lint-rust

# Lint Go code
lint-go:
    cd go && go vet ./...

# Lint Rust code
lint-rust:
    cd rust/sim && cargo clippy -- -D warnings

# -- Building ------------------------------------------------------

# Build all
build: build-go build-rust

# Build Go binary
build-go:
    cd go && go build -o ../out/sim-go ./cmd/sim-go

# Build Rust binary (release)
build-rust:
    cd rust/sim && cargo build --release

# -- Infrastructure ------------------------------------------------

# Start dev infrastructure (Docker Compose)
infra-up:
    cd infra && docker compose up -d --build

# Stop dev infrastructure
infra-down:
    cd infra && docker compose down

# Show infrastructure logs
infra-logs:
    cd infra && docker compose logs -f

# Run database migrations via Flyway
migrate:
    cd infra && docker compose up migrate --build

# -- Development ---------------------------------------------------

# Start web dev server
dev-web:
    cd web && npm run dev

# -- Lockfiles -----------------------------------------------------

# Generate/update all lockfiles
lockfiles:
    cd web && npm install --package-lock-only
    cd rust/sim && cargo generate-lockfile
