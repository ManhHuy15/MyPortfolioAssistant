use application::dtos::{DeleteKnowledgeResponseDto, IngestedChunkItemDto, ListChunksResponseDto};
use common::response::ApiResponse;
use domain::errors::DomainError;
use domain::ports::inbound::ManageKnowledgeUseCasePort;
use worker::{Request, Response, Result};

use super::response::json_response;

pub async fn handle_list_chunks(
    req: &Request,
    use_case: &dyn ManageKnowledgeUseCasePort,
) -> Result<Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();

    match use_case.list_chunks().await {
        Ok(chunks) => {
            let items: Vec<IngestedChunkItemDto> = chunks
                .into_iter()
                .map(|c| IngestedChunkItemDto {
                    id: c.id,
                    doc_title: c.doc_title,
                    category: c.category,
                    char_count: c.content.chars().count(),
                })
                .collect();
            let dto = ListChunksResponseDto {
                total: items.len(),
                chunks: items,
            };
            let envelope = ApiResponse::success(dto, "Success", trace_id);
            json_response(200, &envelope)
        }
        Err(err) => {
            let err_env =
                ApiResponse::<()>::error("INTERNAL_ERROR", err.to_string(), None, trace_id);
            json_response(500, &err_env)
        }
    }
}

pub async fn handle_delete_knowledge(
    req: &Request,
    target: &str,
    use_case: &dyn ManageKnowledgeUseCasePort,
) -> Result<Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();

    match use_case.delete_by_target(target).await {
        Ok(res) => {
            let dto = DeleteKnowledgeResponseDto {
                target: res.target,
                deleted_count: res.deleted_count,
                deleted_ids: res.deleted_ids,
            };
            let envelope = ApiResponse::success(dto, "Knowledge deleted successfully", trace_id);
            json_response(200, &envelope)
        }
        Err(DomainError::Validation { field, message }) => {
            let err_env =
                ApiResponse::<()>::error("VALIDATION_FAILED", message, Some(field), trace_id);
            json_response(400, &err_env)
        }
        Err(DomainError::NotFound(message)) => {
            let err_env = ApiResponse::<()>::error("NOT_FOUND", message, None, trace_id);
            json_response(404, &err_env)
        }
        Err(err) => {
            let err_env =
                ApiResponse::<()>::error("INTERNAL_ERROR", err.to_string(), None, trace_id);
            json_response(500, &err_env)
        }
    }
}
