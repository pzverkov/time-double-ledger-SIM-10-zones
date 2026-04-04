use axum::{extract::{Path, Query, State}, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;
use crate::state::AppState;
use crate::util::fmt_rfc3339;

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 { 100 }

#[derive(Serialize)]
struct AuditEntry {
    id: String,
    actor: String,
    action: String,
    target_type: String,
    target_id: String,
    reason: Option<String>,
    details: serde_json::Value,
    created_at: String,
}

pub async fn list_audit(
    State(st): State<AppState>,
    Path(zone_id): Path<String>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.clamp(1, 500);
    let client = st.db.get().await?;

    let rows = client
        .query(
            "(SELECT a.id::text, a.actor, a.action, a.target_type, a.target_id, a.reason, a.details, a.created_at \
             FROM audit_log a WHERE a.target_type='zone' AND a.target_id=$1 \
             ORDER BY a.created_at DESC LIMIT $2) \
             UNION ALL \
             (SELECT a.id::text, a.actor, a.action, a.target_type, a.target_id, a.reason, a.details, a.created_at \
             FROM audit_log a WHERE a.target_type='incident' AND a.target_id IN \
             (SELECT id::text FROM incidents WHERE zone_id=$1) \
             ORDER BY a.created_at DESC LIMIT $2) \
             ORDER BY created_at DESC LIMIT $2",
            &[&zone_id, &limit],
        )
        .await?;

    let entries: Vec<AuditEntry> = rows
        .into_iter()
        .map(|r| {
            let dt: time::OffsetDateTime = r.get("created_at");
            AuditEntry {
                id: r.get("id"),
                actor: r.get("actor"),
                action: r.get("action"),
                target_type: r.get("target_type"),
                target_id: r.get("target_id"),
                reason: r.get("reason"),
                details: r.get("details"),
                created_at: fmt_rfc3339(dt),
            }
        })
        .collect();

    Ok(Json(json!({ "audit": entries })))
}
