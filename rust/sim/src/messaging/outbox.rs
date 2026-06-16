use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::broker::EventPublisher;
use super::store::OutboxStore;

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
                    // The store claims rows with FOR UPDATE SKIP LOCKED, publishes,
                    // and marks them in one transaction, so running this on several
                    // replicas does not double-publish. On error the batch rolls
                    // back and is retried next tick.
                    if let Err(e) = self.store.publish_batch(50, &*self.publisher).await {
                        warn!(error = %e, "outbox publish batch failed");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::broker::BrokerError;
    use crate::messaging::store::OutboxRow;
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
            _traceparent: Option<&str>,
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

    /// In-memory outbox that mirrors the transactional store: it publishes each
    /// claimed row and only marks it on success; any publish error aborts the
    /// batch and marks nothing (atomic rollback).
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

    fn inject(payload: &serde_json::Value, id: &str) -> serde_json::Value {
        let mut m = payload.clone();
        if let Some(o) = m.as_object_mut() {
            o.insert("event_id".into(), serde_json::json!(id));
        }
        m
    }

    #[async_trait]
    impl OutboxStore for FakeOutboxStore {
        async fn publish_batch(
            &self,
            limit: i64,
            publisher: &dyn EventPublisher,
        ) -> Result<usize, BrokerError> {
            let batch: Vec<OutboxRow> = {
                let rows = self.rows.lock().unwrap();
                rows.iter().take(limit as usize).cloned().collect()
            };
            let mut newly_marked = Vec::new();
            for row in &batch {
                let body = serde_json::to_vec(&inject(&row.payload, &row.id)).unwrap();
                // On failure, abort without recording any marks (rollback).
                publisher
                    .publish(
                        super::super::broker::SUBJECT_TRANSFER_POSTED,
                        &row.id,
                        None,
                        body,
                    )
                    .await?;
                newly_marked.push(row.id.clone());
            }
            self.marked
                .lock()
                .unwrap()
                .extend(newly_marked.iter().cloned());
            self.rows
                .lock()
                .unwrap()
                .retain(|r| !newly_marked.contains(&r.id));
            Ok(newly_marked.len())
        }
    }

    #[tokio::test]
    async fn empty_batch_is_noop() {
        let store = FakeOutboxStore::with(&[]);
        let pubr = MockPublisher::default();
        assert_eq!(store.publish_batch(50, &pubr).await.unwrap(), 0);
        assert!(store.marked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn publishes_in_order_with_id_as_msg_id() {
        let store = FakeOutboxStore::with(&["a", "b", "c"]);
        let pubr = MockPublisher::default();
        let n = store.publish_batch(50, &pubr).await.unwrap();
        assert_eq!(n, 3);
        let published = pubr.published.lock().unwrap();
        let ids: Vec<&str> = published.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]); // msg_id == outbox id, in order
        assert_eq!(published[0].1["event_id"], "a"); // placeholder rewritten
        assert_eq!(*store.marked.lock().unwrap(), vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn respects_limit() {
        let store = FakeOutboxStore::with(&["a", "b", "c"]);
        let pubr = MockPublisher::default();
        assert_eq!(store.publish_batch(2, &pubr).await.unwrap(), 2);
        assert_eq!(pubr.published.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn publish_failure_rolls_back_whole_batch() {
        let store = FakeOutboxStore::with(&["a", "b", "c"]);
        let pubr = MockPublisher {
            fail_after: Some(2),
            ..Default::default()
        };
        let res = store.publish_batch(50, &pubr).await;
        assert!(res.is_err());
        // atomic: nothing is marked even though two reached the broker; they are
        // republished next tick and dedup absorbs the repeat.
        assert!(store.marked.lock().unwrap().is_empty());
        assert_eq!(store.rows.lock().unwrap().len(), 3);
    }
}
