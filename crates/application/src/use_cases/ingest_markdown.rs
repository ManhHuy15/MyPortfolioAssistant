use std::collections::HashSet;
use std::rc::Rc;

use async_trait::async_trait;
use domain::errors::DomainError;
use domain::ports::inbound::{
    IngestMarkdownCommand, IngestMarkdownResult, IngestMarkdownUseCasePort,
};
use domain::ports::outbound::{
    AiCompletionPort, AnswerCachePort, ChatRepositoryPort, VectorIndexPort, VectorRecord,
};

pub use super::markdown_chunker::{DEFAULT_MAX_CHUNK_CHARS, DEFAULT_OVERLAP_CHARS, chunk_markdown};

const EMBED_BATCH_SIZE: usize = 50;

pub struct IngestMarkdownUseCase {
    repo: Rc<dyn ChatRepositoryPort>,
    vector_index: Rc<dyn VectorIndexPort>,
    ai: Rc<dyn AiCompletionPort>,
    cache: Rc<dyn AnswerCachePort>,
}

impl IngestMarkdownUseCase {
    pub fn new(
        repo: Rc<dyn ChatRepositoryPort>,
        vector_index: Rc<dyn VectorIndexPort>,
        ai: Rc<dyn AiCompletionPort>,
        cache: Rc<dyn AnswerCachePort>,
    ) -> Self {
        Self {
            repo,
            vector_index,
            ai,
            cache,
        }
    }
}

#[async_trait(?Send)]
impl IngestMarkdownUseCasePort for IngestMarkdownUseCase {
    async fn execute(
        &self,
        cmd: IngestMarkdownCommand,
    ) -> Result<IngestMarkdownResult, DomainError> {
        let doc_id_prefix = cmd.doc_id_prefix.trim().to_string();
        if doc_id_prefix.is_empty() {
            return Err(DomainError::Validation {
                field: "docIdPrefix".to_string(),
                message: "docIdPrefix cannot be empty".to_string(),
            });
        }

        let markdown_content = cmd.markdown_content.trim().to_string();
        if markdown_content.is_empty() {
            return Err(DomainError::Validation {
                field: "markdownContent".to_string(),
                message: "markdownContent cannot be empty".to_string(),
            });
        }

        let category = {
            let c = cmd.category.trim();
            if c.is_empty() {
                "general".to_string()
            } else {
                c.to_string()
            }
        };

        let chunks = chunk_markdown(
            &doc_id_prefix,
            cmd.default_doc_title.as_deref(),
            &category,
            &markdown_content,
            cmd.max_chunk_chars,
            cmd.overlap_chars,
        );

        if chunks.is_empty() {
            return Err(DomainError::Validation {
                field: "markdownContent".to_string(),
                message: "No valid chunks could be extracted from markdown".to_string(),
            });
        }

        // Snapshot existing chunk IDs for this doc_id_prefix so we can clean up stale ones after upsert
        let existing_ids = self.repo.find_chunk_ids_by_target(&doc_id_prefix).await?;

        // Contextual Header Injection: attach full heading breadcrumb + category before embedding
        let texts_to_embed: Vec<String> = chunks
            .iter()
            .map(|c| {
                format!(
                    "[Ngữ cảnh: {} | Loại: {}]\n{}",
                    c.doc_title, c.category, c.content
                )
            })
            .collect();

        // Embed in safe batches to stay well within Workers AI payload limits
        let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
        for batch in texts_to_embed.chunks(EMBED_BATCH_SIZE) {
            let batch_embeddings = self.ai.embed_texts(batch).await?;
            all_embeddings.extend(batch_embeddings);
        }

        if all_embeddings.len() != chunks.len() {
            return Err(DomainError::AiProvider(format!(
                "Expected {} embeddings, received {}",
                chunks.len(),
                all_embeddings.len()
            )));
        }

        let dimensions = all_embeddings.first().map(Vec::len).unwrap_or(0);

        // Upsert new chunks & vectors first (zero-downtime if embedding/upsert fails)
        self.repo.upsert_chunks_batch(&chunks).await?;

        let vector_records: Vec<VectorRecord> = chunks
            .iter()
            .zip(all_embeddings)
            .map(|(chunk, values)| VectorRecord {
                id: chunk.id.clone(),
                values,
                doc_title: chunk.doc_title.clone(),
                category: chunk.category.clone(),
            })
            .collect();

        for batch in vector_records.chunks(EMBED_BATCH_SIZE) {
            self.vector_index.upsert_vectors_batch(batch).await?;
        }

        // Reconcile: remove any leftover stale chunks from previous versions of this document
        let new_id_set: HashSet<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        let stale_ids: Vec<String> = existing_ids
            .into_iter()
            .filter(|id| !new_id_set.contains(id.as_str()))
            .collect();

        if !stale_ids.is_empty() {
            for batch in stale_ids.chunks(EMBED_BATCH_SIZE) {
                self.vector_index.delete_vectors_by_ids(batch).await?;
            }
            self.repo.delete_chunks_by_ids(&stale_ids).await?;
        }

        // Immediately invalidate cached answers via namespace version bump
        let _ = self.cache.bump_cache_version().await;

        Ok(IngestMarkdownResult {
            chunks_created: chunks.len(),
            dimensions,
            chunks,
        })
    }
}
