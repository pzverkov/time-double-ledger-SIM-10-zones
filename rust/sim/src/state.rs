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
}

pub fn init_metrics() -> (Arc<prometheus::Registry>, Arc<Metrics>) {
    let reg = prometheus::Registry::new();
    let transfers_total =
        prometheus::IntCounter::new("transfers_total", "Transfers created").unwrap();
    reg.register(Box::new(transfers_total.clone())).unwrap();
    (Arc::new(reg), Arc::new(Metrics { transfers_total }))
}
