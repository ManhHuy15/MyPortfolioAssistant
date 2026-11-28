# Tài liệu API — MyPortfolioAssistant

- **Swagger UI**: `GET /docs` (bấm **Authorize** và nhập `X-API-Key` để test trực tiếp trên trình duyệt)
- **OpenAPI 3.0.3 Spec**: `GET /api/openapi.json` (hoặc xem file [`docs/openapi.json`](./openapi.json) để import vào Postman / Insomnia)

---

## Chuẩn Envelope Response (`ApiResponse<T>`)

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

### Bảo mật & Rate Limit
- **Xác thực (2 mức quyền qua header `X-API-Key`)**:
  - `API_KEY` *(Chat-only)*: chỉ được gọi `/api/chat` và `/api/chat/stream`. Nếu gọi các endpoint `/api/chunks*` sẽ trả **HTTP 403** (`FORBIDDEN`).
  - `MASTER_API_KEY` *(Full quyền)*: truy cập toàn bộ endpoint (quản lý knowledge `/api/chunks*` và chat).
  - Sai hoặc thiếu key trả **HTTP 401** (`UNAUTHORIZED`).
- **Rate Limit (Sliding Window Counter)**: áp dụng trên `/api/chat` và `/api/chat/stream` theo cả **IP** (mặc định `20 req / 60s`) và **`sessionId`** (mặc định `10 req / 60s`). Vượt ngưỡng trả **HTTP 429** (`RATE_LIMITED`) kèm header `Retry-After`.

---

## Danh sách Endpoints

### 1. `GET /api/health` *(Công khai)*
Kiểm tra trạng thái Worker.

```bash
curl https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/health
```

### 2. `POST /api/chunks/markdown` *(Yêu cầu `MASTER_API_KEY`)*
Nạp tài liệu Markdown, tự động cắt chunk theo ngữ cảnh, tạo embedding và upsert vào cả D1 lẫn Vectorize. Hỗ trợ 3 định dạng:

- **Upload file `.md` (`multipart/form-data`)**:
  ```bash
  curl -X POST https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks/markdown \
    -H "X-API-Key: <MASTER_API_KEY>" \
    -F "category=profile" \
    -F "file=@./portfolio.md"
  ```
  *(Nếu không truyền `docIdPrefix`, BE tự lấy theo tên file `.md`; `defaultDocTitle` tự lấy theo heading `#` trong file.)*
- **Gửi JSON (`application/json`)**:
  ```bash
  curl -X POST https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks/markdown \
    -H "Content-Type: application/json" \
    -H "X-API-Key: <MASTER_API_KEY>" \
    -d '{
      "docIdPrefix": "profile",
      "category": "profile",
      "markdownContent": "# Giới thiệu\nNội dung portfolio..."
    }'
  ```
- **Gửi raw Markdown (`text/markdown` hoặc `text/plain`)**: truyền metadata qua query params `?docIdPrefix=profile&category=profile`.

### 3. `GET /api/chunks` *(Yêu cầu `MASTER_API_KEY`)*
Liệt kê toàn bộ danh sách knowledge chunks hiện đang lưu trong D1.

```bash
curl https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks \
  -H "X-API-Key: <MASTER_API_KEY>"
```

### 4. `DELETE /api/chunks/:target` *(Yêu cầu `MASTER_API_KEY`)*
Xóa đồng bộ knowledge ở cả **D1** và **Vectorize**. `:target` có thể là:
- `docIdPrefix` (ví dụ `portfolio`) → xóa toàn bộ chunk `portfolio-1`, `portfolio-2`, ...
- ID của 1 chunk lẻ (ví dụ `portfolio-2`) → chỉ xóa đúng chunk đó.

```bash
curl -X DELETE https://my-portfolio-assistant.nguyenmanhhuy-dev.workers.dev/api/chunks/portfolio \
  -H "X-API-Key: <MASTER_API_KEY>"
```

### 5. `POST /api/chat` *(Yêu cầu `API_KEY` hoặc `MASTER_API_KEY`)*
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

### 6. `POST /api/chat/stream` *(Yêu cầu `API_KEY` hoặc `MASTER_API_KEY`)*
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

---

## Gợi ý tích hợp FE (Next.js)
- Tự sinh `sessionId` bằng `crypto.randomUUID()` và lưu trong `sessionStorage`.
- Gọi Worker thông qua **Next.js Route Handler** ở server-side với biến môi trường `PORTFOLIO_API_KEY` (không dùng `NEXT_PUBLIC_`) để không lộ `API_KEY` ra trình duyệt.
