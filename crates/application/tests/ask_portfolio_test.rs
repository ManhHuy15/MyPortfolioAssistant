use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use application::use_cases::ask_portfolio::AskPortfolioUseCase;
use application::use_cases::ingest_markdown::IngestMarkdownUseCase;
use application::use_cases::manage_knowledge::ManageKnowledgeUseCase;
use application::use_cases::markdown_chunker::chunk_markdown;
use async_trait::async_trait;
use common::config::{RagConfig, RateLimitConfig};
use domain::entities::{CachedAnswer, ChatMessageRecord, KnowledgeChunk, SessionMeta};
use domain::errors::DomainError;
use domain::ports::inbound::{
    AskPortfolioCommand, AskPortfolioUseCasePort, ChatStreamEvent, IngestMarkdownCommand,
    IngestMarkdownUseCasePort, ManageKnowledgeUseCasePort,
};
use domain::ports::outbound::{
    AiCompletionPort, AnswerCachePort, ChatRepositoryPort, RateLimitDecision, RateLimiterPort,
    TokenStream, VectorIndexPort, VectorMatch, VectorRecord,
};
use futures::executor::block_on;
use futures_util::StreamExt;

#[derive(Default)]
struct MockRepo {
    saved_messages: RefCell<Vec<ChatMessageRecord>>,
    chunks: RefCell<Vec<KnowledgeChunk>>,
}

#[async_trait(?Send)]
impl ChatRepositoryPort for MockRepo {
    async fn upsert_session(&self, _session: &SessionMeta) -> Result<(), DomainError> {
        Ok(())
    }

    async fn save_message(&self, message: &ChatMessageRecord) -> Result<(), DomainError> {
        self.saved_messages.borrow_mut().push(message.clone());
        Ok(())
    }

    async fn fetch_recent_messages(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessageRecord>, DomainError> {
        let msgs: Vec<ChatMessageRecord> = self
            .saved_messages
            .borrow()
            .iter()
            .filter(|m| m.session_id == session_id)
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Ok(msgs)
    }

    async fn upsert_chunks_batch(&self, chunks: &[KnowledgeChunk]) -> Result<(), DomainError> {
        let mut store = self.chunks.borrow_mut();
        for chunk in chunks {
            if let Some(existing) = store.iter_mut().find(|c| c.id == chunk.id) {
                *existing = chunk.clone();
            } else {
                store.push(chunk.clone());
            }
        }
        Ok(())
    }

    async fn fetch_chunks_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<KnowledgeChunk>, DomainError> {
        let all = self.chunks.borrow();
        let matched = ids
            .iter()
            .filter_map(|id| all.iter().find(|c| &c.id == id).cloned())
            .collect();
        Ok(matched)
    }

    async fn fetch_recent_chunks(&self, _limit: usize) -> Result<Vec<KnowledgeChunk>, DomainError> {
        Ok(self.chunks.borrow().clone())
    }

    async fn list_chunks(&self) -> Result<Vec<KnowledgeChunk>, DomainError> {
        Ok(self.chunks.borrow().clone())
    }

    async fn find_chunk_ids_by_target(&self, target: &str) -> Result<Vec<String>, DomainError> {
        let prefix = format!("{target}-");
        let ids = self
            .chunks
            .borrow()
            .iter()
            .filter(|c| c.id == target || c.id.starts_with(&prefix))
            .map(|c| c.id.clone())
            .collect();
        Ok(ids)
    }

    async fn delete_chunks_by_ids(&self, ids: &[String]) -> Result<(), DomainError> {
        self.chunks.borrow_mut().retain(|c| !ids.contains(&c.id));
        Ok(())
    }
}

#[derive(Default)]
struct MockVectorIndex {
    upserted_records: RefCell<Vec<VectorRecord>>,
    deleted_ids: RefCell<Vec<String>>,
    matches: Vec<VectorMatch>,
}

#[async_trait(?Send)]
impl VectorIndexPort for MockVectorIndex {
    async fn upsert_vectors_batch(&self, records: &[VectorRecord]) -> Result<(), DomainError> {
        self.upserted_records
            .borrow_mut()
            .extend_from_slice(records);
        Ok(())
    }

    async fn query_top_k(
        &self,
        _vector: &[f32],
        _top_k: usize,
    ) -> Result<Vec<VectorMatch>, DomainError> {
        Ok(self.matches.clone())
    }

    async fn delete_vectors_by_ids(&self, ids: &[String]) -> Result<(), DomainError> {
        self.deleted_ids.borrow_mut().extend_from_slice(ids);
        Ok(())
    }
}

struct MockCache {
    entries: RefCell<HashMap<String, CachedAnswer>>,
    version: Cell<u64>,
}

impl Default for MockCache {
    fn default() -> Self {
        Self {
            entries: RefCell::new(HashMap::new()),
            version: Cell::new(1),
        }
    }
}

#[async_trait(?Send)]
impl AnswerCachePort for MockCache {
    async fn get_cache_version(&self) -> Result<u64, DomainError> {
        Ok(self.version.get())
    }

    async fn bump_cache_version(&self) -> Result<u64, DomainError> {
        let next = self.version.get() + 1;
        self.version.set(next);
        Ok(next)
    }

    async fn get_cached_answer(&self, key: &str) -> Result<Option<CachedAnswer>, DomainError> {
        Ok(self.entries.borrow().get(key).cloned())
    }

    async fn set_cached_answer(
        &self,
        key: &str,
        answer: &CachedAnswer,
        _ttl_seconds: u64,
    ) -> Result<(), DomainError> {
        self.entries
            .borrow_mut()
            .insert(key.to_string(), answer.clone());
        Ok(())
    }
}

#[derive(Default)]
struct MockRateLimiter {
    counts: RefCell<HashMap<String, u32>>,
}

#[async_trait(?Send)]
impl RateLimiterPort for MockRateLimiter {
    async fn check_and_increment(
        &self,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> Result<RateLimitDecision, DomainError> {
        let mut map = self.counts.borrow_mut();
        let count = map.entry(key.to_string()).or_insert(0);
        if *count >= max_requests {
            Ok(RateLimitDecision {
                allowed: false,
                remaining: 0,
                retry_after_seconds: window_seconds,
            })
        } else {
            *count += 1;
            Ok(RateLimitDecision {
                allowed: true,
                remaining: max_requests - *count,
                retry_after_seconds: window_seconds,
            })
        }
    }
}

struct MockAi {
    response: String,
}

#[async_trait(?Send)]
impl AiCompletionPort for MockAi {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, DomainError> {
        Ok(texts.iter().map(|_| vec![0.1; 1024]).collect())
    }

    async fn rewrite_question(
        &self,
        _recent_messages: &[ChatMessageRecord],
        user_question: &str,
    ) -> Result<String, DomainError> {
        Ok(user_question.to_string())
    }

    async fn generate_reply(
        &self,
        _system_prompt: &str,
        _user_question: &str,
    ) -> Result<String, DomainError> {
        Ok(self.response.clone())
    }

    async fn generate_reply_stream(
        &self,
        _system_prompt: &str,
        _user_question: &str,
    ) -> Result<TokenStream, DomainError> {
        let tokens: Vec<Result<String, DomainError>> = self
            .response
            .split_inclusive(' ')
            .map(|part| Ok(part.to_string()))
            .collect();
        Ok(Box::pin(futures_util::stream::iter(tokens)))
    }
}

#[test]
fn test_validation_empty_message() {
    let repo = Rc::new(MockRepo::default());
    let vector_index = Rc::new(MockVectorIndex::default());
    let cache = Rc::new(MockCache::default());
    let ai = Rc::new(MockAi {
        response: "Hi".to_string(),
    });
    let rate_limiter = Rc::new(MockRateLimiter::default());
    let use_case = AskPortfolioUseCase::new(
        repo,
        vector_index,
        cache,
        ai,
        rate_limiter,
        RagConfig::default(),
        RateLimitConfig::default(),
    );

    let cmd = AskPortfolioCommand {
        session: SessionMeta {
            session_id: "sess-1".to_string(),
            ip_address: "127.0.0.1".to_string(),
            country: "VN".to_string(),
            user_agent: None,
        },
        question: "   ".to_string(),
    };

    let res = block_on(use_case.execute(cmd));
    assert!(matches!(
        res,
        Err(DomainError::Validation { field, .. }) if field == "message"
    ));
}

#[test]
fn test_ask_portfolio_rag_with_vectorize_and_cache() {
    let repo = Rc::new(MockRepo {
        saved_messages: RefCell::new(Vec::new()),
        chunks: RefCell::new(vec![
            KnowledgeChunk {
                id: "chunk-1".to_string(),
                doc_title: "Dự án YuhTube".to_string(),
                category: "project".to_string(),
                content: "Ứng dụng xem video".to_string(),
            },
            KnowledgeChunk {
                id: "chunk-2".to_string(),
                doc_title: "Kỹ năng Rust".to_string(),
                category: "skill".to_string(),
                content: "Hexagonal Architecture trên Cloudflare Workers".to_string(),
            },
        ]),
    });
    let vector_index = Rc::new(MockVectorIndex {
        upserted_records: RefCell::new(Vec::new()),
        deleted_ids: RefCell::new(Vec::new()),
        matches: vec![VectorMatch {
            id: "chunk-1".to_string(),
            score: 0.89,
        }],
    });
    let cache = Rc::new(MockCache::default());
    let ai = Rc::new(MockAi {
        response: "Huy đã làm dự án YuhTube.".to_string(),
    });
    let rate_limiter = Rc::new(MockRateLimiter::default());
    let use_case = AskPortfolioUseCase::new(
        repo.clone(),
        vector_index,
        cache,
        ai,
        rate_limiter,
        RagConfig::default(),
        RateLimitConfig::default(),
    );

    let cmd = AskPortfolioCommand {
        session: SessionMeta {
            session_id: "sess-1".to_string(),
            ip_address: "127.0.0.1".to_string(),
            country: "VN".to_string(),
            user_agent: None,
        },
        question: "Huy có dự án nào?".to_string(),
    };

    let first = block_on(use_case.execute(cmd.clone())).expect("should succeed");
    assert!(!first.is_cached);
    assert_eq!(first.reply, "Huy đã làm dự án YuhTube.");
    assert_eq!(first.sources, vec!["Dự án YuhTube".to_string()]);

    let second = block_on(use_case.execute(cmd)).expect("should succeed");
    assert!(second.is_cached);
    assert_eq!(second.reply, "Huy đã làm dự án YuhTube.");
    assert_eq!(repo.saved_messages.borrow().len(), 4);
}

#[test]
fn test_ingest_markdown_full_breadcrumb_and_code_block_safety() {
    let repo = Rc::new(MockRepo::default());
    let vector_index = Rc::new(MockVectorIndex::default());
    let ai = Rc::new(MockAi {
        response: String::new(),
    });
    let cache = Rc::new(MockCache::default());
    let use_case = IngestMarkdownUseCase::new(repo.clone(), vector_index.clone(), ai, cache);

    let markdown = "\
# Hồ sơ Mạnh Huy
Kỹ sư phần mềm chuyên về Rust và hệ thống phân tán.

## Dự án nổi bật
### YuhTube
Xây dựng nền tảng chia sẻ video hiệu năng cao.

```bash
# This comment must NOT be treated as a Markdown heading
cargo test --workspace
```

## Kỹ năng Cloud
Triển khai serverless trên Cloudflare Workers, D1, KV và Vectorize.
";

    let cmd = IngestMarkdownCommand {
        doc_id_prefix: "portfolio-md".to_string(),
        default_doc_title: Some("Portfolio".to_string()),
        category: "profile".to_string(),
        markdown_content: markdown.to_string(),
        max_chunk_chars: 1200,
        overlap_chars: 150,
    };

    let res = block_on(use_case.execute(cmd)).expect("markdown ingest should succeed");
    assert_eq!(res.chunks_created, 3);
    assert_eq!(res.dimensions, 1024);

    let saved_chunks = repo.chunks.borrow();
    assert_eq!(saved_chunks[0].doc_title, "Hồ sơ Mạnh Huy");
    assert_eq!(
        saved_chunks[1].doc_title,
        "Hồ sơ Mạnh Huy > Dự án nổi bật > YuhTube"
    );
    assert!(saved_chunks[1].content.contains("# This comment must NOT"));
    assert_eq!(saved_chunks[2].doc_title, "Hồ sơ Mạnh Huy > Kỹ năng Cloud");

    let saved_vectors = vector_index.upserted_records.borrow();
    assert_eq!(saved_vectors.len(), 3);
    assert_eq!(saved_vectors[0].values.len(), 1024);
}

#[test]
fn test_sentence_boundary_chunking_does_not_cut_mid_sentence() {
    let markdown = "\
# Giới thiệu
Đây là câu đầu tiên rất rõ ràng về kinh nghiệm Rust. Đây là câu thứ hai nói về Cloudflare Workers và D1. Đây là câu thứ ba mô tả Vectorize RAG với mô hình bge-m3.
";
    let chunks = chunk_markdown("intro", None, "profile", markdown, 110, 50);
    assert!(chunks.len() >= 2);
    for chunk in &chunks {
        assert!(
            chunk.content.ends_with('.'),
            "chunk should end on a sentence boundary: {}",
            chunk.content
        );
    }
}

#[test]
fn test_ask_portfolio_stream_tokens_and_final_done_event() {
    let repo = Rc::new(MockRepo {
        saved_messages: RefCell::new(Vec::new()),
        chunks: RefCell::new(vec![KnowledgeChunk {
            id: "chunk-1".to_string(),
            doc_title: "Dự án YuhTube".to_string(),
            category: "project".to_string(),
            content: "Ứng dụng xem video".to_string(),
        }]),
    });
    let vector_index = Rc::new(MockVectorIndex {
        upserted_records: RefCell::new(Vec::new()),
        deleted_ids: RefCell::new(Vec::new()),
        matches: vec![VectorMatch {
            id: "chunk-1".to_string(),
            score: 0.91,
        }],
    });
    let cache = Rc::new(MockCache::default());
    let ai = Rc::new(MockAi {
        response: "Huy làm dự án YuhTube.".to_string(),
    });
    let rate_limiter = Rc::new(MockRateLimiter::default());
    let use_case = AskPortfolioUseCase::new(
        repo.clone(),
        vector_index,
        cache.clone(),
        ai,
        rate_limiter,
        RagConfig::default(),
        RateLimitConfig::default(),
    );

    let cmd = AskPortfolioCommand {
        session: SessionMeta {
            session_id: "sess-stream".to_string(),
            ip_address: "127.0.0.1".to_string(),
            country: "VN".to_string(),
            user_agent: None,
        },
        question: "Huy làm dự án gì?".to_string(),
    };

    let events: Vec<ChatStreamEvent> = block_on(async {
        let mut stream = use_case
            .execute_stream(cmd)
            .await
            .expect("stream should start");
        let mut collected = Vec::new();
        while let Some(item) = stream.next().await {
            collected.push(item.expect("event should be ok"));
        }
        collected
    });

    assert!(events.len() >= 2);
    let mut combined = String::new();
    for event in &events[..events.len() - 1] {
        match event {
            ChatStreamEvent::Token(tok) => combined.push_str(tok),
            other => panic!("expected Token event, got {other:?}"),
        }
    }
    assert_eq!(combined, "Huy làm dự án YuhTube.");

    match events.last() {
        Some(ChatStreamEvent::Done(final_res)) => {
            assert_eq!(final_res.reply, "Huy làm dự án YuhTube.");
            assert_eq!(final_res.sources, vec!["Dự án YuhTube".to_string()]);
            assert!(!final_res.is_cached);
        }
        other => panic!("expected Done event last, got {other:?}"),
    }

    assert_eq!(repo.saved_messages.borrow().len(), 2);
    assert!(!cache.entries.borrow().is_empty());
}

#[test]
fn test_manage_knowledge_list_and_delete_by_prefix_and_not_found() {
    let repo = Rc::new(MockRepo {
        saved_messages: RefCell::new(Vec::new()),
        chunks: RefCell::new(vec![
            KnowledgeChunk {
                id: "portfolio-1".to_string(),
                doc_title: "Giới thiệu".to_string(),
                category: "profile".to_string(),
                content: "A".to_string(),
            },
            KnowledgeChunk {
                id: "portfolio-2".to_string(),
                doc_title: "Kỹ năng".to_string(),
                category: "profile".to_string(),
                content: "B".to_string(),
            },
            KnowledgeChunk {
                id: "blog-1".to_string(),
                doc_title: "Rust Wasm".to_string(),
                category: "blog".to_string(),
                content: "C".to_string(),
            },
        ]),
    });
    let vector_index = Rc::new(MockVectorIndex::default());
    let cache = Rc::new(MockCache::default());
    let use_case = ManageKnowledgeUseCase::new(repo.clone(), vector_index.clone(), cache.clone());

    let listed = block_on(use_case.list_chunks()).expect("list should succeed");
    assert_eq!(listed.len(), 3);

    let deleted = block_on(use_case.delete_by_target("portfolio")).expect("delete should succeed");
    assert_eq!(deleted.deleted_count, 2);
    assert_eq!(
        deleted.deleted_ids,
        vec!["portfolio-1".to_string(), "portfolio-2".to_string()]
    );
    assert_eq!(
        *vector_index.deleted_ids.borrow(),
        vec!["portfolio-1".to_string(), "portfolio-2".to_string()]
    );
    assert_eq!(repo.chunks.borrow().len(), 1);
    assert_eq!(repo.chunks.borrow()[0].id, "blog-1");

    let missing = block_on(use_case.delete_by_target("portfolio"));
    assert!(matches!(missing, Err(DomainError::NotFound(_))));
}

#[test]
fn test_rate_limit_blocks_by_session_and_by_ip() {
    let repo = Rc::new(MockRepo::default());
    let vector_index = Rc::new(MockVectorIndex::default());
    let cache = Rc::new(MockCache::default());
    let ai = Rc::new(MockAi {
        response: "OK".to_string(),
    });
    let rate_limiter = Rc::new(MockRateLimiter::default());
    let rl_cfg = RateLimitConfig {
        window_seconds: 60,
        max_requests_per_ip: 3,
        max_requests_per_session: 2,
    };
    let use_case = AskPortfolioUseCase::new(
        repo,
        vector_index,
        cache,
        ai,
        rate_limiter,
        RagConfig::default(),
        rl_cfg,
    );

    let make_cmd = |session_id: &str, ip: &str| AskPortfolioCommand {
        session: SessionMeta {
            session_id: session_id.to_string(),
            ip_address: ip.to_string(),
            country: "VN".to_string(),
            user_agent: None,
        },
        question: "Hello".to_string(),
    };

    // Session A: 2 requests allowed, 3rd blocked by session limit
    assert!(block_on(use_case.execute(make_cmd("sess-a", "1.1.1.1"))).is_ok());
    assert!(block_on(use_case.execute(make_cmd("sess-a", "1.1.1.1"))).is_ok());
    let session_blocked = block_on(use_case.execute(make_cmd("sess-a", "1.1.1.1")));
    assert!(matches!(
        session_blocked,
        Err(DomainError::RateLimited {
            retry_after_seconds: 60,
            ..
        })
    ));

    // Even if attacker switches to a fresh session ID from same IP 1.1.1.1:
    // IP already used 3 slots (2 + 1 before session check), so 4th request is blocked by IP limit
    let ip_blocked = block_on(use_case.execute(make_cmd("sess-b", "1.1.1.1")));
    assert!(matches!(ip_blocked, Err(DomainError::RateLimited { .. })));

    // Different IP + different session is still allowed
    assert!(block_on(use_case.execute(make_cmd("sess-c", "2.2.2.2"))).is_ok());
}

#[test]
fn test_reingest_cleans_stale_chunks_and_invalidates_cache() {
    let repo = Rc::new(MockRepo::default());
    let vector_index = Rc::new(MockVectorIndex::default());
    let cache = Rc::new(MockCache::default());
    let ai = Rc::new(MockAi {
        response: "Câu trả lời mới nhất".to_string(),
    });
    let rate_limiter = Rc::new(MockRateLimiter::default());

    let ingest = IngestMarkdownUseCase::new(
        repo.clone(),
        vector_index.clone(),
        ai.clone(),
        cache.clone(),
    );
    let ask = AskPortfolioUseCase::new(
        repo.clone(),
        vector_index.clone(),
        cache.clone(),
        ai,
        rate_limiter,
        RagConfig::default(),
        RateLimitConfig::default(),
    );

    // 1. Initial ingest creates 3 chunks: portfolio-1, portfolio-2, portfolio-3
    let v1_md = "# Phần 1\nNội dung 1\n\n## Phần 2\nNội dung 2\n\n## Phần 3\nNội dung 3";
    let res1 = block_on(ingest.execute(IngestMarkdownCommand {
        doc_id_prefix: "portfolio".to_string(),
        default_doc_title: None,
        category: "profile".to_string(),
        markdown_content: v1_md.to_string(),
        max_chunk_chars: 1200,
        overlap_chars: 150,
    }))
    .expect("first ingest should succeed");
    assert_eq!(res1.chunks_created, 3);
    assert_eq!(cache.version.get(), 2);

    // 2. Ask once -> caches under v2; ask again -> cache hit
    let cmd = AskPortfolioCommand {
        session: SessionMeta {
            session_id: "sess-reingest".to_string(),
            ip_address: "10.0.0.1".to_string(),
            country: "VN".to_string(),
            user_agent: None,
        },
        question: "Cho tôi xem thông tin".to_string(),
    };
    let first_ask = block_on(ask.execute(cmd.clone())).expect("ask 1 should succeed");
    assert!(!first_ask.is_cached);
    let second_ask = block_on(ask.execute(cmd.clone())).expect("ask 2 should succeed");
    assert!(second_ask.is_cached);

    // 3. Re-ingest shorter document (only 1 chunk: portfolio-1)
    let v2_md = "# Phần mới\nChỉ còn 1 đoạn duy nhất.";
    let res2 = block_on(ingest.execute(IngestMarkdownCommand {
        doc_id_prefix: "portfolio".to_string(),
        default_doc_title: None,
        category: "profile".to_string(),
        markdown_content: v2_md.to_string(),
        max_chunk_chars: 1200,
        overlap_chars: 150,
    }))
    .expect("second ingest should succeed");
    assert_eq!(res2.chunks_created, 1);

    // Stale chunks (portfolio-2, portfolio-3) must be deleted from both D1 and Vectorize
    assert_eq!(repo.chunks.borrow().len(), 1);
    assert_eq!(repo.chunks.borrow()[0].id, "portfolio-1");
    assert_eq!(
        *vector_index.deleted_ids.borrow(),
        vec!["portfolio-2".to_string(), "portfolio-3".to_string()]
    );

    // Cache version bumped to 3 -> previous cached answer is invalidated immediately
    assert_eq!(cache.version.get(), 3);
    let after_reingest = block_on(ask.execute(cmd)).expect("ask 3 should succeed");
    assert!(!after_reingest.is_cached);
}
