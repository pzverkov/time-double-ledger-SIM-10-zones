use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use serde_json::json;

use crate::state::AppState;
use crate::util::fmt_rfc3339;

#[derive(Serialize)]
struct BalanceRow {
    account_id: String,
    balance_units: i64,
    updated_at: String,
}

pub async fn list_balances(
    State(st): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let client = st.db.get().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let rows = client
        .query(
            "SELECT account_id, balance_units, updated_at FROM balances ORDER BY updated_at DESC LIMIT 100",
            &[],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let balances: Vec<BalanceRow> = rows
        .into_iter()
        .map(|r| {
            let updated_at: time::OffsetDateTime = r.get("updated_at");
            BalanceRow {
                account_id: r.get("account_id"),
                balance_units: r.get("balance_units"),
                updated_at: fmt_rfc3339(updated_at),
            }
        })
        .collect();

    Ok(Json(json!({ "balances": balances })))
}
