use std::rc::Rc;

use async_trait::async_trait;
use domain::entities::KnowledgeChunk;
use domain::errors::DomainError;
use domain::ports::inbound::{DeleteKnowledgeResult, ManageKnowledgeUseCasePort};
use domain::ports::outbound::{AnswerCachePort, ChatRepositoryPort, VectorIndexPort};

const DELETE_BATCH_SIZE: usize = 100;

pub struct ManageKnowledgeUseCase {
    repo: Rc<dyn ChatRepositoryPort>,
    vector_index: Rc<dyn VectorIndexPort>,
    cache: Rc<dyn AnswerCachePort>,
}

impl ManageKnowledgeUseCase {
    pub fn new(
        repo: Rc<dyn ChatRepositoryPort>,
        vector_index: Rc<dyn VectorIndexPort>,
        cache: Rc<dyn AnswerCachePort>,
    ) -> Self {
        Self {
            repo,
            vector_index,
            cache,
        }
    }
}

#[async_trait(?Send)]
impl ManageKnowledgeUseCasePort for ManageKnowledgeUseCase {
    async fn list_chunks(&self) -> Result<Vec<KnowledgeChunk>, DomainError> {
        self.repo.list_chunks().await
    }

    async fn delete_by_target(&self, target: &str) -> Result<DeleteKnowledgeResult, DomainError> {
        let target = target.trim();
        if target.is_empty() {
            return Err(DomainError::Validation {
                field: "target".to_string(),
                message: "target cannot be empty".to_string(),
            });
        }

        let ids = self.repo.find_chunk_ids_by_target(target).await?;
        if ids.is_empty() {
            return Err(DomainError::NotFound(format!(
                "No knowledge chunks found for '{target}'"
            )));
        }

        for batch in ids.chunks(DELETE_BATCH_SIZE) {
            self.vector_index.delete_vectors_by_ids(batch).await?;
        }

        self.repo.delete_chunks_by_ids(&ids).await?;
        let _ = self.cache.bump_cache_version().await;

        Ok(DeleteKnowledgeResult {
            target: target.to_string(),
            deleted_count: ids.len(),
            deleted_ids: ids,
        })
    }
}
