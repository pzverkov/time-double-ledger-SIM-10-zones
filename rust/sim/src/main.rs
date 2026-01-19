use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use prometheus::Encoder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::{env, net::SocketAddr, sync::Arc};
use tracing::{info};
// uuid kept in Cargo.toml for DB ids and request ids in other modules, but not used in this binary.

#[derive(Clone)]
struct AppState {
    db: PgPool,
    admin_key: Option<String>,
    registry: Arc<prometheus::Registry>,
    metrics: Arc<Metrics>,
}

struct Metrics {
    transfers_total: prometheus::IntCounter,
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}

fn init_metrics() -> (Arc<prometheus::Registry>, Arc<Metrics>) {
    let reg = prometheus::Registry::new();
    let transfers_total =
        prometheus::IntCounter::new("transfers_total", "Transfers created").unwrap();
    reg.register(Box::new(transfers_total.clone())).unwrap();
    (Arc::new(reg), Arc::new(Metrics { transfers_total }))
}

async fn cors(mut req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let allowed = std::env::var("CORS_ALLOW_ORIGINS").unwrap_or_else(|_| "http://localhost:5173,http://localhost:4173".to_string());
    let allow_any = allowed.split(',').any(|x| x.trim() == "*");

    let mut allowed_origin: Option<String> = None;
    if let Some(o) = origin.clone() {
        if allow_any {
            allowed_origin = Some(o);
        } else {
            for a in allowed.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()) {
                if a == o {
                    allowed_origin = Some(o);
                    break;
                }
            }
        }
    }

    if req.method() == Method::OPTIONS {
        let mut res = Response::new(Body::empty());
        *res.status_mut() = StatusCode::NO_CONTENT;
        apply_cors_headers(&mut res, allowed_origin);
        return res;
    }

    let mut res = next.run(req).await;
    apply_cors_headers(&mut res, allowed_origin);
    res
}

fn apply_cors_headers(res: &mut Response, allowed_origin: Option<String>) {
    if let Some(o) = allowed_origin {
        if let Ok(v) = HeaderValue::from_str(&o) {
            res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
            res.headers_mut().insert(header::VARY, HeaderValue::from_static("Origin"));
        }
        res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET,POST,OPTIONS"));
        res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Content-Type,X-Admin-Key"));
    }
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}


#[derive(serde::Serialize)]
struct VersionInfo {
    service: &'static str,
    language: &'static str,
    version: &'static str,
    revision: Option<&'static str>,
    build_time: Option<&'static str>,
}

async fn version() -> impl IntoResponse {
    axum::Json(VersionInfo {
        service: "time-ledger-sim",
        language: "rust",
        version: env!("CARGO_PKG_VERSION"),
        revision: option_env!("GIT_SHA"),
        build_time: option_env!("BUILD_TIME"),
    })
}

async fn metrics(State(st): State<AppState>) -> impl IntoResponse {
    let mf = st.registry.gather();
    let mut buf = Vec::new();
    let enc = prometheus::TextEncoder::new();
    enc.encode(&mf, &mut buf).unwrap();
    (StatusCode::OK, String::from_utf8_lossy(&buf).to_string())
}

#[derive(Serialize)]
struct Zone {
    id: String,
    name: String,
    status: String,
    updated_at: String,
}

async fn list_zones(State(st): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = sqlx::query("SELECT id,name,status,updated_at FROM zones ORDER BY id")
        .fetch_all(&st.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let zones: Vec<Zone> = rows
        .into_iter()
        .map(|r| {
            let updated_at: time::OffsetDateTime = r.get("updated_at");
            Zone {
                id: r.get("id"),
                name: r.get("name"),
                status: r.get("status"),
                updated_at: updated_at
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap(),
            }
        })
        .collect();

    Ok(Json(json!({ "zones": zones })))
}

#[derive(Serialize)]
struct BalanceRow {
    account_id: String,
    balance_units: i64,
    updated_at: String,
}

async fn list_balances(State(st): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let rows = sqlx::query("SELECT account_id, balance_units, updated_at FROM balances ORDER BY updated_at DESC LIMIT 100")
        .fetch_all(&st.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let balances: Vec<BalanceRow> = rows.into_iter().map(|r| {
        let updated_at: time::OffsetDateTime = r.get("updated_at");
        BalanceRow{
            account_id: r.get("account_id"),
            balance_units: r.get("balance_units"),
            updated_at: updated_at.format(&time::format_description::well_known::Rfc3339).unwrap(),
        }
    }).collect();

    Ok(Json(json!({ "balances": balances })))
}

#[derive(Serialize)]
struct TxnRow {
    id: String,
    request_id: String,
    from_account: String,
    to_account: String,
    amount_units: i64,
    zone_id: String,
    created_at: String,
}

async fn list_transactions(State(st): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let rows = sqlx::query("SELECT id::text as id, request_id, from_account, to_account, amount_units, zone_id, created_at FROM transactions ORDER BY created_at DESC LIMIT 100")
        .fetch_all(&st.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let txns: Vec<TxnRow> = rows.into_iter().map(|r| {
        let created_at: time::OffsetDateTime = r.get("created_at");
        TxnRow{
            id: r.get("id"),
            request_id: r.get("request_id"),
            from_account: r.get("from_account"),
            to_account: r.get("to_account"),
            amount_units: r.get("amount_units"),
            zone_id: r.get("zone_id"),
            created_at: created_at.format(&time::format_description::well_known::Rfc3339).unwrap(),
        }
    }).collect();

    Ok(Json(json!({ "transactions": txns })))
}

#[derive(Serialize)]
struct PostingRow {
    account_id: String,
    direction: String,
    amount_units: i64,
}

async fn get_transaction(Path(transaction_id): Path<String>, State(st): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let row = sqlx::query("SELECT id::text as id, request_id, from_account, to_account, amount_units, zone_id, created_at, metadata FROM transactions WHERE id::text=$1")
        .bind(&transaction_id)
        .fetch_one(&st.db)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let created_at: time::OffsetDateTime = row.get("created_at");
    let metadata: serde_json::Value = row.get("metadata");

    let post_rows = sqlx::query("SELECT account_id, direction, amount_units FROM postings WHERE txn_id::text=$1 ORDER BY direction ASC")
        .bind(&transaction_id)
        .fetch_all(&st.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let postings: Vec<PostingRow> = post_rows.into_iter().map(|r| PostingRow{
        account_id: r.get("account_id"),
        direction: r.get("direction"),
        amount_units: r.get("amount_units"),
    }).collect();

    Ok(Json(json!({
        "id": row.get::<String,_>("id"),
        "request_id": row.get::<String,_>("request_id"),
        "from_account": row.get::<String,_>("from_account"),
        "to_account": row.get::<String,_>("to_account"),
        "amount_units": row.get::<i64,_>("amount_units"),
        "zone_id": row.get::<String,_>("zone_id"),
        "created_at": created_at.format(&time::format_description::well_known::Rfc3339).unwrap(),
        "metadata": metadata,
        "postings": postings
    })))
}

#[derive(Serialize, Deserialize)]
struct CreateTransferRequest {
    request_id: String,
    from_account: String,
    to_account: String,
    amount_units: i64,
    zone_id: String,
    #[serde(default)]
    metadata: serde_json::Value,
}

#[derive(Serialize)]
struct TransferResponse {
    transaction_id: String,
    request_id: String,
    created_at: String,
}

// Canonical JSON hashing (stable map key order)
fn canonicalize(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize(&map[&k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(canonicalize).collect()),
        _ => v.clone(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    hex::encode(out)
}

fn payload_hash(req: &CreateTransferRequest) -> Result<String, StatusCode> {
    let v = serde_json::to_value(req).map_err(|_| StatusCode::BAD_REQUEST)?;
    let canon = canonicalize(&v);
    let bytes = serde_json::to_vec(&canon).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(sha256_hex(&bytes))
}

async fn create_transfer(
    State(st): State<AppState>,
    Json(req): Json<CreateTransferRequest>,
) -> Result<Json<TransferResponse>, StatusCode> {
    if req.amount_units <= 0 || req.request_id.is_empty() || req.zone_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let hash = payload_hash(&req)?;
    let mut tx = st
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // zone gate
    let status: String = sqlx::query_scalar("SELECT status FROM zones WHERE id=$1")
        .bind(&req.zone_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if status == "DOWN" {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // idempotency check
    let existing = sqlx::query(
        "SELECT id::text, payload_hash, created_at FROM transactions WHERE request_id=$1",
    )
    .bind(&req.request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(r) = existing {
        let id: String = r.get(0);
        let ph: String = r.get(1);
        let created_at: time::OffsetDateTime = r.get(2);
        if ph != hash {
            return Err(StatusCode::CONFLICT);
        }
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(TransferResponse {
            transaction_id: id,
            request_id: req.request_id,
            created_at: created_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
        }));
    }

    // ensure accounts exist (zone-scoped)
    sqlx::query("INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(&req.from_account)
        .bind(&req.zone_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(&req.to_account)
        .bind(&req.zone_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query("INSERT INTO transactions(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id::text, created_at")
        .bind(&req.request_id)
        .bind(&hash)
        .bind(&req.from_account)
        .bind(&req.to_account)
        .bind(req.amount_units)
        .bind(&req.zone_id)
        .bind(&req.metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let txn_id: String = row.get(0);
    let created_at: time::OffsetDateTime = row.get(1);

    // postings
    sqlx::query("INSERT INTO postings(txn_id,account_id,direction,amount_units) VALUES($1::uuid,$2,'DEBIT',$3),($1::uuid,$4,'CREDIT',$3)")
        .bind(&txn_id)
        .bind(&req.from_account)
        .bind(req.amount_units)
        .bind(&req.to_account)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // balances projection
    sqlx::query("INSERT INTO balances(account_id,balance_units) VALUES($1,$2) ON CONFLICT (account_id) DO UPDATE SET balance_units=balances.balance_units + EXCLUDED.balance_units, updated_at=now()")
        .bind(&req.from_account)
        .bind(-req.amount_units)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO balances(account_id,balance_units) VALUES($1,$2) ON CONFLICT (account_id) DO UPDATE SET balance_units=balances.balance_units + EXCLUDED.balance_units, updated_at=now()")
        .bind(&req.to_account)
        .bind(req.amount_units)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // outbox
    let payload = json!({
        "event_id": "generated_by_db",
        "type":"TransferPosted",
        "transaction_id": txn_id,
        "request_id": req.request_id,
        "zone_id": req.zone_id,
        "amount_units": req.amount_units,
        "created_at": created_at.format(&time::format_description::well_known::Rfc3339).unwrap()
    });
    sqlx::query("INSERT INTO outbox_events(event_type,aggregate_type,aggregate_id,payload) VALUES('TransferPosted','transaction',$1,$2)")
        .bind(payload["transaction_id"].as_str().unwrap())
        .bind(&payload)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    st.metrics.transfers_total.inc();

    Ok(Json(TransferResponse {
        transaction_id: payload["transaction_id"].as_str().unwrap().to_string(),
        request_id: payload["request_id"].as_str().unwrap().to_string(),
        created_at: payload["created_at"].as_str().unwrap().to_string(),
    }))
}

#[derive(Deserialize)]
struct SetZoneStatusRequest {
    status: String,
    actor: String,
    #[serde(default)]
    reason: String,
}

async fn set_zone_status(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
    Json(req): Json<SetZoneStatusRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if req.actor.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.status != "OK" && req.status != "DEGRADED" && req.status != "DOWN" {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut tx = st
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = sqlx::query(
        "UPDATE zones SET status=$2, updated_at=now() WHERE id=$1 RETURNING id,name,status,updated_at",
    )
    .bind(&zone_id)
    .bind(&req.status)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("INSERT INTO audit_log(actor,action,target_type,target_id,reason,details) VALUES($1,'SET_ZONE_STATUS','zone',$2,$3, jsonb_build_object('status',$4))")
        .bind(&req.actor)
        .bind(&zone_id)
        .bind(&req.reason)
        .bind(&req.status)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if req.status == "DOWN" {
        sqlx::query("INSERT INTO incidents(zone_id,severity,title,details) VALUES($1,'CRITICAL','Zone marked DOWN', jsonb_build_object('reason',$2,'actor',$3))")
            .bind(&zone_id)
            .bind(&req.reason)
            .bind(&req.actor)
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let updated_at: time::OffsetDateTime = row.get("updated_at");
    Ok(Json(json!({
        "id": row.get::<String,_>("id"),
        "name": row.get::<String,_>("name"),
        "status": row.get::<String,_>("status"),
        "updated_at": updated_at.format(&time::format_description::well_known::Rfc3339).unwrap()
    })))
}

async fn list_incidents_by_zone(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = sqlx::query("SELECT id::text, zone_id, severity, status, title, details, detected_at FROM incidents WHERE zone_id=$1 ORDER BY detected_at DESC LIMIT 200")
        .bind(&zone_id)
        .fetch_all(&st.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let incs: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let dt: time::OffsetDateTime = r.get("detected_at");
            json!({
                "id": r.get::<String,_>("id"),
                "zone_id": r.get::<String,_>("zone_id"),
                "severity": r.get::<String,_>("severity"),
                "status": r.get::<String,_>("status"),
                "title": r.get::<String,_>("title"),
                "details": r.get::<serde_json::Value,_>("details"),
                "detected_at": dt.format(&time::format_description::well_known::Rfc3339).unwrap(),
            })
        })
        .collect();
    Ok(Json(json!({ "incidents": incs })))
}

async fn get_incident(
    State(st): State<AppState>,
    Path(incident_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = sqlx::query("SELECT id::text, zone_id, severity, status, title, details, detected_at FROM incidents WHERE id=$1::uuid")
        .bind(&incident_id)
        .fetch_one(&st.db)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let dt: time::OffsetDateTime = row.get("detected_at");
    Ok(Json(json!({
        "id": row.get::<String,_>("id"),
        "zone_id": row.get::<String,_>("zone_id"),
        "severity": row.get::<String,_>("severity"),
        "status": row.get::<String,_>("status"),
        "title": row.get::<String,_>("title"),
        "details": row.get::<serde_json::Value,_>("details"),
        "detected_at": dt.format(&time::format_description::well_known::Rfc3339).unwrap()
    })))
}

fn admin_guard(st: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    match &st.admin_key {
        None => Err(StatusCode::FORBIDDEN),
        Some(k) => {
            let got = headers
                .get("x-admin-key")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if got == k {
                Ok(())
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
    }
}

async fn snapshot(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    admin_guard(&st, &headers)?;
    // Minimal snapshot: zones + balances
    let zones_val = list_zones(State(st.clone())).await?.0;
    let rows = sqlx::query("SELECT account_id, balance_units FROM balances ORDER BY account_id LIMIT 5000")
        .fetch_all(&st.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let balances: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "account_id": r.get::<String,_>("account_id"),
                "balance_units": r.get::<i64,_>("balance_units")
            })
        })
        .collect();
    Ok(Json(json!({ "zones": zones_val["zones"], "balances": balances })))
}

async fn restore(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(snap): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    admin_guard(&st, &headers)?;
    let mut tx = st
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("TRUNCATE TABLE balances RESTART IDENTITY CASCADE")
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(bals) = snap.get("balances").and_then(|v| v.as_array()) {
        for b in bals {
            let aid = b.get("account_id").and_then(|v| v.as_str()).unwrap_or("");
            let bu = b.get("balance_units").and_then(|v| v.as_i64()).unwrap_or(0);
            if !aid.is_empty() {
                sqlx::query("INSERT INTO accounts(id, zone_id) VALUES($1,'zone-eu') ON CONFLICT DO NOTHING")
                    .bind(aid)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                sqlx::query("INSERT INTO balances(account_id,balance_units) VALUES($1,$2)")
                    .bind(aid)
                    .bind(bu)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "status": "ok" })))
}

#[tokio::main]
async fn main() {
    init_tracing();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL required");
    let port = env::var("PORT").unwrap_or_else(|_| "8081".into());
    let admin_key = env::var("ADMIN_KEY").ok();

    let (registry, metrics_state) = init_metrics();

    let db = PgPool::connect(&database_url).await.expect("db connect");

    let st = AppState {
        db,
        admin_key,
        registry,
        metrics: metrics_state,
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
        .route("/v1/version", get(version))
        .route("/v1/zones", get(list_zones))
        .route("/v1/transfers", post(create_transfer))
        .route("/v1/balances", get(list_balances))
        .route("/v1/transactions", get(list_transactions))
        .route("/v1/transactions/:transaction_id", get(get_transaction))
        .route("/v1/zones/:zone_id/status", post(set_zone_status))
        .route("/v1/zones/:zone_id/incidents", get(list_incidents_by_zone))
        .route("/v1/incidents/:incident_id", get(get_incident))
        .route("/v1/sim/snapshot", post(snapshot))
        .route("/v1/sim/restore", post(restore))
        .layer(middleware::from_fn(cors))
        .with_state(st);

    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
    info!(%addr, "sim-rust listening");
    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}
