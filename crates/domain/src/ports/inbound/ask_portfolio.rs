use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::entities::SessionMeta;
use crate::errors::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskPortfolioCommand {
    pub session: SessionMeta,
    pub question: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskPortfolioResult {
    pub session_id: String,
    pub reply: String,
    pub sources: Vec<String>,
    pub is_cached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStreamEvent {
    Token(String),
    Done(AskPortfolioResult),
}

pub type ChatEventStream = Pin<Box<dyn Stream<Item = Result<ChatStreamEvent, DomainError>>>>;

#[async_trait(?Send)]
pub trait AskPortfolioUseCasePort {
    async fn execute(&self, cmd: AskPortfolioCommand) -> Result<AskPortfolioResult, DomainError>;
    async fn execute_stream(
        &self,
        cmd: AskPortfolioCommand,
    ) -> Result<ChatEventStream, DomainError>;
}
