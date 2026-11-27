use std::rc::Rc;

use adapters::inbound::http::{
    handle_chat, handle_chat_stream, handle_delete_knowledge, handle_health,
    handle_ingest_markdown, handle_list_chunks, handle_openapi_spec, handle_swagger_ui,
    verify_api_key,
};
use adapters::outbound::persistence::{
    D1ChatRepository, KvAnswerCache, VectorizeIndexAdapter, WorkersAiCompletion,
};
use application::use_cases::ask_portfolio::AskPortfolioUseCase;
use application::use_cases::ingest_markdown::IngestMarkdownUseCase;
use application::use_cases::manage_knowledge::ManageKnowledgeUseCase;
use common::config::{AiConfig, RagConfig};
use worker::*;

fn load_ai_config(env: &Env) -> AiConfig {
    let mut cfg = AiConfig::default();

    if let Ok(val) = env.var("AI_EMBEDDING_MODEL") {
        let s = val.to_string();
        if !s.trim().is_empty() {
            cfg.embedding_model = s;
        }
    }
    if let Ok(val) = env.var("AI_REWRITE_MODEL") {
        let s = val.to_string();
        if !s.trim().is_empty() {
            cfg.rewrite_model = s;
        }
    }
    if let Ok(val) = env.var("AI_ANSWER_MODEL") {
        let s = val.to_string();
        if !s.trim().is_empty() {
            cfg.answer_model = s;
        }
    }
    if let Ok(val) = env.var("AI_ANSWER_TEMPERATURE")
        && let Ok(parsed) = val.to_string().parse::<f32>()
    {
        cfg.answer_temperature = parsed;
    }

    cfg
}

fn load_rag_config(env: &Env) -> RagConfig {
    let mut cfg = RagConfig::default();

    if let Ok(val) = env.var("RAG_TOP_K")
        && let Ok(parsed) = val.to_string().parse::<usize>()
        && parsed > 0
    {
        cfg.top_k = parsed;
    }
    if let Ok(val) = env.var("RAG_MIN_SCORE")
        && let Ok(parsed) = val.to_string().parse::<f32>()
    {
        cfg.min_score = parsed;
    }
    if let Ok(val) = env.var("RAG_FALLBACK_LIMIT")
        && let Ok(parsed) = val.to_string().parse::<usize>()
        && parsed > 0
    {
        cfg.fallback_limit = parsed;
    }
    if let Ok(val) = env.var("RAG_RECENT_MESSAGES")
        && let Ok(parsed) = val.to_string().parse::<usize>()
    {
        cfg.recent_messages_limit = parsed;
    }
    if let Ok(val) = env.var("RAG_CACHE_TTL_SECONDS")
        && let Ok(parsed) = val.to_string().parse::<u64>()
        && parsed > 0
    {
        cfg.cache_ttl_seconds = parsed;
    }

    cfg
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/docs", |_req, _ctx| async move { handle_swagger_ui() })
        .get_async("/api/openapi.json", |_req, _ctx| async move {
            handle_openapi_spec()
        })
        .get_async(
            "/api/health",
            |req, _ctx| async move { handle_health(&req) },
        )
        .get_async("/api/chunks", |req, ctx| async move {
            if let Err(resp) = verify_api_key(&req, &ctx.env) {
                return Ok(resp);
            }

            let use_case = build_manage_knowledge_use_case(&ctx.env)?;
            handle_list_chunks(&req, &use_case).await
        })
        .delete_async("/api/chunks/:target", |req, ctx| async move {
            if let Err(resp) = verify_api_key(&req, &ctx.env) {
                return Ok(resp);
            }

            let target = ctx.param("target").map(String::as_str).unwrap_or_default();
            let use_case = build_manage_knowledge_use_case(&ctx.env)?;
            handle_delete_knowledge(&req, target, &use_case).await
        })
        .post_async("/api/chunks/markdown", |req, ctx| async move {
            if let Err(resp) = verify_api_key(&req, &ctx.env) {
                return Ok(resp);
            }

            let db = ctx.env.d1("DB")?;
            let ai = ctx.env.ai("AI")?;
            let vectorize = VectorizeIndexAdapter::from_env(&ctx.env, "VECTORIZE")
                .map_err(|e| Error::RustError(e.to_string()))?;
            let ai_config = load_ai_config(&ctx.env);

            let repo = Rc::new(D1ChatRepository::new(db));
            let vector_index = Rc::new(vectorize);
            let ai_adapter = Rc::new(WorkersAiCompletion::new(ai, ai_config));

            let use_case = IngestMarkdownUseCase::new(repo, vector_index, ai_adapter);
            handle_ingest_markdown(req, &use_case).await
        })
        .post_async("/api/chat", |req, ctx| async move {
            if let Err(resp) = verify_api_key(&req, &ctx.env) {
                return Ok(resp);
            }

            let use_case = build_ask_use_case(&ctx.env)?;
            handle_chat(req, &use_case).await
        })
        .post_async("/api/chat/stream", |req, ctx| async move {
            if let Err(resp) = verify_api_key(&req, &ctx.env) {
                return Ok(resp);
            }

            let use_case = build_ask_use_case(&ctx.env)?;
            handle_chat_stream(req, &use_case).await
        })
        .run(req, env)
        .await
}

fn build_ask_use_case(env: &Env) -> Result<AskPortfolioUseCase> {
    let db = env.d1("DB")?;
    let kv = env.kv("CACHE_KV")?;
    let ai = env.ai("AI")?;
    let vectorize = VectorizeIndexAdapter::from_env(env, "VECTORIZE")
        .map_err(|e| Error::RustError(e.to_string()))?;
    let ai_config = load_ai_config(env);
    let rag_config = load_rag_config(env);

    let repo = Rc::new(D1ChatRepository::new(db));
    let vector_index = Rc::new(vectorize);
    let cache = Rc::new(KvAnswerCache::new(kv));
    let ai_adapter = Rc::new(WorkersAiCompletion::new(ai, ai_config));

    Ok(AskPortfolioUseCase::new(
        repo,
        vector_index,
        cache,
        ai_adapter,
        rag_config,
    ))
}

fn build_manage_knowledge_use_case(env: &Env) -> Result<ManageKnowledgeUseCase> {
    let db = env.d1("DB")?;
    let vectorize = VectorizeIndexAdapter::from_env(env, "VECTORIZE")
        .map_err(|e| Error::RustError(e.to_string()))?;

    let repo = Rc::new(D1ChatRepository::new(db));
    let vector_index = Rc::new(vectorize);

    Ok(ManageKnowledgeUseCase::new(repo, vector_index))
}
