use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::entities::ChatMessageRecord;
use crate::errors::DomainError;

pub type TokenStream = Pin<Box<dyn Stream<Item = Result<String, DomainError>>>>;

#[async_trait(?Send)]
pub trait AiCompletionPort {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, DomainError>;
    async fn rewrite_question(
        &self,
        recent_messages: &[ChatMessageRecord],
        user_question: &str,
    ) -> Result<String, DomainError>;
    async fn generate_reply(
        &self,
        system_prompt: &str,
        user_question: &str,
    ) -> Result<String, DomainError>;
    async fn generate_reply_stream(
        &self,
        system_prompt: &str,
        user_question: &str,
    ) -> Result<TokenStream, DomainError>;
}
