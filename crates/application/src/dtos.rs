use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequestDto {
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponseDto {
    pub session_id: String,
    pub reply: String,
    pub sources: Vec<String>,
    pub is_cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestMarkdownRequestDto {
    pub doc_id_prefix: String,
    pub default_doc_title: Option<String>,
    #[serde(default)]
    pub category: String,
    pub markdown_content: String,
    pub max_chunk_chars: Option<usize>,
    pub overlap_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestedChunkItemDto {
    pub id: String,
    pub doc_title: String,
    pub category: String,
    pub char_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestMarkdownResponseDto {
    pub chunks_created: usize,
    pub dimensions: usize,
    pub chunks: Vec<IngestedChunkItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponseDto {
    pub status: String,
    pub service: String,
}
