use crate::acp::connection::{AcpConnection, SharedHandle};
use crate::config::AgentConfig;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::Instant;
use tracing::{info, warn};

pub struct SessionPool {
    connections: RwLock<HashMap<String, Arc<Mutex<AcpConnection>>>>,
    shared_handles: RwLock<HashMap<String, SharedHandle>>,
    /// Per-thread working directory overrides (persists across reconnects)
    thread_cwds: RwLock<HashMap<String, String>>,
    config: AgentConfig,
    max_sessions: usize,
}

impl SessionPool {
    pub fn new(config: AgentConfig, max_sessions: usize) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            shared_handles: RwLock::new(HashMap::new()),
            thread_cwds: RwLock::new(HashMap::new()),
            config,
            max_sessions,
        }
    }

    pub async fn get_or_create(&self, thread_id: &str, cwd: Option<&str>) -> Result<()> {
        // Store per-thread CWD if provided (first message sets it)
        if let Some(dir) = cwd {
            let mut cwds = self.thread_cwds.write().await;
            cwds.entry(thread_id.to_string()).or_insert_with(|| dir.to_string());
        }

        // Resolve effective working directory
        let effective_cwd = {
            let cwds = self.thread_cwds.read().await;
            cwds.get(thread_id).cloned().unwrap_or_else(|| self.config.working_dir.clone())
        };

        // Check if alive connection exists (read lock only)
        {
            let conns = self.connections.read().await;
            if let Some(conn_arc) = conns.get(thread_id) {
                let conn = conn_arc.lock().await;
                if conn.alive() {
                    return Ok(());
                }
            }
        }

        // Need to create or rebuild (write lock)
        let mut conns = self.connections.write().await;

        // Double-check after acquiring write lock
        if let Some(conn_arc) = conns.get(thread_id) {
            let conn = conn_arc.lock().await;
            if conn.alive() {
                return Ok(());
            }
            drop(conn);
            warn!(thread_id, "stale connection, rebuilding");
            conns.remove(thread_id);
        }

        if conns.len() >= self.max_sessions {
            return Err(anyhow!("pool exhausted ({} sessions)", self.max_sessions));
        }

        let mut conn = AcpConnection::spawn(
            &self.config.command,
            &self.config.args,
            &effective_cwd,
            &self.config.env,
        )
        .await?;

        conn.initialize().await?;
        conn.session_new(&effective_cwd).await?;

        let is_rebuild = conns.contains_key(thread_id);
        if is_rebuild {
            conn.session_reset = true;
        }

        let shared = conn.shared_handle();
        conns.insert(thread_id.to_string(), Arc::new(Mutex::new(conn)));
        if let Some(handle) = shared {
            self.shared_handles.write().await.insert(thread_id.to_string(), handle);
        }
        Ok(())
    }

    /// Get the shared stdin handle for a connection without locking it.
    /// Used for steer (prompt queueing) and interactive permission replies.
    pub async fn get_shared_handle(&self, thread_id: &str) -> Result<SharedHandle> {
        let handles = self.shared_handles.read().await;
        handles.get(thread_id).cloned()
            .ok_or_else(|| anyhow!("no shared handle for thread {thread_id}"))
    }

    /// Get a shared reference to a connection. The caller locks only this connection,
    /// not the entire pool — other sessions remain accessible.
    pub async fn get_connection(&self, thread_id: &str) -> Result<Arc<Mutex<AcpConnection>>> {
        let conns = self.connections.read().await;
        conns
            .get(thread_id)
            .cloned()
            .ok_or_else(|| anyhow!("no connection for thread {thread_id}"))
    }

    pub async fn cleanup_idle(&self, ttl_secs: u64) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(ttl_secs);
        let mut conns = self.connections.write().await;
        let mut stale = Vec::new();
        for (key, conn_arc) in conns.iter() {
            let conn = conn_arc.lock().await;
            if conn.last_active < cutoff || !conn.alive() {
                stale.push(key.clone());
            }
        }
        for key in stale {
            info!(thread_id = %key, "cleaning up idle session");
            conns.remove(&key);
        }
    }

    /// Return status of all active sessions.
    pub async fn status(&self) -> Vec<(String, String, bool, u64)> {
        let conns = self.connections.read().await;
        let cwds = self.thread_cwds.read().await;
        let mut result = Vec::new();
        for (thread_id, conn_arc) in conns.iter() {
            let conn = conn_arc.lock().await;
            let cwd = cwds.get(thread_id).cloned().unwrap_or_default();
            let alive = conn.alive();
            let idle_secs = conn.last_active.elapsed().as_secs();
            result.push((thread_id.clone(), cwd, alive, idle_secs));
        }
        result
    }

    pub async fn get_working_dir(&self, thread_id: &str) -> String {
        let cwds = self.thread_cwds.read().await;
        cwds.get(thread_id).cloned().unwrap_or_else(|| self.config.working_dir.clone())
    }

    /// Kill a single session by thread_id.
    pub async fn kill_session(&self, thread_id: &str) -> bool {
        let mut conns = self.connections.write().await;
        let removed = conns.remove(thread_id).is_some();
        if removed {
            self.shared_handles.write().await.remove(thread_id);
            self.thread_cwds.write().await.remove(thread_id);
            info!(thread_id, "session killed");
        }
        removed
    }

    pub async fn shutdown(&self) {
        let mut conns = self.connections.write().await;
        let count = conns.len();
        conns.clear();
        info!(count, "pool shutdown complete");
    }
}
