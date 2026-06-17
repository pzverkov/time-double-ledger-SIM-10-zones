use deadpool_postgres::Pool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Pool,
    pub admin_key: Option<String>,
    pub registry: Arc<prometheus::Registry>,
    pub metrics: Arc<Metrics>,
}

pub struct Metrics {
    pub transfers_total: prometheus::IntCounter,
    /// Unpublished outbox rows: the pipeline backlog SLI. Rising = consumers/publisher behind.
    pub outbox_backlog: prometheus::IntGauge,
    /// Accounts whose materialized balance disagrees with the postings-derived
    /// balance. Should always be 0; nonzero means ledger drift.
    pub ledger_balance_drift_accounts: prometheus::IntGauge,
    /// Transactions whose postings do not net to zero (debits != credits).
    /// Should always be 0; nonzero means a broken double-entry write.
    pub ledger_unbalanced_transactions: prometheus::IntGauge,
}

pub fn init_metrics() -> (Arc<prometheus::Registry>, Arc<Metrics>) {
    let reg = prometheus::Registry::new();
    let transfers_total =
        prometheus::IntCounter::new("transfers_total", "Transfers created").unwrap();
    let outbox_backlog =
        prometheus::IntGauge::new("outbox_backlog", "Unpublished outbox rows").unwrap();
    let ledger_balance_drift_accounts = prometheus::IntGauge::new(
        "ledger_balance_drift_accounts",
        "Accounts whose balance disagrees with postings",
    )
    .unwrap();
    let ledger_unbalanced_transactions = prometheus::IntGauge::new(
        "ledger_unbalanced_transactions",
        "Transactions whose postings do not net to zero",
    )
    .unwrap();
    reg.register(Box::new(transfers_total.clone())).unwrap();
    reg.register(Box::new(outbox_backlog.clone())).unwrap();
    reg.register(Box::new(ledger_balance_drift_accounts.clone()))
        .unwrap();
    reg.register(Box::new(ledger_unbalanced_transactions.clone()))
        .unwrap();
    (
        Arc::new(reg),
        Arc::new(Metrics {
            transfers_total,
            outbox_backlog,
            ledger_balance_drift_accounts,
            ledger_unbalanced_transactions,
        }),
    )
}
