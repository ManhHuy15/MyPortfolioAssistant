use adapters::inbound::http::cors::resolve_allowed_origin;

#[test]
fn resolve_allowed_origin_supports_wildcard_default() {
    assert_eq!(
        resolve_allowed_origin(Some("https://example.com"), "*"),
        Some("*".to_string())
    );
    assert_eq!(resolve_allowed_origin(None, ""), Some("*".to_string()));
}

#[test]
fn resolve_allowed_origin_matches_explicit_allowlist() {
    let allowlist = "https://huy.dev, http://localhost:3000";

    assert_eq!(
        resolve_allowed_origin(Some("https://huy.dev"), allowlist),
        Some("https://huy.dev".to_string())
    );
    assert_eq!(
        resolve_allowed_origin(Some("http://localhost:3000"), allowlist),
        Some("http://localhost:3000".to_string())
    );
    assert_eq!(
        resolve_allowed_origin(Some("https://evil.com"), allowlist),
        None
    );
    assert_eq!(resolve_allowed_origin(None, allowlist), None);
}
