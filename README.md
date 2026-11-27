# MyPortfolioAssistant

Trợ lý AI RAG (Retrieval-Augmented Generation) cho trang Portfolio cá nhân, viết bằng **Rust** (WebAssembly) và chạy serverless hoàn toàn trên hệ sinh thái **Cloudflare Workers**.

---

## Tính năng chính

- **Full RAG Pipeline trên Cloudflare**:
  - **Context-Aware Markdown Chunking**: đọc file `.md`, theo dõi full heading breadcrumb (`H1 > H2 > H3 > H4`), bảo vệ code block (` ``` ` / `~~~`), cắt theo block/câu tự nhiên kèm overlap.
  - **Contextual Header Injection**: gắn ngữ cảnh `[Ngữ cảnh: <breadcrumb> | Loại: <category>]` trước khi tạo vector embedding.
  - **Embedding & Vector Search**: tạo vector 1024 chiều bằng `@cf/baai/bge-m3` (batch 50) và lưu/truy vấn cosine similarity trên **Cloudflare Vectorize**.
  - **Follow-up Question Rewrite**: tự động viết lại câu hỏi nối tiếp dựa trên lịch sử tin nhắn gần nhất bằng `@cf/meta/llama-3.1-8b-instruct-fast`.
  - **Answer Generation + SSE Streaming**: sinh câu trả lời bám sát ngữ cảnh bằng `@cf/meta/llama-3.3-70b-instruct-fp8-fast`, hỗ trợ cả JSON 1 lần (`/api/chat`) và streaming từng chữ SSE (`/api/chat/stream`).
  - **Workers KV Caching**: cache câu trả lời theo câu hỏi đã chuẩn hóa (mặc định 24h).
  - **D1 Persistence**: lưu trữ knowledge chunks, phiên chat (`chat_sessions` kèm IP/Country/User-Agent) và lịch sử tin nhắn (`chat_messages`).
- **Bảo mật API Key**: xác thực bằng header `X-API-Key` với so sánh `constant_time_eq`.
- **CI/CD tự động**: GitHub Actions chạy `fmt`, `test`, `clippy`, kiểm tra `wasm32-unknown-unknown` và tự động deploy lên Cloudflare Workers khi push vào `main`.

---

## Kiến trúc hệ thống (Hexagonal Architecture)

Dự án tổ chức theo Cargo workspace gồm 5 crates tách biệt rõ ràng:

```text
MyPortfolioAssistant/
├── .github/workflows/deploy.yml   # CI/CD GitHub Actions -> Cloudflare Workers
├── db/schema.sql                  # Schema D1 (chunks, chat_sessions, chat_messages)
├── wrangler.toml                  # Cấu hình Worker, Bindings & biến môi trường
└── crates/
    ├── common/                    # ApiResponse<T>, AiConfig, RagConfig
    ├── domain/                    # Entities, DomainError, Inbound/Outbound Ports (Pure Rust)
    ├── application/               # DTOs, Use Cases (AskPortfolio, IngestMarkdown, MarkdownChunker)
    ├── adapters/                  # HTTP Handlers (SSE, Auth, Multipart), D1, KV, Vectorize FFI, Workers AI
    └── server/                    # Composition Root (#[event(fetch)]), DI wiring & Router
```

---

## Yêu cầu môi trường

- **Rust** (stable) + target `wasm32-unknown-unknown`:
  ```bash
  rustup target add wasm32-unknown-unknown
  cargo install worker-build --locked
  ```
- **Node.js** `>= 20` và npm (`npm ci` để cài Wrangler local).
- Tài khoản **Cloudflare** đã tạo sẵn:
  - D1 Database (`DB`)
  - KV Namespace (`CACHE_KV`)
  - Vectorize Index (`VECTORIZE` — 1024 dimensions, `cosine` metric)
  - Workers AI (`AI`)

---

## Cấu hình biến môi trường & Secrets

### 1. Biến môi trường trong `wrangler.toml` (`[vars]`)

| Biến | Mặc định | Ý nghĩa |
|---|---|---|
| `AI_EMBEDDING_MODEL` | `@cf/baai/bge-m3` | Model tạo vector embedding (1024 chiều) |
| `AI_REWRITE_MODEL` | `@cf/meta/llama-3.1-8b-instruct-fast` | Model viết lại câu hỏi nối tiếp |
| `AI_ANSWER_MODEL` | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` | Model sinh câu trả lời chính (hỗ trợ streaming) |
| `AI_ANSWER_TEMPERATURE` | `0.1` | Mức độ sáng tạo của LLM (`0.0` – `1.0`) |
| `RAG_TOP_K` | `5` | Số lượng vector gần nhất lấy từ Vectorize |
| `RAG_MIN_SCORE` | `0.35` | Ngưỡng cosine similarity tối thiểu |
| `RAG_FALLBACK_LIMIT` | `10` | Số chunk D1 fallback khi không có vector đạt ngưỡng |
| `RAG_RECENT_MESSAGES` | `4` | Số tin nhắn gần nhất dùng để rewrite câu hỏi |
| `RAG_CACHE_TTL_SECONDS` | `86400` | Thời gian cache câu trả lời trên KV (giây) |
| `RATE_LIMIT_WINDOW_SECONDS` | `60` | Cửa sổ thời gian tính rate limit (giây) |
| `RATE_LIMIT_IP_MAX` | `20` | Số request tối đa mỗi IP trong 1 cửa sổ (`0` để tắt) |
| `RATE_LIMIT_SESSION_MAX` | `10` | Số request tối đa mỗi `sessionId` trong 1 cửa sổ (`0` để tắt) |

### 2. Secret `API_KEY`

Tạo 1 API key ngẫu nhiên mạnh:
```bash
openssl rand -hex 32
```

- **Local dev**: copy `.dev.vars.example` thành `.dev.vars` (đã nằm trong `.gitignore`) và điền `API_KEY=...`.
- **Cloudflare Production**:
  ```bash
  npx wrangler secret put API_KEY
  ```

---

## Tài liệu API

- **Swagger UI**: `GET /docs` (bấm **Authorize** và nhập `X-API-Key` để test trực tiếp trên trình duyệt)
- **OpenAPI 3.0.3 Spec**: `GET /api/openapi.json` (có thể import thẳng vào Postman / Insomnia)

Mọi response JSON đều tuân theo chuẩn `ApiResponse<T>` (`camelCase`, các trường `null` luôn được serialize rõ ràng):

```json
{
  "success": true,
  "message": "OK",
  "data": {},
  "errors": null,
  "traceId": "cf-ray-id"
}
```

### 1. `GET /api/health` *(Công khai)*
Kiểm tra trạng thái Worker.

```bash
curl https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/health
```

### 2. `POST /api/chunks/markdown` *(Yêu cầu `X-API-Key`)*
Nạp tài liệu Markdown, tự động cắt chunk theo ngữ cảnh, tạo embedding và upsert vào cả D1 lẫn Vectorize. Hỗ trợ 3 định dạng:

- **Upload file `.md` (`multipart/form-data`)**:
  ```bash
  curl -X POST https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks/markdown \
    -H "X-API-Key: <API_KEY>" \
    -F "docIdPrefix=portfolio" \
    -F "category=profile" \
    -F "file=@./portfolio.md"
  ```
- **Gửi JSON (`application/json`)**:
  ```bash
  curl -X POST https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks/markdown \
    -H "Content-Type: application/json" \
    -H "X-API-Key: <API_KEY>" \
    -d '{
      "docIdPrefix": "profile",
      "category": "profile",
      "markdownContent": "# Giới thiệu\nNội dung portfolio..."
    }'
  ```
- **Gửi raw Markdown (`text/markdown` hoặc `text/plain`)**: truyền metadata qua query params `?docIdPrefix=profile&category=profile`.

### 3. `GET /api/chunks` *(Yêu cầu `X-API-Key`)*
Liệt kê toàn bộ danh sách knowledge chunks hiện đang lưu trong D1.

```bash
curl https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks \
  -H "X-API-Key: <API_KEY>"
```

### 4. `DELETE /api/chunks/:target` *(Yêu cầu `X-API-Key`)*
Xóa đồng bộ knowledge ở cả **D1** và **Vectorize**. `:target` có thể là:
- `docIdPrefix` (ví dụ `portfolio`) → xóa toàn bộ chunk `portfolio-1`, `portfolio-2`, ...
- ID của 1 chunk lẻ (ví dụ `portfolio-2`) → chỉ xóa đúng chunk đó.

```bash
curl -X DELETE https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks/portfolio \
  -H "X-API-Key: <API_KEY>"
```

### 5. `POST /api/chat` *(Yêu cầu `X-API-Key`)*
Hỏi đáp RAG trả về 1 response JSON hoàn chỉnh.

```bash
curl -X POST https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chat \
  -H "Content-Type: application/json" \
  -H "X-API-Key: <API_KEY>" \
  -d '{
    "sessionId": "550e8400-e29b-41d4-a716-446655440000",
    "message": "Giới thiệu về các dự án nổi bật của Huy"
  }'
```

### 6. `POST /api/chat/stream` *(Yêu cầu `X-API-Key`)*
Hỏi đáp RAG trả về từng chữ qua **Server-Sent Events (`text/event-stream`)**:

```bash
curl -N -X POST https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chat/stream \
  -H "Content-Type: application/json" \
  -H "X-API-Key: <API_KEY>" \
  -d '{
    "sessionId": "550e8400-e29b-41d4-a716-446655440000",
    "message": "Giới thiệu về các dự án nổi bật của Huy"
  }'
```

Các event trả về trong stream:
- `event: token` → `data: {"token":"..."}` *(đẩy liên tục từng token khi LLM đang sinh)*
- `event: done` → `data: <ApiResponse<ChatResponseDto>>` *(gửi toàn bộ câu trả lời hoàn chỉnh + `sources` + `isCached` sau khi lưu D1 & KV)*
- `event: error` → `data: <ApiResponse<null>>` *(nếu có lỗi trong quá trình stream)*

> **Gợi ý tích hợp FE (Next.js)**:
> - Tự sinh `sessionId` bằng `crypto.randomUUID()` và lưu trong `sessionStorage`.
> - Gọi Worker thông qua **Next.js Route Handler** ở server-side với biến môi trường `PORTFOLIO_API_KEY` (không dùng `NEXT_PUBLIC_`) để không lộ `API_KEY` ra trình duyệt.

---

## Phát triển Local, Test & Deploy

### Cài đặt & chạy local
```bash
npm ci
npm run db:local          # Tạo bảng trên D1 local
npm run dev               # Chạy local dev (hoặc `npx wrangler dev --remote` để dùng Vectorize/AI thật)
```

### Kiểm tra chất lượng code Rust
```bash
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --target wasm32-unknown-unknown --workspace
```

### Migrate D1 remote & Deploy thủ công
```bash
npm run db:remote         # Chạy khi có thay đổi db/schema.sql
npm run deploy            # Build Wasm và deploy lên Cloudflare Workers
```

### Tự động Deploy qua GitHub Actions
Cấu hình 2 secret trong **GitHub Repo → Settings → Secrets and variables → Actions**:
- `CLOUDFLARE_API_TOKEN` *(tạo từ Cloudflare Worker → Settings → Builds → API token for external CI/CD)*
- `CLOUDFLARE_ACCOUNT_ID`

Mỗi khi push lên nhánh `main`, workflow `.github/workflows/deploy.yml` sẽ tự động chạy toàn bộ CI check và deploy lên Cloudflare Workers.
