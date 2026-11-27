use adapters::outbound::persistence::kv_rate_limiter::{
    compute_sliding_window, evaluate_sliding_window,
};

#[test]
fn compute_sliding_window_calculates_ids_and_weight() {
    let slot = compute_sliding_window(150, 60);
    assert_eq!(slot.current_window_id, 2);
    assert_eq!(slot.previous_window_id, 1);
    assert_eq!(slot.elapsed_seconds, 30);
    assert!((slot.previous_weight - 0.5).abs() < f64::EPSILON);
}

#[test]
fn evaluate_sliding_window_prevents_boundary_burst() {
    // 10 requests at end of previous window, 0 in current window, right at boundary (elapsed = 0)
    let start_slot = compute_sliding_window(120, 60);
    let blocked = evaluate_sliding_window(10, 0, 10, start_slot);
    assert!(!blocked.allowed);
    assert_eq!(blocked.remaining, 0);
    assert_eq!(blocked.retry_after_seconds, 6);

    // 6 seconds later: weight = 54/60 = 0.9 => effective = 9.0 < 10 => allowed
    let after_6s = compute_sliding_window(126, 60);
    let allowed = evaluate_sliding_window(10, 0, 10, after_6s);
    assert!(allowed.allowed);
    assert_eq!(allowed.remaining, 0);

    // 30 seconds into current window: weight = 0.5 => effective = 5.0 => 5 slots available (4 remaining after this one)
    let mid_slot = compute_sliding_window(150, 60);
    let mid = evaluate_sliding_window(10, 0, 10, mid_slot);
    assert!(mid.allowed);
    assert_eq!(mid.remaining, 4);
}

#[test]
fn evaluate_sliding_window_blocks_when_weighted_sum_reaches_limit() {
    let mid_slot = compute_sliding_window(150, 60);
    // prev=10 * 0.5 + curr=5 = 10.0 >= 10 => blocked
    let blocked = evaluate_sliding_window(10, 5, 10, mid_slot);
    assert!(!blocked.allowed);
    assert_eq!(blocked.remaining, 0);
    assert_eq!(blocked.retry_after_seconds, 6);
}

#[test]
fn evaluate_sliding_window_disables_when_max_is_zero() {
    let slot = compute_sliding_window(150, 60);
    let decision = evaluate_sliding_window(100, 100, 0, slot);
    assert!(decision.allowed);
}
