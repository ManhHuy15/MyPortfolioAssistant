# Rule: API Contract & Communication Standard (BE <-> FE)

Tài liệu này định nghĩa giao thức giao tiếp và định dạng dữ liệu chuẩn giữa Backend Rust và Frontend Next.js.

---

## 1. Chuẩn Envelope Response (`ApiResponse<T>`)

Mọi endpoint REST API trả về dữ liệu JSON bắt buộc phải tuân theo cấu trúc Envelope khớp với runtime type guard `parseEnvelope` của Frontend:

### Định nghĩa TypeScript (`FE/shared/types/api.ts`):
```typescript
export interface ApiError {
  code: string;
  message: string;
  field: string | null;
}

export interface ApiResponse<T> {
  success: boolean;
  message: string;
  data: T | null;
  errors: ApiError[] | null;
  traceId: string | null;
}
```

### Định nghĩa Rust (`BE/crates/common/src/response.rs`):
```rust
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorItem {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
    pub errors: Option<Vec<ApiErrorItem>>,
    pub trace_id: Option<String>,
}
```

> **LƯU Ý QUAN TRỌNG**: Không dùng `#[serde(skip_serializing_if = "Option::is_none")]` trên các trường `data`, `errors`, `trace_id` và `field`. Frontend (`apiClient.ts`) kiểm tra runtime nghiêm ngặt các trường này phải là giá trị cụ thể hoặc `null` (không được `undefined`). Trường `message` luôn phải là `string`.

---

## 2. Quy ước Đặt tên JSON (Field Casing)

- Toàn bộ JSON keys giao tiếp giữa BE và FE **BẮT BUỘC dùng `camelCase`**.
- Trong Rust: Luôn sử dụng attribute `#[serde(rename_all = "camelCase")]` trên các DTOs.

---

## 3. Quy chuẩn Mã lỗi (HTTP Status Codes & Error Codes)

| HTTP Status | Trường hợp sử dụng | Format Lỗi trong `errors` |
|---|---|---|
| **200 OK** | Request thành công. | `success: true, errors: null` |
| **201 Created** | Tạo mới tài nguyên thành công. | `success: true, errors: null` |
| **400 Bad Request** | Lỗi validation tham số đầu vào. | `errors: [{ code: "VALIDATION_FAILED", message: "...", field: "email" }]` |
| **401 Unauthorized** | Token không hợp lệ hoặc hết hạn. | `errors: [{ code: "UNAUTHORIZED", message: "...", field: null }]` |
| **403 Forbidden** | Không có quyền truy cập tài nguyên. | `errors: [{ code: "FORBIDDEN", message: "...", field: null }]` |
| **404 Not Found** | Không tìm thấy tài nguyên. | `errors: [{ code: "NOT_FOUND", message: "...", field: null }]` |
| **409 Conflict** | Xung đột dữ liệu (VD: trùng mã/email). | `errors: [{ code: "CONFLICT", message: "...", field: "code" }]` |
| **500 Internal** | Lỗi hệ thống server không mong muốn. | `errors: [{ code: "INTERNAL_ERROR", message: "...", field: null }]` |

---

## 4. Cơ chế Xác thực & Proxy (Authentication & Routing)

1. **Access Token**: Gửi qua header `Authorization: Bearer <token>`.
2. **Refresh Token**: Lưu trữ trong Cookie `HttpOnly; Secure; SameSite=Strict`.
3. **Next.js Proxy**: Frontend cấu hình rewrite `/api/:path*` sang `${API_BASE_URL}/api/:path*` (mặc định `http://localhost:8080/api/:path*`). Mọi API route của Backend phục vụ Frontend phải đặt dưới prefix `/api/...` (ví dụ `/api/health`, `/api/auth/...`).
