# Rule: Backend Rust - Hexagonal Architecture (Cloudflare Workers)

Tài liệu này áp dụng cho toàn bộ mã nguồn Rust trong workspace.

---

## 1. Nguyên tắc Ranh giới Kiến trúc (Architectural Boundaries)

Hệ thống sử dụng **Cargo Workspace** gồm 5 crate đặt tại `crates/*` với ranh giới phụ thuộc nghiêm ngặt:

```text
server (Composition Root - Cloudflare Worker Entrypoint)
  └── adapters (Driving & Driven Adapters)
        ├── application (Use Cases & DTOs)
        │     └── domain (Core Business Entities & Ports Traits)
        └── common (Shared Utils: Config, ApiResponse)
```

### Quy tắc Vàng (The Golden Rules):
1. **`crates/domain` tuyệt đối không phụ thuộc vào `adapters` hoặc `server`**:
   - `domain` chỉ chứa Pure Rust structs (Entities, Value Objects), Domain Errors, và định nghĩa `trait` Ports (`#[async_trait(?Send)]`).
   - CẤM import crate `worker`, `wasm-bindgen`, framework HTTP hay database vào `domain`.
2. **Inbound vs Outbound Ports**:
   - **Inbound Ports (Driving Ports)**: Đặt tại `crates/domain/src/ports/inbound/` (Ví dụ: `trait AskPortfolioUseCasePort`).
   - **Outbound Ports (Driven Ports)**: Đặt tại `crates/domain/src/ports/outbound/` (Ví dụ: `trait ChatRepositoryPort`, `trait AnswerCachePort`, `trait AiCompletionPort`).
3. **Use Cases & Smart Pointers trên Wasm (`Rc<dyn SomePort>`)**:
   - Đặt tại `crates/application/src/use_cases/`.
   - Do Cloudflare Workers (`wasm32-unknown-unknown`) chạy trên V8 Isolate single-threaded và các bindings (`D1Database`, `KvStore`, `Ai`) không implement `Send + Sync`, các trait Ports sử dụng `#[async_trait(?Send)]` và Use Cases nhận dependency qua `Rc<dyn SomePort>` (không dùng `Arc` để tránh overhead atomic không cần thiết và lỗi `clippy::arc_with_non_send_sync`).
4. **Adapters**:
   - **Driving Adapter**: `crates/adapters/src/inbound/http/` (Cloudflare Worker HTTP handlers, routing helpers).
   - **Driven Adapter**: `crates/adapters/src/outbound/persistence/` (Cloudflare D1, Workers KV, Workers AI).
5. **Composition Root**:
   - Duy nhất `crates/server/src/lib.rs` (`#[event(fetch)]`) là nơi lấy bindings từ `Env`, đọc cấu hình, thực hiện Dependency Injection và định tuyến request.
6. **Quy tắc Tổ chức Module (`mod.rs`)**:
   - Không viết gộp nhiều struct/logic của các ngữ cảnh khác nhau vào chung một file `mod.rs`.
   - Tách mỗi ngữ cảnh/thành phần ra một file `.rs` riêng biệt (ví dụ: `d1_chat_repository.rs`, `kv_answer_cache.rs`, `vectorize_index.rs`, `workers_ai.rs`). File `mod.rs` chỉ dùng để khai báo `pub mod ...` và `pub use ...` ra ngoài.

---

## 2. Quy tắc Xử lý Lỗi & Response (Error Handling)

1. **Domain & Application**:
   - Sử dụng `thiserror` để định nghĩa enum lỗi có kiểu tĩnh (`DomainError`, `AppError`).
2. **API Handlers**:
   - Map lỗi sang HTTP Status Code và bọc trong struct `ApiResponse<T>` chuẩn (đặt tại `common::response::ApiResponse`).
   - Mọi response JSON trả về cho FE bắt buộc dùng format **`camelCase`** (`#[serde(rename_all = "camelCase")]`).

---

## 3. Quy tắc Kiểm thử (Testing Standard)

1. **Unit Tests**:
   - Tách ra file test riêng biệt trong thư mục `tests/` của từng crate (ví dụ: `crates/application/tests/`, `crates/common/tests/`), không viết chung vào cuối file mã nguồn chính.
   - Unit test Use Case bằng cách mock các Outbound Port traits, không phụ thuộc Cloudflare D1/KV/AI thật.

---

## 4. Lệnh Kiểm tra Bắt buộc (Verification Commands)

Trước khi kết thúc bất kỳ chỉnh sửa nào, AI Agent **PHẢI** chạy thành công lệnh sau tại thư mục gốc của dự án:

```bash
cargo fmt --check && cargo check --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
```
