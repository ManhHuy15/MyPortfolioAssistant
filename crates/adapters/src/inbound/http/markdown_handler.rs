use application::dtos::{
    IngestMarkdownRequestDto, IngestMarkdownResponseDto, IngestedChunkItemDto,
};
use application::use_cases::ingest_markdown::{DEFAULT_MAX_CHUNK_CHARS, DEFAULT_OVERLAP_CHARS};
use common::response::ApiResponse;
use domain::errors::DomainError;
use domain::ports::inbound::{IngestMarkdownCommand, IngestMarkdownUseCasePort};
use worker::{FormEntry, Request, Response, Result};

use super::response::json_response;

async fn parse_markdown_command(
    mut req: Request,
) -> std::result::Result<IngestMarkdownCommand, String> {
    let content_type = req
        .headers()
        .get("content-type")
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_lowercase();

    if content_type.contains("multipart/form-data") {
        let form = req
            .form_data()
            .await
            .map_err(|_| "Invalid multipart/form-data payload".to_string())?;

        let mut doc_id_prefix = String::new();
        let mut default_doc_title: Option<String> = None;
        let mut category = "general".to_string();
        let mut markdown_content = String::new();

        if let Some(FormEntry::Field(val)) = form.get("docIdPrefix") {
            doc_id_prefix = val;
        }
        if let Some(FormEntry::Field(val)) = form.get("defaultDocTitle") {
            default_doc_title = Some(val);
        }
        if let Some(FormEntry::Field(val)) = form.get("category") {
            category = val;
        }
        if let Some(entry) = form.get("file") {
            match entry {
                FormEntry::File(file) => {
                    if doc_id_prefix.trim().is_empty() {
                        doc_id_prefix = file
                            .name()
                            .trim_end_matches(".md")
                            .trim_end_matches(".markdown")
                            .to_string();
                    }
                    let bytes = file
                        .bytes()
                        .await
                        .map_err(|_| "Failed to read uploaded Markdown file".to_string())?;
                    markdown_content = String::from_utf8(bytes)
                        .map_err(|_| "Markdown file must be valid UTF-8".to_string())?;
                }
                FormEntry::Field(text) => {
                    markdown_content = text;
                }
            }
        }

        return Ok(IngestMarkdownCommand {
            doc_id_prefix,
            default_doc_title,
            category,
            markdown_content,
            max_chunk_chars: DEFAULT_MAX_CHUNK_CHARS,
            overlap_chars: DEFAULT_OVERLAP_CHARS,
        });
    }

    if content_type.contains("text/markdown") || content_type.contains("text/plain") {
        let url = req.url().map_err(|_| "Invalid request URL".to_string())?;
        let mut doc_id_prefix = "doc".to_string();
        let mut default_doc_title: Option<String> = None;
        let mut category = "general".to_string();

        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "docIdPrefix" => doc_id_prefix = v.into_owned(),
                "defaultDocTitle" => default_doc_title = Some(v.into_owned()),
                "category" => category = v.into_owned(),
                _ => {}
            }
        }

        let markdown_content = req
            .text()
            .await
            .map_err(|_| "Failed to read raw Markdown body".to_string())?;

        return Ok(IngestMarkdownCommand {
            doc_id_prefix,
            default_doc_title,
            category,
            markdown_content,
            max_chunk_chars: DEFAULT_MAX_CHUNK_CHARS,
            overlap_chars: DEFAULT_OVERLAP_CHARS,
        });
    }

    let payload: IngestMarkdownRequestDto = req
        .json()
        .await
        .map_err(|_| "Invalid JSON or multipart Markdown payload".to_string())?;

    Ok(IngestMarkdownCommand {
        doc_id_prefix: payload.doc_id_prefix,
        default_doc_title: payload.default_doc_title,
        category: payload.category,
        markdown_content: payload.markdown_content,
        max_chunk_chars: payload.max_chunk_chars.unwrap_or(DEFAULT_MAX_CHUNK_CHARS),
        overlap_chars: payload.overlap_chars.unwrap_or(DEFAULT_OVERLAP_CHARS),
    })
}

pub async fn handle_ingest_markdown(
    req: Request,
    use_case: &dyn IngestMarkdownUseCasePort,
) -> Result<Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();

    let cmd = match parse_markdown_command(req).await {
        Ok(c) => c,
        Err(msg) => {
            let err_env = ApiResponse::<()>::error("VALIDATION_FAILED", msg, None, trace_id);
            return json_response(400, &err_env);
        }
    };

    match use_case.execute(cmd).await {
        Ok(result) => {
            let dto = IngestMarkdownResponseDto {
                chunks_created: result.chunks_created,
                dimensions: result.dimensions,
                chunks: result
                    .chunks
                    .into_iter()
                    .map(|c| IngestedChunkItemDto {
                        id: c.id,
                        doc_title: c.doc_title,
                        category: c.category,
                        char_count: c.content.chars().count(),
                    })
                    .collect(),
            };
            let envelope =
                ApiResponse::success(dto, "Markdown chunked and indexed successfully", trace_id);
            json_response(201, &envelope)
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
