use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Serialize;
use serde_json::json;

use crate::state::AppState;
use crate::util::fmt_rfc3339;

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

#[derive(Serialize)]
struct PostingRow {
    account_id: String,
    direction: String,
    amount_units: i64,
}

pub async fn list_transactions(
    State(st): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let client = st.db.get().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let rows = client
        .query(
            "SELECT id::text as id, request_id, from_account, to_account, amount_units, zone_id, created_at FROM transactions ORDER BY created_at DESC LIMIT 100",
            &[],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let txns: Vec<TxnRow> = rows
        .into_iter()
        .map(|r| {
            let created_at: time::OffsetDateTime = r.get("created_at");
            TxnRow {
                id: r.get("id"),
                request_id: r.get("request_id"),
                from_account: r.get("from_account"),
                to_account: r.get("to_account"),
                amount_units: r.get("amount_units"),
                zone_id: r.get("zone_id"),
                created_at: fmt_rfc3339(created_at),
            }
        })
        .collect();

    Ok(Json(json!({ "transactions": txns })))
}

pub async fn get_transaction(
    Path(transaction_id): Path<String>,
    State(st): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let client = st.db.get().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let row = client
        .query_one(
            "SELECT id::text as id, request_id, from_account, to_account, amount_units, zone_id, created_at, metadata FROM transactions WHERE id::text=$1",
            &[&transaction_id],
        )
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let id: String = row.get("id");
    let request_id: String = row.get("request_id");
    let from_account: String = row.get("from_account");
    let to_account: String = row.get("to_account");
    let amount_units: i64 = row.get("amount_units");
    let zone_id: String = row.get("zone_id");
    let created_at: time::OffsetDateTime = row.get("created_at");
    let metadata: serde_json::Value = row.get("metadata");

    let post_rows = client
        .query(
            "SELECT account_id, direction, amount_units FROM postings WHERE txn_id::text=$1 ORDER BY direction ASC",
            &[&transaction_id],
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let postings: Vec<PostingRow> = post_rows
        .into_iter()
        .map(|r| PostingRow {
            account_id: r.get("account_id"),
            direction: r.get("direction"),
            amount_units: r.get("amount_units"),
        })
        .collect();

    Ok(Json(json!({
        "id": id, "request_id": request_id,
        "from_account": from_account, "to_account": to_account,
        "amount_units": amount_units, "zone_id": zone_id,
        "created_at": fmt_rfc3339(created_at),
        "metadata": metadata, "postings": postings
    })))
}
