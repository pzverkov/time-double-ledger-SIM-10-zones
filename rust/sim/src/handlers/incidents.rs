use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde_json::json;

use crate::state::AppState;
use crate::util::fmt_rfc3339;

pub async fn list_incidents_by_zone(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = client
        .query(
            "SELECT id::text, zone_id, severity, status, title, details, detected_at FROM incidents WHERE zone_id=$1 ORDER BY detected_at DESC LIMIT 200",
            &[&zone_id],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let incs: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let dt: time::OffsetDateTime = r.get("detected_at");
            json!({
                "id": r.get::<_, String>("id"),
                "zone_id": r.get::<_, String>("zone_id"),
                "severity": r.get::<_, String>("severity"),
                "status": r.get::<_, String>("status"),
                "title": r.get::<_, String>("title"),
                "details": r.get::<_, serde_json::Value>("details"),
                "detected_at": fmt_rfc3339(dt),
            })
        })
        .collect();

    Ok(Json(json!({ "incidents": incs })))
}

pub async fn get_incident(
    State(st): State<AppState>,
    Path(incident_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = st.db.get().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = client
        .query_one(
            "SELECT id::text, zone_id, severity, status, title, details, detected_at FROM incidents WHERE id=$1::uuid",
            &[&incident_id],
        )
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let dt: time::OffsetDateTime = row.get("detected_at");
    Ok(Json(json!({
        "id": row.get::<_, String>("id"),
        "zone_id": row.get::<_, String>("zone_id"),
        "severity": row.get::<_, String>("severity"),
        "status": row.get::<_, String>("status"),
        "title": row.get::<_, String>("title"),
        "details": row.get::<_, serde_json::Value>("details"),
        "detected_at": fmt_rfc3339(dt),
    })))
}
