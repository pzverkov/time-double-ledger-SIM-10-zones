use async_nats::jetstream;
use deadpool_postgres::Pool;
use serde::Deserialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::streams::STREAM_NAME;

pub struct FraudConsumer {
    db: Pool,
    js: jetstream::Context,
}

#[derive(Deserialize)]
struct TransferPosted {
    event_id: Option<String>,
    transaction_id: Option<String>,
    zone_id: Option<String>,
    amount_units: Option<i64>,
}

impl FraudConsumer {
    pub fn new(db: Pool, js: jetstream::Context) -> Self {
        Self { db, js }
    }

    pub async fn run(&self, cancel: CancellationToken) {
        let consumer = match self.create_consumer().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "fraud consumer setup failed, skipping");
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
                    use futures::StreamExt;
                    while let Some(msg_result) = msgs.next().await {
                        if let Ok(msg) = msg_result {
                            let _ = self.handle_msg(&msg).await;
                            let _ = msg.ack().await;
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "fraud fetch failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn create_consumer(
        &self,
    ) -> Result<jetstream::consumer::PullConsumer, async_nats::Error> {
        let stream = self.js.get_stream(STREAM_NAME).await?;
        let consumer = stream
            .get_or_create_consumer(
                "fraud-v1",
                jetstream::consumer::pull::Config {
                    durable_name: Some("fraud-v1".into()),
                    filter_subject: "events.transfer_posted".into(),
                    ..Default::default()
                },
            )
            .await?;
        Ok(consumer)
    }

    async fn handle_msg(
        &self,
        msg: &async_nats::jetstream::message::Message,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ev: TransferPosted = serde_json::from_slice(&msg.payload)?;

        let event_id = ev
            .event_id
            .or_else(|| {
                msg.headers
                    .as_ref()
                    .and_then(|h| h.get("Nats-Msg-Id"))
                    .map(|v| v.to_string())
            })
            .unwrap_or_default();

        if event_id.is_empty() {
            return Ok(());
        }

        let client = self.db.get().await?;

        // inbox dedup
        client
            .execute(
                "INSERT INTO inbox_events(consumer,event_id) VALUES('fraud-v1',$1::uuid) ON CONFLICT DO NOTHING",
                &[&event_id],
            )
            .await?;

        // fraud rule: large transfer (>= 1 hour in seconds)
        if ev.amount_units.unwrap_or(0) >= 3600 {
            let zone_id = ev.zone_id.unwrap_or_default();
            let txn_id = ev.transaction_id.unwrap_or_default();
            let amount = ev.amount_units.unwrap_or(0);
            client
                .execute(
                    "INSERT INTO incidents(zone_id, related_txn_id, severity, title, details) VALUES($1, $2::uuid, 'WARN', 'Large time transfer', jsonb_build_object('amount_units',$3,'rule','large_transfer'))",
                    &[&zone_id, &txn_id, &amount],
                )
                .await?;
        }

        Ok(())
    }
}
