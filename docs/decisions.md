# 決策記錄

## 否決方案

| 日期 | 決策 | 原因 | 何時重新考慮 |
|------|------|------|-------------|
| 2026-04-08 | Plan mode: PlanDecision enum + decision channel (mpsc) | stream_prompt 跨越 decision 邊界 → phantom end_turn、時序錯亂 | 不需要，CC App 模型已根本解決 |
| 2026-04-08 | Plan mode: skip_end_turn flag | 修一個 bug 引入另一個（Execute 後重複 ExitPlanMode） | 不需要 |
| 2026-04-08 | Plan mode: auto_approve_exit_plan flag | 同上，都是 patch 根本設計錯誤 | 不需要 |
| 2026-04-10 | Docker auth: 掛載 macOS ~/.claude 讓 Docker 共用 token | refresh token 被 macOS CC CLI rotate → invalid_grant，且 broker 寫 file 破壞 CC CLI | Docker 有獨立 Keychain 時 |
| 2026-04-10 | macOS broker 寫 .credentials.json 做 OAuth refresh | 觸發 CC CLI reload → 破壞本機 auth | 永遠不要 |
| 2026-04-06 | 硬編碼 `allow_always` 作為 permission auto-allow | ExitPlanMode 的 options 沒有 allow_always → CC 視為 rejected | 永遠不要 |

## 採用方案

| 日期 | 決策 | 原因 | 備註 |
|------|------|------|------|
| 2026-04-08 | Plan mode: CC App 模型（auto-approve + turn 結束 + 新 turn） | 跟 CC App 同構，不跨越 decision 邊界 | 砍掉大量複雜度 |
| 2026-04-10 | Docker auth: `claude setup-token` + `CLAUDE_CODE_OAUTH_TOKEN` env var | 1 年 token，免 refresh，跟上游一致 | |
| 2026-04-06 | per-connection lock: `Arc<Mutex<AcpConnection>>` | pool 寫鎖從 streaming 期間降到毫秒級查找 | PR #77 |
| 2026-04-07 | Permission auto-allow: 動態從 options 選最寬鬆合法 optionId | 相容所有 permission 類型，不只 ExitPlanMode | |
| 2026-04-06 | toolCallId matching 取代 title matching | title 在 start/done 間經常不一致 | PR #53 |
| 2026-04-05 | 從 acpx/ACP 遷移到 agent-broker (Rust) | 效能、穩定性、Docker 沙盒支援 | 傭兵制架構 |
| 2026-04-07 | 三 repo 工作流（private + fork + upstream） | 保護 secrets 同時持續貢獻上游 | |
| 2026-04-10 | 分支策略：方案 A（main 全功能部署 + feat branch 用於上游 PR） | 功能間耦合深（共用 discord.rs/connection.rs），拆 main 會大量 merge conflict；一人開發不需要多分支合流 | 多人協作時考慮方案 B |
| 2026-04-10 | 移除 OAuth runtime refresh code，純用 `setup-token` | `setup-token` 1 年有效，refresh_oauth_if_needed + try_refresh_via_http + auth retry 都是死 code | |
| 2026-04-10 | Thread continuity：移除 startup archive + stale epoch rejection，舊 thread 訊息重建 session | Discord thread 保留對話歷史對使用者有價值；broker 重啟不應該封掉所有 thread | 搭配 cwd 持久化才完整 |
| 2026-04-10 | cwd 持久化：`PoolConfig.state_file` JSON 檔，load on startup + save on mutation（atomic tmp+rename） | 重啟後若無持久化，舊 thread 會 fallback 到 `config.working_dir`，丟失每個 thread 的專案上下文 | 對應 RFC #78 Phase 2 of 1d。上游貢獻會拆成兩個 PR：先 Phase 1d shutdown notification，再 persistence |
| 2026-04-10 | Shutdown broadcast：neutral 措辭（「You can continue the conversation when the broker is back.」），不做 grace period | 「will resume」暗示持久化（PR 2 範圍）；grace period 是獨立優化；PR 1 範圍越小越容易 merge | 對應 RFC #78 Phase 1 of 1d。加上 SIGTERM listener（原本只聽 ctrl_c，systemd / docker stop 等都不會觸發 shutdown hook） |
| 2026-04-10 | Permission reply 包 `outcome` envelope（ACP spec compliance） | Flat `{"optionId":"..."}` 在 bypass mode 下幸運避開（一般 tool 不走 permission flow），但 ExitPlanMode 仍會經過；non-bypass mode 下每個 tool 都會踩到 #130。補 wrapper 是 spec fix，跟 bypass 與否無關 | 對應 Issue #130（chenjian-agent PR #147 在修）。我們不追上游 — fork 已有互動式 plan mode UX，等 #147 merge 後再 rebase |
| 2026-04-10 | `cleanup_idle` 改 snapshot + `try_lock` 模式（不在持 pool 寫鎖時 await connection mutex） | 原本 `write().await` 後再逐個 `conn_arc.lock().await`，如果碰到正在 streaming 的 session 會 hang 在 connection mutex 上同時握著 pool 寫鎖 → 每 60s 一次 cleanup 週期性癱瘓整個 pool。`try_lock` 跳過忙碌的 connection（忙碌 = 按定義不是 idle），write lock 只在真有 stale 要刪時才拿 | Codex bot 在原 PR #77 的 P1 review 就指出這點，我們 fork 一直沒修。PR #183 的 cleanup_idle 跟 fork 同步修，這次有寫進 commit message 說明 |
