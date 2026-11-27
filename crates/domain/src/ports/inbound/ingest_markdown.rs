use async_trait::async_trait;

use crate::entities::KnowledgeChunk;
use crate::errors::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestMarkdownCommand {
    pub doc_id_prefix: String,
    pub default_doc_title: Option<String>,
    pub category: String,
    pub markdown_content: String,
    pub max_chunk_chars: usize,
    pub overlap_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestMarkdownResult {
    pub chunks_created: usize,
    pub dimensions: usize,
    pub chunks: Vec<KnowledgeChunk>,
}

#[async_trait(?Send)]
pub trait IngestMarkdownUseCasePort {
    async fn execute(
        &self,
        cmd: IngestMarkdownCommand,
    ) -> Result<IngestMarkdownResult, DomainError>;
}
