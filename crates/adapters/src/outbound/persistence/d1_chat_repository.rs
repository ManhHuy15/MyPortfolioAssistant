use std::collections::HashMap;

use async_trait::async_trait;
use domain::entities::{ChatMessageRecord, KnowledgeChunk, MessageRole, SessionMeta};
use domain::errors::DomainError;
use domain::ports::outbound::ChatRepositoryPort;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::{D1Database, D1PreparedStatement};

pub struct D1ChatRepository {
    db: D1Database,
}

impl D1ChatRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct ChunkRow {
    id: String,
    doc_title: String,
    category: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MessageRow {
    session_id: String,
    role: String,
    content: String,
    sources: Option<String>,
    is_cached: i32,
}

#[async_trait(?Send)]
impl ChatRepositoryPort for D1ChatRepository {
    async fn upsert_session(&self, session: &SessionMeta) -> Result<(), DomainError> {
        let stmt = self.db.prepare(
            "INSERT INTO chat_sessions (session_id, ip_address, country, user_agent, message_count)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(session_id) DO UPDATE SET
               message_count = message_count + 1,
               last_active_at = CURRENT_TIMESTAMP",
        );
        let ua_val = session
            .user_agent
            .clone()
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL);

        stmt.bind(&[
            session.session_id.clone().into(),
            session.ip_address.clone().into(),
            session.country.clone().into(),
            ua_val,
        ])
        .map_err(|e| DomainError::Storage(e.to_string()))?
        .run()
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn save_message(&self, message: &ChatMessageRecord) -> Result<(), DomainError> {
        let sources_val = if message.sources.is_empty() {
            JsValue::NULL
        } else {
            let json = serde_json::to_string(&message.sources).unwrap_or_else(|_| "[]".to_string());
            JsValue::from(json)
        };
        let is_cached_int = if message.is_cached { 1 } else { 0 };

        self.db
            .prepare(
                "INSERT INTO chat_messages (session_id, role, content, sources, is_cached)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(&[
                message.session_id.clone().into(),
                message.role.as_str().into(),
                message.content.clone().into(),
                sources_val,
                is_cached_int.into(),
            ])
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .run()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn fetch_recent_messages(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessageRecord>, DomainError> {
        let rows = self
            .db
            .prepare(
                "SELECT session_id, role, content, sources, is_cached
                 FROM chat_messages
                 WHERE session_id = ?1
                 ORDER BY id DESC
                 LIMIT ?2",
            )
            .bind(&[session_id.into(), (limit as i32).into()])
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .all()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .results::<MessageRow>()
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        let mut records: Vec<ChatMessageRecord> = rows
            .into_iter()
            .map(|r| {
                let role = if r.role == "assistant" {
                    MessageRole::Assistant
                } else {
                    MessageRole::User
                };
                let sources = r
                    .sources
                    .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
                    .unwrap_or_default();
                ChatMessageRecord {
                    session_id: r.session_id,
                    role,
                    content: r.content,
                    sources,
                    is_cached: r.is_cached != 0,
                }
            })
            .collect();

        records.reverse();
        Ok(records)
    }

    async fn upsert_chunks_batch(&self, chunks: &[KnowledgeChunk]) -> Result<(), DomainError> {
        if chunks.is_empty() {
            return Ok(());
        }

        let mut statements: Vec<D1PreparedStatement> = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let stmt = self
                .db
                .prepare(
                    "INSERT INTO chunks (id, doc_title, category, content, updated_at)
                     VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
                     ON CONFLICT(id) DO UPDATE SET
                       doc_title = excluded.doc_title,
                       category = excluded.category,
                       content = excluded.content,
                       updated_at = CURRENT_TIMESTAMP",
                )
                .bind(&[
                    chunk.id.clone().into(),
                    chunk.doc_title.clone().into(),
                    chunk.category.clone().into(),
                    chunk.content.clone().into(),
                ])
                .map_err(|e| DomainError::Storage(e.to_string()))?;
            statements.push(stmt);
        }

        self.db
            .batch(statements)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn fetch_chunks_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<KnowledgeChunk>, DomainError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "SELECT id, doc_title, category, content FROM chunks WHERE id IN ({})",
            placeholders.join(", ")
        );

        let bind_values: Vec<JsValue> = ids.iter().map(|id| JsValue::from(id.as_str())).collect();

        let rows = self
            .db
            .prepare(&sql)
            .bind(&bind_values)
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .all()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .results::<ChunkRow>()
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        let mut map: HashMap<String, KnowledgeChunk> = rows
            .into_iter()
            .map(|r| {
                (
                    r.id.clone(),
                    KnowledgeChunk {
                        id: r.id,
                        doc_title: r.doc_title,
                        category: r.category,
                        content: r.content,
                    },
                )
            })
            .collect();

        let ordered = ids
            .iter()
            .filter_map(|id| map.remove(id))
            .collect::<Vec<_>>();

        Ok(ordered)
    }

    async fn fetch_recent_chunks(&self, limit: usize) -> Result<Vec<KnowledgeChunk>, DomainError> {
        let rows = self
            .db
            .prepare(
                "SELECT id, doc_title, category, content FROM chunks ORDER BY updated_at DESC LIMIT ?1",
            )
            .bind(&[(limit as i32).into()])
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .all()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .results::<ChunkRow>()
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| KnowledgeChunk {
                id: r.id,
                doc_title: r.doc_title,
                category: r.category,
                content: r.content,
            })
            .collect())
    }
}
