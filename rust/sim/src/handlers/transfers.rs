use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;
use crate::util::{fmt_rfc3339, payload_hash_compat};

#[derive(Serialize, Deserialize)]
pub struct CreateTransferRequest {
    pub request_id: String,
    pub from_account: String,
    pub to_account: String,
    pub amount_units: i64,
    pub zone_id: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Serialize)]
pub struct TransferResponse {
    pub transaction_id: String,
    pub request_id: String,
    pub created_at: String,
}

pub async fn create_transfer(
    State(st): State<AppState>,
    Json(req): Json<CreateTransferRequest>,
) -> Result<Json<TransferResponse>, StatusCode> {
    if req.amount_units <= 0 || req.request_id.is_empty() || req.zone_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let hash = payload_hash_compat(&req)?;
    let mut client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tx = client.transaction().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // zone gate
    let status_row = tx
        .query_one("SELECT status FROM zones WHERE id=$1", &[&req.zone_id])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status: String = status_row.get(0);
    if status == "DOWN" {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // idempotency check
    let existing = tx
        .query_opt(
            "SELECT id::text, payload_hash, created_at FROM transactions WHERE request_id=$1",
            &[&req.request_id],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(r) = existing {
        let id: String = r.get(0);
        let ph: String = r.get(1);
        let created_at: time::OffsetDateTime = r.get(2);
        if ph != hash {
            return Err(StatusCode::CONFLICT);
        }
        tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(TransferResponse {
            transaction_id: id,
            request_id: req.request_id,
            created_at: fmt_rfc3339(created_at),
        }));
    }

    // ensure accounts exist
    tx.execute(
        "INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&req.from_account, &req.zone_id],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.execute(
        "INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&req.to_account, &req.zone_id],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = tx
        .query_one(
            "INSERT INTO transactions(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id::text, created_at",
            &[&req.request_id, &hash, &req.from_account, &req.to_account, &req.amount_units, &req.zone_id, &req.metadata],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let txn_id: String = row.get(0);
    let created_at: time::OffsetDateTime = row.get(1);

    // postings
    tx.execute(
        "INSERT INTO postings(txn_id,account_id,direction,amount_units) VALUES($1::uuid,$2,'DEBIT',$3),($1::uuid,$4,'CREDIT',$3)",
        &[&txn_id, &req.from_account, &req.amount_units, &req.to_account],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // balances projection
    let neg_amount = -req.amount_units;
    tx.execute(
        "INSERT INTO balances(account_id,balance_units) VALUES($1,$2) ON CONFLICT (account_id) DO UPDATE SET balance_units=balances.balance_units + EXCLUDED.balance_units, updated_at=now()",
        &[&req.from_account, &neg_amount],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.execute(
        "INSERT INTO balances(account_id,balance_units) VALUES($1,$2) ON CONFLICT (account_id) DO UPDATE SET balance_units=balances.balance_units + EXCLUDED.balance_units, updated_at=now()",
        &[&req.to_account, &req.amount_units],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // outbox
    let payload = json!({
        "event_id": "generated_by_db",
        "type": "TransferPosted",
        "transaction_id": txn_id,
        "request_id": req.request_id,
        "zone_id": req.zone_id,
        "amount_units": req.amount_units,
        "created_at": fmt_rfc3339(created_at),
    });
    let aggregate_id = txn_id.as_str();
    tx.execute(
        "INSERT INTO outbox_events(event_type,aggregate_type,aggregate_id,payload) VALUES('TransferPosted','transaction',$1,$2)",
        &[&aggregate_id, &payload],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    st.metrics.transfers_total.inc();

    Ok(Json(TransferResponse {
        transaction_id: payload["transaction_id"].as_str().unwrap().to_string(),
        request_id: payload["request_id"].as_str().unwrap().to_string(),
        created_at: payload["created_at"].as_str().unwrap().to_string(),
    }))
}
