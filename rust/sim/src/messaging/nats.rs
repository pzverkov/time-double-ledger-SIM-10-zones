use async_nats::jetstream;
use async_trait::async_trait;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::warn;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::broker::{
    BrokerError, EventConsumer, EventHandler, EventPublisher, IncomingEvent, MAX_DELIVER,
};
use super::store::DeadLetterStore;

pub const STREAM_NAME: &str = "EVENTS";

/// Create the EVENTS stream if it does not exist.
pub async fn ensure_streams(js: &jetstream::Context) -> Result<(), async_nats::Error> {
    js.get_or_create_stream(jetstream::stream::Config {
        name: STREAM_NAME.into(),
        subjects: vec!["events.>".into()],
        storage: jetstream::stream::StorageType::File,
        retention: jetstream::stream::RetentionPolicy::Limits,
        max_messages_per_subject: 1_000_000,
        discard: jetstream::stream::DiscardPolicy::Old,
        duplicate_window: Duration::from_secs(120),
        ..Default::default()
    })
    .await?;
    Ok(())
}

/// Publishes events to NATS JetStream, carrying `msg_id` as the `Nats-Msg-Id`
/// header so JetStream deduplicates within its duplicate window.
pub struct NatsPublisher {
    js: jetstream::Context,
}

impl NatsPublisher {
    pub fn new(js: jetstream::Context) -> Self {
        Self { js }
    }
}

#[async_trait]
impl EventPublisher for NatsPublisher {
    async fn publish(
        &self,
        subject: &str,
        msg_id: &str,
        traceparent: Option<&str>,
        body: Vec<u8>,
    ) -> Result<(), BrokerError> {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("Nats-Msg-Id", msg_id);
        if let Some(tp) = traceparent {
            headers.insert("traceparent", tp);
        }
        self.js
            .publish_with_headers(subject.to_string(), headers, body.into())
            .await?
            .await?;
        Ok(())
    }
}

/// A durable pull consumer on the EVENTS stream.
pub struct NatsConsumer {
    js: jetstream::Context,
    durable: String,
    filter_subject: String,
    dlq: Arc<dyn DeadLetterStore>,
}

impl NatsConsumer {
    pub fn new(
        js: jetstream::Context,
        durable: impl Into<String>,
        filter_subject: impl Into<String>,
        dlq: Arc<dyn DeadLetterStore>,
    ) -> Self {
        Self {
            js,
            durable: durable.into(),
            filter_subject: filter_subject.into(),
            dlq,
        }
    }

    async fn create_consumer(
        &self,
    ) -> Result<jetstream::consumer::PullConsumer, async_nats::Error> {
        let stream = self.js.get_stream(STREAM_NAME).await?;
        let consumer = stream
            .get_or_create_consumer(
                &self.durable,
                jetstream::consumer::pull::Config {
                    durable_name: Some(self.durable.clone()),
                    filter_subject: self.filter_subject.clone(),
                    max_deliver: MAX_DELIVER,
                    ..Default::default()
                },
            )
            .await?;
        Ok(consumer)
    }
}

#[async_trait]
impl EventConsumer for NatsConsumer {
    async fn run(&self, handler: Arc<dyn EventHandler>, cancel: CancellationToken) {
        let consumer = match self.create_consumer().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, durable = %self.durable, "consumer setup failed, skipping");
                return;
            }
        };

        loop {
            if cancel.is_cancelled() {
                return;
            }

            let batch = consumer
                .fetch()
                .max_messages(10)
                .expires(Duration::from_secs(1))
                .messages()
                .await;

            match batch {
                Ok(mut msgs) => {
                    while let Some(Ok(msg)) = msgs.next().await {
                        let header = |k| {
                            msg.headers
                                .as_ref()
                                .and_then(|h| h.get(k))
                                .map(|v| v.to_string())
                        };
                        let event = IncomingEvent {
                            msg_id: header("Nats-Msg-Id"),
                            traceparent: header("traceparent"),
                            payload: msg.payload.to_vec(),
                        };
                        // Link this handler span to the originating trace.
                        let span = tracing::info_span!("consume", durable = %self.durable);
                        // Best-effort: fails only when no OTel layer is active (tracing off).
                        let _ = span.set_parent(crate::otel::context_from_traceparent(
                            event.traceparent.as_deref(),
                        ));
                        match handler.handle(&event).instrument(span).await {
                            Ok(()) => {
                                let _ = msg.ack().await;
                            }
                            Err(e) => {
                                let delivered = msg.info().map(|i| i.delivered).unwrap_or(0);
                                if delivered >= MAX_DELIVER {
                                    // Last allowed delivery: dead-letter and ack to
                                    // terminate (naking here would drop it unrecorded).
                                    match self
                                        .dlq
                                        .dead_letter(
                                            &self.durable,
                                            event.msg_id.as_deref(),
                                            &event.payload,
                                            &e.to_string(),
                                            delivered,
                                        )
                                        .await
                                    {
                                        Ok(()) => {
                                            warn!(error = %e, durable = %self.durable, delivered, "poison message dead-lettered");
                                            let _ = msg.ack().await;
                                        }
                                        Err(dlq_err) => {
                                            // Do not ack: redeliver rather than lose it.
                                            warn!(error = %dlq_err, durable = %self.durable, "dead-letter write failed, leaving message for redelivery");
                                            let _ = msg
                                                .ack_with(jetstream::AckKind::Nak(Some(
                                                    Duration::from_secs(1),
                                                )))
                                                .await;
                                        }
                                    }
                                } else {
                                    // Nak with a short delay so a persistently-failing
                                    // handler backs off instead of hot-looping.
                                    warn!(error = %e, durable = %self.durable, delivered, "handler failed, leaving message for redelivery");
                                    let _ = msg
                                        .ack_with(jetstream::AckKind::Nak(Some(
                                            Duration::from_secs(1),
                                        )))
                                        .await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, durable = %self.durable, "fetch failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
