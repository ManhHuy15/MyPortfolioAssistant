use async_trait::async_trait;

use crate::errors::DomainError;

#[derive(Debug, Clone, PartialEq)]
pub struct VectorMatch {
    pub id: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorRecord {
    pub id: String,
    pub values: Vec<f32>,
    pub doc_title: String,
    pub category: String,
}

#[async_trait(?Send)]
pub trait VectorIndexPort {
    async fn upsert_vectors_batch(&self, records: &[VectorRecord]) -> Result<(), DomainError>;

    async fn query_top_k(
        &self,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorMatch>, DomainError>;
}
