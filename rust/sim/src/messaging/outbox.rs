use async_nats::jetstream;
use deadpool_postgres::Pool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub struct OutboxPublisher {
    db: Pool,
    js: jetstream::Context,
}

impl OutboxPublisher {
    pub fn new(db: Pool, js: jetstream::Context) -> Self {
        Self { db, js }
    }

    pub async fn run(&self, cancel: CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = interval.tick() => {
                    if let Err(e) = self.publish_batch(50).await {
                        warn!(error = %e, "outbox publish batch failed");
                    }
                }
            }
        }
    }

    async fn publish_batch(&self, limit: i64) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.db.get().await?;
        let rows = client
            .query(
                "SELECT id::text, event_type, payload FROM outbox_events WHERE published_at IS NULL ORDER BY created_at LIMIT $1",
                &[&limit],
            )
            .await?;

        if rows.is_empty() {
            return Ok(());
        }

        for row in &rows {
            let id: String = row.get("id");
            let payload: serde_json::Value = row.get("payload");

            // replace event_id if still placeholder
            let mut m = payload;
            if let Some(obj) = m.as_object_mut() {
                let eid = obj.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
                if eid.is_empty() || eid == "generated_by_db" {
                    obj.insert("event_id".into(), serde_json::json!(id));
                }
            }
            let body = serde_json::to_vec(&m)?;

            // publish with Nats-Msg-Id for JetStream dedup
            let mut headers = async_nats::HeaderMap::new();
            headers.insert("Nats-Msg-Id", id.as_str());

            self.js
                .publish_with_headers::<String>("events.transfer_posted".into(), headers, body.into())
                .await?
                .await?;

            client
                .execute("UPDATE outbox_events SET published_at=now() WHERE id=$1::uuid", &[&id])
                .await?;
        }

        Ok(())
    }
}
