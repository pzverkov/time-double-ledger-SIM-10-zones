use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use serde_json::json;
use std::env;

use crate::state::AppState;
use crate::handlers::zones::list_zones;

pub async fn healthz() -> impl IntoResponse {
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

pub async fn version() -> impl IntoResponse {
    Json(VersionInfo {
        service: "time-ledger-sim",
        language: "rust",
        version: env!("CARGO_PKG_VERSION"),
        revision: option_env!("GIT_SHA"),
        build_time: option_env!("BUILD_TIME"),
    })
}

pub async fn metrics(State(st): State<AppState>) -> impl IntoResponse {
    use prometheus::Encoder;
    let mf = st.registry.gather();
    let mut buf = Vec::new();
    let enc = prometheus::TextEncoder::new();
    enc.encode(&mf, &mut buf).unwrap();
    (StatusCode::OK, String::from_utf8_lossy(&buf).to_string())
}

pub fn admin_guard(st: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
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

pub async fn snapshot(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    admin_guard(&st, &headers)?;
    let zones_val = list_zones(State(st.clone())).await?.0;
    let client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = client
        .query("SELECT account_id, balance_units FROM balances ORDER BY account_id LIMIT 5000", &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let balances: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "account_id": r.get::<_, String>("account_id"),
                "balance_units": r.get::<_, i64>("balance_units"),
            })
        })
        .collect();
    Ok(Json(json!({ "zones": zones_val["zones"], "balances": balances })))
}

pub async fn restore(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(snap): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    admin_guard(&st, &headers)?;
    let mut client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tx = client.transaction().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.execute("TRUNCATE TABLE balances RESTART IDENTITY CASCADE", &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(bals) = snap.get("balances").and_then(|v| v.as_array()) {
        for b in bals {
            let aid = b.get("account_id").and_then(|v| v.as_str()).unwrap_or("");
            let bu = b.get("balance_units").and_then(|v| v.as_i64()).unwrap_or(0);
            if !aid.is_empty() {
                tx.execute(
                    "INSERT INTO accounts(id, zone_id) VALUES($1,'zone-eu') ON CONFLICT DO NOTHING",
                    &[&aid],
                )
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                tx.execute(
                    "INSERT INTO balances(account_id,balance_units) VALUES($1,$2)",
                    &[&aid, &bu],
                )
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
    }
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "status": "ok" })))
}
