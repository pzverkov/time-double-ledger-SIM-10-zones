use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use crate::config::THROTTLE_MAX_PCT;
use crate::error::AppError;
use crate::handlers::admin;
use crate::state::AppState;
use crate::util::fmt_rfc3339;

#[derive(Serialize)]
pub struct ZoneControls {
    pub zone_id: String,
    pub writes_blocked: bool,
    pub cross_zone_throttle: i32,
    pub spool_enabled: bool,
    pub updated_at: String,
}

pub async fn get_zone_controls(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
) -> Result<Json<ZoneControls>, AppError> {
    let client = st.db.get().await?;
    let row = client
        .query_opt(
            "SELECT zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at FROM zone_controls WHERE zone_id=$1",
            &[&zone_id],
        )
        .await?;

    if let Some(r) = row {
        let updated_at: time::OffsetDateTime = r.get("updated_at");
        return Ok(Json(ZoneControls {
            zone_id: r.get("zone_id"),
            writes_blocked: r.get("writes_blocked"),
            cross_zone_throttle: r.get("cross_zone_throttle"),
            spool_enabled: r.get("spool_enabled"),
            updated_at: fmt_rfc3339(updated_at),
        }));
    }

    // lazy-init default row
    client
        .execute(
            "INSERT INTO zone_controls(zone_id) VALUES($1) ON CONFLICT DO NOTHING",
            &[&zone_id],
        )
        .await?;

    let r = client
        .query_one(
            "SELECT zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at FROM zone_controls WHERE zone_id=$1",
            &[&zone_id],
        )
        .await?;
    let updated_at: time::OffsetDateTime = r.get("updated_at");
    Ok(Json(ZoneControls {
        zone_id: r.get("zone_id"),
        writes_blocked: r.get("writes_blocked"),
        cross_zone_throttle: r.get("cross_zone_throttle"),
        spool_enabled: r.get("spool_enabled"),
        updated_at: fmt_rfc3339(updated_at),
    }))
}

#[derive(Deserialize)]
pub struct SetZoneControlsRequest {
    pub writes_blocked: Option<bool>,
    pub cross_zone_throttle: Option<i32>,
    pub spool_enabled: Option<bool>,
    #[serde(default)]
    pub reason: String,
}

pub async fn set_zone_controls(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SetZoneControlsRequest>,
) -> Result<Json<ZoneControls>, AppError> {
    let actor = admin::authenticate_operator(&st, &headers)?;
    let wb = req.writes_blocked.unwrap_or(false);
    let throttle = req.cross_zone_throttle.unwrap_or(THROTTLE_MAX_PCT);
    let spool = req.spool_enabled.unwrap_or(false);

    if !(0..=THROTTLE_MAX_PCT).contains(&throttle) {
        return Err(AppError::BadRequest(
            "cross_zone_throttle must be 0-100".into(),
        ));
    }

    let mut client = st.db.get().await?;
    let tx = client.transaction().await?;

    // ensure row exists
    tx.execute(
        "INSERT INTO zone_controls(zone_id) VALUES($1) ON CONFLICT DO NOTHING",
        &[&zone_id],
    )
    .await?;

    let r = tx
        .query_one(
            "UPDATE zone_controls SET writes_blocked=$2, cross_zone_throttle=$3, spool_enabled=$4, updated_at=now() WHERE zone_id=$1 RETURNING zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at",
            &[&zone_id, &wb, &throttle, &spool],
        )
        .await?;

    tx.execute(
        "INSERT INTO audit_log(actor,action,target_type,target_id,reason,details) VALUES($1,'SET_ZONE_CONTROLS','zone',$2,$3, jsonb_build_object('writes_blocked',$4::bool,'cross_zone_throttle',$5::int,'spool_enabled',$6::bool))",
        &[&actor, &zone_id, &req.reason, &wb, &throttle, &spool],
    )
    .await?;

    if wb || throttle == 0 {
        let sev = if wb { "CRITICAL" } else { "WARN" };
        let title = if wb {
            "Writes blocked by operator"
        } else {
            "Zone controls tightened"
        };
        tx.execute(
            "INSERT INTO incidents(zone_id,severity,title,details) VALUES($1,$2,$3, jsonb_build_object('reason',$4::text,'actor',$5::text,'writes_blocked',$6::bool,'cross_zone_throttle',$7::int,'spool_enabled',$8::bool))",
            &[&zone_id, &sev, &title, &req.reason, &actor, &wb, &throttle, &spool],
        )
        .await?;
    }

    tx.commit().await?;

    let updated_at: time::OffsetDateTime = r.get("updated_at");
    Ok(Json(ZoneControls {
        zone_id: r.get("zone_id"),
        writes_blocked: r.get("writes_blocked"),
        cross_zone_throttle: r.get("cross_zone_throttle"),
        spool_enabled: r.get("spool_enabled"),
        updated_at: fmt_rfc3339(updated_at),
    }))
}
