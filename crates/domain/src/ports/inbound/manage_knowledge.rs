use async_trait::async_trait;

use crate::entities::KnowledgeChunk;
use crate::errors::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteKnowledgeResult {
    pub target: String,
    pub deleted_count: usize,
    pub deleted_ids: Vec<String>,
}

#[async_trait(?Send)]
pub trait ManageKnowledgeUseCasePort {
    async fn list_chunks(&self) -> Result<Vec<KnowledgeChunk>, DomainError>;
    async fn delete_by_target(&self, target: &str) -> Result<DeleteKnowledgeResult, DomainError>;
}
