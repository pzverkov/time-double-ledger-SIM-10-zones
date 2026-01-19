package app

import (
  "context"
  "time"

  "go.opentelemetry.io/otel"
  "go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp"
  "go.opentelemetry.io/otel/sdk/resource"
  sdktrace "go.opentelemetry.io/otel/sdk/trace"
  semconv "go.opentelemetry.io/otel/semconv/v1.30.0"
)

func initTracer(ctx context.Context, endpoint string) (func(context.Context) error, error) {
  if endpoint == "" {
    tp := sdktrace.NewTracerProvider()
    otel.SetTracerProvider(tp)
    return tp.Shutdown, nil
  }
  exp, err := otlptracehttp.New(ctx, otlptracehttp.WithEndpointURL(endpoint))
  if err != nil {
    return nil, err
  }
  tp := sdktrace.NewTracerProvider(
    sdktrace.WithBatcher(exp, sdktrace.WithBatchTimeout(2*time.Second)),
    sdktrace.WithResource(resource.NewWithAttributes(
      semconv.SchemaURL,
      semconv.ServiceName("time-ledger-sim-go"),
    )),
  )
  otel.SetTracerProvider(tp)
  return tp.Shutdown, nil
}
