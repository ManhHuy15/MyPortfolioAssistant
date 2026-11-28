use worker::{Env, Request, Response, Result};

const DEFAULT_ALLOW_METHODS: &str = "GET, POST, DELETE, OPTIONS";
const DEFAULT_ALLOW_HEADERS: &str = "Content-Type, X-API-Key, Authorization";
const DEFAULT_EXPOSE_HEADERS: &str = "Retry-After";
const DEFAULT_MAX_AGE: &str = "86400";

/// Resolves the value for `Access-Control-Allow-Origin` given the request's `Origin`
/// header and the configured `CORS_ALLOWED_ORIGINS` value (comma-separated list or `*`).
pub fn resolve_allowed_origin(req_origin: Option<&str>, allowed_origins: &str) -> Option<String> {
    let allowed = allowed_origins.trim();
    if allowed.is_empty() || allowed == "*" {
        return Some("*".to_string());
    }

    let origin = req_origin?.trim().trim_end_matches('/');
    if origin.is_empty() {
        return None;
    }

    for candidate in allowed
        .split(',')
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
    {
        if candidate == "*" {
            return Some("*".to_string());
        }
        if candidate.eq_ignore_ascii_case(origin) {
            return Some(origin.to_string());
        }
    }

    None
}

pub fn load_allowed_origins(env: &Env) -> String {
    env.var("CORS_ALLOWED_ORIGINS")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "*".to_string())
}

pub fn handle_cors_preflight(req: &Request, env: &Env) -> Result<Response> {
    let req_origin = req.headers().get("origin").ok().flatten();
    let allowed_origins = load_allowed_origins(env);

    let mut resp = Response::empty()?.with_status(204);
    apply_cors_headers(&mut resp, req_origin.as_deref(), &allowed_origins)?;
    resp.headers_mut()
        .set("Access-Control-Max-Age", DEFAULT_MAX_AGE)?;
    Ok(resp)
}

pub fn apply_cors_headers(
    resp: &mut Response,
    req_origin: Option<&str>,
    allowed_origins: &str,
) -> Result<()> {
    if let Some(allow_origin) = resolve_allowed_origin(req_origin, allowed_origins) {
        let headers = resp.headers_mut();
        headers.set("Access-Control-Allow-Origin", &allow_origin)?;
        headers.set("Access-Control-Allow-Methods", DEFAULT_ALLOW_METHODS)?;
        headers.set("Access-Control-Allow-Headers", DEFAULT_ALLOW_HEADERS)?;
        headers.set("Access-Control-Expose-Headers", DEFAULT_EXPOSE_HEADERS)?;
        if allow_origin != "*" {
            headers.set("Vary", "Origin")?;
        }
    }

    Ok(())
}
