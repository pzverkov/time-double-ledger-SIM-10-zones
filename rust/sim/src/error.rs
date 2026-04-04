use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            Self::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            Self::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", m),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
        };
        (status, Json(json!({ "error": message, "code": code }))).into_response()
    }
}

impl From<deadpool_postgres::PoolError> for AppError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<tokio_postgres::Error> for AppError {
    fn from(e: tokio_postgres::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    async fn error_body(err: AppError) -> (StatusCode, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn bad_request_returns_400_with_json() {
        let (status, body) = error_body(AppError::BadRequest("bad field".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "bad_request");
        assert_eq!(body["error"], "bad field");
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let (status, body) = error_body(AppError::NotFound("missing".into())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "not_found");
    }

    #[tokio::test]
    async fn conflict_returns_409() {
        let (status, body) = error_body(AppError::Conflict("dup".into())).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["code"], "conflict");
    }

    #[tokio::test]
    async fn unavailable_returns_503() {
        let (status, body) = error_body(AppError::Unavailable("zone down".into())).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "unavailable");
    }

    #[tokio::test]
    async fn internal_returns_500() {
        let (status, body) = error_body(AppError::Internal("db error".into())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "internal");
    }
}
