//! Exercises the EventPublisher/EventConsumer/EventHandler contract with an
//! in-memory broker double: fan-out to independent consumer groups and
//! redelivery-on-error. The real NATS/Redpanda redelivery is covered by the
//! integration layer (requires a live broker).

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use time_ledger_sim_rust::messaging::broker::{
    BrokerError, EventConsumer, EventHandler, EventPublisher, IncomingEvent, MAX_DELIVER,
};

/// Append-only log shared by a publisher and N consumer groups.
type LogEntries = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

#[derive(Default, Clone)]
struct Log(LogEntries);

#[async_trait]
impl EventPublisher for Log {
    async fn publish(
        &self,
        _subject: &str,
        msg_id: &str,
        _traceparent: Option<&str>,
        body: Vec<u8>,
    ) -> Result<(), BrokerError> {
        self.0.lock().unwrap().push((msg_id.to_string(), body));
        Ok(())
    }
}

/// One consumer group: its own cursor over the shared log, so each group sees
/// every event (fan-out). Re-reads an offset on handler error, bounded by MAX_DELIVER.
struct GroupConsumer {
    log: Log,
}

#[async_trait]
impl EventConsumer for GroupConsumer {
    async fn run(&self, handler: Arc<dyn EventHandler>, cancel: CancellationToken) {
        let mut idx = 0usize;
        let mut attempts = 0i64;
        loop {
            if cancel.is_cancelled() {
                return;
            }
            let item = self.log.0.lock().unwrap().get(idx).cloned();
            match item {
                None => tokio::time::sleep(Duration::from_millis(2)).await,
                Some((msg_id, payload)) => {
                    let ev = IncomingEvent {
                        msg_id: Some(msg_id),
                        traceparent: None,
                        payload,
                    };
                    match handler.handle(&ev).await {
                        Ok(()) => {
                            idx += 1;
                            attempts = 0;
                        }
                        Err(_) => {
                            attempts += 1;
                            if attempts >= MAX_DELIVER {
                                idx += 1; // poison: drop and move on
                                attempts = 0;
                            }
                        }
                    }
                }
            }
        }
    }
}

struct CountingHandler {
    calls: Arc<Mutex<usize>>,
    fail_first: usize,
}

#[async_trait]
impl EventHandler for CountingHandler {
    async fn handle(&self, _ev: &IncomingEvent) -> Result<(), BrokerError> {
        let mut c = self.calls.lock().unwrap();
        *c += 1;
        if *c <= self.fail_first {
            Err("flaky".into())
        } else {
            Ok(())
        }
    }
}

async fn wait_until(deadline_ms: u64, mut pred: impl FnMut() -> bool) {
    let step = 5;
    let mut waited = 0;
    while waited < deadline_ms {
        if pred() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(step)).await;
        waited += step;
    }
}

#[tokio::test]
async fn one_event_fans_out_to_two_consumer_groups() {
    let log = Log::default();
    log.publish("e1", "e1", None, b"{}".to_vec()).await.unwrap();

    let fraud_calls = Arc::new(Mutex::new(0));
    let analytics_calls = Arc::new(Mutex::new(0));
    let cancel = CancellationToken::new();

    let c1 = GroupConsumer { log: log.clone() };
    let c2 = GroupConsumer { log: log.clone() };
    let h1: Arc<dyn EventHandler> = Arc::new(CountingHandler {
        calls: fraud_calls.clone(),
        fail_first: 0,
    });
    let h2: Arc<dyn EventHandler> = Arc::new(CountingHandler {
        calls: analytics_calls.clone(),
        fail_first: 0,
    });

    let (cc1, cc2) = (cancel.clone(), cancel.clone());
    let t1 = tokio::spawn(async move { c1.run(h1, cc1).await });
    let t2 = tokio::spawn(async move { c2.run(h2, cc2).await });

    wait_until(500, || {
        *fraud_calls.lock().unwrap() == 1 && *analytics_calls.lock().unwrap() == 1
    })
    .await;
    cancel.cancel();
    let _ = tokio::join!(t1, t2);

    assert_eq!(*fraud_calls.lock().unwrap(), 1);
    assert_eq!(*analytics_calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn handler_error_triggers_redelivery() {
    let log = Log::default();
    log.publish("e1", "e1", None, b"{}".to_vec()).await.unwrap();

    let calls = Arc::new(Mutex::new(0));
    let cancel = CancellationToken::new();
    let c = GroupConsumer { log: log.clone() };
    // fail once, then succeed: handler must be invoked at least twice.
    let h: Arc<dyn EventHandler> = Arc::new(CountingHandler {
        calls: calls.clone(),
        fail_first: 1,
    });

    let cc = cancel.clone();
    let t = tokio::spawn(async move { c.run(h, cc).await });

    wait_until(500, || *calls.lock().unwrap() >= 2).await;
    cancel.cancel();
    let _ = t.await;

    assert!(
        *calls.lock().unwrap() >= 2,
        "expected redelivery after error"
    );
}
