use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;
use crate::state::AppState;
use crate::util::fmt_rfc3339;

#[derive(Deserialize)]
pub struct IncidentQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 { 100 }

fn format_incident(r: &tokio_postgres::Row) -> serde_json::Value {
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
}

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

    let incs: Vec<serde_json::Value> = rows.iter().map(format_incident).collect();
    Ok(Json(json!({ "incidents": incs })))
}

pub async fn list_recent_incidents(
    State(st): State<AppState>,
    Query(q): Query<IncidentQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.clamp(1, 2000);
    let client = st.db.get().await?;
    let rows = client
        .query(
            "SELECT id::text, zone_id, severity, status, title, details, detected_at FROM incidents ORDER BY detected_at DESC LIMIT $1",
            &[&limit],
        )
        .await?;

    let incs: Vec<serde_json::Value> = rows.iter().map(format_incident).collect();
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

    Ok(Json(format_incident(&row)))
}

#[derive(Deserialize)]
pub struct IncidentActionRequest {
    pub action: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub reason: String,
}

pub async fn apply_incident_action(
    State(st): State<AppState>,
    Path(incident_id): Path<String>,
    Json(req): Json<IncidentActionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if req.actor.is_empty() {
        return Err(AppError::BadRequest("actor required".into()));
    }
    if req.action != "ACK" && req.action != "ASSIGN" && req.action != "RESOLVE" {
        return Err(AppError::BadRequest("action must be ACK, ASSIGN, or RESOLVE".into()));
    }
    if req.action == "ASSIGN" && req.assignee.is_empty() {
        return Err(AppError::BadRequest("assignee required for ASSIGN".into()));
    }

    let mut client = st.db.get().await?;
    let tx = client.transaction().await?;

    // fetch current incident
    let current = tx
        .query_one(
            "SELECT id::text, zone_id, severity, status, title, details, detected_at FROM incidents WHERE id=$1::uuid",
            &[&incident_id],
        )
        .await
        .map_err(|_| AppError::NotFound("incident not found".into()))?;

    let mut details: serde_json::Value = current.get("details");

    // mutate details
    if req.action == "ASSIGN" {
        details.as_object_mut().map(|m| m.insert("assignee".into(), json!(req.assignee)));
    }
    if !req.note.is_empty() {
        let entry = json!({
            "at": time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap(),
            "actor": req.actor,
            "note": req.note,
            "action": req.action,
        });
        let notes = details
            .as_object_mut()
            .and_then(|m| m.entry("notes").or_insert(json!([])).as_array_mut().cloned())
            .unwrap_or_default();
        let mut notes = notes;
        notes.push(entry);
        details.as_object_mut().map(|m| m.insert("notes".into(), json!(notes)));
    }

    let new_status = match req.action.as_str() {
        "ACK" => "ACK",
        "RESOLVE" => "RESOLVED",
        _ => current.get::<_, &str>("status"),
    };

    let details_str = serde_json::to_string(&details).unwrap();
    let updated = tx
        .query_one(
            "UPDATE incidents SET status=$2, details=$3::jsonb WHERE id=$1::uuid RETURNING id::text, zone_id, severity, status, title, details, detected_at",
            &[&incident_id, &new_status, &details_str],
        )
        .await?;

    let audit_action = format!("INCIDENT_{}", req.action);
    tx.execute(
        "INSERT INTO audit_log(actor,action,target_type,target_id,reason,details) VALUES($1,$2,'incident',$3,$4, jsonb_build_object('assignee',$5,'note',$6,'status',$7))",
        &[&req.actor, &audit_action, &incident_id, &req.reason, &req.assignee, &req.note, &new_status],
    ).await?;

    tx.commit().await?;

    Ok(Json(format_incident(&updated)))
}
