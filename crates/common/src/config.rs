pub const DEFAULT_EMBEDDING_MODEL: &str = "@cf/baai/bge-m3";
pub const DEFAULT_REWRITE_MODEL: &str = "@cf/meta/llama-3.1-8b-instruct-fast";
pub const DEFAULT_ANSWER_MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
pub const DEFAULT_ANSWER_TEMPERATURE: f32 = 0.1;

pub const DEFAULT_RAG_TOP_K: usize = 5;
pub const DEFAULT_RAG_MIN_SCORE: f32 = 0.35;
pub const DEFAULT_RAG_FALLBACK_LIMIT: usize = 10;
pub const DEFAULT_RAG_RECENT_MESSAGES: usize = 4;
pub const DEFAULT_RAG_CACHE_TTL_SECONDS: u64 = 86_400;

pub const DEFAULT_RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
pub const DEFAULT_RATE_LIMIT_IP_MAX: u32 = 20;
pub const DEFAULT_RATE_LIMIT_SESSION_MAX: u32 = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct AiConfig {
    pub embedding_model: String,
    pub rewrite_model: String,
    pub answer_model: String,
    pub answer_temperature: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            embedding_model: DEFAULT_EMBEDDING_MODEL.to_string(),
            rewrite_model: DEFAULT_REWRITE_MODEL.to_string(),
            answer_model: DEFAULT_ANSWER_MODEL.to_string(),
            answer_temperature: DEFAULT_ANSWER_TEMPERATURE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RagConfig {
    pub top_k: usize,
    pub min_score: f32,
    pub fallback_limit: usize,
    pub recent_messages_limit: usize,
    pub cache_ttl_seconds: u64,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_RAG_TOP_K,
            min_score: DEFAULT_RAG_MIN_SCORE,
            fallback_limit: DEFAULT_RAG_FALLBACK_LIMIT,
            recent_messages_limit: DEFAULT_RAG_RECENT_MESSAGES,
            cache_ttl_seconds: DEFAULT_RAG_CACHE_TTL_SECONDS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    pub window_seconds: u64,
    pub max_requests_per_ip: u32,
    pub max_requests_per_session: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            window_seconds: DEFAULT_RATE_LIMIT_WINDOW_SECONDS,
            max_requests_per_ip: DEFAULT_RATE_LIMIT_IP_MAX,
            max_requests_per_session: DEFAULT_RATE_LIMIT_SESSION_MAX,
        }
    }
}
