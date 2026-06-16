//! OpenTelemetry tracing setup. Optional: a provider is built only when
//! `OTEL_EXPORTER_OTLP_ENDPOINT` is set; otherwise the app runs logging-only and
//! the global propagator stays a no-op, so trace-context capture/extraction are
//! safe no-ops.

use opentelemetry_sdk::trace::SdkTracerProvider;

/// Build an OTLP tracer provider and install the W3C trace-context propagator.
/// Must run inside the tokio runtime (the exporter uses an HTTP client). Returns
/// the provider so the caller can flush it on shutdown.
pub fn build_provider(endpoint: &str) -> Result<SdkTracerProvider, Box<dyn std::error::Error>> {
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name("time-ledger-sim")
                .build(),
        )
        .build();

    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );

    Ok(provider)
}

/// Inject the current span's trace context into a `traceparent` string. Returns
/// `None` when there is no active OTel context (e.g. OTel disabled), so callers
/// store NULL rather than an empty/invalid header.
pub fn current_traceparent() -> Option<String> {
    use opentelemetry::trace::TraceContextExt;
    use std::collections::HashMap;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let cx = tracing::Span::current().context();
    if !cx.span().span_context().is_valid() {
        return None;
    }
    let mut carrier = HashMap::new();
    opentelemetry::global::get_text_map_propagator(|prop| {
        prop.inject_context(&cx, &mut carrier);
    });
    carrier.remove("traceparent")
}

/// Build an OTel context from a `traceparent` header for use as a span parent.
/// An absent or malformed value yields an empty context (a no-op parent).
pub fn context_from_traceparent(traceparent: Option<&str>) -> opentelemetry::Context {
    use std::collections::HashMap;
    let mut carrier = HashMap::new();
    if let Some(tp) = traceparent {
        carrier.insert("traceparent".to_string(), tp.to_string());
    }
    opentelemetry::global::get_text_map_propagator(|prop| prop.extract(&carrier))
}
