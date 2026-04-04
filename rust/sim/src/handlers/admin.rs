use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use serde_json::json;
use std::env;

use crate::error::AppError;
use crate::state::AppState;
use crate::util::fmt_rfc3339;

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[derive(serde::Serialize)]
struct VersionInfo {
    service: &'static str,
    language: &'static str,
    version: &'static str,
    revision: Option<&'static str>,
    build_time: Option<&'static str>,
}

pub async fn version() -> impl IntoResponse {
    Json(VersionInfo {
        service: "time-ledger-sim",
        language: "rust",
        version: env!("CARGO_PKG_VERSION"),
        revision: option_env!("GIT_SHA"),
        build_time: option_env!("BUILD_TIME"),
    })
}

pub async fn metrics(State(st): State<AppState>) -> impl IntoResponse {
    use prometheus::Encoder;
    let mf = st.registry.gather();
    let mut buf = Vec::new();
    let enc = prometheus::TextEncoder::new();
    enc.encode(&mf, &mut buf).unwrap();
    (StatusCode::OK, String::from_utf8_lossy(&buf).to_string())
}

pub fn admin_guard(st: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    match &st.admin_key {
        None => Err(StatusCode::FORBIDDEN),
        Some(k) => {
            let got = headers
                .get("x-admin-key")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if got == k { Ok(()) } else { Err(StatusCode::FORBIDDEN) }
        }
    }
}

pub async fn snapshot(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    admin_guard(&st, &headers).map_err(|_| AppError::BadRequest("forbidden".into()))?;
    let client = st.db.get().await?;

    let mut snap = json!({
        "version": "v2",
        "created_at": fmt_rfc3339(time::OffsetDateTime::now_utc()),
        "note": "Restore resets transaction history; balances/incidents/controls/spool/audit are restored.",
    });

    // zones
    let rows = client.query("SELECT id,name,status,updated_at FROM zones ORDER BY id", &[]).await?;
    let zones: Vec<serde_json::Value> = rows.iter().map(|r| {
        let dt: time::OffsetDateTime = r.get("updated_at");
        json!({"id": r.get::<_,String>("id"), "name": r.get::<_,String>("name"), "status": r.get::<_,String>("status"), "updated_at": fmt_rfc3339(dt)})
    }).collect();
    snap["zones"] = json!(zones);

    // zone controls
    let rows = client.query("SELECT zone_id, writes_blocked, cross_zone_throttle, spool_enabled, updated_at FROM zone_controls ORDER BY zone_id", &[]).await?;
    let ctrls: Vec<serde_json::Value> = rows.iter().map(|r| {
        let dt: time::OffsetDateTime = r.get("updated_at");
        json!({"zone_id": r.get::<_,String>("zone_id"), "writes_blocked": r.get::<_,bool>("writes_blocked"), "cross_zone_throttle": r.get::<_,i32>("cross_zone_throttle"), "spool_enabled": r.get::<_,bool>("spool_enabled"), "updated_at": fmt_rfc3339(dt)})
    }).collect();
    snap["zone_controls"] = json!(ctrls);

    // accounts + balances
    let rows = client.query("SELECT a.id, a.zone_id, COALESCE(b.balance_units,0) as balance_units FROM accounts a LEFT JOIN balances b ON b.account_id=a.id ORDER BY a.id LIMIT 20000", &[]).await?;
    let accts: Vec<serde_json::Value> = rows.iter().map(|r| {
        json!({"id": r.get::<_,String>("id"), "zone_id": r.get::<_,String>("zone_id"), "balance_units": r.get::<_,i64>("balance_units")})
    }).collect();
    snap["accounts"] = json!(accts);

    // incidents
    let rows = client.query("SELECT id::text, zone_id, related_txn_id::text, severity, status, title, details, detected_at FROM incidents ORDER BY detected_at DESC LIMIT 5000", &[]).await?;
    let incs: Vec<serde_json::Value> = rows.iter().map(|r| {
        let dt: time::OffsetDateTime = r.get("detected_at");
        let rel: Option<String> = r.get("related_txn_id");
        json!({"id": r.get::<_,String>("id"), "zone_id": r.get::<_,String>("zone_id"), "related_txn_id": rel, "severity": r.get::<_,String>("severity"), "status": r.get::<_,String>("status"), "title": r.get::<_,String>("title"), "details": r.get::<_,serde_json::Value>("details"), "detected_at": fmt_rfc3339(dt)})
    }).collect();
    snap["incidents"] = json!(incs);

    // spooled transfers
    let rows = client.query("SELECT id::text, request_id, payload_hash, from_account, to_account, amount_units, zone_id, metadata, status, fail_reason, created_at, updated_at, applied_at FROM spooled_transfers ORDER BY created_at DESC LIMIT 5000", &[]).await?;
    let spools: Vec<serde_json::Value> = rows.iter().map(|r| {
        let ca: time::OffsetDateTime = r.get("created_at");
        let ua: time::OffsetDateTime = r.get("updated_at");
        let aa: Option<time::OffsetDateTime> = r.get("applied_at");
        json!({
            "id": r.get::<_,String>("id"), "request_id": r.get::<_,String>("request_id"),
            "payload_hash": r.get::<_,String>("payload_hash"),
            "from_account": r.get::<_,String>("from_account"), "to_account": r.get::<_,String>("to_account"),
            "amount_units": r.get::<_,i64>("amount_units"), "zone_id": r.get::<_,String>("zone_id"),
            "metadata": r.get::<_,serde_json::Value>("metadata"),
            "status": r.get::<_,String>("status"), "fail_reason": r.get::<_,Option<String>>("fail_reason"),
            "created_at": fmt_rfc3339(ca), "updated_at": fmt_rfc3339(ua),
            "applied_at": aa.map(fmt_rfc3339),
        })
    }).collect();
    snap["spooled_transfers"] = json!(spools);

    // audit tail
    let rows = client.query("SELECT id::text, actor, action, target_type, target_id, reason, details, created_at FROM audit_log ORDER BY created_at DESC LIMIT 2000", &[]).await?;
    let audits: Vec<serde_json::Value> = rows.iter().map(|r| {
        let dt: time::OffsetDateTime = r.get("created_at");
        json!({"id": r.get::<_,String>("id"), "actor": r.get::<_,String>("actor"), "action": r.get::<_,String>("action"), "target_type": r.get::<_,String>("target_type"), "target_id": r.get::<_,String>("target_id"), "reason": r.get::<_,Option<String>>("reason"), "details": r.get::<_,serde_json::Value>("details"), "created_at": fmt_rfc3339(dt)})
    }).collect();
    snap["audit_log"] = json!(audits);

    Ok(Json(snap))
}

pub async fn restore(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(snap): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    admin_guard(&st, &headers).map_err(|_| AppError::BadRequest("forbidden".into()))?;
    let mut client = st.db.get().await?;
    let tx = client.transaction().await?;

    // truncate mutable tables
    for table in &[
        "postings", "transactions", "balances", "accounts", "incidents",
        "outbox_events", "inbox_events", "audit_log", "spooled_transfers", "zone_controls",
    ] {
        tx.execute(&format!("TRUNCATE TABLE {table} RESTART IDENTITY CASCADE"), &[]).await?;
    }

    // zones: update statuses
    if let Some(zs) = snap.get("zones").and_then(|v| v.as_array()) {
        for z in zs {
            let id = z.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let status = z.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if !id.is_empty() && (status == "OK" || status == "DEGRADED" || status == "DOWN") {
                tx.execute("UPDATE zones SET status=$2, updated_at=now() WHERE id=$1", &[&id, &status]).await?;
            }
        }
    }

    // zone controls
    if let Some(cs) = snap.get("zone_controls").and_then(|v| v.as_array()) {
        for c in cs {
            let zid = c.get("zone_id").and_then(|v| v.as_str()).unwrap_or("");
            if zid.is_empty() { continue; }
            let wb = c.get("writes_blocked").and_then(|v| v.as_bool()).unwrap_or(false);
            let thr = c.get("cross_zone_throttle").and_then(|v| v.as_i64()).unwrap_or(100) as i32;
            let sp = c.get("spool_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            tx.execute(
                "INSERT INTO zone_controls(zone_id,writes_blocked,cross_zone_throttle,spool_enabled,updated_at) VALUES($1,$2,$3,$4,now()) ON CONFLICT (zone_id) DO UPDATE SET writes_blocked=EXCLUDED.writes_blocked, cross_zone_throttle=EXCLUDED.cross_zone_throttle, spool_enabled=EXCLUDED.spool_enabled, updated_at=now()",
                &[&zid, &wb, &thr, &sp],
            ).await?;
        }
    } else {
        tx.execute("INSERT INTO zone_controls(zone_id) SELECT id FROM zones ON CONFLICT DO NOTHING", &[]).await?;
    }

    // accounts + balances
    if let Some(acs) = snap.get("accounts").and_then(|v| v.as_array()) {
        for a in acs {
            let id = a.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() { continue; }
            let zid = a.get("zone_id").and_then(|v| v.as_str()).unwrap_or("zone-eu");
            let bal = a.get("balance_units").and_then(|v| v.as_i64()).unwrap_or(0);
            tx.execute("INSERT INTO accounts(id, zone_id) VALUES($1,$2) ON CONFLICT DO NOTHING", &[&id, &zid]).await?;
            tx.execute("INSERT INTO balances(account_id,balance_units,updated_at) VALUES($1,$2,now()) ON CONFLICT (account_id) DO UPDATE SET balance_units=EXCLUDED.balance_units, updated_at=now()", &[&id, &bal]).await?;
        }
    }

    // incidents
    if let Some(ins) = snap.get("incidents").and_then(|v| v.as_array()) {
        for i in ins {
            let zid = i.get("zone_id").and_then(|v| v.as_str()).unwrap_or("");
            let title = i.get("title").and_then(|v| v.as_str()).unwrap_or("");
            if zid.is_empty() || title.is_empty() { continue; }
            let sev = i.get("severity").and_then(|v| v.as_str()).unwrap_or("INFO");
            let st = i.get("status").and_then(|v| v.as_str()).unwrap_or("OPEN");
            let rel = i.get("related_txn_id").and_then(|v| v.as_str());
            let details = i.get("details").unwrap_or(&serde_json::Value::Null);
            let details_str = serde_json::to_string(details).unwrap_or_default();
            if let Some(r) = rel {
                tx.execute("INSERT INTO incidents(zone_id,related_txn_id,severity,status,title,details) VALUES($1,$2::uuid,$3,$4,$5,$6::jsonb)", &[&zid, &r, &sev, &st, &title, &details_str]).await?;
            } else {
                tx.execute("INSERT INTO incidents(zone_id,severity,status,title,details) VALUES($1,$2,$3,$4,$5::jsonb)", &[&zid, &sev, &st, &title, &details_str]).await?;
            }
        }
    }

    // spooled transfers
    if let Some(sp) = snap.get("spooled_transfers").and_then(|v| v.as_array()) {
        for s in sp {
            let req = s.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
            if req.is_empty() { continue; }
            let ph = s.get("payload_hash").and_then(|v| v.as_str()).unwrap_or("");
            let from = s.get("from_account").and_then(|v| v.as_str()).unwrap_or("");
            let to = s.get("to_account").and_then(|v| v.as_str()).unwrap_or("");
            let amt = s.get("amount_units").and_then(|v| v.as_i64()).unwrap_or(0);
            let zid = s.get("zone_id").and_then(|v| v.as_str()).unwrap_or("");
            let st = s.get("status").and_then(|v| v.as_str()).unwrap_or("PENDING");
            let fail = s.get("fail_reason").and_then(|v| v.as_str());
            let meta = s.get("metadata").unwrap_or(&serde_json::Value::Null);
            let meta_str = serde_json::to_string(meta).unwrap_or_default();
            tx.execute(
                "INSERT INTO spooled_transfers(request_id,payload_hash,from_account,to_account,amount_units,zone_id,metadata,status,fail_reason,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9,now()) ON CONFLICT (request_id) DO NOTHING",
                &[&req, &ph, &from, &to, &amt, &zid, &meta_str, &st, &fail],
            ).await?;
        }
    }

    // audit tail
    if let Some(al) = snap.get("audit_log").and_then(|v| v.as_array()) {
        for a in al {
            let actor = a.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let action = a.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let tt = a.get("target_type").and_then(|v| v.as_str()).unwrap_or("");
            let tid = a.get("target_id").and_then(|v| v.as_str()).unwrap_or("");
            if actor.is_empty() || action.is_empty() || tt.is_empty() || tid.is_empty() { continue; }
            let reason = a.get("reason").and_then(|v| v.as_str());
            let details = a.get("details").unwrap_or(&serde_json::Value::Null);
            let details_str = serde_json::to_string(details).unwrap_or_default();
            tx.execute(
                "INSERT INTO audit_log(actor,action,target_type,target_id,reason,details,created_at) VALUES($1,$2,$3,$4,$5,$6::jsonb,now())",
                &[&actor, &action, &tt, &tid, &reason, &details_str],
            ).await?;
        }
    }

    tx.commit().await?;
    Ok(Json(json!({"status": "ok"})))
}
