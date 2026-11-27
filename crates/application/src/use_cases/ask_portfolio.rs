use std::rc::Rc;

use async_trait::async_trait;
use common::config::RagConfig;
use domain::entities::{CachedAnswer, ChatMessageRecord, MessageRole};
use domain::errors::DomainError;
use domain::ports::inbound::{
    AskPortfolioCommand, AskPortfolioResult, AskPortfolioUseCasePort, ChatEventStream,
    ChatStreamEvent,
};
use domain::ports::outbound::{
    AiCompletionPort, AnswerCachePort, ChatRepositoryPort, TokenStream, VectorIndexPort,
};
use futures_util::StreamExt;

const FALLBACK_EMPTY_REPLY: &str = "Xin lỗi, hiện tại tôi chưa thể trả lời câu hỏi này.";

enum PreparedChat {
    Cached(AskPortfolioResult),
    Ready {
        session_id: String,
        effective_question: String,
        cache_key: String,
        system_prompt: String,
        sources: Vec<String>,
    },
}

struct StreamState {
    token_stream: TokenStream,
    full_reply: String,
    repo: Rc<dyn ChatRepositoryPort>,
    cache: Rc<dyn AnswerCachePort>,
    cache_ttl_seconds: u64,
    session_id: String,
    cache_key: String,
    sources: Vec<String>,
    finished: bool,
}

pub struct AskPortfolioUseCase {
    repo: Rc<dyn ChatRepositoryPort>,
    vector_index: Rc<dyn VectorIndexPort>,
    cache: Rc<dyn AnswerCachePort>,
    ai: Rc<dyn AiCompletionPort>,
    rag_config: RagConfig,
}

impl AskPortfolioUseCase {
    pub fn new(
        repo: Rc<dyn ChatRepositoryPort>,
        vector_index: Rc<dyn VectorIndexPort>,
        cache: Rc<dyn AnswerCachePort>,
        ai: Rc<dyn AiCompletionPort>,
        rag_config: RagConfig,
    ) -> Self {
        Self {
            repo,
            vector_index,
            cache,
            ai,
            rag_config,
        }
    }

    pub fn normalize_cache_key(question: &str) -> String {
        let normalized = question
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        format!("chat_cache:{normalized}")
    }

    async fn prepare_chat(&self, cmd: AskPortfolioCommand) -> Result<PreparedChat, DomainError> {
        let session_id = cmd.session.session_id.trim().to_string();
        if session_id.is_empty() {
            return Err(DomainError::Validation {
                field: "sessionId".to_string(),
                message: "sessionId cannot be empty".to_string(),
            });
        }

        let question = cmd.question.trim().to_string();
        if question.is_empty() {
            return Err(DomainError::Validation {
                field: "message".to_string(),
                message: "Message cannot be empty".to_string(),
            });
        }

        self.repo.upsert_session(&cmd.session).await?;

        // 1. Fetch recent messages before inserting the current user message
        let recent_messages = self
            .repo
            .fetch_recent_messages(&session_id, self.rag_config.recent_messages_limit)
            .await?;

        self.repo
            .save_message(&ChatMessageRecord {
                session_id: session_id.clone(),
                role: MessageRole::User,
                content: question.clone(),
                sources: Vec::new(),
                is_cached: false,
            })
            .await?;

        // 2. Rewrite question using recent messages if conversation history exists
        let effective_question = if recent_messages.is_empty() {
            question.clone()
        } else {
            let rewritten = self
                .ai
                .rewrite_question(&recent_messages, &question)
                .await
                .unwrap_or_else(|_| question.clone());
            let trimmed = rewritten.trim();
            if trimmed.is_empty() {
                question.clone()
            } else {
                trimmed.to_string()
            }
        };

        // 3. Check Workers KV Cache
        let cache_key = Self::normalize_cache_key(&effective_question);
        if let Ok(Some(cached)) = self.cache.get_cached_answer(&cache_key).await {
            self.repo
                .save_message(&ChatMessageRecord {
                    session_id: session_id.clone(),
                    role: MessageRole::Assistant,
                    content: cached.reply.clone(),
                    sources: cached.sources.clone(),
                    is_cached: true,
                })
                .await?;

            return Ok(PreparedChat::Cached(AskPortfolioResult {
                session_id,
                reply: cached.reply,
                sources: cached.sources,
                is_cached: true,
            }));
        }

        // 4. Embed effective_question and query Vectorize top-K
        let embeddings = self
            .ai
            .embed_texts(std::slice::from_ref(&effective_question))
            .await?;
        let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
            DomainError::AiProvider("Empty embedding vector returned".to_string())
        })?;
        let vector_matches = self
            .vector_index
            .query_top_k(&query_embedding, self.rag_config.top_k)
            .await
            .unwrap_or_default();

        let matched_ids: Vec<String> = vector_matches
            .into_iter()
            .filter(|m| m.score >= self.rag_config.min_score)
            .map(|m| m.id)
            .collect();

        let chunks = if !matched_ids.is_empty() {
            let fetched = self.repo.fetch_chunks_by_ids(&matched_ids).await?;
            if fetched.is_empty() {
                self.repo
                    .fetch_recent_chunks(self.rag_config.fallback_limit)
                    .await?
            } else {
                fetched
            }
        } else {
            self.repo
                .fetch_recent_chunks(self.rag_config.fallback_limit)
                .await?
        };

        let mut sources = Vec::new();
        let mut context_text = String::new();
        for chunk in &chunks {
            if !sources.contains(&chunk.doc_title) {
                sources.push(chunk.doc_title.clone());
            }
            context_text.push_str(&format!(
                "--- [{}] ({}) ---\n{}\n\n",
                chunk.doc_title, chunk.category, chunk.content
            ));
        }

        let system_prompt = if context_text.is_empty() {
            "Bạn là trợ lý AI cho trang Portfolio cá nhân. Hãy trả lời lịch sự, ngắn gọn."
                .to_string()
        } else {
            format!(
                "Bạn là trợ lý AI cho trang Portfolio cá nhân. Chỉ dựa vào thông tin dưới đây để trả lời câu hỏi của khách truy cập một cách chính xác, ngắn gọn và thân thiện:\n\n{context_text}"
            )
        };

        Ok(PreparedChat::Ready {
            session_id,
            effective_question,
            cache_key,
            system_prompt,
            sources,
        })
    }
}

#[async_trait(?Send)]
impl AskPortfolioUseCasePort for AskPortfolioUseCase {
    async fn execute(&self, cmd: AskPortfolioCommand) -> Result<AskPortfolioResult, DomainError> {
        let (session_id, effective_question, cache_key, system_prompt, sources) =
            match self.prepare_chat(cmd).await? {
                PreparedChat::Cached(result) => return Ok(result),
                PreparedChat::Ready {
                    session_id,
                    effective_question,
                    cache_key,
                    system_prompt,
                    sources,
                } => (
                    session_id,
                    effective_question,
                    cache_key,
                    system_prompt,
                    sources,
                ),
            };

        let reply = self
            .ai
            .generate_reply(&system_prompt, &effective_question)
            .await?;

        let _ = self
            .cache
            .set_cached_answer(
                &cache_key,
                &CachedAnswer {
                    reply: reply.clone(),
                    sources: sources.clone(),
                },
                self.rag_config.cache_ttl_seconds,
            )
            .await;

        self.repo
            .save_message(&ChatMessageRecord {
                session_id: session_id.clone(),
                role: MessageRole::Assistant,
                content: reply.clone(),
                sources: sources.clone(),
                is_cached: false,
            })
            .await?;

        Ok(AskPortfolioResult {
            session_id,
            reply,
            sources,
            is_cached: false,
        })
    }

    async fn execute_stream(
        &self,
        cmd: AskPortfolioCommand,
    ) -> Result<ChatEventStream, DomainError> {
        match self.prepare_chat(cmd).await? {
            PreparedChat::Cached(result) => {
                let events = vec![
                    Ok(ChatStreamEvent::Token(result.reply.clone())),
                    Ok(ChatStreamEvent::Done(result)),
                ];
                Ok(Box::pin(futures_util::stream::iter(events)))
            }
            PreparedChat::Ready {
                session_id,
                effective_question,
                cache_key,
                system_prompt,
                sources,
            } => {
                let token_stream = self
                    .ai
                    .generate_reply_stream(&system_prompt, &effective_question)
                    .await?;

                let state = StreamState {
                    token_stream,
                    full_reply: String::new(),
                    repo: Rc::clone(&self.repo),
                    cache: Rc::clone(&self.cache),
                    cache_ttl_seconds: self.rag_config.cache_ttl_seconds,
                    session_id,
                    cache_key,
                    sources,
                    finished: false,
                };

                let stream = futures_util::stream::unfold(state, |mut st| async move {
                    if st.finished {
                        return None;
                    }

                    match st.token_stream.next().await {
                        Some(Ok(token)) => {
                            st.full_reply.push_str(&token);
                            Some((Ok(ChatStreamEvent::Token(token)), st))
                        }
                        Some(Err(err)) => {
                            st.finished = true;
                            Some((Err(err), st))
                        }
                        None => {
                            st.finished = true;
                            let final_reply = if st.full_reply.trim().is_empty() {
                                FALLBACK_EMPTY_REPLY.to_string()
                            } else {
                                st.full_reply.clone()
                            };

                            let _ = st
                                .cache
                                .set_cached_answer(
                                    &st.cache_key,
                                    &CachedAnswer {
                                        reply: final_reply.clone(),
                                        sources: st.sources.clone(),
                                    },
                                    st.cache_ttl_seconds,
                                )
                                .await;

                            if let Err(err) = st
                                .repo
                                .save_message(&ChatMessageRecord {
                                    session_id: st.session_id.clone(),
                                    role: MessageRole::Assistant,
                                    content: final_reply.clone(),
                                    sources: st.sources.clone(),
                                    is_cached: false,
                                })
                                .await
                            {
                                return Some((Err(err), st));
                            }

                            let result = AskPortfolioResult {
                                session_id: st.session_id.clone(),
                                reply: final_reply,
                                sources: st.sources.clone(),
                                is_cached: false,
                            };
                            Some((Ok(ChatStreamEvent::Done(result)), st))
                        }
                    }
                });

                Ok(Box::pin(stream))
            }
        }
    }
}
