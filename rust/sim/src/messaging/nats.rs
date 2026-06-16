use async_nats::jetstream;
use async_trait::async_trait;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::broker::{
    BrokerError, EventConsumer, EventHandler, EventPublisher, IncomingEvent, MAX_DELIVER,
};

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
    async fn publish(&self, subject: &str, msg_id: &str, body: Vec<u8>) -> Result<(), BrokerError> {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("Nats-Msg-Id", msg_id);
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
}

impl NatsConsumer {
    pub fn new(
        js: jetstream::Context,
        durable: impl Into<String>,
        filter_subject: impl Into<String>,
    ) -> Self {
        Self {
            js,
            durable: durable.into(),
            filter_subject: filter_subject.into(),
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
                        let event = IncomingEvent {
                            msg_id: msg
                                .headers
                                .as_ref()
                                .and_then(|h| h.get("Nats-Msg-Id"))
                                .map(|v| v.to_string()),
                            payload: msg.payload.to_vec(),
                        };
                        match handler.handle(&event).await {
                            Ok(()) => {
                                let _ = msg.ack().await;
                            }
                            Err(e) => {
                                // Leave un-acked: JetStream redelivers up to MAX_DELIVER,
                                // then drops. Never silent.
                                let delivered = msg.info().map(|i| i.delivered).unwrap_or(0);
                                warn!(
                                    error = %e,
                                    durable = %self.durable,
                                    delivered,
                                    "handler failed, leaving message for redelivery"
                                );
                                // Nak with a short delay so a persistently-failing
                                // handler backs off instead of hot-looping to max_deliver.
                                let _ = msg
                                    .ack_with(jetstream::AckKind::Nak(Some(Duration::from_secs(1))))
                                    .await;
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
