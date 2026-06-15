use async_trait::async_trait;
use deadpool_postgres::Pool;
use serde::Deserialize;

use super::broker::BrokerError;

/// Large-transfer threshold: 1 hour expressed in seconds.
pub const LARGE_TRANSFER_UNITS: i64 = 3600;

/// Wire event published on `events.transfer_posted`.
#[derive(Deserialize)]
pub struct TransferPosted {
    pub event_id: Option<String>,
    pub transaction_id: Option<String>,
    pub zone_id: Option<String>,
    pub amount_units: Option<i64>,
}

/// An unpublished outbox row.
#[derive(Clone)]
pub struct OutboxRow {
    pub id: String,
    pub payload: serde_json::Value,
}

/// A fraud incident to record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incident {
    pub zone_id: String,
    pub txn_id: String,
    pub amount_units: i64,
}

/// Running per-zone aggregate. Pure value used by analytics and its tests.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ZoneStat {
    pub event_count: i64,
    pub total_amount_units: i64,
}

/// The large-transfer fraud rule. Pure for direct unit testing.
pub fn fraud_verdict(amount_units: Option<i64>) -> bool {
    amount_units.unwrap_or(0) >= LARGE_TRANSFER_UNITS
}

/// Fold one event into a zone aggregate. Saturating to avoid overflow panics.
pub fn fold_stats(prev: ZoneStat, amount_units: i64) -> ZoneStat {
    ZoneStat {
        event_count: prev.event_count.saturating_add(1),
        total_amount_units: prev.total_amount_units.saturating_add(amount_units),
    }
}

#[async_trait]
pub trait OutboxStore: Send + Sync {
    async fn fetch_unpublished(&self, limit: i64) -> Result<Vec<OutboxRow>, BrokerError>;
    async fn mark_published(&self, id: &str) -> Result<(), BrokerError>;
}

/// Fraud-consumer persistence. The claim (inbox dedup) and the incident write
/// are one atomic unit: if the write fails the claim is rolled back, so a
/// redelivery reprocesses cleanly. Returns true if newly processed, false if a
/// duplicate.
#[async_trait]
pub trait FraudStore: Send + Sync {
    async fn record_fraud(&self, event_id: &str, incident: Option<&Incident>) -> Result<bool, BrokerError>;
}

/// Analytics-consumer persistence. Inbox claim + zone aggregate update are
/// atomic. Returns true if newly processed, false if a duplicate.
#[async_trait]
pub trait AnalyticsStore: Send + Sync {
    async fn record_stats(&self, event_id: &str, zone_id: &str, amount_units: i64) -> Result<bool, BrokerError>;
}

/// Postgres-backed implementation of all DB ports.
#[derive(Clone)]
pub struct PgStore {
    db: Pool,
}

impl PgStore {
    pub fn new(db: Pool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl OutboxStore for PgStore {
    async fn fetch_unpublished(&self, limit: i64) -> Result<Vec<OutboxRow>, BrokerError> {
        let client = self.db.get().await?;
        let rows = client
            .query(
                "SELECT id::text, payload FROM outbox_events WHERE published_at IS NULL ORDER BY created_at LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows
            .iter()
            .map(|r| OutboxRow {
                id: r.get("id"),
                payload: r.get("payload"),
            })
            .collect())
    }

    async fn mark_published(&self, id: &str) -> Result<(), BrokerError> {
        let client = self.db.get().await?;
        client
            .execute("UPDATE outbox_events SET published_at=now() WHERE id=$1::text::uuid", &[&id])
            .await?;
        Ok(())
    }
}

#[async_trait]
impl FraudStore for PgStore {
    async fn record_fraud(&self, event_id: &str, incident: Option<&Incident>) -> Result<bool, BrokerError> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let claimed = tx
            .execute(
                "INSERT INTO inbox_events(consumer,event_id) VALUES('fraud-v1',$1::text::uuid) ON CONFLICT DO NOTHING",
                &[&event_id],
            )
            .await?;
        if claimed == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        if let Some(inc) = incident {
            tx.execute(
                "INSERT INTO incidents(zone_id, related_txn_id, severity, title, details) VALUES($1, $2::text::uuid, 'WARN', 'Large time transfer', jsonb_build_object('amount_units',$3::bigint,'rule','large_transfer'))",
                &[&inc.zone_id, &inc.txn_id, &inc.amount_units],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }
}

#[async_trait]
impl AnalyticsStore for PgStore {
    async fn record_stats(&self, event_id: &str, zone_id: &str, amount_units: i64) -> Result<bool, BrokerError> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let claimed = tx
            .execute(
                "INSERT INTO inbox_events(consumer,event_id) VALUES('analytics-v1',$1::text::uuid) ON CONFLICT DO NOTHING",
                &[&event_id],
            )
            .await?;
        if claimed == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO zone_event_stats(zone_id, event_count, total_amount_units) VALUES($1, 1, $2) \
             ON CONFLICT (zone_id) DO UPDATE SET event_count = zone_event_stats.event_count + 1, \
             total_amount_units = zone_event_stats.total_amount_units + $2, updated_at = now()",
            &[&zone_id, &amount_units],
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraud_verdict_boundary() {
        assert!(!fraud_verdict(Some(3599)));
        assert!(fraud_verdict(Some(3600)));
        assert!(fraud_verdict(Some(3601)));
    }

    #[test]
    fn fraud_verdict_edge_amounts() {
        assert!(!fraud_verdict(Some(0)));
        assert!(!fraud_verdict(Some(-100)));
        assert!(!fraud_verdict(None));
        assert!(fraud_verdict(Some(i64::MAX)));
    }

    #[test]
    fn fold_stats_accumulates() {
        let s = fold_stats(ZoneStat::default(), 100);
        let s = fold_stats(s, 50);
        assert_eq!(s, ZoneStat { event_count: 2, total_amount_units: 150 });
    }

    #[test]
    fn fold_stats_saturates_at_max() {
        let prev = ZoneStat { event_count: i64::MAX, total_amount_units: i64::MAX };
        let s = fold_stats(prev, 1000);
        assert_eq!(s.event_count, i64::MAX);
        assert_eq!(s.total_amount_units, i64::MAX);
    }
}
