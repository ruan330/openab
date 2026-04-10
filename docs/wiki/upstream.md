# 上游協作

> 本頁涵蓋三 repo 工作流、PR/Issue 追蹤、貢獻策略。
> 關聯程式碼：N/A（工作流文件）

## 核心知識

### 三 Repo 定位

| Remote | Repo | 性質 | 用途 |
|--------|------|------|------|
| `origin` | `ruan330/agent-broker` | Private | 日常開發、部署。自由 commit，含 secrets |
| `upstream` | `openabdev/openab` | Public 上游 | 原始專案，拉取更新 |
| `fork` | `ruan330/openab` | Public fork | 對上游提 PR。零 secrets |

### 工作流

```
origin (private)          fork (public)              upstream
ruan330/agent-broker      ruan330/openab             openabdev/openab

日常開發 → commit/push    整理好的功能 → push        fetch 同步
隨意 commit               squash merge, 零 secrets
main branch               fork-main branch           main branch
                          ↗ feature branches → PR
```

**同步步驟**：
1. 在 private `origin` 開發完成
2. Checkout `fork-main` branch
3. Squash merge main（確保歷史乾淨、零 secrets）
4. Push 到 `fork`（用 `--force-with-lease`）
5. 從 `fork` 開 feature branch 對 `upstream` 提 PR
6. 在 `wiki/log.md` 記一條 sync 紀錄（squash commit hash + 主要類別）

**⚠️ Diverged history 陷阱（2026-04-10 踩雷）：**
`fork-main` 和 `main` 相對 merge base 有各自的變更時，`git merge --squash -X theirs main` **不會**覆蓋掉 fork-main 獨有的 one-sided additions（例如 fork-main 之前 squash 進來、後來在 origin 被移除的程式碼）。`-X theirs` 只在 conflict 出現時才偏向 `main`，對非 conflict 的單邊加法沒效。

**正確做法：** 先 `git reset --hard fork/main` 清乾淨，然後 `git checkout origin/main -- src/ Cargo.toml Cargo.lock CLAUDE.md docs/` 明確覆蓋工作樹。這會把 origin/main 當前狀態完整拷過去，避開 3-way merge 的舊 delta 記憶。

**為什麼不能 reset fork-main 到 origin/main：** fork-main 是線性 squash 歷史（一次 sync 一個 commit），reset 會炸掉歷史記錄，對 public 觀察者造成 force push 震盪。squash merge 才是對的語義。

### PR / Issue 追蹤（截至 2026-04-10）

| 類型 | 編號 | 主題 | 狀態 | 備註 |
|------|------|------|------|------|
| PR | [#53](https://github.com/openabdev/openab/pull/53) | toolCallId + streaming fix | open | UTF-8 fix 已推（`3cbc865`），已回覆 chaodu-agent review，已請求 re-review |
| PR | ~~[#77](https://github.com/openabdev/openab/pull/77)~~ | lock + alive + drain + cleanup | **closed** | 2026-04-10 關閉，拆成 3 個 focused PR（PR 1 = #183） |
| PR | [#183](https://github.com/openabdev/openab/pull/183) | per-connection `Arc<Mutex>` | open | PR #77 拆分 1/3。僅 pool.rs +51/-21，`with_connection` 簽名不變 → 零 call-site 改動。`cleanup_idle` 用 snapshot + `try_lock`（回應 Codex bot 在原 #77 的 P1 review）。closes #58 |
| PR | ~~[#59](https://github.com/openabdev/openab/pull/59)~~ | per-connection locking | **closed** | 已被 #77 supersede，2026-04-10 關閉 |
| PR | #147 (chenjian-agent) | permission outcome wrapper | open | 我們已留 review：指出 `allow_always` 硬編碼問題，附動態 optionId 選取程式碼 |
| PR | #179 (masami-agent) | streaming truncation fix | open | 跟我們 #53 的 streaming fix 方法相同，另有 #135/#159/#162 也在修 |
| PR | [#182](https://github.com/openabdev/openab/pull/182) | shutdown broadcast (RFC #78 §1d Phase 1) | open | 我們提，從 `fork/feat/shutdown-broadcast` 開出。`active_thread_ids()` helper + SIGTERM listener + neutral-wording broadcast |
| Comment | [RFC #78](https://github.com/openabdev/openab/issues/78#issuecomment-4223515423) | Phase 2 persistence trade-offs | posted | 分享本地 cwd persistence 實作經驗 + 4 個 trade-offs + 4 個 schema 問題。不發 PR，等 maintainer 定 SessionMetadata schema |
| Issue | [#39](https://github.com/openabdev/openab/issues/39) | management API | open | 上游 PR #57 在做，我們已留 production review |
| Comment | [PR #57](https://github.com/openabdev/openab/pull/57#issuecomment-4223684794) | management API production review | posted | 4 點：healthz flap / bulk DELETE 風險 / cwd 欄位 / auth token。0 previous comments，first reviewer |
| Issue | [#49](https://github.com/openabdev/openab/issues/49) | output gate pipeline | open | 無新回覆 |
| Issue | [#58](https://github.com/openabdev/openab/issues/58) | pool write lock deadlock | open | 被 RFC #78 納入 sub-items |
| Issue | [#76](https://github.com/openabdev/openab/issues/76) | notification loop 三假設不成立 | open | 無新回覆 |
| Issue | [#81](https://github.com/openabdev/openab/issues/81) | duplicate long messages | open | 已留言指向 PR #53。5 個競爭 PR（#53/#135/#159/#162/#179） |
| Issue | ~~[#111](https://github.com/openabdev/openab/issues/111)~~ | ExitPlanMode permission fix | **closed** | 2026-04-10 關閉為 #130 的子集 |
| Issue | [#130](https://github.com/openabdev/openab/issues/130) | permission outcome wrapper 缺失 | open | 已留言連結 #111。chenjian-agent 提 PR #147 |
| RFC | [#78](https://github.com/openabdev/openab/issues/78) | Session Management 設計提案 | open | chaodu-agent 確認收到，列入 sub-items。qijie850、m13v 也有回覆 |

### 貢獻策略（優先序）

1. **拆 PR #77** 成 3 個獨立 focused PR（見下方「PR #77 拆分計畫」），降低 review 門檻
2. **參與 RFC #78 落地**，主動提 PR 實現其中一塊
3. **在 PR #57 留言**分享 management API production 經驗

### PR #77 拆分計畫

原 PR 含 5 塊改動（lock + alive + drain + fallback + startup cleanup），其中 startup cleanup 我們已在 `807fb4f`（thread continuity）反向決策，不再推。**剩餘 4 塊合併成 3 個 focused PR，按順序發送：**

| 順序 | PR 主題 | Scope | 策略重點 |
|---|---|---|---|
| 1 | **Per-connection `Arc<Mutex>`** | pool.rs 架構 + discord.rs call site | 主打「實作 RFC §2b 預先 agree 的架構」，closes #58 / supersedes #59 |
| 2 | **Notification loop resilience**（drain window + empty response fallback） | discord.rs | Fixes #76（我們有 production repro），20~30 行 |
| 3 | **Alive check + hard timeout** | discord.rs | Defensive 安全網，`tokio::select!` 30s alive / 30min hard timeout |

**執行順序硬約束：**
- **依序發**，不要平行 — 三個 PR 都動 discord.rs notification loop，同時送會讓 reviewer 困惑 dependency
- 每個 PR 從 `upstream/main` 開乾淨 branch，不 rebase 舊 `fix/robust-notification-loop`
- PR 1 merge（或方向明確）前不動 PR 2

**⚠️ Fallback 策略（如果 PR 1 卡超過 3 天）：**

**PR 2 可以從 `upstream/main` 獨立開，不需要等 PR 1**。兩者技術上沒有依賴關係 — PR 2 只動 discord.rs 裡 notification loop 的 event handling 邏輯（drain + fallback），PR 1 只動 pool.rs 的 type 結構 + discord.rs 的 `pool.with_connection(...)` 呼叫點。唯一會重疊的是 discord.rs 的 merge conflict，但 conflict 解起來不難（手動 rebase 10 分鐘內）。

判斷標準：
- PR 1 開出去後 3 天內沒 reviewer activity → 啟動 fallback，獨立送 PR 2
- PR 1 有 maintainer 討論但卡在 nitpick → 繼續等，不啟動 fallback
- PR 1 被 reject → 重新評估整個拆分（可能整個方案要改）

**PR 3 不走 fallback**：它的 notification loop 改動跟 PR 2 的 drain 邏輯緊密交織，等 PR 2 先落地比較乾淨。

### 風險與注意

- **PR #53 streaming fix 競爭**：5 個 PR 在修同一個 bug（#81），我們是最早（4/5）且最完整（toolCallId + streaming + UTF-8）
- **0 個外部功能 PR 被 merge**：上游 maintainer 在 rapid build mode，近期 merge 的都是 docs/chart/CI
- **讀舊 PR 的 bot review**：原 PR #77 有 Codex bot 留 P1 finding（`cleanup_idle` 死鎖），我們一直沒讀也沒修，直到 PR #183 才補上。教訓：開新 PR 前先爬一遍舊 PR 的 review comment，尤其是同檔案的 bot 發現

### 待辦

- [x] 推 UTF-8 fix（floor_char_boundary）到 fork，更新 PR #53
- [x] 在 Issue #81 留言指向 PR #53
- [x] 在 PR #53 回覆 chaodu-agent review，請求 re-review
- [x] Close PR #59（被 #77 supersede）
- [x] Close Issue #111（#130 的子集）
- [x] 在 Issue #130 留言連結 #111
- [x] 在 PR #147 留 review（動態 optionId 選取建議）
- [x] 在 RFC #78 留言分享 Phase 2 persistence 實作經驗與 schema 問題
- [x] 在 PR #57 留言分享 management API production 經驗（healthz flap / bulk DELETE / cwd / auth）
- [x] 拆 PR #77 成小 PR — 關閉 #77，開 PR #183（per-connection lock, 1/3）
- [ ] PR 2（notification loop resilience）— 等 #183 方向明確或 3 天 fallback
- [ ] PR 3（alive check + hard timeout）— 等 PR 2 merge

### 上游新增功能追蹤

| Branch | 內容 | 跟我們的關聯 |
|--------|------|-------------|
| `feat/management-api` | 上游在做 management API（PR #57） | 跟 Issue #39 重疊 |
| `feat/agents-map-s3-persistence` | Helm multi-agent + S3 auth 備份 | neilkuan 提案 |
| `feat/qwen-support` | Qwen 支援 | 跟 5090 GPU 相關 |
| sender identity injection | `<sender_context>` JSON 注入 | 已整合 |

### 社群動態（截至 2026-04-10）

- **chengli**：在 Issue #81 提供完整 repro log（9 tool calls + 3000-char zh-TW），確認我們的 root cause analysis
- **qijie850**：在 RFC #78 提出 worktree isolation + session continuity 需求
- **m13v**：在 RFC #78 分享 session recording + journal 方案
- **masami-agent**：提 PR #179 修 streaming duplicate（跟我們同方法）

## PR / Issue 撰寫準則

> 來源：Peter Steinberger（PSPDFKit 創辦人）的 PR review 流程。
> 上游 maintainer 用 AI（Codex）輔助 review，會按以下標準篩選：

### Reviewer 視角（對方怎麼看我們的 PR）

1. **AI Review** — 先讓 AI 掃一遍，找潛在問題
2. **問題是否清楚？** — PR 連要解決什麼問題都說不清 → 直接拒絕
3. **這是最好的解法嗎？** — 95% 的情況答案是「不是」
4. **討論取捨，通常要求重寫**

> "Most folks send too localized, small fixes that would end up making the project unmaintainable."
> 只修眼前的症狀，不考慮整體架構 → 累積下來專案越來越難維護。

### 寫出不被退回的 PR — Checklist

| 原則 | 說明 | 範例（我們的 PR） |
|------|------|-----------------|
| **說清楚問題** | 不只「修了 bug」，描述問題的根因 | PR #53：不是「tool 顯示錯」，而是「title matching 在 start/done 間不一致，因為 ACP 事件 title 會在 streaming 中改變」 |
| **解釋為什麼是最好的解法** | 列出考慮過的替代方案和 tradeoff | PR #77：列出 RwLock vs Arc<Mutex> 的取捨 |
| **修根因不修症狀** | 找到根本原因，一起處理 | 不是在 Discord 端 dedupe 訊息，而是從 streaming 源頭截斷 |
| **PR 描述要有 context** | 讓 reviewer 不需要猜意圖 | 附 production log、重現步驟、架構圖 |

### Issue 撰寫準則

| 原則 | 說明 |
|------|------|
| **描述觀察到的行為** | 附 log、截圖、重現步驟 |
| **分析根因**（如果已知）| 不只報症狀，附上你的調查結果 |
| **提出建議解法** | 說明你考慮過的方案和 tradeoff |
| **連結相關 Issue/PR** | 建立脈絡，讓 maintainer 看到全貌 |

### 自檢流程（提交前）

1. 重讀 PR/Issue 描述 — 一個不了解脈絡的人能看懂嗎？
2. 根因是否清楚？還是只在描述症狀？
3. 有沒有考慮過替代方案？為什麼這個最好？
4. 改動範圍是否恰當？太小（patch 症狀）或太大（混多個 concern）？
5. 有沒有附 production 證據（log、數據、重現步驟）？

## 實作筆記

- Public fork 的 CI workflows（Build & Release、Release Charts）已禁用 — 需要上游的 GitHub App secrets
- PR #59 已被 #77 supersede

## Bug 經驗庫

（無 — 工作流相關）

## 待釐清

- 無
