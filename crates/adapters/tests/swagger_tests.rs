use adapters::inbound::http::swagger_handler::OPENAPI_SPEC_JSON;
use serde_json::Value;

#[test]
fn openapi_spec_is_valid_and_documents_all_routes() {
    let spec: Value =
        serde_json::from_str(OPENAPI_SPEC_JSON).expect("docs/openapi.json must be valid JSON");

    assert_eq!(spec["openapi"], "3.0.3");
    assert_eq!(
        spec["components"]["securitySchemes"]["ApiKeyAuth"]["name"],
        "X-API-Key"
    );

    let paths = spec["paths"]
        .as_object()
        .expect("openapi.json must contain paths");
    for expected in [
        "/api/health",
        "/api/chunks",
        "/api/chunks/markdown",
        "/api/chunks/{target}",
        "/api/chat",
        "/api/chat/stream",
    ] {
        assert!(paths.contains_key(expected), "missing path {expected}");
    }
}
