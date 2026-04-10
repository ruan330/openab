# 系統架構

> 本頁涵蓋三 Bot 混合架構、系統拓撲、派工流程。
> 關聯程式碼：整體架構設計

## 核心知識

### 系統拓撲

```
                    ┌─────────────────────┐
                    │   主公大人 (揚洲)    │
                    │   人在國外           │
                    └──────────┬──────────┘
                               │ Discord
                               v
┌──────────────────────────────────────────────────────────┐
│                      Mac Mini (總機)                      │
│           Apple Silicon, 16 GB, macOS 26.3.1             │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │  OpenClaw    │  │  agent-broker│  │  agent-broker  │  │
│  │  Gateway     │  │  (bare metal)│  │  (Docker)      │  │
│  │              │  │              │  │                │  │
│  │  阮蛋       │  │  幕府令      │  │  幕府行令      │  │
│  │  (Bot 1)    │  │  (Bot 2)     │  │  (Bot 3)       │  │
│  │  gpt-5.4    │  │  macOS 原生  │  │  Linux 沙盒    │  │
│  │  規劃/審查  │  │  Flutter/iOS │  │  一般專案      │  │
│  └──────────────┘  └──────────────┘  └────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

### 三 Bot 分工

| Bot | 名稱 | 平台 | 頻道 | 適用 |
|-----|------|------|------|------|
| Bot 1 | 阮蛋 | OpenClaw (gpt-5.4) | 主頻道 | 規劃、分析、審核、對話 |
| Bot 2 | 幕府令 | agent-broker (bare metal) | #sandbox-native | 需要 macOS 原生工具（Flutter、iOS） |
| Bot 3 | 幕府行令 | agent-broker (Docker/OrbStack) | #sandbox-docker | 一般專案（Python、Markdown、沙盒隔離） |

### 派工流程

```
主公 → @阮蛋：派家臣去做 MeeePTT 登入功能
阮蛋 → 「主公，請到 #sandbox-native 貼上這段：
        @幕府令 [cwd:/Users/ruandan/Documents/claude_code/MeeePtt]」
主公 → 貼到對應頻道 → Bot 建 thread
主公 → 在 thread 裡直接跟家臣互動、驗收
```

### 頻道選擇

| 專案類型 | 頻道 | Bot | CWD 格式 |
|---------|------|-----|----------|
| Flutter / iOS / macOS 原生 | #sandbox-native | @幕府令 | `[cwd:/Users/ruandan/Documents/claude_code/專案]` |
| Python / Markdown / 一般 | #sandbox-docker | @幕府行令 | `[cwd:/workspace/專案]` |

### 四層防護機制

防止 session 卡死 + 回覆丟失：

```
第 1 層：CLAUDE.md 規則 — 禁止前景跑長駐 process（預防）
第 2 層：alive check — process 死了 30 秒內偵測（快速恢復）
第 3 層：硬性 30 分鐘 timeout — 萬一前兩層沒擋住（安全網）
第 4 層：drain + fallback — end_turn 後 200ms drain，空回覆用 tool summary 保底
```

### Pool 鎖定策略

`SessionPool` 的並行性設計有兩個硬規則：

1. **不要在 streaming 時持有 pool 寫鎖**。每個 `AcpConnection` 包在 `Arc<Mutex<_>>` 裡 — `with_connection` 只在 read lock 下複製 Arc，然後鎖 **該 connection 自己的 mutex**。Pool 寫鎖只在 HashMap 結構變動時（建/刪 entry）短暫持有。
2. **不要在持 pool 鎖時 `await` connection mutex**。`cleanup_idle` 用 snapshot + `try_lock` 模式：read lock 下複製 Arcs → 釋放 → 逐個 `try_lock`（忙碌的 connection 按定義不是 idle，skip）→ 只在有 stale 要刪時才拿 write lock。違反這個規則會讓 cleanup 在 streaming session 上 hang 住同時卡住整個 pool（每 60s 週期性癱瘓）。

這兩個規則都是 production repro 過的 bug（對應 Issue #58 + Codex bot 在原 PR #77 的 P1 review）。

## 實作筆記

- 兩個 agent-broker instance 各聽各的 Discord 頻道，互不干擾
- pool config：max_sessions=10, session_ttl_hours=168 (7 天)
- Binary 已 rename：`agent-broker` → `openab`（Cargo.toml `name = "openab"`）

## Bug 經驗庫

| 問題 | 原因 | 解法 |
|------|------|------|
| 一個 session 串流時整個 pool 被卡住 | `with_connection` 在 callback 期間持有 pool 寫鎖 → 其他 thread `get_or_create` 全部排隊 | `Arc<Mutex<AcpConnection>>` per-connection lock — 只在複製 Arc 時握 read lock |
| Cleanup 週期性癱瘓 pool（每 60s） | `cleanup_idle` 在持 pool 寫鎖時 `await conn_arc.lock()`，碰到正在 streaming 的 session 就 hang 住 | Snapshot + `try_lock`：read lock 下拍快照 → 釋放 → 逐個 `try_lock` → 只在真有 stale 要刪時才拿 write lock |

## 待釐清

- 無
