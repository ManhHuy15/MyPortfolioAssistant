use async_trait::async_trait;
use domain::entities::CachedAnswer;
use domain::errors::DomainError;
use domain::ports::outbound::AnswerCachePort;
use worker::kv::KvStore;

pub struct KvAnswerCache {
    kv: KvStore,
}

impl KvAnswerCache {
    pub fn new(kv: KvStore) -> Self {
        Self { kv }
    }
}

#[async_trait(?Send)]
impl AnswerCachePort for KvAnswerCache {
    async fn get_cached_answer(&self, key: &str) -> Result<Option<CachedAnswer>, DomainError> {
        self.kv
            .get(key)
            .json::<CachedAnswer>()
            .await
            .map_err(|e| DomainError::Cache(e.to_string()))
    }

    async fn set_cached_answer(
        &self,
        key: &str,
        answer: &CachedAnswer,
        ttl_seconds: u64,
    ) -> Result<(), DomainError> {
        self.kv
            .put(key, answer)
            .map_err(|e| DomainError::Cache(e.to_string()))?
            .expiration_ttl(ttl_seconds)
            .execute()
            .await
            .map_err(|e| DomainError::Cache(e.to_string()))
    }
}
