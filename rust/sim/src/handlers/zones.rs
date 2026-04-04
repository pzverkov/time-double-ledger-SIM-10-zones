use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;
use crate::util::fmt_rfc3339;

#[derive(Serialize)]
struct Zone {
    id: String,
    name: String,
    status: String,
    updated_at: String,
}

pub async fn list_zones(State(st): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = client
        .query("SELECT id,name,status,updated_at FROM zones ORDER BY id", &[])
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
                updated_at: fmt_rfc3339(updated_at),
            }
        })
        .collect();

    Ok(Json(json!({ "zones": zones })))
}

#[derive(Deserialize)]
pub struct SetZoneStatusRequest {
    status: String,
    actor: String,
    #[serde(default)]
    reason: String,
}

pub async fn set_zone_status(
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
    let mut client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tx = client.transaction().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = tx
        .query_one(
            "UPDATE zones SET status=$2, updated_at=now() WHERE id=$1 RETURNING id,name,status,updated_at",
            &[&zone_id, &req.status],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.execute(
        "INSERT INTO audit_log(actor,action,target_type,target_id,reason,details) VALUES($1,'SET_ZONE_STATUS','zone',$2,$3, jsonb_build_object('status',$4))",
        &[&req.actor, &zone_id, &req.reason, &req.status],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if req.status == "DOWN" {
        tx.execute(
            "INSERT INTO incidents(zone_id,severity,title,details) VALUES($1,'CRITICAL','Zone marked DOWN', jsonb_build_object('reason',$2,'actor',$3))",
            &[&zone_id, &req.reason, &req.actor],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id: String = row.get("id");
    let name: String = row.get("name");
    let status: String = row.get("status");
    let updated_at: time::OffsetDateTime = row.get("updated_at");
    Ok(Json(json!({
        "id": id, "name": name, "status": status,
        "updated_at": fmt_rfc3339(updated_at)
    })))
}
