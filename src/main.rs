mod acp;
mod config;
mod discord;
mod format;
mod gates;
mod reactions;

use serenity::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        reactions = cfg.reactions.enabled,
        "config loaded"
    );

    let gate_pipeline = gates::GatePipeline::from_config(&cfg.gates, &cfg.agent)?;
    let gate_pipeline = if gate_pipeline.is_empty() {
        None
    } else {
        info!("gate pipeline loaded");
        Some(Arc::new(gate_pipeline))
    };

    let pool = Arc::new(acp::SessionPool::new(cfg.agent, cfg.pool.max_sessions));
    let ttl_secs = cfg.pool.session_ttl_hours * 3600;

    let allowed_channels: HashSet<u64> = cfg
        .discord
        .allowed_channels
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    let allowed_bots: HashSet<u64> = cfg
        .discord
        .allowed_bots
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    let start_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let handler = discord::Handler {
        pool: pool.clone(),
        allowed_channels,
        allowed_bots,
        reactions_config: cfg.reactions,
        gate_pipeline,
        pending_permissions: Arc::new(acp::PendingPermissions::default()),
        start_epoch_ms,
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&cfg.discord.bot_token, intents)
        .event_handler(handler)
        .await?;

    // Spawn HTTP status endpoint on port 8090
    let status_pool = pool.clone();
    tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind("0.0.0.0:8090").await {
            Ok(l) => l,
            Err(e) => { tracing::error!("status server bind failed: {e}"); return; }
        };
        info!("status server listening on :8090");
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            let pool = status_pool.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);

                // Parse request path
                let path = req.lines().next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let (status, body) = if path == "/status" || path == "/" {
                    let sessions = pool.status().await;
                    let body = serde_json::json!({
                        "active_sessions": sessions.len(),
                        "sessions": sessions.iter().map(|(tid, cwd, alive, idle)| {
                            serde_json::json!({
                                "thread_id": tid,
                                "cwd": cwd,
                                "alive": alive,
                                "idle_seconds": idle
                            })
                        }).collect::<Vec<_>>()
                    });
                    ("200 OK", body)
                } else if path.starts_with("/kill/") {
                    let thread_id = &path[6..];
                    if pool.kill_session(thread_id).await {
                        ("200 OK", serde_json::json!({"killed": thread_id}))
                    } else {
                        ("404 Not Found", serde_json::json!({"error": "session not found", "thread_id": thread_id}))
                    }
                } else {
                    ("404 Not Found", serde_json::json!({"error": "not found"}))
                };

                let body_str = serde_json::to_string_pretty(&body).unwrap_or_default();
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body_str.len(), body_str
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });

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
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("shutdown signal received");
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
