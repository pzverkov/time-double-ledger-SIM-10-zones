use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Broker-agnostic error.
pub type BrokerError = Box<dyn std::error::Error + Send + Sync>;

/// Subject events are published to and consumed from.
pub const SUBJECT_TRANSFER_POSTED: &str = "events.transfer_posted";

/// Bound on redeliveries before a message is treated as poison.
pub const MAX_DELIVER: i64 = 5;

/// An event delivered to a consumer.
pub struct IncomingEvent {
    /// Dedup key. NATS carries it in the `Nats-Msg-Id` header; Kafka in the record key.
    pub msg_id: Option<String>,
    /// W3C `traceparent` carried as a broker header, used to link the consumer
    /// span to the originating request's trace. `None` when tracing is off.
    pub traceparent: Option<String>,
    pub payload: Vec<u8>,
}

/// Publishes events to the broker.
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish `body` to `subject`, carrying `msg_id` for broker/consumer dedup
    /// and an optional W3C `traceparent` header for trace propagation.
    async fn publish(
        &self,
        subject: &str,
        msg_id: &str,
        traceparent: Option<&str>,
        body: Vec<u8>,
    ) -> Result<(), BrokerError>;
}

/// Business logic for a single event. Broker-agnostic.
///
/// Returning `Ok(())` tells the consumer the event is processed and may be
/// acked/committed. Returning `Err` leaves the event un-acked so the broker
/// redelivers it; inbox dedup makes that retry safe.
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: &IncomingEvent) -> Result<(), BrokerError>;
}

/// Owns the fetch/poll + ack/commit loop for one consumer group.
#[async_trait]
pub trait EventConsumer: Send + Sync {
    /// Consume until `cancel` fires, invoking `handler` per message.
    /// Acks on `Ok`, leaves un-acked on `Err` (bounded by `MAX_DELIVER`).
    async fn run(&self, handler: Arc<dyn EventHandler>, cancel: CancellationToken);
}
