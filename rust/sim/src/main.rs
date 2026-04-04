use axum::{middleware, routing::{get, post}, Router};
use std::{env, net::SocketAddr};
use tokio_postgres::NoTls;
use tracing::info;

use time_ledger_sim_rust::handlers::{admin, audit, balances, controls, incidents, spool, transactions, transfers, zones};
use time_ledger_sim_rust::middleware::cors;
use time_ledger_sim_rust::state::{init_metrics, AppState};

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}

#[tokio::main]
async fn main() {
    init_tracing();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL required");
    let port = env::var("PORT").unwrap_or_else(|_| "8081".into());
    let admin_key = env::var("ADMIN_KEY").ok();

    let (registry, metrics_state) = init_metrics();

    let pg_config = database_url
        .parse::<tokio_postgres::Config>()
        .expect("invalid DATABASE_URL");
    let mgr = deadpool_postgres::Manager::new(pg_config, NoTls);
    let pool = deadpool_postgres::Pool::builder(mgr)
        .max_size(16)
        .build()
        .expect("pool build");

    let st = AppState {
        db: pool,
        admin_key,
        registry,
        metrics: metrics_state,
    };

    let app = Router::new()
        .route("/healthz", get(admin::healthz))
        .route("/metrics", get(admin::metrics))
        .route("/v1/version", get(admin::version))
        .route("/v1/zones", get(zones::list_zones))
        .route("/v1/transfers", post(transfers::create_transfer))
        .route("/v1/balances", get(balances::list_balances))
        .route("/v1/transactions", get(transactions::list_transactions))
        .route("/v1/transactions/{transaction_id}", get(transactions::get_transaction))
        .route("/v1/zones/{zone_id}/status", post(zones::set_zone_status))
        .route("/v1/zones/{zone_id}/incidents", get(incidents::list_incidents_by_zone))
        .route("/v1/incidents", get(incidents::list_recent_incidents))
        .route("/v1/incidents/{incident_id}", get(incidents::get_incident))
        .route("/v1/incidents/{incident_id}/action", post(incidents::apply_incident_action))
        .route("/v1/zones/{zone_id}/controls", get(controls::get_zone_controls).post(controls::set_zone_controls))
        .route("/v1/zones/{zone_id}/spool", get(spool::get_spool_stats))
        .route("/v1/zones/{zone_id}/spool/replay", post(spool::replay_spool))
        .route("/v1/zones/{zone_id}/audit", get(audit::list_audit))
        .route("/v1/sim/snapshot", post(admin::snapshot))
        .route("/v1/sim/restore", post(admin::restore))
        .layer(middleware::from_fn(cors))
        .with_state(st);

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    info!(%addr, "sim-rust listening");
    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}
