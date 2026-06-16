use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::broker::{BrokerError, EventPublisher, SUBJECT_TRANSFER_POSTED};
use super::store::{OutboxRow, OutboxStore};

pub struct OutboxPublisher {
    store: Arc<dyn OutboxStore>,
    publisher: Arc<dyn EventPublisher>,
}

impl OutboxPublisher {
    pub fn new(store: Arc<dyn OutboxStore>, publisher: Arc<dyn EventPublisher>) -> Self {
        Self { store, publisher }
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

    async fn publish_batch(&self, limit: i64) -> Result<(), BrokerError> {
        let rows = self.store.fetch_unpublished(limit).await?;
        for row in rows {
            let body = serde_json::to_vec(&inject_event_id(row.payload, &row.id))?;
            // On publish failure, stop the batch: remaining rows stay unpublished
            // and are retried on the next tick (at-least-once).
            self.publisher
                .publish(SUBJECT_TRANSFER_POSTED, &row.id, body)
                .await?;
            self.store.mark_published(&row.id).await?;
        }
        Ok(())
    }
}

impl OutboxRow {
    #[cfg(test)]
    pub fn new(id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            payload,
        }
    }
}

/// Replace a placeholder `event_id` in the payload with the outbox row id so the
/// consumer-side dedup key matches the publish dedup key.
fn inject_event_id(payload: serde_json::Value, id: &str) -> serde_json::Value {
    let mut m = payload;
    if let Some(obj) = m.as_object_mut() {
        let eid = obj.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
        if eid.is_empty() || eid == "generated_by_db" {
            obj.insert("event_id".into(), serde_json::json!(id));
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockPublisher {
        published: Mutex<Vec<(String, serde_json::Value)>>, // (msg_id, body)
        fail_after: Option<usize>,
    }

    #[async_trait]
    impl EventPublisher for MockPublisher {
        async fn publish(
            &self,
            _subject: &str,
            msg_id: &str,
            body: Vec<u8>,
        ) -> Result<(), BrokerError> {
            let mut p = self.published.lock().unwrap();
            if let Some(k) = self.fail_after
                && p.len() >= k
            {
                return Err("publish boom".into());
            }
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            p.push((msg_id.to_string(), v));
            Ok(())
        }
    }

    struct FakeOutboxStore {
        rows: Mutex<Vec<OutboxRow>>,
        marked: Mutex<Vec<String>>,
    }

    impl FakeOutboxStore {
        fn with(ids: &[&str]) -> Self {
            let rows = ids
                .iter()
                .map(|id| {
                    OutboxRow::new(
                        *id,
                        serde_json::json!({"event_id": "generated_by_db", "amount_units": 1}),
                    )
                })
                .collect();
            Self {
                rows: Mutex::new(rows),
                marked: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl OutboxStore for FakeOutboxStore {
        async fn fetch_unpublished(&self, limit: i64) -> Result<Vec<OutboxRow>, BrokerError> {
            let rows = self.rows.lock().unwrap();
            Ok(rows.iter().take(limit as usize).cloned().collect())
        }
        async fn mark_published(&self, id: &str) -> Result<(), BrokerError> {
            self.marked.lock().unwrap().push(id.to_string());
            self.rows.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn empty_batch_is_noop() {
        let store = Arc::new(FakeOutboxStore::with(&[]));
        let pubr = Arc::new(MockPublisher::default());
        let p = OutboxPublisher::new(store.clone(), pubr.clone());
        p.publish_batch(50).await.unwrap();
        assert!(pubr.published.lock().unwrap().is_empty());
        assert!(store.marked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn publishes_in_order_with_id_as_msg_id() {
        let store = Arc::new(FakeOutboxStore::with(&["a", "b", "c"]));
        let pubr = Arc::new(MockPublisher::default());
        let p = OutboxPublisher::new(store.clone(), pubr.clone());
        p.publish_batch(50).await.unwrap();
        let published = pubr.published.lock().unwrap();
        let ids: Vec<&str> = published.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]); // msg_id == outbox id, in order
        // event_id placeholder rewritten to the row id
        assert_eq!(published[0].1["event_id"], "a");
        assert_eq!(*store.marked.lock().unwrap(), vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn respects_limit() {
        let store = Arc::new(FakeOutboxStore::with(&["a", "b", "c"]));
        let pubr = Arc::new(MockPublisher::default());
        let p = OutboxPublisher::new(store, pubr.clone());
        p.publish_batch(2).await.unwrap();
        assert_eq!(pubr.published.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn publish_failure_stops_batch_leaving_rest_unpublished() {
        let store = Arc::new(FakeOutboxStore::with(&["a", "b", "c"]));
        let pubr = Arc::new(MockPublisher {
            fail_after: Some(2),
            ..Default::default()
        });
        let p = OutboxPublisher::new(store.clone(), pubr.clone());
        let res = p.publish_batch(50).await;
        assert!(res.is_err());
        // first two published + marked; third (and the rest) remain for retry
        assert_eq!(pubr.published.lock().unwrap().len(), 2);
        assert_eq!(*store.marked.lock().unwrap(), vec!["a", "b"]);
        assert_eq!(store.rows.lock().unwrap().len(), 1); // "c" still unpublished
    }

    #[test]
    fn inject_event_id_replaces_placeholder() {
        let p = serde_json::json!({"event_id": "generated_by_db", "amount_units": 10});
        let out = inject_event_id(p, "abc-123");
        assert_eq!(out["event_id"], "abc-123");
    }

    #[test]
    fn inject_event_id_replaces_empty() {
        let p = serde_json::json!({"event_id": "", "amount_units": 10});
        let out = inject_event_id(p, "abc-123");
        assert_eq!(out["event_id"], "abc-123");
    }

    #[test]
    fn inject_event_id_keeps_existing() {
        let p = serde_json::json!({"event_id": "real-id", "amount_units": 10});
        let out = inject_event_id(p, "abc-123");
        assert_eq!(out["event_id"], "real-id");
    }
}
