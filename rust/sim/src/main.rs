use axum::{
    Router, middleware,
    routing::{get, post},
};
use std::sync::Arc;
use std::{env, net::SocketAddr};
use tokio_postgres::NoTls;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use time_ledger_sim_rust::handlers::{
    admin, audit, balances, controls, incidents, spool, transactions, transfers, zones,
};
use time_ledger_sim_rust::messaging::broker::{
    EventConsumer, EventHandler, EventPublisher, SUBJECT_TRANSFER_POSTED,
};
use time_ledger_sim_rust::messaging::store::PgStore;
use time_ledger_sim_rust::messaging::{analytics, fraud, nats, outbox};
use time_ledger_sim_rust::middleware::cors;
use time_ledger_sim_rust::state::{AppState, init_metrics};

type Publisher = Arc<dyn EventPublisher>;
type Consumer = Arc<dyn EventConsumer>;

/// Build a publisher and the per-group consumers for the selected broker.
/// Returns None when messaging is disabled or setup fails (the API still runs).
async fn build_messaging(broker: &str) -> Option<(Publisher, Consumer, Consumer)> {
    match broker {
        "nats" => {
            let nats_url = match env::var("NATS_URL") {
                Ok(u) => u,
                Err(_) => {
                    info!("NATS_URL not set, messaging disabled");
                    return None;
                }
            };
            // Retry the initial connect so a broker that is briefly unavailable at
            // startup (a common compose/orchestration race) does not permanently
            // disable messaging. This runs in a background task, so waiting here
            // never blocks the API from serving.
            let nc = match async_nats::ConnectOptions::new()
                .retry_on_initial_connect()
                .connect(&nats_url)
                .await
            {
                Ok(nc) => nc,
                Err(e) => {
                    warn!(error = %e, "NATS connection failed, messaging disabled");
                    return None;
                }
            };
            let js = async_nats::jetstream::new(nc);
            if let Err(e) = nats::ensure_streams(&js).await {
                warn!(error = %e, "NATS stream setup failed, messaging disabled");
                return None;
            }
            info!("NATS connected, starting outbox publisher and consumers");
            let publisher: Publisher = Arc::new(nats::NatsPublisher::new(js.clone()));
            let fraud_c: Consumer = Arc::new(nats::NatsConsumer::new(
                js.clone(),
                fraud::CONSUMER,
                SUBJECT_TRANSFER_POSTED,
            ));
            let analytics_c: Consumer = Arc::new(nats::NatsConsumer::new(
                js,
                analytics::CONSUMER,
                SUBJECT_TRANSFER_POSTED,
            ));
            Some((publisher, fraud_c, analytics_c))
        }
        #[cfg(feature = "redpanda")]
        "redpanda" => {
            use time_ledger_sim_rust::messaging::redpanda;
            let brokers = match env::var("REDPANDA_BROKERS") {
                Ok(b) => b,
                Err(_) => {
                    info!("REDPANDA_BROKERS not set, messaging disabled");
                    return None;
                }
            };
            match redpanda::build(&brokers).await {
                Ok((publisher, fraud_c, analytics_c)) => {
                    info!("Redpanda connected, starting outbox publisher and consumers");
                    Some((publisher, fraud_c, analytics_c))
                }
                Err(e) => {
                    warn!(error = %e, "Redpanda setup failed, messaging disabled");
                    None
                }
            }
        }
        other => {
            warn!(broker = %other, "unknown or uncompiled EVENT_BROKER, messaging disabled");
            None
        }
    }
}

fn init_tracing() {
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
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

    let mut pg_config = database_url
        .parse::<tokio_postgres::Config>()
        .expect("invalid DATABASE_URL");
    // Bound every connection: cap single statements and idle-in-transaction time
    // (the latter must exceed the outbox per-publish timeout).
    pg_config.options(format!(
        "-c statement_timeout={} -c idle_in_transaction_session_timeout={}",
        time_ledger_sim_rust::config::DB_STATEMENT_TIMEOUT_MS,
        time_ledger_sim_rust::config::DB_IDLE_TX_TIMEOUT_MS,
    ));
    let mgr = deadpool_postgres::Manager::new(pg_config, NoTls);
    let pool = deadpool_postgres::Pool::builder(mgr)
        .max_size(16)
        .build()
        .expect("pool build");

    // Messaging (optional: disabled only if the broker is unset). The whole
    // bring-up runs in a background task so the API serves immediately and a
    // broker that is slow or briefly unavailable at startup does not block the
    // server or permanently disable messaging (the connect retries).
    let cancel = CancellationToken::new();
    {
        let broker = env::var("EVENT_BROKER").unwrap_or_else(|_| "nats".into());
        let pool = pool.clone();
        let cancel = cancel.clone();
        let backlog = metrics_state.outbox_backlog.clone();
        tokio::spawn(async move {
            let Some((publisher, fraud_c, analytics_c)) = build_messaging(&broker).await else {
                return;
            };
            let store = Arc::new(PgStore::new(pool.clone()));
            let outbox = outbox::OutboxPublisher::new(store.clone(), publisher);
            let fraud_handler: Arc<dyn EventHandler> =
                Arc::new(fraud::FraudHandler::new(store.clone()));
            let analytics_handler: Arc<dyn EventHandler> =
                Arc::new(analytics::AnalyticsHandler::new(store));
            let (c1, c2, c3) = (cancel.clone(), cancel.clone(), cancel.clone());
            tokio::spawn(async move { outbox.run(c1).await });
            tokio::spawn(async move { fraud_c.run(fraud_handler, c2).await });
            tokio::spawn(async move { analytics_c.run(analytics_handler, c3).await });

            // Pipeline backlog SLI: poll unpublished outbox rows into a gauge.
            let c4 = cancel.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    tokio::select! {
                        _ = c4.cancelled() => return,
                        _ = interval.tick() => {
                            if let Ok(client) = pool.get().await
                                && let Ok(row) = client.query_one("SELECT count(*) FROM outbox_events WHERE published_at IS NULL", &[]).await
                            {
                                let n: i64 = row.get(0);
                                backlog.set(n);
                            }
                        }
                    }
                }
            });
        });
    }

    let st = AppState {
        db: pool,
        admin_key,
        registry,
        metrics: metrics_state,
    };

    let app = Router::new()
        .route("/healthz", get(admin::healthz))
        .route("/readyz", get(admin::readyz))
        .route("/metrics", get(admin::metrics))
        .route("/v1/version", get(admin::version))
        .route("/v1/zones", get(zones::list_zones))
        .route("/v1/transfers", post(transfers::create_transfer))
        .route("/v1/balances", get(balances::list_balances))
        .route("/v1/transactions", get(transactions::list_transactions))
        .route(
            "/v1/transactions/{transaction_id}",
            get(transactions::get_transaction),
        )
        .route("/v1/zones/{zone_id}/status", post(zones::set_zone_status))
        .route(
            "/v1/zones/{zone_id}/incidents",
            get(incidents::list_incidents_by_zone),
        )
        .route("/v1/incidents", get(incidents::list_recent_incidents))
        .route("/v1/incidents/{incident_id}", get(incidents::get_incident))
        .route(
            "/v1/incidents/{incident_id}/action",
            post(incidents::apply_incident_action),
        )
        .route(
            "/v1/zones/{zone_id}/controls",
            get(controls::get_zone_controls).post(controls::set_zone_controls),
        )
        .route("/v1/zones/{zone_id}/spool", get(spool::get_spool_stats))
        .route(
            "/v1/zones/{zone_id}/spool/replay",
            post(spool::replay_spool),
        )
        .route("/v1/zones/{zone_id}/audit", get(audit::list_audit))
        .route("/v1/sim/snapshot", post(admin::snapshot))
        .route("/v1/sim/restore", post(admin::restore))
        .layer(middleware::from_fn(cors))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            time_ledger_sim_rust::config::REQUEST_TIMEOUT,
        ))
        .with_state(st);

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    info!(%addr, "sim-rust listening");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            info!("shutting down");
            cancel.cancel();
        })
        .await
        .unwrap();
}
