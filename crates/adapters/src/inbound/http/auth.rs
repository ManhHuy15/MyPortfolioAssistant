use common::response::ApiResponse;
use worker::{Env, Request, Response};

use super::response::json_response;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyRole {
    Chat,
    Master,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredPermission {
    Chat,
    Master,
}

impl ApiKeyRole {
    pub fn has_permission(self, required: RequiredPermission) -> bool {
        match (self, required) {
            (ApiKeyRole::Master, _) => true,
            (ApiKeyRole::Chat, RequiredPermission::Chat) => true,
            (ApiKeyRole::Chat, RequiredPermission::Master) => false,
        }
    }
}

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

pub fn resolve_api_key_role(
    master_key: Option<&str>,
    chat_key: Option<&str>,
    provided: Option<&str>,
) -> Option<ApiKeyRole> {
    if let Some(master) = master_key
        && is_valid_api_key(master, provided)
    {
        return Some(ApiKeyRole::Master);
    }

    if let Some(chat) = chat_key
        && is_valid_api_key(chat, provided)
    {
        return Some(ApiKeyRole::Chat);
    }

    None
}

fn read_secret_or_var(env: &Env, name: &str) -> Option<String> {
    env.secret(name)
        .or_else(|_| env.var(name))
        .ok()
        .map(|v| v.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn verify_api_key(
    req: &Request,
    env: &Env,
    required: RequiredPermission,
) -> std::result::Result<ApiKeyRole, Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();

    let master_key = read_secret_or_var(env, "MASTER_API_KEY");
    let chat_key = read_secret_or_var(env, "API_KEY");

    match required {
        RequiredPermission::Master if master_key.is_none() => {
            let err_env = ApiResponse::<()>::error(
                "INTERNAL_ERROR",
                "Server MASTER_API_KEY is not configured",
                None,
                trace_id,
            );
            let resp = json_response(500, &err_env).unwrap_or_else(|_| {
                Response::error("Server MASTER_API_KEY is not configured", 500).unwrap()
            });
            return Err(resp);
        }
        RequiredPermission::Chat if master_key.is_none() && chat_key.is_none() => {
            let err_env = ApiResponse::<()>::error(
                "INTERNAL_ERROR",
                "Server API_KEY is not configured",
                None,
                trace_id,
            );
            let resp = json_response(500, &err_env).unwrap_or_else(|_| {
                Response::error("Server API_KEY is not configured", 500).unwrap()
            });
            return Err(resp);
        }
        _ => {}
    }

    let x_api_key = req.headers().get("x-api-key").ok().flatten();

    match resolve_api_key_role(
        master_key.as_deref(),
        chat_key.as_deref(),
        x_api_key.as_deref(),
    ) {
        Some(role) if role.has_permission(required) => Ok(role),
        Some(_) => {
            let err_env = ApiResponse::<()>::error(
                "FORBIDDEN",
                "Insufficient permissions: master API key required",
                None,
                trace_id,
            );
            let resp = json_response(403, &err_env)
                .unwrap_or_else(|_| Response::error("Forbidden", 403).unwrap());
            Err(resp)
        }
        None => {
            let err_env = ApiResponse::<()>::error(
                "UNAUTHORIZED",
                "Invalid or missing API key",
                None,
                trace_id,
            );
            let resp = json_response(401, &err_env)
                .unwrap_or_else(|_| Response::error("Unauthorized", 401).unwrap());
            Err(resp)
        }
    }
}

pub fn verify_chat_api_key(req: &Request, env: &Env) -> std::result::Result<ApiKeyRole, Response> {
    verify_api_key(req, env, RequiredPermission::Chat)
}

pub fn verify_master_api_key(
    req: &Request,
    env: &Env,
) -> std::result::Result<ApiKeyRole, Response> {
    verify_api_key(req, env, RequiredPermission::Master)
}
