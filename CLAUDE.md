# Open Agent Broker (openab) — 揚洲幕府 Fork

我們的 fork，基於 https://github.com/openabdev/openab（原 thepagent/agent-broker）。

## 文件系統

- 領域知識見 `docs/wiki/index.md`（按需查閱，不要一次全讀）
- 否決方案與決策記錄見 `docs/decisions.md`
- 實作新功能前，先查 wiki 對應領域頁
- 文件與 code 同一個 commit 提交

## 文件同步規則

| 觸發事件 | 更新目標 |
|----------|---------|
| 實作了新功能 | 對應 wiki 領域頁 |
| 修了 Bug | wiki 領域頁（Bug 經驗庫段落） |
| 發現 edge case | wiki 領域頁 + code 中加 WHY NOT 註釋 |
| 做了決策或否決方案 | `decisions.md` |
| Public fork sync（squash merge 到 fork-main） | `wiki/log.md`（記 squash commit + 主要內容類別） |
| 以上任何更新 | `wiki/log.md`（append 一條） |

## 專案結構

- `src/` — Rust 源碼（我們的修改都在這裡）
- `config.toml` — bare metal instance（幕府令 Bot 2，#sandbox-native）
- `config-docker.toml` — Docker instance（幕府行令 Bot 3，#sandbox-docker）
- `prompts/` — output gate pipeline 的 review prompt
- `src/gates/` — output gate pipeline（builtin + agent gates）

## Git Remotes

| Remote | URL | 用途 |
|--------|-----|------|
| `origin` | `ruan330/agent-broker` (private) | 日常開發、部署 |
| `upstream` | `openabdev/openab` (public) | 上游更新 |
| `fork` | `ruan330/openab` (public) | 公開 fork、提 PR 用 |

### 工作流
- Private `origin`：自由 commit，含部署設定
- Public `fork`：checkout `fork-main` branch → squash merge main → push。零 secrets
- 提 PR：從 `fork` 的 feature branch 對 upstream 開

## 我們的修改（vs 上游）

| 檔案 | 修改 |
|------|------|
| `pool.rs` | `Arc<Mutex<AcpConnection>>` per-connection lock、per-thread CWD、SharedHandle、session status/kill |
| `connection.rs` | SharedHandle、PermissionRequest、PendingPermissions、ExitPlanMode auto-approve、auto-allow 從 options 選合法 optionId、OAuth token refresh（Docker: HTTP refresh，macOS: 不寫 file 避免破壞 CC CLI）|
| `protocol.rs` | toolCallId extraction、跳過 sub-tool 事件（parentToolUseId） |
| `discord.rs` | `[cwd:]`/`[name:]` directives、toolCallId matching、streaming truncation、alive check + hard timeout、drain window + fallback、啟動自動封存舊 thread、gate pipeline loop、steer (try_lock + SharedHandle)、plan mode（CC App 模型：auto-approve + plan review loop）、401 auth retry、spawn_edit_task |
| `config.rs` | `allowed_bots`、`GatesConfig` |
| `main.rs` | HTTP management API (:8090)、gate pipeline init、allowed_bots、PendingPermissions |
| `src/gates/` | 整個 output gate pipeline module（builtin + agent） |

## 上游貢獻

| 類型 | 編號 | 主題 |
|------|------|------|
| PR | #53 | toolCallId + streaming fix |
| PR | #77 | per-connection lock + alive check + drain + startup cleanup |
| Issue | #39 | management API |
| Issue | #49 | output gate pipeline |
| Issue | #58 | pool write lock deadlock |
| Issue | #76 | notification loop 三假設不成立 |
| Issue | #111 | ExitPlanMode permission fix |
| RFC | #78 | Session Management（已回覆 production 經驗）|

## 運行方式

```bash
# Bare metal（幕府令）
RUST_LOG=openab=debug nohup ./target/release/openab config.toml > /tmp/openab.log 2>&1 &

# Docker（幕府行令）— 需要 CLAUDE_CODE_OAUTH_TOKEN（由 claude setup-token 生成，1 年有效）
~/.orbstack/bin/docker restart agent-broker-docker

# 查狀態
curl -s http://localhost:8090/status   # 幕府令
curl -s http://localhost:8091/status   # 幕府行令
```

## 上游 PR / Issue 撰寫規則

撰寫對上游的 PR 或 Issue 前，**必須先讀 `docs/wiki/upstream.md` 的「PR / Issue 撰寫準則」段落**。

核心要求：
- 說清楚根因，不只描述症狀
- 解釋為什麼這是最好的解法，列出替代方案和 tradeoff
- 附 production 證據（log、數據、重現步驟）
- 提交前跑一遍自檢流程（5 點 checklist 在 wiki 裡）

## 注意事項

- config.toml 和 config-docker.toml 含 Discord Bot token，已 gitignore，**不可推到 public fork**
- Binary 已 rename：`target/release/openab`（不再是 `agent-broker`）
- 上游 sender identity injection 已整合
- Public fork 的 CI workflows（Build & Release、Release Charts）已禁用
