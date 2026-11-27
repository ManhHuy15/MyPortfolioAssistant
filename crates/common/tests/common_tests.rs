use common::config::{AiConfig, RagConfig};
use common::response::ApiResponse;

#[test]
fn test_default_ai_config() {
    let cfg = AiConfig::default();
    assert_eq!(cfg.embedding_model, "@cf/baai/bge-m3");
    assert_eq!(cfg.rewrite_model, "@cf/meta/llama-3.1-8b-instruct-fast");
    assert_eq!(cfg.answer_model, "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
    assert!((cfg.answer_temperature - 0.1).abs() < f32::EPSILON);
}

#[test]
fn test_default_rag_config() {
    let cfg = RagConfig::default();
    assert_eq!(cfg.top_k, 5);
    assert!((cfg.min_score - 0.35).abs() < f32::EPSILON);
    assert_eq!(cfg.fallback_limit, 10);
    assert_eq!(cfg.recent_messages_limit, 4);
    assert_eq!(cfg.cache_ttl_seconds, 86_400);
}

#[test]
fn test_api_response_success() {
    let resp = ApiResponse::success("hello", "OK", Some("ray-123".to_string()));
    assert!(resp.success);
    assert_eq!(resp.data, Some("hello"));
    assert!(resp.errors.is_none());
    assert_eq!(resp.trace_id.as_deref(), Some("ray-123"));
}

#[test]
fn test_api_response_error() {
    let resp = ApiResponse::<()>::error(
        "VALIDATION_FAILED",
        "Invalid field",
        Some("message".to_string()),
        None,
    );
    assert!(!resp.success);
    assert!(resp.data.is_none());
    assert_eq!(resp.errors.as_ref().map(|e| e.len()), Some(1));
}
