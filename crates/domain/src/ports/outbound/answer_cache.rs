use async_trait::async_trait;

use crate::entities::CachedAnswer;
use crate::errors::DomainError;

#[async_trait(?Send)]
pub trait AnswerCachePort {
    async fn get_cache_version(&self) -> Result<u64, DomainError>;
    async fn bump_cache_version(&self) -> Result<u64, DomainError>;
    async fn get_cached_answer(&self, key: &str) -> Result<Option<CachedAnswer>, DomainError>;
    async fn set_cached_answer(
        &self,
        key: &str,
        answer: &CachedAnswer,
        ttl_seconds: u64,
    ) -> Result<(), DomainError>;
}
