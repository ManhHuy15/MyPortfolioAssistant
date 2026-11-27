use application::dtos::{ChatRequestDto, ChatResponseDto};
use common::response::ApiResponse;
use domain::entities::SessionMeta;
use domain::errors::DomainError;
use domain::ports::inbound::{AskPortfolioCommand, AskPortfolioUseCasePort};
use worker::{Request, Response, Result};

use super::response::json_response;

pub async fn handle_chat(
    mut req: Request,
    use_case: &dyn AskPortfolioUseCasePort,
) -> Result<Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();

    let payload: ChatRequestDto = match req.json().await {
        Ok(p) => p,
        Err(_) => {
            let err_env = ApiResponse::<()>::error(
                "VALIDATION_FAILED",
                "Invalid JSON payload",
                None,
                trace_id,
            );
            return json_response(400, &err_env);
        }
    };

    let ip_address = req
        .headers()
        .get("cf-connecting-ip")
        .ok()
        .flatten()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let country = req
        .cf()
        .and_then(|cf| cf.country())
        .unwrap_or_else(|| "XX".to_string());
    let user_agent = req.headers().get("user-agent").ok().flatten();

    let cmd = AskPortfolioCommand {
        session: SessionMeta {
            session_id: payload.session_id,
            ip_address,
            country,
            user_agent,
        },
        question: payload.message,
    };

    match use_case.execute(cmd).await {
        Ok(res) => {
            let msg = if res.is_cached {
                "Success (cached)"
            } else {
                "Success"
            };
            let dto = ChatResponseDto {
                session_id: res.session_id,
                reply: res.reply,
                sources: res.sources,
                is_cached: res.is_cached,
            };
            let envelope = ApiResponse::success(dto, msg, trace_id);
            json_response(200, &envelope)
        }
        Err(DomainError::Validation { field, message }) => {
            let err_env =
                ApiResponse::<()>::error("VALIDATION_FAILED", message, Some(field), trace_id);
            json_response(400, &err_env)
        }
        Err(err) => {
            let err_env =
                ApiResponse::<()>::error("INTERNAL_ERROR", err.to_string(), None, trace_id);
            json_response(500, &err_env)
        }
    }
}
