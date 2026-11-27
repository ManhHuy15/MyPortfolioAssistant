use async_trait::async_trait;

use crate::entities::{ChatMessageRecord, KnowledgeChunk, SessionMeta};
use crate::errors::DomainError;

#[async_trait(?Send)]
pub trait ChatRepositoryPort {
    async fn upsert_session(&self, session: &SessionMeta) -> Result<(), DomainError>;
    async fn save_message(&self, message: &ChatMessageRecord) -> Result<(), DomainError>;
    async fn fetch_recent_messages(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessageRecord>, DomainError>;
    async fn upsert_chunks_batch(&self, chunks: &[KnowledgeChunk]) -> Result<(), DomainError>;
    async fn fetch_chunks_by_ids(&self, ids: &[String])
    -> Result<Vec<KnowledgeChunk>, DomainError>;
    async fn fetch_recent_chunks(&self, limit: usize) -> Result<Vec<KnowledgeChunk>, DomainError>;
}
