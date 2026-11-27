use adapters::inbound::http::auth::{constant_time_eq, is_valid_api_key};

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
