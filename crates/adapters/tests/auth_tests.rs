use adapters::inbound::http::auth::{
    ApiKeyRole, RequiredPermission, constant_time_eq, is_valid_api_key, resolve_api_key_role,
};

#[test]
fn constant_time_eq_compares_bytes_accurately() {
    assert!(constant_time_eq(b"abc123", b"abc123"));
    assert!(!constant_time_eq(b"abc123", b"abc124"));
    assert!(!constant_time_eq(b"abc123", b"abc12"));
}

#[test]
fn is_valid_api_key_accepts_matching_x_api_key() {
    assert!(is_valid_api_key("top-secret", Some("top-secret")));
    assert!(is_valid_api_key("top-secret", Some("  top-secret  ")));
}

#[test]
fn is_valid_api_key_rejects_empty_or_mismatched_keys() {
    assert!(!is_valid_api_key("top-secret", Some("wrong")));
    assert!(!is_valid_api_key("top-secret", None));
    assert!(!is_valid_api_key("", Some("")));
    assert!(!is_valid_api_key("   ", Some("   ")));
}

#[test]
fn resolve_api_key_role_distinguishes_master_and_chat_keys() {
    assert_eq!(
        resolve_api_key_role(
            Some("master-secret"),
            Some("chat-secret"),
            Some("master-secret")
        ),
        Some(ApiKeyRole::Master)
    );
    assert_eq!(
        resolve_api_key_role(
            Some("master-secret"),
            Some("chat-secret"),
            Some("chat-secret")
        ),
        Some(ApiKeyRole::Chat)
    );
    assert_eq!(
        resolve_api_key_role(Some("master-secret"), Some("chat-secret"), Some("wrong")),
        None
    );
    assert_eq!(
        resolve_api_key_role(Some("master-secret"), Some("chat-secret"), None),
        None
    );
}

#[test]
fn resolve_api_key_role_prefers_master_when_keys_are_equal() {
    assert_eq!(
        resolve_api_key_role(Some("same-key"), Some("same-key"), Some("same-key")),
        Some(ApiKeyRole::Master)
    );
}

#[test]
fn api_key_role_permissions_enforce_chat_only_vs_master() {
    assert!(ApiKeyRole::Master.has_permission(RequiredPermission::Chat));
    assert!(ApiKeyRole::Master.has_permission(RequiredPermission::Master));
    assert!(ApiKeyRole::Chat.has_permission(RequiredPermission::Chat));
    assert!(!ApiKeyRole::Chat.has_permission(RequiredPermission::Master));
}
