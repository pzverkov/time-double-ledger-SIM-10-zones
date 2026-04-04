use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;
use crate::handlers::transfers::{apply_transfer_bypass, TransferInput};

#[derive(Serialize)]
pub struct SpoolStats {
    pub zone_id: String,
    pub pending: i64,
    pub applied: i64,
    pub failed: i64,
}

pub async fn get_spool_stats(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
) -> Result<Json<SpoolStats>, AppError> {
    let client = st.db.get().await?;
    let row = client
        .query_one(
            "SELECT COUNT(*) FILTER (WHERE status='PENDING') as pending, COUNT(*) FILTER (WHERE status='APPLIED') as applied, COUNT(*) FILTER (WHERE status='FAILED') as failed FROM spooled_transfers WHERE zone_id=$1",
            &[&zone_id],
        )
        .await?;

    Ok(Json(SpoolStats {
        zone_id,
        pending: row.get("pending"),
        applied: row.get("applied"),
        failed: row.get("failed"),
    }))
}

#[derive(Deserialize)]
pub struct ReplayRequest {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub reason: String,
}

fn default_limit() -> i64 { 50 }

#[derive(Serialize)]
pub struct ReplayResult {
    pub zone_id: String,
    pub applied: i64,
    pub failed: i64,
}

pub async fn replay_spool(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
    Json(req): Json<ReplayRequest>,
) -> Result<Json<ReplayResult>, AppError> {
    let limit = req.limit.clamp(1, 500);
    let client = st.db.get().await?;

    // check zone readiness
    let status_row = client
        .query_one("SELECT status FROM zones WHERE id=$1", &[&zone_id])
        .await?;
    let status: String = status_row.get(0);

    let ctrl_row = client
        .query_opt("SELECT writes_blocked, cross_zone_throttle FROM zone_controls WHERE zone_id=$1", &[&zone_id])
        .await?;
    let (wb, throttle) = ctrl_row
        .map(|r| (r.get::<_, bool>("writes_blocked"), r.get::<_, i32>("cross_zone_throttle")))
        .unwrap_or((false, 100));

    if status == "DOWN" || wb || throttle == 0 {
        return Err(AppError::Conflict("zone not ready for replay".into()));
    }

    // fetch pending spooled transfers
    let rows = client
        .query(
            "SELECT id::text, request_id, payload_hash, from_account, to_account, amount_units, zone_id, metadata FROM spooled_transfers WHERE zone_id=$1 AND status='PENDING' ORDER BY created_at ASC LIMIT $2",
            &[&zone_id, &limit],
        )
        .await?;

    let mut applied: i64 = 0;
    let mut failed: i64 = 0;

    for row in &rows {
        let spool_id: String = row.get("id");
        let request_id: String = row.get("request_id");
        let payload_hash: String = row.get("payload_hash");
        let from_account: String = row.get("from_account");
        let to_account: String = row.get("to_account");
        let amount_units: i64 = row.get("amount_units");
        let zone_id_val: String = row.get("zone_id");
        let metadata: serde_json::Value = row.get("metadata");

        let result = apply_transfer_bypass(&st, &TransferInput {
            request_id: &request_id, payload_hash: &payload_hash,
            from_account: &from_account, to_account: &to_account,
            amount_units, zone_id: &zone_id_val, metadata: &metadata,
        }).await;

        match result {
            Ok(_) => {
                applied += 1;
                let _ = client
                    .execute(
                        "UPDATE spooled_transfers SET status='APPLIED', updated_at=now(), applied_at=now(), fail_reason=NULL WHERE id=$1::uuid",
                        &[&spool_id],
                    )
                    .await;
            }
            Err(e) => {
                failed += 1;
                let reason = format!("{e:?}");
                let _ = client
                    .execute(
                        "UPDATE spooled_transfers SET status='FAILED', updated_at=now(), fail_reason=$2 WHERE id=$1::uuid",
                        &[&spool_id, &reason],
                    )
                    .await;
            }
        }
    }

    // audit summary
    let _ = client
        .execute(
            "INSERT INTO audit_log(actor,action,target_type,target_id,reason,details) VALUES($1,'REPLAY_SPOOL','zone',$2,$3, jsonb_build_object('applied',$4,'failed',$5,'limit',$6))",
            &[&req.actor, &zone_id, &req.reason, &applied, &failed, &limit],
        )
        .await;

    Ok(Json(ReplayResult { zone_id, applied, failed }))
}
