use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;
use crate::state::AppState;
use crate::util::{fmt_rfc3339, hash_percent, payload_hash};

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
    pub status: String,
    pub transaction_id: String,
    pub request_id: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct SpooledResponse {
    pub status: String,
    pub spool_id: String,
    pub request_id: String,
}

pub async fn create_transfer(
    State(st): State<AppState>,
    Json(req): Json<CreateTransferRequest>,
) -> Result<axum::response::Response, AppError> {
    if req.amount_units <= 0 || req.request_id.is_empty() || req.zone_id.is_empty() {
        return Err(AppError::BadRequest("missing required fields or invalid amount".into()));
    }
    let hash = payload_hash(&req)?;
    let mut client = st.db.get().await?;
    let tx = client.transaction().await?;

    // zone gate + controls
    let status: String = tx
        .query_one("SELECT status FROM zones WHERE id=$1", &[&req.zone_id])
        .await
        .map_err(|_| AppError::Internal("zone not found".into()))?
        .get(0);

    let ctrl_row = tx
        .query_opt("SELECT writes_blocked, cross_zone_throttle, spool_enabled FROM zone_controls WHERE zone_id=$1", &[&req.zone_id])
        .await?;
    let (wb, throttle, spool_enabled) = ctrl_row
        .map(|r| (r.get::<_, bool>(0), r.get::<_, i32>(1), r.get::<_, bool>(2)))
        .unwrap_or((false, 100, false));

    let blocked_reason = if status == "DOWN" {
        Some("zone down")
    } else if wb {
        Some("writes blocked")
    } else if throttle < 100 && (throttle <= 0 || hash_percent(&req.request_id) >= throttle as u32) {
        Some("throttled")
    } else {
        None
    };

    // idempotency check (transactions table)
    let existing = tx
        .query_opt("SELECT id::text, payload_hash, created_at FROM transactions WHERE request_id=$1", &[&req.request_id])
        .await?;
    if let Some(r) = existing {
        let ph: String = r.get(1);
        if ph != hash {
            return Err(AppError::Conflict("idempotency conflict: same request_id, different payload".into()));
        }
        tx.commit().await?;
        let created_at: time::OffsetDateTime = r.get(2);
        return Ok(Json(TransferResponse {
            status: "APPLIED".into(),
            transaction_id: r.get(0),
            request_id: req.request_id,
            created_at: fmt_rfc3339(created_at),
        }).into_response());
    }

    // idempotency check (spooled_transfers table)
    let existing_spool = tx
        .query_opt("SELECT id::text, payload_hash FROM spooled_transfers WHERE request_id=$1", &[&req.request_id])
        .await?;
    if let Some(r) = existing_spool {
        let ph: String = r.get(1);
        if ph != hash {
            return Err(AppError::Conflict("idempotency conflict: same request_id, different payload".into()));
        }
        tx.commit().await?;
        return Ok((StatusCode::ACCEPTED, Json(SpooledResponse {
            status: "SPOOLED".into(),
            spool_id: r.get(0),
            request_id: req.request_id,
        })).into_response());
    }

    // blocked? spool or reject
    if let Some(reason) = blocked_reason {
        if spool_enabled {
            let spool_row = tx
                .query_one(
                    "INSERT INTO spooled_transfers(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata,fail_reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id::text",
                    &[&req.request_id, &hash, &req.from_account, &req.to_account, &req.amount_units, &req.zone_id, &req.metadata, &reason],
                )
                .await?;
            let spool_id: String = spool_row.get(0);

            tx.execute(
                "INSERT INTO audit_log(actor,action,target_type,target_id,reason,details) VALUES('system','SPOOL_TRANSFER','zone',$1,$2, jsonb_build_object('request_id',$3,'spool_id',$4))",
                &[&req.zone_id, &reason, &req.request_id, &spool_id],
            ).await?;

            tx.commit().await?;
            return Ok((StatusCode::ACCEPTED, Json(SpooledResponse {
                status: "SPOOLED".into(),
                spool_id,
                request_id: req.request_id,
            })).into_response());
        }

        return if status == "DOWN" {
            Err(AppError::Unavailable("zone down".into()))
        } else {
            Err(AppError::Unavailable(format!("zone blocked: {reason}")))
        };
    }

    // apply transfer
    tx.execute(
        "INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&req.from_account, &req.zone_id],
    ).await?;
    tx.execute(
        "INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&req.to_account, &req.zone_id],
    ).await?;

    let (txn_id, created_at) = apply_transfer_inner(&tx, &TransferInput {
        request_id: &req.request_id, payload_hash: &hash,
        from_account: &req.from_account, to_account: &req.to_account,
        amount_units: req.amount_units, zone_id: &req.zone_id, metadata: &req.metadata,
    }).await?;

    tx.commit().await?;
    st.metrics.transfers_total.inc();

    Ok(Json(TransferResponse {
        status: "APPLIED".into(),
        transaction_id: txn_id,
        request_id: req.request_id,
        created_at: fmt_rfc3339(created_at),
    }).into_response())
}

pub struct TransferInput<'a> {
    pub request_id: &'a str,
    pub payload_hash: &'a str,
    pub from_account: &'a str,
    pub to_account: &'a str,
    pub amount_units: i64,
    pub zone_id: &'a str,
    pub metadata: &'a serde_json::Value,
}

async fn apply_transfer_inner(
    tx: &deadpool_postgres::Transaction<'_>,
    inp: &TransferInput<'_>,
) -> Result<(String, time::OffsetDateTime), AppError> {
    let TransferInput { request_id, payload_hash: hash, from_account, to_account, amount_units, zone_id, metadata } = inp;
    let row = tx
        .query_one(
            "INSERT INTO transactions(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id::text, created_at",
            &[&request_id, &hash, &from_account, &to_account, &amount_units, &zone_id, metadata],
        )
        .await?;
    let txn_id: String = row.get(0);
    let created_at: time::OffsetDateTime = row.get(1);

    tx.execute(
        "INSERT INTO postings(txn_id,account_id,direction,amount_units) VALUES($1::uuid,$2,'DEBIT',$3),($1::uuid,$4,'CREDIT',$3)",
        &[&txn_id, &from_account, &amount_units, &to_account],
    ).await?;

    let neg_amount = -amount_units;
    tx.execute(
        "INSERT INTO balances(account_id,balance_units) VALUES($1,$2) ON CONFLICT (account_id) DO UPDATE SET balance_units=balances.balance_units + EXCLUDED.balance_units, updated_at=now()",
        &[&from_account, &neg_amount],
    ).await?;
    tx.execute(
        "INSERT INTO balances(account_id,balance_units) VALUES($1,$2) ON CONFLICT (account_id) DO UPDATE SET balance_units=balances.balance_units + EXCLUDED.balance_units, updated_at=now()",
        &[&to_account, &amount_units],
    ).await?;

    let payload = json!({
        "event_id": "generated_by_db",
        "type": "TransferPosted",
        "transaction_id": txn_id,
        "request_id": request_id,
        "zone_id": zone_id,
        "amount_units": amount_units,
        "created_at": fmt_rfc3339(created_at),
    });
    tx.execute(
        "INSERT INTO outbox_events(event_type,aggregate_type,aggregate_id,payload) VALUES('TransferPosted','transaction',$1,$2)",
        &[&txn_id, &payload],
    ).await?;

    Ok((txn_id, created_at))
}

/// Apply a transfer bypassing zone gating (used by spool replay).
/// Idempotency is still enforced.
pub async fn apply_transfer_bypass(
    st: &AppState,
    inp: &TransferInput<'_>,
) -> Result<String, AppError> {
    let TransferInput { request_id, payload_hash, from_account, to_account, zone_id, .. } = inp;
    let mut client = st.db.get().await?;
    let tx = client.transaction().await?;

    // idempotency check
    let existing = tx
        .query_opt("SELECT id::text, payload_hash FROM transactions WHERE request_id=$1", &[&request_id])
        .await?;
    if let Some(r) = existing {
        let ph: String = r.get(1);
        if ph != *payload_hash {
            return Err(AppError::Conflict("idempotency conflict".into()));
        }
        tx.commit().await?;
        return Ok(r.get(0));
    }

    tx.execute(
        "INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&from_account, &zone_id],
    ).await?;
    tx.execute(
        "INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&to_account, &zone_id],
    ).await?;

    let (txn_id, _) = apply_transfer_inner(&tx, inp).await?;

    tx.commit().await?;
    Ok(txn_id)
}
