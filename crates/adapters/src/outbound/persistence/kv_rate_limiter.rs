use async_trait::async_trait;
use domain::errors::DomainError;
use domain::ports::outbound::{RateLimitDecision, RateLimiterPort};
use futures_util::future::join;
use worker::kv::KvStore;

const MIN_KV_TTL_SECONDS: u64 = 60;

pub struct KvRateLimiter {
    kv: KvStore,
}

impl KvRateLimiter {
    pub fn new(kv: KvStore) -> Self {
        Self { kv }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlidingWindowSlot {
    pub current_window_id: u64,
    pub previous_window_id: u64,
    pub elapsed_seconds: u64,
    pub window_seconds: u64,
    pub previous_weight: f64,
}

pub fn compute_sliding_window(now_secs: u64, window_seconds: u64) -> SlidingWindowSlot {
    let safe_window = window_seconds.max(1);
    let current_window_id = now_secs / safe_window;
    let previous_window_id = current_window_id.saturating_sub(1);
    let elapsed_seconds = now_secs % safe_window;
    let previous_weight = (safe_window - elapsed_seconds) as f64 / safe_window as f64;

    SlidingWindowSlot {
        current_window_id,
        previous_window_id,
        elapsed_seconds,
        window_seconds: safe_window,
        previous_weight,
    }
}

pub fn evaluate_sliding_window(
    previous_count: u32,
    current_count: u32,
    max_requests: u32,
    slot: SlidingWindowSlot,
) -> RateLimitDecision {
    if max_requests == 0 {
        return RateLimitDecision {
            allowed: true,
            remaining: u32::MAX,
            retry_after_seconds: 0,
        };
    }

    let remaining_in_window = (slot.window_seconds - slot.elapsed_seconds).max(1);
    let effective_count = (previous_count as f64) * slot.previous_weight + (current_count as f64);

    if effective_count >= max_requests as f64 {
        let retry_after = if current_count >= max_requests || previous_count == 0 {
            remaining_in_window
        } else {
            let needed_drop = effective_count - (max_requests as f64 - 1.0);
            let drop_per_sec = previous_count as f64 / slot.window_seconds as f64;
            ((needed_drop / drop_per_sec).ceil().max(1.0) as u64).min(remaining_in_window)
        };

        RateLimitDecision {
            allowed: false,
            remaining: 0,
            retry_after_seconds: retry_after,
        }
    } else {
        let remaining = ((max_requests as f64) - effective_count - 1.0)
            .max(0.0)
            .floor() as u32;

        RateLimitDecision {
            allowed: true,
            remaining,
            retry_after_seconds: remaining_in_window,
        }
    }
}

#[async_trait(?Send)]
impl RateLimiterPort for KvRateLimiter {
    async fn check_and_increment(
        &self,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> Result<RateLimitDecision, DomainError> {
        if max_requests == 0 {
            return Ok(RateLimitDecision {
                allowed: true,
                remaining: u32::MAX,
                retry_after_seconds: 0,
            });
        }

        let now_secs = (js_sys::Date::now() / 1000.0) as u64;
        let slot = compute_sliding_window(now_secs, window_seconds);
        let curr_key = format!("{key}:{}", slot.current_window_id);
        let prev_key = format!("{key}:{}", slot.previous_window_id);

        let (prev_res, curr_res) =
            join(self.kv.get(&prev_key).text(), self.kv.get(&curr_key).text()).await;

        let previous_count = prev_res
            .map_err(|e| DomainError::Cache(e.to_string()))?
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let current_count = curr_res
            .map_err(|e| DomainError::Cache(e.to_string()))?
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let decision = evaluate_sliding_window(previous_count, current_count, max_requests, slot);
        if !decision.allowed {
            return Ok(decision);
        }

        let next_count = current_count.saturating_add(1).to_string();
        let ttl = slot
            .window_seconds
            .saturating_mul(2)
            .max(MIN_KV_TTL_SECONDS);

        self.kv
            .put(&curr_key, next_count)
            .map_err(|e| DomainError::Cache(e.to_string()))?
            .expiration_ttl(ttl)
            .execute()
            .await
            .map_err(|e| DomainError::Cache(e.to_string()))?;

        Ok(decision)
    }
}
