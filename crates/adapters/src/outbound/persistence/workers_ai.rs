use std::collections::VecDeque;

use async_trait::async_trait;
use common::config::AiConfig;
use domain::entities::{ChatMessageRecord, MessageRole};
use domain::errors::DomainError;
use domain::ports::outbound::{AiCompletionPort, TokenStream};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use worker::{Ai, ByteStream};

pub struct WorkersAiCompletion {
    ai: Ai,
    config: AiConfig,
}

impl WorkersAiCompletion {
    pub fn new(ai: Ai, config: AiConfig) -> Self {
        Self { ai, config }
    }
}

#[derive(Debug, Serialize)]
struct AiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct AiChatInput {
    messages: Vec<AiMessage>,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct AiChatStreamInput {
    messages: Vec<AiMessage>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct AiChatOutput {
    response: Option<String>,
}

#[derive(Debug, Serialize)]
struct EmbeddingInput<'a> {
    text: &'a [String],
}

#[derive(Debug, Deserialize)]
struct EmbeddingOutput {
    data: Vec<Vec<f32>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorkersAiSseLine {
    Token(String),
    Done,
    Ignore,
}

pub fn parse_workers_ai_sse_line(line: &str) -> WorkersAiSseLine {
    let trimmed = line.trim();
    let Some(data) = trimmed.strip_prefix("data:") else {
        return WorkersAiSseLine::Ignore;
    };
    let payload = data.trim();
    if payload.is_empty() {
        return WorkersAiSseLine::Ignore;
    }
    if payload == "[DONE]" {
        return WorkersAiSseLine::Done;
    }

    match serde_json::from_str::<AiChatOutput>(payload) {
        Ok(parsed) => match parsed.response {
            Some(token) if !token.is_empty() => WorkersAiSseLine::Token(token),
            _ => WorkersAiSseLine::Ignore,
        },
        Err(_) => WorkersAiSseLine::Ignore,
    }
}

struct SseParserState {
    byte_stream: ByteStream,
    raw_buffer: Vec<u8>,
    pending_tokens: VecDeque<String>,
    done: bool,
}

impl SseParserState {
    fn drain_complete_lines(&mut self) {
        while let Some(newline_pos) = self.raw_buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.raw_buffer.drain(..=newline_pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            match parse_workers_ai_sse_line(&line) {
                WorkersAiSseLine::Token(token) => self.pending_tokens.push_back(token),
                WorkersAiSseLine::Done => {
                    self.done = true;
                    break;
                }
                WorkersAiSseLine::Ignore => {}
            }
        }
    }

    fn flush_remaining(&mut self) {
        if self.raw_buffer.is_empty() {
            return;
        }
        let line_bytes = std::mem::take(&mut self.raw_buffer);
        let line = String::from_utf8_lossy(&line_bytes);
        if let WorkersAiSseLine::Token(token) = parse_workers_ai_sse_line(&line) {
            self.pending_tokens.push_back(token);
        }
    }
}

#[async_trait(?Send)]
impl AiCompletionPort for WorkersAiCompletion {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, DomainError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let input = EmbeddingInput { text: texts };
        let out: EmbeddingOutput = self
            .ai
            .run(&self.config.embedding_model, input)
            .await
            .map_err(|e| DomainError::AiProvider(e.to_string()))?;

        if out.data.is_empty() {
            return Err(DomainError::AiProvider(
                "Empty embedding vectors returned".to_string(),
            ));
        }

        Ok(out.data)
    }

    async fn rewrite_question(
        &self,
        recent_messages: &[ChatMessageRecord],
        user_question: &str,
    ) -> Result<String, DomainError> {
        let mut history_lines = String::new();
        for msg in recent_messages {
            let speaker = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
            };
            history_lines.push_str(&format!("{speaker}: {}\n", msg.content));
        }

        let system_prompt = "Nhiệm vụ của bạn là viết lại câu hỏi hiện tại của User thành một câu hỏi độc lập, đầy đủ chủ ngữ dựa trên lịch sử hội thoại gần nhất. Chỉ trả về duy nhất câu hỏi đã viết lại, không giải thích, không trả lời câu hỏi.";
        let user_prompt = format!(
            "Lịch sử hội thoại:\n{history_lines}\nCâu hỏi hiện tại: {user_question}\nCâu hỏi viết lại:"
        );

        let input = AiChatInput {
            messages: vec![
                AiMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                AiMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
            temperature: 0.0,
        };

        let out: AiChatOutput = self
            .ai
            .run(&self.config.rewrite_model, input)
            .await
            .map_err(|e| DomainError::AiProvider(e.to_string()))?;

        Ok(out.response.unwrap_or_else(|| user_question.to_string()))
    }

    async fn generate_reply(
        &self,
        system_prompt: &str,
        user_question: &str,
    ) -> Result<String, DomainError> {
        let input = AiChatInput {
            messages: vec![
                AiMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                AiMessage {
                    role: "user".to_string(),
                    content: user_question.to_string(),
                },
            ],
            temperature: self.config.answer_temperature,
        };

        let out: AiChatOutput = self
            .ai
            .run(&self.config.answer_model, input)
            .await
            .map_err(|e| DomainError::AiProvider(e.to_string()))?;

        Ok(out
            .response
            .unwrap_or_else(|| "Xin lỗi, hiện tại tôi chưa thể trả lời câu hỏi này.".to_string()))
    }

    async fn generate_reply_stream(
        &self,
        system_prompt: &str,
        user_question: &str,
    ) -> Result<TokenStream, DomainError> {
        let input = AiChatStreamInput {
            messages: vec![
                AiMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                AiMessage {
                    role: "user".to_string(),
                    content: user_question.to_string(),
                },
            ],
            temperature: self.config.answer_temperature,
            stream: true,
        };

        let byte_stream = self
            .ai
            .run_bytes(&self.config.answer_model, input)
            .await
            .map_err(|e| DomainError::AiProvider(e.to_string()))?;

        let state = SseParserState {
            byte_stream,
            raw_buffer: Vec::new(),
            pending_tokens: VecDeque::new(),
            done: false,
        };

        let token_stream = futures_util::stream::unfold(state, |mut st| async move {
            loop {
                if let Some(token) = st.pending_tokens.pop_front() {
                    return Some((Ok(token), st));
                }

                if st.done {
                    return None;
                }

                match st.byte_stream.next().await {
                    Some(Ok(bytes)) => {
                        st.raw_buffer.extend_from_slice(&bytes);
                        st.drain_complete_lines();
                    }
                    Some(Err(err)) => {
                        st.done = true;
                        return Some((Err(DomainError::AiProvider(err.to_string())), st));
                    }
                    None => {
                        st.done = true;
                        st.flush_remaining();
                        if let Some(token) = st.pending_tokens.pop_front() {
                            return Some((Ok(token), st));
                        }
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(token_stream))
    }
}
