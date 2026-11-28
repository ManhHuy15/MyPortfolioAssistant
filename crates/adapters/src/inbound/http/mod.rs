pub mod auth;
pub mod chat_handler;
pub mod chat_stream_handler;
pub mod health_handler;
pub mod knowledge_handler;
pub mod markdown_handler;
pub mod response;
pub mod swagger_handler;

pub use auth::{
    ApiKeyRole, RequiredPermission, verify_api_key, verify_chat_api_key, verify_master_api_key,
};
pub use chat_handler::handle_chat;
pub use chat_stream_handler::handle_chat_stream;
pub use health_handler::handle_health;
pub use knowledge_handler::{handle_delete_knowledge, handle_list_chunks};
pub use markdown_handler::handle_ingest_markdown;
pub use response::json_response;
pub use swagger_handler::{handle_openapi_spec, handle_swagger_ui};
