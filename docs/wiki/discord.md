# Discord 整合

> 本頁涵蓋 Discord 訊息處理、streaming、directives、threading。
> 關聯程式碼：`src/discord.rs`

## 核心知識

### Directives（訊息指令）

| Directive | 用途 | 範例 |
|-----------|------|------|
| `[cwd:路徑]` | 設定 session 工作目錄 | `[cwd:/Users/ruandan/Documents/claude_code/MeeePtt]` |
| `[name:名稱]` | 設定 thread 名稱 | `[name:MeeePtt 登入功能]` |

- Directives 在 parse 後從訊息文字中移除，不傳給 Claude
- 只在建 session 時有效（第一條訊息）

### Streaming 訊息處理

- 長訊息截斷至 1900 字元（Discord 限制 2000）
- 使用 `floor_char_boundary` 做 UTF-8 safe truncation
- 單訊息截斷防止 overflow 重複

### Steer（中途插話）

- Session 忙碌時 `try_lock()` 偵測 → 用 SharedHandle 直接 queue prompt
- 顯示「📨 Queued」回饋
- 不 block 其他操作

### Thread 管理

- 每個 session 對應一個 Discord thread
- **Thread continuity across restarts**：舊 thread 收到訊息會重建 session（`session_reset=true` → 顯示「Session expired, starting fresh...」），而不是被封存
- cwd mapping（thread_id → cwd）持久化在 `state_file`（config 的 `[pool].state_file`），重啟後 load 回來，舊 thread 不需重新 `[cwd:]`
- 寫入採 tmp + rename 原子操作

### Graceful shutdown

- SIGINT / SIGTERM 都會觸發 shutdown hook（`main.rs`）
- Shutdown 時迭代 `pool.active_thread_ids()`，對每個 thread post「🔄 Broker restarting... You can continue the conversation when the broker is back.」
- 通知發完才 `shard_manager.shutdown_all()` + `pool.shutdown()`
- 對應 RFC #78 Phase 1d — 已開 [PR #182](https://github.com/openabdev/openab/pull/182)

### 401 Auth Retry

- 偵測 auth error → refresh token → kill session → retry once
- 用戶只看到「🔄 Reconnecting...」

## 實作筆記

- `allowed_bots` config 控制哪些 bot 可以互相觸發（防止無限迴圈）
- gate pipeline loop 在回覆發送前執行 gate 檢查

## Bug 經驗庫

| 問題 | 原因 | 解法 |
|------|------|------|
| Tool 狀態顯示空白/錯誤 | title matching 在 start/done 間不一致 | toolCallId matching（PR #53） |
| 長回覆重複顯示 | streaming 沒有截斷 | 單訊息截斷至 1900 字元 |
| 多字節字元 panic | `&content[..1900]` 在 UTF-8 多字節邊界切斷 | `floor_char_boundary` |
| 子工具顯示殘留吃字 | 子工具沒有 ToolStart 只有 ToolDone，title 為空 | 跳過 parentToolUseId 不為空的事件 |
| 重啟後舊 thread 接訊息用錯 cwd | thread_cwds HashMap 只存記憶體，重啟後落到 `config.working_dir` | `PoolConfig.state_file` 持久化 thread_cwds JSON，load on startup |

## 待釐清

- Tool display 殘留吃字：子工具沒 ToolStart 只有 ToolDone（title 為空）— 目前已跳過 sub-tool，但根本原因在上游 ACP 事件設計
