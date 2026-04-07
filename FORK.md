# OpenAB — Production-Hardened Fork

> Upstream: [openabdev/openab](https://github.com/openabdev/openab)
> Goal: contribute everything back upstream.

This fork adds production hardening and operational features developed while running OpenAB 24/7 with multiple concurrent Discord bots. All changes are intended for upstream contribution — see [Upstream PRs](#upstream-prs) for tracking.

## What This Fork Adds

### Stability & Reliability

| Feature | Problem It Solves |
|---------|-------------------|
| **Per-connection locking** (`Arc<Mutex<AcpConnection>>`) | One busy session no longer blocks the entire pool ([#58](https://github.com/openabdev/openab/issues/58)) |
| **Alive check** (30s interval) | Detect crashed CLI processes within 30 seconds |
| **Hard timeout** (30 min) | Safety net for tool calls that never finish |
| **Drain window** (200ms after `end_turn`) | Catch late message chunks due to ACP event ordering ([#76](https://github.com/openabdev/openab/issues/76)) |
| **Empty response fallback** | Show tool summary instead of silent empty reply |
| **Stale thread rejection** | Reject threads from before bot startup to prevent zombie sessions |
| **UTF-8 safe truncation** | Prevent panic on multi-byte characters (CJK, emoji) in long messages |

### Operational Features

| Feature | Description |
|---------|-------------|
| **Management API** (HTTP) | `/status` and `/kill/<thread_id>` endpoints ([#39](https://github.com/openabdev/openab/issues/39)) |
| **`[cwd:]` directive** | Per-thread working directory via `[cwd:/path/to/project]` in prompt |
| **`[name:]` directive** | Custom thread naming via `[name:my-agent]` |
| **`allowed_bots`** | Allow specific bot users to trigger this broker (for multi-bot architectures) |
| **`toolCallId` matching** | Match tool status by ID instead of title text (titles change between start/done) |
| **Sub-tool filtering** | Skip noisy sub-tool events (`parentToolUseId`) that have no matching ToolStart |
| **Smart auto-allow** | Pick most permissive option from permission request instead of hardcoding `allow_always` |

### Interactive Features

| Feature | Description |
|---------|-------------|
| **Steer (prompt queueing)** | Send messages to a busy session via `SharedHandle` without blocking |
| **Interactive plan mode** | Display plan in Discord, wait for user to say "execute" or give feedback |
| **ExitPlanMode handling** | Proper interactive permission flow for plan mode transitions |

### Output Gate Pipeline

A configurable middleware layer for agent response verification ([#49](https://github.com/openabdev/openab/issues/49)):

- **Builtin gates** — regex-based secret scanning with redaction
- **Agent gates** — spawn a reviewer session to evaluate responses
- **Gate loop** — retry with findings up to `max_rounds`

Currently disabled by default; enable via `config.toml`.

## Architecture

We run two instances on a single Mac Mini:

| Instance | Environment | Channel | Use Case |
|----------|-------------|---------|----------|
| Bare metal | macOS native | #sandbox-native | Flutter, iOS development |
| Docker (OrbStack) | Linux sandbox | #sandbox-docker | General projects |

Both share the same codebase, different `config.toml`.

## Setup

```bash
cp config.example.toml config.toml
# Edit config.toml with your Discord bot token and channel IDs

cargo build --release
RUST_LOG=agent_broker=debug ./target/release/agent-broker config.toml
```

## Upstream PRs

| PR | Description | Status |
|----|-------------|--------|
| [#53](https://github.com/openabdev/openab/pull/53) | `toolCallId` matching + streaming truncation fix | Open |
| [#77](https://github.com/openabdev/openab/pull/77) | Per-connection lock + alive check + drain + startup cleanup | Open |

## Upstream Issues Filed

| Issue | Description |
|-------|-------------|
| [#39](https://github.com/openabdev/openab/issues/39) | Management API |
| [#49](https://github.com/openabdev/openab/issues/49) | Output gate pipeline |
| [#58](https://github.com/openabdev/openab/issues/58) | Pool write lock deadlock |
| [#76](https://github.com/openabdev/openab/issues/76) | Notification loop assumptions |
| [#111](https://github.com/openabdev/openab/issues/111) | ExitPlanMode permission fix |

## Differences from Upstream

This fork **keeps all upstream features** and adds on top. Key source files with modifications:

| File | What Changed |
|------|-------------|
| `src/acp/pool.rs` | `Arc<Mutex>` per-connection lock, per-thread CWD, SharedHandle, status/kill |
| `src/acp/connection.rs` | SharedHandle, PendingPermissions, smart auto-allow, interactive ExitPlanMode |
| `src/acp/protocol.rs` | `toolCallId` extraction, sub-tool filtering |
| `src/discord.rs` | Directives, steer, plan mode, gate loop, alive check, hard timeout, drain, stale thread rejection |
| `src/config.rs` | `allowed_bots`, `GatesConfig` |
| `src/main.rs` | Management API, gate pipeline init |
| `src/gates/` | Entire output gate pipeline module |

## License

Same as upstream (MIT).
