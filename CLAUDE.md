# Open Agent Broker (openab) — 揚洲幕府 Fork

我們的 fork，基於 https://github.com/openabdev/openab（原 thepagent/agent-broker）。

## 架構文件

所有架構、變更記錄、待辦追蹤在 Obsidian Vault：
- **架構總覽**：`/Users/ruandan/Documents/claude_code/Obsidian-Vault/05_Resources/揚洲幕府/Architecture.md`
- **需求追蹤**：`/Users/ruandan/Documents/claude_code/Obsidian-Vault/05_Resources/揚洲幕府/Backlog.md`
- **變更記錄**：`/Users/ruandan/Documents/claude_code/Obsidian-Vault/05_Resources/揚洲幕府/Changelog.md`

**啟動前務必先讀上述三份文件**，了解完整的架構、已知問題、和待辦事項。

## 專案結構

- `src/` — Rust 源碼（我們的修改都在這裡）
- `config.toml` — bare metal instance（幕府令 Bot 2，#sandbox-native）
- `config-docker.toml` — Docker instance（幕府行令 Bot 3，#sandbox-docker）
- `prompts/` — output gate pipeline 的 review prompt
- `src/gates/` — output gate pipeline（builtin + agent gates）

## Git Remotes

| Remote | URL | 用途 |
|--------|-----|------|
| `origin` | `ruan330/agent-broker` (private) | 我們的版本 |
| `upstream` | `openabdev/openab` (public) | 上游（已 rename） |
| `fork` | `ruan330/agent-broker-1` (public fork) | 提 PR 用 |

## 我們的修改（vs 上游）

| 檔案 | 修改 |
|------|------|
| `pool.rs` | `Arc<Mutex<AcpConnection>>` per-connection lock、per-thread CWD、SharedHandle、session status/kill |
| `connection.rs` | SharedHandle、PermissionRequest、PendingPermissions、interactive ExitPlanMode、auto-allow 從 options 選合法 optionId |
| `protocol.rs` | toolCallId extraction、跳過 sub-tool 事件（parentToolUseId） |
| `discord.rs` | `[cwd:]`/`[name:]` directives、toolCallId matching、streaming truncation、alive check + hard timeout、drain window + fallback、啟動自動封存舊 thread、gate pipeline loop、steer (try_lock + SharedHandle)、interactive plan mode (perm_rx) |
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

# Docker（幕府行令）
~/.orbstack/bin/docker restart agent-broker-docker

# 查狀態
curl -s http://localhost:8090/status   # 幕府令
curl -s http://localhost:8091/status   # 幕府行令
```

## 注意事項

- config.toml 和 config-docker.toml 含 Discord Bot token，**不可 open source**
- 上游已 rename 為 openabdev/openab，但我們的 remote 名稱還是 upstream
- 上游有 sender identity injection（feat/issue-61），下次 pull 需處理衝突
