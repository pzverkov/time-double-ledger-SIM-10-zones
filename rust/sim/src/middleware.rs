use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};

pub async fn cors(req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let allowed = std::env::var("CORS_ALLOW_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:5173,http://localhost:4173".to_string());
    let allow_any = allowed.split(',').any(|x| x.trim() == "*");

    let mut allowed_origin: Option<String> = None;
    if let Some(o) = origin {
        if allow_any {
            allowed_origin = Some(o);
        } else {
            for a in allowed.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()) {
                if a == o {
                    allowed_origin = Some(o);
                    break;
                }
            }
        }
    }

    if req.method() == Method::OPTIONS {
        let mut res = Response::new(Body::empty());
        *res.status_mut() = StatusCode::NO_CONTENT;
        apply_cors_headers(&mut res, allowed_origin);
        return res;
    }

    let mut res = next.run(req).await;
    apply_cors_headers(&mut res, allowed_origin);
    res
}

fn apply_cors_headers(res: &mut Response, allowed_origin: Option<String>) {
    if let Some(o) = allowed_origin {
        if let Ok(v) = HeaderValue::from_str(&o) {
            res.headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
            res.headers_mut()
                .insert(header::VARY, HeaderValue::from_static("Origin"));
        }
        res.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET,POST,OPTIONS"),
        );
        res.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Content-Type,X-Admin-Key"),
        );
    }
}
