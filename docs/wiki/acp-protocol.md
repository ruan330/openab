# ACP Protocol 處理

> 本頁涵蓋 ACP 事件分類、plan mode 互動、permission flow。
> 關聯程式碼：`src/acp/protocol.rs`, `src/acp/connection.rs`

## 核心知識

### 事件分類（protocol.rs）

| ACP 事件 | 對應 AcpEvent | 說明 |
|----------|--------------|------|
| `agent_message_chunk` | `Text` | 最終回覆文字，進 text_buf |
| `plan` | `Status` | Plan 內容，不進 text_buf |
| `switch_mode` | 不 emit | 曾 emit Text 造成重複，已移除 |
| tool 相關 | `ToolStart` / `ToolDone` | toolCallId matching |
| `session/request_permission` | 觸發 permission handler | 見 permission flow |

### toolCallId matching

- 上游用 tool title 比對 start/done，但 title 在兩階段經常不一致
- 我們改用 `toolCallId` 精準 matching（PR #53）
- 跳過 `parentToolUseId` 不為空的 sub-tool 事件（減少噪音）

### Plan Mode 互動（CC App 模型）

**根本設計原則**：不跨越「用戶做決定」的邊界保持 stream_prompt 存活。

```
Plan 生成 → ExitPlanMode permission
  → perm_rx handler：顯示 plan + auto-approve bypassPermissions
  → 存 PendingPermission { plan_text }
  → turn 自然結束

用戶說「執行」→ message handler 找到 PendingPermission
  → 構造 execute prompt → 新的 stream_prompt turn
```

**關鍵要點**：
- 每個 `stream_prompt` call 自包含 — reaction、display、timeout 天然正確
- plan 由 perm_rx handler 專門處理（帶「回覆執行或修改意見」提示）
- `plan_shown` flag 防止同一 turn 內重複 ExitPlanMode 導致重複顯示

### Permission Flow

auto-allow 邏輯（connection.rs）：
1. 從 options 陣列選最寬鬆的合法 optionId
2. 優先序：allow_always kind > allow_once kind > 第一個非 reject
3. **不是**硬編碼 `allow_always` — ExitPlanMode 的 options 沒有 `allow_always`
4. 回覆 JSON shape 必須包 `outcome` envelope（ACP spec compliance，見 Issue #130）：
   ```json
   { "outcome": { "outcome": "selected", "optionId": "..." } }
   ```
   Flat `{"optionId": "..."}` 在 Kiro CLI 舊版可以，但 Claude Code / Cursor SDK 會視為 refusal。兩個 reply 點都要包：`connection.rs` auto-allow + `discord.rs` ExitPlanMode interactive reply。

## 實作筆記

- `send_response(bypassPermissions)` 必須在 `channel.say(plan)` 前面 — CC 有 permission response timeout，延遲回覆會被 reject
- 用戶說「執行」時 agent 已在 auto-approve 後開始執行，回覆「已確認」+ return 即可

### 已砍掉的失敗設計

以下方案在迭代中被否決，不要重新引入：
- PlanDecision enum + decision channel (mpsc)
- skip_end_turn flag
- auto_approve_exit_plan flag
- reactions swap
- plan_rpc_id

## Bug 經驗庫

| 問題 | 原因 | 解法 |
|------|------|------|
| ExitPlanMode 被 CC 視為 rejected | auto-allow 硬編碼 `allow_always`，但 ExitPlanMode options 裡沒有這個值 | 從 options 動態選最寬鬆的合法 optionId |
| permission 回覆被視為 refusal（#130） | 缺 `outcome` envelope，send flat `{"optionId":"..."}` | 包進 `{"outcome":{"outcome":"selected","optionId":"..."}}` — 兩個 reply 點都要改 |
| Plan text 重複顯示 | protocol.rs emit Text + perm_rx handler say 重複送 | 移除 protocol.rs emission，plan 由 perm_rx 專門處理 |
| Plan mode 回覆丟失 | stream_prompt 試圖跨越 decision 邊界保持存活 | CC App 模型：auto-approve + turn 結束 + 新 turn 執行 |
| plan_shown 後仍重複 | 同一 turn 內 ExitPlanMode 被 call 多次 | plan_shown flag 只顯示一次 |
| ACP end_turn 先於 message_chunk | ACP 協議不保證事件順序 | end_turn 後 200ms drain window + 空回覆 fallback |

## 待釐清

- 無
