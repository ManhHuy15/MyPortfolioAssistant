use application::dtos::{ChatRequestDto, ChatResponseDto};
use common::response::ApiResponse;
use domain::entities::SessionMeta;
use domain::errors::DomainError;
use domain::ports::inbound::{AskPortfolioCommand, AskPortfolioUseCasePort, ChatStreamEvent};
use futures_util::StreamExt;
use serde::Serialize;
use worker::{Request, Response, Result};

use super::response::json_response;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamTokenPayload {
    token: String,
}

fn format_sse_event<T: Serialize>(event_name: &str, payload: &T) -> Vec<u8> {
    let json = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    format!("event: {event_name}\ndata: {json}\n\n").into_bytes()
}

pub async fn handle_chat_stream(
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

    let event_stream = match use_case.execute_stream(cmd).await {
        Ok(stream) => stream,
        Err(DomainError::Validation { field, message }) => {
            let err_env =
                ApiResponse::<()>::error("VALIDATION_FAILED", message, Some(field), trace_id);
            return json_response(400, &err_env);
        }
        Err(err) => {
            let err_env =
                ApiResponse::<()>::error("INTERNAL_ERROR", err.to_string(), None, trace_id);
            return json_response(500, &err_env);
        }
    };

    let sse_stream = event_stream.map(move |item| -> Result<Vec<u8>> {
        let bytes = match item {
            Ok(ChatStreamEvent::Token(token)) => {
                format_sse_event("token", &StreamTokenPayload { token })
            }
            Ok(ChatStreamEvent::Done(res)) => {
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
                let envelope = ApiResponse::success(dto, msg, trace_id.clone());
                format_sse_event("done", &envelope)
            }
            Err(err) => {
                let err_env = ApiResponse::<()>::error(
                    "INTERNAL_ERROR",
                    err.to_string(),
                    None,
                    trace_id.clone(),
                );
                format_sse_event("error", &err_env)
            }
        };
        Ok(bytes)
    });

    let mut response = Response::from_stream(sse_stream)?;
    response
        .headers_mut()
        .set("Content-Type", "text/event-stream; charset=utf-8")?;
    response.headers_mut().set("Cache-Control", "no-cache")?;
    response.headers_mut().set("Connection", "keep-alive")?;
    Ok(response)
}
