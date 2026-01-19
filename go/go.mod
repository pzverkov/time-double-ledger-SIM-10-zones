module time-ledger-sim/go

go 1.25

toolchain go1.25.5

require (
  github.com/go-chi/chi/v5 v5.2.3
  github.com/jackc/pgx/v5 v5.7.5
  github.com/nats-io/nats.go v1.48.0
  github.com/prometheus/client_golang v1.23.2
  go.opentelemetry.io/otel v1.39.0
  go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp v1.39.0
  go.opentelemetry.io/otel/sdk v1.39.0
  go.opentelemetry.io/otel/trace v1.39.0
)
