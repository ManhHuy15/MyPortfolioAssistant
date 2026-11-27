use async_trait::async_trait;

use crate::errors::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub remaining: u32,
    pub retry_after_seconds: u64,
}

#[async_trait(?Send)]
pub trait RateLimiterPort {
    async fn check_and_increment(
        &self,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> Result<RateLimitDecision, DomainError>;
}
