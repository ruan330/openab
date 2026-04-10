mod acp;
mod config;
mod discord;
mod format;
mod reactions;

use serenity::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openab=info".into()),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    let cfg = config::load_config(&config_path)?;
    info!(
        agent_cmd = %cfg.agent.command,
        pool_max = cfg.pool.max_sessions,
        channels = ?cfg.discord.allowed_channels,
        users = ?cfg.discord.allowed_users,
        reactions = cfg.reactions.enabled,
        "config loaded"
    );

    let pool = Arc::new(acp::SessionPool::new(cfg.agent, cfg.pool.max_sessions));
    let ttl_secs = cfg.pool.session_ttl_hours * 3600;

    let allowed_channels = parse_id_set(&cfg.discord.allowed_channels, "allowed_channels")?;
    let allowed_users = parse_id_set(&cfg.discord.allowed_users, "allowed_users")?;
    info!(channels = allowed_channels.len(), users = allowed_users.len(), "parsed allowlists");

    let handler = discord::Handler {
        pool: pool.clone(),
        allowed_channels,
        allowed_users,
        reactions_config: cfg.reactions,
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&cfg.discord.bot_token, intents)
        .event_handler(handler)
        .await?;

    // Spawn cleanup task
    let cleanup_pool = pool.clone();
    let cleanup_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            cleanup_pool.cleanup_idle(ttl_secs).await;
        }
    });

    // Run bot until SIGINT/SIGTERM
    let shard_manager = client.shard_manager.clone();
    let shutdown_pool = pool.clone();
    let broadcast_pool = pool.clone();
    let shutdown_http = client.http.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        info!("shutdown signal received");

        // Broadcast shutdown notification to active threads before closing the pool.
        // Neutral wording — we don't promise automatic resume; Phase 2 of RFC #78 1d
        // (session persistence) is a separate follow-up.
        let thread_ids = broadcast_pool.active_thread_ids().await;
        info!(count = thread_ids.len(), "broadcasting shutdown notification");
        for thread_id in thread_ids {
            if let Ok(id) = thread_id.parse::<u64>() {
                let channel = serenity::model::id::ChannelId::new(id);
                if let Err(e) = channel
                    .say(
                        &shutdown_http,
                        "🔄 Broker restarting. You can continue the conversation when the broker is back.",
                    )
                    .await
                {
                    tracing::warn!(thread_id, error = %e, "failed to post shutdown notification");
                }
            }
        }

        shard_manager.shutdown_all().await;
    });

    info!("starting discord bot");
    client.start().await?;

    // Cleanup
    cleanup_handle.abort();
    shutdown_pool.shutdown().await;
    info!("openab shut down");
    Ok(())
}

fn parse_id_set(raw: &[String], label: &str) -> anyhow::Result<HashSet<u64>> {
    let set: HashSet<u64> = raw
        .iter()
        .filter_map(|s| match s.parse() {
            Ok(id) => Some(id),
            Err(_) => {
                tracing::warn!(value = %s, label = label, "ignoring invalid entry");
                None
            }
        })
        .collect();
    if !raw.is_empty() && set.is_empty() {
        anyhow::bail!("all {label} entries failed to parse — refusing to start with an empty allowlist");
    }
    Ok(set)
}
