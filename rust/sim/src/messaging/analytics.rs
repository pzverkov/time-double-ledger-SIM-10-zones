use async_trait::async_trait;
use std::sync::Arc;

use super::broker::{BrokerError, EventHandler, IncomingEvent};
use super::store::{AnalyticsStore, TransferPosted};

pub const CONSUMER: &str = "analytics-v1";

/// Maintains per-zone running aggregates. Independent durable consumer group,
/// so it receives every event alongside the fraud consumer (fan-out).
pub struct AnalyticsHandler {
    store: Arc<dyn AnalyticsStore>,
}

impl AnalyticsHandler {
    pub fn new(store: Arc<dyn AnalyticsStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EventHandler for AnalyticsHandler {
    async fn handle(&self, event: &IncomingEvent) -> Result<(), BrokerError> {
        let ev: TransferPosted = serde_json::from_slice(&event.payload)?;
        let event_id = ev
            .event_id
            .clone()
            .or_else(|| event.msg_id.clone())
            .unwrap_or_default();
        if event_id.is_empty() {
            return Ok(());
        }
        let zone_id = ev.zone_id.unwrap_or_default();
        self.store
            .record_stats(&event_id, &zone_id, ev.amount_units.unwrap_or(0))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::{ZoneStat, fold_stats};
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeAnalyticsStore {
        seen: Mutex<HashSet<String>>,
        stats: Mutex<HashMap<String, ZoneStat>>,
        fail: bool,
    }

    #[async_trait]
    impl AnalyticsStore for FakeAnalyticsStore {
        async fn record_stats(
            &self,
            event_id: &str,
            zone_id: &str,
            amount: i64,
        ) -> Result<bool, BrokerError> {
            if self.fail {
                return Err("store boom".into());
            }
            if !self.seen.lock().unwrap().insert(event_id.to_string()) {
                return Ok(false);
            }
            let mut stats = self.stats.lock().unwrap();
            let entry = stats.entry(zone_id.to_string()).or_default();
            *entry = fold_stats(*entry, amount);
            Ok(true)
        }
    }

    fn event(event_id: &str, zone: &str, amount: i64) -> IncomingEvent {
        let body = serde_json::to_vec(&serde_json::json!({
            "event_id": event_id, "zone_id": zone, "amount_units": amount,
        }))
        .unwrap();
        IncomingEvent {
            msg_id: Some(event_id.to_string()),
            payload: body,
        }
    }

    #[tokio::test]
    async fn aggregates_per_zone() {
        let store = Arc::new(FakeAnalyticsStore::default());
        let h = AnalyticsHandler::new(store.clone());
        h.handle(&event("e1", "zone-eu", 100)).await.unwrap();
        h.handle(&event("e2", "zone-eu", 50)).await.unwrap();
        h.handle(&event("e3", "zone-na", 10)).await.unwrap();
        let stats = store.stats.lock().unwrap();
        assert_eq!(
            stats["zone-eu"],
            ZoneStat {
                event_count: 2,
                total_amount_units: 150
            }
        );
        assert_eq!(
            stats["zone-na"],
            ZoneStat {
                event_count: 1,
                total_amount_units: 10
            }
        );
    }

    #[tokio::test]
    async fn duplicate_does_not_double_count() {
        let store = Arc::new(FakeAnalyticsStore::default());
        let h = AnalyticsHandler::new(store.clone());
        h.handle(&event("e1", "zone-eu", 100)).await.unwrap();
        h.handle(&event("e1", "zone-eu", 100)).await.unwrap();
        assert_eq!(
            store.stats.lock().unwrap()["zone-eu"],
            ZoneStat {
                event_count: 1,
                total_amount_units: 100
            }
        );
    }

    #[tokio::test]
    async fn store_error_propagates() {
        let store = Arc::new(FakeAnalyticsStore {
            fail: true,
            ..Default::default()
        });
        let h = AnalyticsHandler::new(store);
        assert!(h.handle(&event("e1", "zone-eu", 100)).await.is_err());
    }
}
