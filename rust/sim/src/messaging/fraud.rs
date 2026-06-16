use async_trait::async_trait;
use std::sync::Arc;

use super::broker::{BrokerError, EventHandler, IncomingEvent};
use super::store::{FraudStore, Incident, TransferPosted, fraud_verdict};

pub const CONSUMER: &str = "fraud-v1";

/// Flags large time transfers as incidents. Idempotent via inbox dedup.
pub struct FraudHandler {
    store: Arc<dyn FraudStore>,
}

impl FraudHandler {
    pub fn new(store: Arc<dyn FraudStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EventHandler for FraudHandler {
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

        let incident = fraud_verdict(ev.amount_units).then(|| Incident {
            zone_id: ev.zone_id.unwrap_or_default(),
            txn_id: ev.transaction_id.unwrap_or_default(),
            amount_units: ev.amount_units.unwrap_or(0),
        });

        // Claim + write are atomic in the store; a duplicate returns false (no-op).
        self.store
            .record_fraud(&event_id, incident.as_ref())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeFraudStore {
        seen: Mutex<HashSet<String>>,
        incidents: Mutex<Vec<Incident>>,
        fail: bool,
    }

    #[async_trait]
    impl FraudStore for FakeFraudStore {
        async fn record_fraud(
            &self,
            event_id: &str,
            incident: Option<&Incident>,
        ) -> Result<bool, BrokerError> {
            if self.fail {
                return Err("store boom".into());
            }
            if !self.seen.lock().unwrap().insert(event_id.to_string()) {
                return Ok(false); // duplicate
            }
            if let Some(inc) = incident {
                self.incidents.lock().unwrap().push(inc.clone());
            }
            Ok(true)
        }
    }

    fn event(event_id: &str, zone: &str, amount: i64) -> IncomingEvent {
        let body = serde_json::to_vec(&serde_json::json!({
            "event_id": event_id,
            "transaction_id": "11111111-1111-1111-1111-111111111111",
            "zone_id": zone,
            "amount_units": amount,
        }))
        .unwrap();
        IncomingEvent {
            msg_id: Some(event_id.to_string()),
            payload: body,
        }
    }

    #[tokio::test]
    async fn large_transfer_records_one_incident() {
        let store = Arc::new(FakeFraudStore::default());
        let h = FraudHandler::new(store.clone());
        h.handle(&event("e1", "zone-eu", 5000)).await.unwrap();
        let inc = store.incidents.lock().unwrap();
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].amount_units, 5000);
        assert_eq!(inc[0].zone_id, "zone-eu");
    }

    #[tokio::test]
    async fn small_transfer_records_no_incident_but_claims() {
        let store = Arc::new(FakeFraudStore::default());
        let h = FraudHandler::new(store.clone());
        h.handle(&event("e1", "zone-eu", 100)).await.unwrap();
        assert_eq!(store.incidents.lock().unwrap().len(), 0);
        assert!(store.seen.lock().unwrap().contains("e1")); // still deduped
    }

    #[tokio::test]
    async fn duplicate_delivery_is_idempotent() {
        let store = Arc::new(FakeFraudStore::default());
        let h = FraudHandler::new(store.clone());
        h.handle(&event("e1", "zone-eu", 5000)).await.unwrap();
        h.handle(&event("e1", "zone-eu", 5000)).await.unwrap(); // redelivery
        assert_eq!(store.incidents.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn store_error_propagates_for_redelivery() {
        let store = Arc::new(FakeFraudStore {
            fail: true,
            ..Default::default()
        });
        let h = FraudHandler::new(store);
        let err = h.handle(&event("e1", "zone-eu", 5000)).await;
        assert!(err.is_err()); // consumer will not ack -> redelivered
    }

    #[tokio::test]
    async fn missing_event_id_is_dropped() {
        let store = Arc::new(FakeFraudStore::default());
        let h = FraudHandler::new(store.clone());
        let body = serde_json::to_vec(&serde_json::json!({"amount_units": 5000})).unwrap();
        h.handle(&IncomingEvent {
            msg_id: None,
            payload: body,
        })
        .await
        .unwrap();
        assert_eq!(store.incidents.lock().unwrap().len(), 0);
        assert!(store.seen.lock().unwrap().is_empty());
    }
}
