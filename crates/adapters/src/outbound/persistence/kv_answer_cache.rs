use async_trait::async_trait;
use domain::entities::CachedAnswer;
use domain::errors::DomainError;
use domain::ports::outbound::AnswerCachePort;
use worker::kv::KvStore;

const CACHE_VERSION_KEY: &str = "chat_cache:version";

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
    async fn get_cache_version(&self) -> Result<u64, DomainError> {
        let version = self
            .kv
            .get(CACHE_VERSION_KEY)
            .text()
            .await
            .map_err(|e| DomainError::Cache(e.to_string()))?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1);
        Ok(version)
    }

    async fn bump_cache_version(&self) -> Result<u64, DomainError> {
        let current = self.get_cache_version().await.unwrap_or(1);
        let next = current.saturating_add(1);
        self.kv
            .put(CACHE_VERSION_KEY, next.to_string())
            .map_err(|e| DomainError::Cache(e.to_string()))?
            .execute()
            .await
            .map_err(|e| DomainError::Cache(e.to_string()))?;
        Ok(next)
    }

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
