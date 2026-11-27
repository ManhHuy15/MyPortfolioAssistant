PRAGMA foreign_keys = ON;

-- 1. Bảng lưu dữ liệu kiến thức Portfolio (Source of Truth cho RAG)
CREATE TABLE IF NOT EXISTS chunks (
  id TEXT PRIMARY KEY,
  doc_title TEXT NOT NULL,
  category TEXT NOT NULL DEFAULT 'general', -- vd: 'profile', 'experience', 'project', 'skill'
  content TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 2. Bảng quản lý phiên chat & thông tin định danh/địa lý (IP, Quốc gia)
CREATE TABLE IF NOT EXISTS chat_sessions (
  session_id TEXT PRIMARY KEY,
  ip_address TEXT NOT NULL,                 -- IP từ CF-Connecting-IP (hoặc IP đã hash)
  country TEXT NOT NULL DEFAULT 'XX',       -- Mã quốc gia ISO 2 ký tự từ req.cf().country() (vd: 'VN', 'US')
  user_agent TEXT,                          -- Tùy chọn: nhận diện trình duyệt/thiết bị
  message_count INTEGER NOT NULL DEFAULT 0, -- Đếm nhanh số lượt chat trong session
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_active_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index phục vụ thống kê lượt truy cập theo quốc gia và kiểm tra IP
CREATE INDEX IF NOT EXISTS idx_sessions_country ON chat_sessions (country, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_ip ON chat_sessions (ip_address, last_active_at DESC);

-- 3. Bảng lưu chi tiết lịch sử tin nhắn trong từng session
CREATE TABLE IF NOT EXISTS chat_messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
  content TEXT NOT NULL,
  sources TEXT,                             -- Chuỗi JSON mảng tên tài liệu trích dẫn, vd: '["Dự án YuhTube"]'
  is_cached INTEGER NOT NULL DEFAULT 0,     -- 0: gọi LLM mới, 1: trả từ KV Cache
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (session_id) REFERENCES chat_sessions(session_id) ON DELETE CASCADE
);

-- Index tối ưu truy vấn lấy N tin nhắn gần nhất của 1 session
CREATE INDEX IF NOT EXISTS idx_messages_session_id ON chat_messages (session_id, id DESC);