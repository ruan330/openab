# Wiki 變更日誌

| 日期 | 操作 | 影響頁面 | 摘要 |
|------|------|---------|------|
| 2026-04-10 | Bootstrap | 全部 | 從 Obsidian Vault 遷移，建立 wiki 自治知識系統 |
| 2026-04-10 | 更新 | upstream.md | 同步 GitHub 最新狀態：PR #179 搶先風險、RFC #78 社群回覆、Issue #81 repro log |
| 2026-04-10 | 新增 | upstream.md | PR/Issue 撰寫準則（Peter Steinberger review 流程），含 checklist + 自檢流程 |
| 2026-04-10 | 更新 | upstream.md | 同步今日操作：PR #53 UTF-8 fix 已推、close #59/#111、review #147、comment #81/#130。更新追蹤表與待辦 |
| 2026-04-10 | 更新 | discord.md, decisions.md | Thread continuity across restarts：移除 startup archive + stale epoch rejection；加上 `PoolConfig.state_file` 持久化 thread_cwds（load on startup, atomic save on mutation）。對應 RFC #78 Phase 2 of 1d |
| 2026-04-10 | 更新 | discord.md, decisions.md | Shutdown broadcast + SIGTERM listener：SIGINT/SIGTERM → 對 active threads post「🔄 Broker restarting...」→ pool shutdown。對應 RFC #78 Phase 1 of 1d（PR 1 候選）|
| 2026-04-10 | 更新 | upstream.md | 開 PR #182（shutdown broadcast，RFC #78 §1d Phase 1）— 從 upstream/main 開乾淨 branch，isolated from persistence |
| 2026-04-10 | 更新 | upstream.md | RFC #78 留 comment 分享 Phase 2 持久化 trade-offs。**不發 PR 2**：上游無 per-thread cwd、RFC §3a SessionMetadata schema 未定案，硬送 cwd-only 版本會被退 |
| 2026-04-10 | 更新 | acp-protocol.md, decisions.md | 補 permission reply 的 `outcome` envelope（connection.rs + discord.rs 兩個 reply 點）— ACP spec compliance（對應 Issue #130）。我們 bypass mode 幸運沒踩到 universal 症狀，ExitPlanMode 也因 claude-agent-acp@0.25.0 loose parsing 沒壞，但 spec 層面還是該包 |
| 2026-04-10 | 更新 | upstream.md | 在 PR #57（thepagent management API）留 production review：healthz flap / bulk DELETE 風險 / cwd 欄位 / auth token。0 comments before us，first reviewer |
| 2026-04-10 | 關閉 | upstream.md | 關閉 PR #77，開 PR #183（per-connection `Arc<Mutex>`, 1/3）。從 `upstream/main` 乾淨 branch，minimal diff +31/-21 僅動 pool.rs，`with_connection` 簽名不變所以 zero call-site 改動。closes #58，supersedes #59/#77 |
| 2026-04-10 | 更新 | pool.rs, decisions.md | `cleanup_idle` 改 snapshot + `try_lock` 模式 — 原實作在持 pool 寫鎖時 await connection mutex，正在 streaming 的 session 會 hang cleanup 任務同時卡住整個 pool。Codex bot 在原 PR #77 就指出這個 P1，我們一直沒修。PR #183 同步 amend + fork 本地也修 |
| 2026-04-10 | 部署 | — | Rolling redeploy 兩個 broker（幕府令 bare metal + 幕府行令 Docker）載入 `cleanup_idle` 修復。SIGTERM → shutdown broadcast → pool shutdown → restart → 讀回持久化 thread cwds。Docker 遇到 CMD 覆蓋踩雷（多打 `openab` 參數跟 ENTRYPOINT 疊加成 `openab openab ...`）已修正 |
| 2026-04-10 | 更新 | architecture.md, discord.md, deployment.md, upstream.md | 同步今日改動：(1) architecture 加 pool lock 策略段 + 兩條新 bug 條目 +移除 startup archive 舊條目；(2) discord 的 shutdown broadcast 從「PR 1 候選」改成 PR #182；(3) deployment 加 rolling redeploy 流程 + Docker ENTRYPOINT 踩雷條目；(4) upstream 的 PR #183 條目補 cleanup_idle 註記 + 風險段加「讀舊 PR bot review」教訓 |
