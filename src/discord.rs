use crate::acp::{classify_notification, AcpEvent, PendingPermissions, SessionPool};
use crate::config::ReactionsConfig;
use crate::format;
use crate::gates::{GatePipeline, GateResult};
use crate::config::GateTrigger;
use crate::reactions::StatusReactionController;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, MessageId};
use serenity::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{error, info};

pub struct Handler {
    pub pool: Arc<SessionPool>,
    pub allowed_channels: HashSet<u64>,
    pub allowed_bots: HashSet<u64>,
    pub reactions_config: ReactionsConfig,
    pub gate_pipeline: Option<Arc<GatePipeline>>,
    pub pending_permissions: Arc<PendingPermissions>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore bot messages unless the bot is in the allowed_bots whitelist
        if msg.author.bot {
            if !self.allowed_bots.contains(&msg.author.id.get()) {
                return;
            }
            tracing::debug!(bot_id = %msg.author.id, bot_name = %msg.author.name, "accepted message from allowed bot");
        }

        let bot_id = ctx.cache.current_user().id;

        let channel_id = msg.channel_id.get();
        let in_allowed_channel =
            self.allowed_channels.is_empty() || self.allowed_channels.contains(&channel_id);

        let is_mentioned = msg.mentions_user_id(bot_id)
            || msg.content.contains(&format!("<@{}>", bot_id))
            || msg.mention_roles.iter().any(|r| msg.content.contains(&format!("<@&{}>", r)));

        let in_thread = if !in_allowed_channel {
            match msg.channel_id.to_channel(&ctx.http).await {
                Ok(serenity::model::channel::Channel::Guild(gc)) => {
                    let result = gc
                        .parent_id
                        .map_or(false, |pid| self.allowed_channels.contains(&pid.get()));
                    tracing::debug!(channel_id = %msg.channel_id, parent_id = ?gc.parent_id, result, "thread check");
                    result
                }
                Ok(other) => {
                    tracing::debug!(channel_id = %msg.channel_id, kind = ?other, "not a guild channel");
                    false
                }
                Err(e) => {
                    tracing::debug!(channel_id = %msg.channel_id, error = %e, "to_channel failed");
                    false
                }
            }
        } else {
            false
        };

        if !in_allowed_channel && !in_thread {
            return;
        }
        if !in_thread && !is_mentioned {
            return;
        }

        let prompt = if is_mentioned {
            strip_mention(&msg.content)
        } else {
            msg.content.trim().to_string()
        };
        if prompt.is_empty() {
            return;
        }

        // Check for pending plan (ExitPlanMode was auto-approved, user is responding)
        let thread_key_check = if in_thread {
            msg.channel_id.get().to_string()
        } else {
            String::new()
        };
        if in_thread {
            if let Some(_pending) = self.pending_permissions.take(&thread_key_check).await {
                let execute_keywords = ["執行", "execute", "go", "yes", "y", "好", "開始"];
                let is_execute = execute_keywords.iter().any(|kw|
                    prompt.to_lowercase().trim() == *kw
                );

                if is_execute {
                    let _ = msg.channel_id.say(&ctx.http, "▶️ 已確認").await;
                    return; // Agent already executing after auto-approve
                } else {
                    let _ = msg.channel_id.say(&ctx.http, "📝 繼續規劃...").await;
                    // prompt stays as user's feedback, fall through to new turn
                }
            }
        }

        // Parse [cwd:/path/to/project] and [name:家臣名] directives from prompt
        let (cwd, name, prompt) = parse_directives(&prompt);
        if prompt.is_empty() {
            return;
        }

        // Inject structured sender context so the downstream CLI can identify who sent the message
        let display_name = msg.member.as_ref()
            .and_then(|m| m.nick.as_ref())
            .unwrap_or(&msg.author.name);
        let sender_ctx = serde_json::json!({
            "schema": "openab.sender.v1",
            "sender_id": msg.author.id.to_string(),
            "sender_name": msg.author.name,
            "display_name": display_name,
            "channel": "discord",
            "channel_id": msg.channel_id.to_string(),
            "is_bot": msg.author.bot,
        });
        let prompt = format!(
            "<sender_context>\n{}\n</sender_context>\n\n{}",
            serde_json::to_string(&sender_ctx).unwrap(),
            prompt
        );

        // Resolve thread name: explicit [name:] > CWD-derived > prompt-based
        let thread_display_name = name
            .clone()
            .map(|n| format!("🤖 {n}"))
            .or_else(|| cwd.as_ref().and_then(|c| thread_name_from_cwd(c)));

        tracing::debug!(prompt = %prompt, ?cwd, ?thread_display_name, in_thread, "processing");

        let thread_id = if in_thread {
            msg.channel_id.get()
        } else {
            let tname = thread_display_name.as_deref().unwrap_or(&prompt);
            match get_or_create_thread(&ctx, &msg, tname).await {
                Ok(id) => id,
                Err(e) => {
                    error!("failed to create thread: {e}");
                    return;
                }
            }
        };

        let thread_channel = ChannelId::new(thread_id);

        let thinking_msg = match thread_channel.say(&ctx.http, "...").await {
            Ok(m) => m,
            Err(e) => {
                error!("failed to post: {e}");
                return;
            }
        };

        let thread_key = thread_id.to_string();
        if let Err(e) = self.pool.get_or_create(&thread_key, cwd.as_deref()).await {
            let _ = edit(&ctx, thread_channel, thinking_msg.id, "⚠️ Failed to start agent.").await;
            error!("pool error: {e}");
            return;
        }

        // Create reaction controller on the user's original message
        let reactions = Arc::new(StatusReactionController::new(
            self.reactions_config.enabled,
            ctx.http.clone(),
            msg.channel_id,
            msg.id,
            self.reactions_config.emojis.clone(),
            self.reactions_config.timing.clone(),
        ));
        reactions.set_queued().await;

        // Stream prompt with gate pipeline
        let max_rounds = self.gate_pipeline.as_ref().map(|g| g.max_rounds).unwrap_or(1);
        let mut current_prompt = prompt;
        let mut current_msg = thinking_msg;
        let mut rounds = 0u32;
        let mut final_ok = false;

        loop {
            rounds += 1;
            let result = stream_prompt(
                &self.pool,
                &thread_key,
                &current_prompt,
                &ctx,
                thread_channel,
                current_msg.id,
                reactions.clone(),
                self.pending_permissions.clone(),
            )
            .await;

            match result {
                Ok(ref response_text) if response_text == "_(queued)_" => {
                    final_ok = true;
                    break;
                }
                Ok(response_text) => {
                    tracing::debug!(response_len = response_text.len(), has_gate = self.gate_pipeline.is_some(), "gate check");
                    if let Some(ref pipeline) = self.gate_pipeline {
                        tracing::debug!(is_empty = pipeline.is_empty(), "gate pipeline check");
                        if !pipeline.is_empty() {
                            let working_dir = self.pool.get_working_dir(&thread_key).await;
                            let trigger = GateTrigger::OnComplete;
                            match pipeline.evaluate(&response_text, &trigger, &working_dir).await {
                                GateResult::Pass => {
                                    final_ok = true;
                                    break;
                                }
                                GateResult::Redacted(redacted) => {
                                    let chunks = format::split_message(&redacted, 2000);
                                    let _ = edit(&ctx, thread_channel, current_msg.id, &chunks[0]).await;
                                    final_ok = true;
                                    break;
                                }
                                GateResult::Blocked(reason) => {
                                    let _ = thread_channel.say(&ctx.http, format!("🚫 {reason}")).await;
                                    break;
                                }
                                GateResult::BlockedWithFindings { reason, findings } => {
                                    if rounds < max_rounds {
                                        let _ = thread_channel.say(&ctx.http,
                                            format!("🔄 Review round {rounds}: {reason}\n\n{findings}")
                                        ).await;
                                        current_prompt = format!(
                                            "A reviewer found issues with your last response. Please fix them and try again:\n\n{}",
                                            findings
                                        );
                                        current_msg = match thread_channel.say(&ctx.http, "...").await {
                                            Ok(m) => m,
                                            Err(e) => { error!("failed to post: {e}"); break; }
                                        };
                                        continue;
                                    } else {
                                        let _ = thread_channel.say(&ctx.http,
                                            format!("🚫 Blocked after {rounds} rounds: {reason}")
                                        ).await;
                                        break;
                                    }
                                }
                            }
                        } else {
                            final_ok = true;
                            break;
                        }
                    } else {
                        final_ok = true;
                        break;
                    }
                }
                Err(e) => {
                    let _ = edit(&ctx, thread_channel, current_msg.id, &format!("⚠️ {e}")).await;
                    break;
                }
            }
        }

        let has_pending_plan = self.pending_permissions.contains(&thread_key).await;

        if has_pending_plan || final_ok {
            reactions.set_done().await;
        } else {
            reactions.set_error().await;
        }

        let hold_ms = if final_ok || has_pending_plan {
            self.reactions_config.timing.done_hold_ms
        } else {
            self.reactions_config.timing.error_hold_ms
        };
        if self.reactions_config.remove_after_reply {
            let reactions = reactions;
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(hold_ms)).await;
                reactions.clear().await;
            });
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!(user = %ready.user.name, "discord bot connected");
    }
}

async fn edit(ctx: &Context, ch: ChannelId, msg_id: MessageId, content: &str) -> serenity::Result<Message> {
    ch.edit_message(&ctx.http, msg_id, serenity::builder::EditMessage::new().content(content)).await
}

async fn stream_prompt(
    pool: &SessionPool,
    thread_key: &str,
    prompt: &str,
    ctx: &Context,
    channel: ChannelId,
    msg_id: MessageId,
    reactions: Arc<StatusReactionController>,
    pending_permissions: Arc<PendingPermissions>,
) -> anyhow::Result<String> {
    // Get per-connection reference — does NOT hold the pool lock.
    let conn_arc = pool.get_connection(thread_key).await?;

    // Try to acquire per-connection lock. If busy (another prompt running),
    // steer the message via prompt queueing instead of blocking.
    let mut conn = match conn_arc.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            // BUSY: steer via shared handle — ACP will queue the prompt
            info!(thread_key, "session busy, using prompt queueing (steer)");
            let shared = pool.get_shared_handle(thread_key).await?;
            let _ = edit(&ctx, channel, msg_id, "📨 _Queued — session busy, will process after current task_").await;
            shared.send_prompt(prompt).await?;
            return Ok("_(queued)_".to_string());
        }
    };

    let reset = conn.session_reset;
    conn.session_reset = false;

    let shared_handle = conn.shared_handle();
    let (mut rx, mut perm_rx, _) = conn.session_prompt(prompt).await?;
    reactions.set_thinking().await;

    let initial = if reset {
        "⚠️ _Session expired, starting fresh..._\n\n...".to_string()
    } else {
        "...".to_string()
    };
    let (buf_tx, buf_rx) = watch::channel(initial);

    let mut text_buf = String::new();
    let mut tool_lines: Vec<String> = Vec::new();
    let mut tool_ids: Vec<String> = Vec::new();
    let current_msg_id = msg_id;

    if reset {
        text_buf.push_str("⚠️ _Session expired, starting fresh..._\n\n");
    }

    // Spawn edit-streaming task
    let edit_handle = spawn_edit_task(ctx, channel, msg_id, buf_rx.clone());

    // Track whether plan has been shown this turn (to suppress duplicates)
    let mut plan_shown = false;

    // Process ACP notifications.
    // Periodically check if the agent process is still alive, and enforce a
    // hard timeout as a safety net against infinite tool calls (e.g. flutter run).
    let mut got_first_text = false;
    let prompt_start = tokio::time::Instant::now();
    let hard_timeout = std::time::Duration::from_secs(30 * 60); // 30 minutes
    loop {
        let notification = tokio::select! {
            msg = rx.recv() => match msg {
                Some(n) => n,
                None => break,
            },
            perm = perm_rx.recv() => {
                if let Some(pr) = perm {
                    // Show plan only once per turn (suppress duplicates from
                    // repeated ExitPlanMode calls in the same turn).
                    // Reply FIRST — CC has a tight timeout on permission responses.
                    // Display plan to Discord AFTER replying.
                    if let Some(ref handle) = shared_handle {
                        let _ = handle.send_response(
                            pr.rpc_id,
                            serde_json::json!({"outcome": {"outcome": "selected", "optionId": "bypassPermissions"}}),
                        ).await;
                    }
                    if !plan_shown {
                        plan_shown = true;
                        if let Some(ref plan) = pr.plan_text {
                            let plan_msg = format!("📋 **Plan**\n\n{plan}\n\n_回覆修改意見繼續規劃，或等待執行完成。_");
                            let chunks = format::split_message(&plan_msg, 2000);
                            for chunk in &chunks {
                                let _ = channel.say(&ctx.http, chunk).await;
                            }
                        }
                        {
                            use crate::acp::connection::PendingPermission;
                            pending_permissions.insert(thread_key.to_string(), PendingPermission {
                                plan_text: pr.plan_text.unwrap_or_default(),
                            }).await;
                        }
                    }
                }
                continue;
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                if !conn.alive() {
                    tracing::warn!("agent process died during prompt");
                    break;
                }
                if prompt_start.elapsed() > hard_timeout {
                    tracing::warn!("hard timeout (30 min) reached, breaking out");
                    break;
                }
                continue; // alive and within timeout → keep waiting
            }
        };

        if notification.id.is_some() {
            // Prompt response arrived. Drain any remaining notifications
            // for a short window — message chunks sometimes arrive after
            // the response due to ACP event ordering.
            let drain_until = tokio::time::Instant::now() + std::time::Duration::from_millis(200);
            while let Ok(remaining) = tokio::time::timeout_at(drain_until, rx.recv()).await {
                match remaining {
                    Some(n) => {
                        if let Some(AcpEvent::Text(t)) = classify_notification(&n) {
                            text_buf.push_str(&t);
                            let _ = buf_tx.send(compose_display(&tool_lines, &text_buf));
                        }
                    }
                    None => break,
                }
            }
            break;
        }

        if let Some(event) = classify_notification(&notification) {
            match event {
                AcpEvent::Text(t) => {
                    if !got_first_text {
                        got_first_text = true;
                    }
                    text_buf.push_str(&t);
                    let _ = buf_tx.send(compose_display(&tool_lines, &text_buf));
                }
                AcpEvent::Thinking => {
                    reactions.set_thinking().await;
                }
                AcpEvent::ToolStart { id, title, .. } if !title.is_empty() => {
                    reactions.set_tool(&title).await;
                    tool_ids.push(id);
                    tool_lines.push(format!("🔧 `{title}`..."));
                    let _ = buf_tx.send(compose_display(&tool_lines, &text_buf));
                }
                AcpEvent::ToolDone { id, title, status, .. } => {
                    reactions.set_thinking().await;
                    let icon = if status == "completed" { "✅" } else { "❌" };
                    let idx = tool_ids.iter().rposition(|tid| !tid.is_empty() && tid == &id);
                    if let Some(i) = idx {
                        let existing = &tool_lines[i];
                        let kept_title = existing
                            .split('`').nth(1)
                            .unwrap_or(&title);
                        tool_lines[i] = format!("{icon} `{kept_title}`");
                    } else if !title.is_empty() {
                        tool_ids.push(id);
                        tool_lines.push(format!("{icon} `{title}`"));
                    }
                    let _ = buf_tx.send(compose_display(&tool_lines, &text_buf));
                }
                _ => {}
            }
        }
    }

    conn.prompt_done().await;
    drop(conn); // release per-connection lock
    drop(buf_tx);
    let _ = edit_handle.await;

    // Final edit — if text_buf is empty but we had tool activity,
    // compose a fallback from tool lines so the user sees something.
    let final_content = compose_display(&tool_lines, &text_buf);
    let final_content = if final_content.trim().is_empty() {
        if !tool_lines.is_empty() {
            let mut fallback = tool_lines.join("\n");
            fallback.push_str("\n\n_Task completed but no text response was captured._");
            fallback
        } else {
            "_(no response)_".to_string()
        }
    } else {
        final_content
    };

    let chunks = format::split_message(&final_content, 2000);
    for (i, chunk) in chunks.iter().enumerate() {
        if i == 0 {
            let _ = edit(&ctx, channel, current_msg_id, chunk).await;
        } else {
            let _ = channel.say(&ctx.http, chunk).await;
        }
    }

    Ok(text_buf)
}

fn compose_display(tool_lines: &[String], text: &str) -> String {
    let mut out = String::new();
    if !tool_lines.is_empty() {
        for line in tool_lines {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(text.trim_end());
    out
}

fn spawn_edit_task(
    ctx: &Context,
    channel: ChannelId,
    msg_id: MessageId,
    buf_rx: watch::Receiver<String>,
) -> tokio::task::JoinHandle<()> {
    let ctx = ctx.clone();
    let mut buf_rx = buf_rx;
    tokio::spawn(async move {
        let mut last_content = String::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            if buf_rx.has_changed().unwrap_or(false) {
                let content = buf_rx.borrow_and_update().clone();
                if content != last_content {
                    let display = if content.len() > 1900 {
                        let boundary = content.floor_char_boundary(1900);
                        format!("{}…", &content[..boundary])
                    } else {
                        content.clone()
                    };
                    let _ = edit(&ctx, channel, msg_id, &display).await;
                    last_content = content;
                }
            }
            if buf_rx.has_changed().is_err() {
                break;
            }
        }
    })
}

fn strip_mention(content: &str) -> String {
    let re = regex::Regex::new(r"<@[!&]?\d+>").unwrap();
    re.replace_all(content, "").trim().to_string()
}

/// Parse `[cwd:/path/to/project]` and `[name:家臣名]` directives from a prompt.
/// Returns (optional cwd, optional name, remaining prompt text).
fn parse_directives(prompt: &str) -> (Option<String>, Option<String>, String) {
    let mut cwd = None;
    let mut name = None;
    let mut rest = prompt.to_string();

    let cwd_re = regex::Regex::new(r"\[cwd:([^\]]+)\]").unwrap();
    if let Some(caps) = cwd_re.captures(&rest) {
        cwd = Some(caps[1].trim().to_string());
        rest = cwd_re.replace(&rest, "").trim().to_string();
    }

    let name_re = regex::Regex::new(r"\[name:([^\]]+)\]").unwrap();
    if let Some(caps) = name_re.captures(&rest) {
        name = Some(caps[1].trim().to_string());
        rest = name_re.replace(&rest, "").trim().to_string();
    }

    (cwd, name, rest)
}

/// Derive a thread name from the CWD path (last directory component).
fn thread_name_from_cwd(cwd: &str) -> Option<String> {
    let dir_name = cwd.trim_end_matches('/').rsplit('/').next()?;
    // Map known project directories to 家臣 names
    let mapped = match dir_name {
        "sijin-finance" => "司金使",
        "taiyi-health" => "太醫令",
        _ => dir_name,
    };
    Some(format!("🤖 {mapped}"))
}

fn shorten_thread_name(prompt: &str) -> String {
    // Shorten GitHub URLs: https://github.com/owner/repo/issues/123 → owner/repo#123
    let re = regex::Regex::new(r"https?://github\.com/([^/]+/[^/]+)/(issues|pull)/(\d+)").unwrap();
    let shortened = re.replace_all(prompt, "$1#$3");
    let name: String = shortened.chars().take(40).collect();
    if name.len() < shortened.len() {
        format!("{name}...")
    } else {
        name
    }
}

async fn get_or_create_thread(ctx: &Context, msg: &Message, name: &str) -> anyhow::Result<u64> {
    let channel = msg.channel_id.to_channel(&ctx.http).await?;
    if let serenity::model::channel::Channel::Guild(ref gc) = channel {
        if gc.thread_metadata.is_some() {
            return Ok(msg.channel_id.get());
        }
    }

    let thread_name = shorten_thread_name(name);

    let thread = msg
        .channel_id
        .create_thread_from_message(
            &ctx.http,
            msg.id,
            serenity::builder::CreateThread::new(thread_name)
                .auto_archive_duration(serenity::model::channel::AutoArchiveDuration::OneDay),
        )
        .await?;

    Ok(thread.id.get())
}
