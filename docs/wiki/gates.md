# Output Gate Pipeline

> 本頁涵蓋 output gate 框架（builtin + agent gates）。
> 關聯程式碼：`src/gates/`, `prompts/`

## 核心知識

### 架構

Gate pipeline 在回覆發送給用戶前執行檢查：

```
Claude 回覆 → builtin gates → agent gates → 通過 → 發送到 Discord
                                           → 不通過 → retry（最多 max_rounds）
```

### Gate 類型

| 類型 | 說明 | 實作 |
|------|------|------|
| Builtin | Regex-based secret scan | `src/gates/builtin.rs` |
| Agent | Spawn reviewer Claude session 審查回覆 | `src/gates/agent.rs` |

### 設定

- `GatesConfig` 在 `config.rs` 定義
- Review prompt 在 `prompts/` 目錄
- 目前 **disabled**，待測試後啟用

## 實作筆記

- Gate loop 支援多輪 retry（max_rounds 設定）
- Agent gate 會 spawn 獨立的 Claude session 來審查主 session 的回覆
- 上游 Issue [#49](https://github.com/openabdev/openab/issues/49)

## Bug 經驗庫

（尚無 — gate 尚未啟用）

## 待釐清

- 啟用 builtin secret-scan gate 後的效能影響
- Agent gate reviewer session 的 token 消耗
