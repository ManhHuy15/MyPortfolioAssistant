use common::response::ApiResponse;
use worker::{Env, Request, Response};

use super::response::json_response;

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn is_valid_api_key(expected: &str, provided: Option<&str>) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }

    let Some(provided) = provided.map(str::trim).filter(|k| !k.is_empty()) else {
        return false;
    };

    constant_time_eq(expected.as_bytes(), provided.as_bytes())
}

pub fn verify_api_key(req: &Request, env: &Env) -> std::result::Result<(), Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();

    let expected = env
        .secret("API_KEY")
        .or_else(|_| env.var("API_KEY"))
        .map(|v| v.to_string())
        .unwrap_or_default();

    if expected.trim().is_empty() {
        let err_env = ApiResponse::<()>::error(
            "INTERNAL_ERROR",
            "Server API_KEY is not configured",
            None,
            trace_id,
        );
        let resp = json_response(500, &err_env)
            .unwrap_or_else(|_| Response::error("Server API_KEY is not configured", 500).unwrap());
        return Err(resp);
    }

    let x_api_key = req.headers().get("x-api-key").ok().flatten();

    if is_valid_api_key(&expected, x_api_key.as_deref()) {
        Ok(())
    } else {
        let err_env =
            ApiResponse::<()>::error("UNAUTHORIZED", "Invalid or missing API key", None, trace_id);
        let resp = json_response(401, &err_env)
            .unwrap_or_else(|_| Response::error("Unauthorized", 401).unwrap());
        Err(resp)
    }
}
