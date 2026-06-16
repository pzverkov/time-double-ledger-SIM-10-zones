//! Kafka-API (Redpanda) implementation of the broker traits, behind the
//! `redpanda` feature. NATS stays the default; this exists to prove the broker
//! seam is real and to back the NATS-vs-Redpanda benchmark.
//!
//! Idempotency note: Kafka has no built-in publish dedup like NATS `Nats-Msg-Id`.
//! Correctness here relies entirely on the inbox table (`record_fraud` /
//! `record_stats` claim atomically), which is exactly why that pattern is
//! broker-agnostic and already in place.

use async_trait::async_trait;
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::{Offset, TopicPartitionList};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::broker::{
    BrokerError, EventConsumer, EventHandler, EventPublisher, IncomingEvent, MAX_DELIVER,
    SUBJECT_TRANSFER_POSTED,
};
use super::store::DeadLetterStore;
use super::{analytics, fraud};

type Publisher = Arc<dyn EventPublisher>;
type ConsumerArc = Arc<dyn EventConsumer>;

/// A subject maps 1:1 to a Kafka topic name (dots are valid in Kafka topics).
pub fn subject_to_topic(subject: &str) -> &str {
    subject
}

/// Build a publisher and the two consumer groups against `brokers`.
pub async fn build(
    brokers: &str,
    dlq: Arc<dyn DeadLetterStore>,
) -> Result<(Publisher, ConsumerArc, ConsumerArc), BrokerError> {
    let topic = subject_to_topic(SUBJECT_TRANSFER_POSTED).to_string();
    ensure_topic(brokers, &topic).await?;

    let publisher: Publisher = Arc::new(RedpandaPublisher::new(brokers)?);
    let fraud_c: ConsumerArc = Arc::new(RedpandaConsumer::new(
        brokers,
        fraud::CONSUMER,
        &topic,
        dlq.clone(),
    )?);
    let analytics_c: ConsumerArc = Arc::new(RedpandaConsumer::new(
        brokers,
        analytics::CONSUMER,
        &topic,
        dlq,
    )?);
    Ok((publisher, fraud_c, analytics_c))
}

async fn ensure_topic(brokers: &str, topic: &str) -> Result<(), BrokerError> {
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .create()?;
    let new = NewTopic::new(topic, 1, TopicReplication::Fixed(1));
    // Ignore "already exists"; surface anything else.
    let res = admin.create_topics([&new], &AdminOptions::new()).await?;
    for r in res {
        if let Err((name, err)) = r
            && !format!("{err:?}").contains("TopicAlreadyExists")
        {
            warn!(topic = %name, error = ?err, "create_topics returned error");
        }
    }
    Ok(())
}

pub struct RedpandaPublisher {
    producer: FutureProducer,
}

impl RedpandaPublisher {
    pub fn new(brokers: &str) -> Result<Self, BrokerError> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl EventPublisher for RedpandaPublisher {
    async fn publish(
        &self,
        subject: &str,
        msg_id: &str,
        traceparent: Option<&str>,
        body: Vec<u8>,
    ) -> Result<(), BrokerError> {
        let topic = subject_to_topic(subject);
        let mut record = FutureRecord::to(topic).key(msg_id).payload(&body);
        if let Some(tp) = traceparent {
            record = record.headers(rdkafka::message::OwnedHeaders::new().insert(
                rdkafka::message::Header {
                    key: "traceparent",
                    value: Some(tp),
                },
            ));
        }
        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| Box::new(e) as BrokerError)?;
        Ok(())
    }
}

pub struct RedpandaConsumer {
    consumer: StreamConsumer,
    topic: String,
    group_id: String,
    dlq: Arc<dyn DeadLetterStore>,
}

impl RedpandaConsumer {
    pub fn new(
        brokers: &str,
        group_id: &str,
        topic: &str,
        dlq: Arc<dyn DeadLetterStore>,
    ) -> Result<Self, BrokerError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "earliest")
            .create()?;
        consumer.subscribe(&[topic])?;
        Ok(Self {
            consumer,
            topic: topic.to_string(),
            group_id: group_id.to_string(),
            dlq,
        })
    }

    fn commit_offset(&self, partition: i32, offset: i64) -> Result<(), BrokerError> {
        let mut tpl = TopicPartitionList::new();
        tpl.add_partition_offset(&self.topic, partition, Offset::Offset(offset + 1))?;
        self.consumer.commit(&tpl, CommitMode::Sync)?;
        Ok(())
    }
}

#[async_trait]
impl EventConsumer for RedpandaConsumer {
    async fn run(&self, handler: Arc<dyn EventHandler>, cancel: CancellationToken) {
        use rdkafka::message::{Headers, Message};
        use tracing::Instrument;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        // Bound reprocessing of a poison message at one offset.
        let mut last_offset: i64 = -1;
        let mut attempts: i64 = 0;
        loop {
            let msg = tokio::select! {
                _ = cancel.cancelled() => return,
                r = self.consumer.recv() => match r {
                    Ok(m) => m.detach(),
                    Err(e) => {
                        warn!(error = %e, "redpanda recv failed");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            };

            let partition = msg.partition();
            let offset = msg.offset();
            let traceparent = msg.headers().and_then(|hs| {
                hs.iter()
                    .find(|h| h.key == "traceparent")
                    .and_then(|h| h.value)
                    .map(|v| String::from_utf8_lossy(v).into_owned())
            });
            let event = IncomingEvent {
                msg_id: msg.key().map(|k| String::from_utf8_lossy(k).into_owned()),
                traceparent,
                payload: msg.payload().map(|p| p.to_vec()).unwrap_or_default(),
            };

            let span = tracing::info_span!("consume", topic = %self.topic);
            let _ = span.set_parent(crate::otel::context_from_traceparent(
                event.traceparent.as_deref(),
            ));
            match handler.handle(&event).instrument(span).await {
                Ok(()) => {
                    if let Err(e) = self.commit_offset(partition, offset) {
                        warn!(error = %e, "commit failed");
                    }
                    last_offset = -1;
                    attempts = 0;
                }
                Err(e) => {
                    attempts = if offset == last_offset {
                        attempts + 1
                    } else {
                        1
                    };
                    last_offset = offset;
                    if attempts >= MAX_DELIVER {
                        // Dead-letter, then commit past it. If the DLQ write fails,
                        // leave the offset uncommitted so it is retried, not lost.
                        match self
                            .dlq
                            .dead_letter(
                                &self.group_id,
                                event.msg_id.as_deref(),
                                &event.payload,
                                &e.to_string(),
                                attempts,
                            )
                            .await
                        {
                            Ok(()) => {
                                warn!(error = %e, offset, attempts, "poison message dead-lettered");
                                let _ = self.commit_offset(partition, offset);
                                last_offset = -1;
                                attempts = 0;
                            }
                            Err(dlq_err) => {
                                warn!(error = %dlq_err, offset, "dead-letter write failed, will retry");
                            }
                        }
                    } else {
                        warn!(error = %e, offset, attempts, "handler failed, seeking back to reprocess");
                        let _ = self.consumer.seek(
                            &self.topic,
                            partition,
                            Offset::Offset(offset),
                            Duration::from_secs(5),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_maps_to_topic_identity() {
        assert_eq!(
            subject_to_topic("events.transfer_posted"),
            "events.transfer_posted"
        );
    }
}
