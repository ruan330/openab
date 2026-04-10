# 部署

> 本頁涵蓋 bare metal / Docker 部署、OAuth 設定、management API。
> 關聯程式碼：`config.toml`, `config-docker.toml`, `Dockerfile.claude`, `src/main.rs`

## 核心知識

### 兩個 Instance

| Instance | 名稱 | 環境 | Port | Channel |
|----------|------|------|------|---------|
| Bare metal | 幕府令 (Bot 2) | macOS 原生 | 8090 | #sandbox-native |
| Docker | 幕府行令 (Bot 3) | Linux/OrbStack | 8091 | #sandbox-docker |

### 啟動指令

```bash
# Bare metal（幕府令）
RUST_LOG=openab=debug nohup ./target/release/openab config.toml > /tmp/openab.log 2>&1 &

# Docker（幕府行令）— 只改 config 可以 restart；改 code 要 rebuild
~/.orbstack/bin/docker restart agent-broker-docker

# 查狀態
curl -s http://localhost:8090/status   # 幕府令
curl -s http://localhost:8091/status   # 幕府行令
```

### Rolling Redeploy（code 改動後）

**幕府令（bare metal）：**
```bash
cargo build --release
kill -TERM $(pgrep -f 'target/release/openab config.toml')  # SIGTERM → broadcast + graceful shutdown
RUST_LOG=openab=debug nohup ./target/release/openab config.toml > /tmp/openab.log 2>&1 &
```

**幕府行令（Docker）— 需要 rebuild image：**
```bash
# 1. 抓出 OAuth token（container 被 rm 後就沒了）
TOKEN=$(~/.orbstack/bin/docker inspect agent-broker-docker \
  --format '{{range .Config.Env}}{{println .}}{{end}}' \
  | awk -F= '/^CLAUDE_CODE_OAUTH_TOKEN=/{sub(/^[^=]*=/,""); print}')

# 2. Rebuild image（multi-stage，Rust build + node runtime）
~/.orbstack/bin/docker build -f Dockerfile.claude -t agent-broker-claude:latest .

# 3. Graceful stop（SIGTERM → broadcast）→ rm → run
~/.orbstack/bin/docker stop -t 10 agent-broker-docker
~/.orbstack/bin/docker rm agent-broker-docker
~/.orbstack/bin/docker run -d \
  --name agent-broker-docker --restart unless-stopped \
  -p 8091:8090 \
  -v /Users/ruandan/.claude:/home/agent/.claude \
  -v /Users/ruandan/Documents/claude_code:/workspace \
  -v /Users/ruandan/Documents/claude_code/agent-broker/config-docker.toml:/etc/agent-broker/config.toml \
  -v /Users/ruandan/Documents/claude_code/agent-broker/prompts:/etc/agent-broker/prompts \
  -e CLAUDE_CODE_OAUTH_TOKEN="$TOKEN" \
  -e RUST_LOG=openab=debug \
  agent-broker-claude:latest
```

**⚠️ Docker run 不要加 `openab ...` 參數**：`Dockerfile.claude` 已有 `ENTRYPOINT ["openab"]` + `CMD ["/etc/agent-broker/config.toml"]`，多打 `openab /etc/...` 會被當成 config path（`openab openab /etc/...`），container 會 crash loop 報 "failed to read openab"。

### Management API

```bash
curl -s http://localhost:{port}/status          # 查看所有 session 狀態
curl -s http://localhost:{port}/kill/<thread_id> # 終止特定 session
```

### OAuth 認證

**Docker**：
- `claude setup-token` 生成 1 年 token
- Docker run 用 `-e CLAUDE_CODE_OAUTH_TOKEN=...` 傳入
- Token 過期時使用 HTTP refresh（寫 file）

**macOS (bare metal)**：
- CC CLI 用 Keychain 存 refreshed token
- `refresh_oauth_if_needed` **不寫 `.credentials.json`** — 避免破壞 CC CLI auth
- 讀取順序：`.credentials.json` → macOS Keychain → HTTP refresh

### Config 注意

- `config.toml` 和 `config-docker.toml` 含 Discord Bot token
- 已 gitignore，**不可推到 public fork**
- Docker mounts：`config-docker.toml` (ro), `/workspace`, `~/.claude`

### 專案模型設定

每個專案的 `.claude/settings.json` 控制模型：

| 專案 | Model | Effort |
|------|-------|--------|
| MeeePtt | claude-opus-4-6 | high |
| Obsidian-Vault | claude-opus-4-6 | high |
| sijin-finance | claude-sonnet-4-6 | medium |
| taiyi-health | claude-sonnet-4-6 | medium |
| 全局預設 | claude-sonnet-4-6 | medium |

全局 `~/.claude/settings.json` 設有 `bypassPermissions`，所有家臣自動跳過權限確認。

## Bug 經驗庫

| 問題 | 原因 | 解法 |
|------|------|------|
| Docker token 4 小時過期 | Docker 掛載 macOS `~/.claude`，refresh token 被 macOS CC CLI rotate → `invalid_grant` | `claude setup-token` 生成 1 年 token + `CLAUDE_CODE_OAUTH_TOKEN` env var |
| Broker 寫 `.credentials.json` 破壞 CC CLI auth | CC CLI reload 時讀到 broker 寫的過期 token | macOS 上 `refresh_oauth_if_needed` 不寫 file |
| 529 error 顯示 no response 但工作實際完成 | CC CLI 收到 529 後立刻回報 error，但內部 retry 成功 | 不是 broker bug，是 adapter error reporting 與執行狀態不一致 |
| Docker rebuild 後 container crash loop（"failed to read openab"） | `docker run` 時多打 `openab /etc/agent-broker/config.toml`，跟 `ENTRYPOINT ["openab"]` 疊加成 `openab openab /etc/...` | 讓 Dockerfile 的 ENTRYPOINT + CMD 自己接手，`docker run` 不要加任何 args |
| Container rm 後 OAuth token 遺失無法重建 | Token 只存在 container env，`docker rm` 後沒地方抓 | Rebuild 前先 `docker inspect` 撈出來暫存，run 完再清掉 |

## 待釐清

- 無
