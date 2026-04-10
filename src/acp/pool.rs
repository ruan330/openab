use crate::acp::connection::{AcpConnection, SharedHandle};
use crate::config::AgentConfig;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
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
    state_file: Option<PathBuf>,
}

impl SessionPool {
    pub fn new(config: AgentConfig, max_sessions: usize, state_file: Option<PathBuf>) -> Self {
        let thread_cwds = state_file
            .as_ref()
            .and_then(|p| load_state(p))
            .unwrap_or_default();
        if !thread_cwds.is_empty() {
            info!(count = thread_cwds.len(), "loaded persisted thread cwds");
        }
        Self {
            connections: RwLock::new(HashMap::new()),
            shared_handles: RwLock::new(HashMap::new()),
            thread_cwds: RwLock::new(thread_cwds),
            config,
            max_sessions,
            state_file,
        }
    }

    fn persist_cwds(&self, cwds: &HashMap<String, String>) {
        let Some(ref path) = self.state_file else { return };
        if let Err(e) = save_state(path, cwds) {
            warn!(error = %e, path = %path.display(), "failed to persist thread cwds");
        }
    }

    pub async fn get_or_create(&self, thread_id: &str, cwd: Option<&str>) -> Result<()> {
        // Store per-thread CWD if provided (first message sets it)
        if let Some(dir) = cwd {
            let mut cwds = self.thread_cwds.write().await;
            let inserted = !cwds.contains_key(thread_id);
            cwds.entry(thread_id.to_string()).or_insert_with(|| dir.to_string());
            if inserted {
                self.persist_cwds(&cwds);
            }
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

        // Build effective args: config.args + model/permission_mode if set
        let mut effective_args = self.config.args.clone();
        if let Some(ref model) = self.config.model {
            effective_args.extend_from_slice(&["--model".to_string(), model.clone()]);
        }
        if let Some(ref mode) = self.config.permission_mode {
            effective_args.extend_from_slice(&["--permission-mode".to_string(), mode.clone()]);
        }

        let mut conn = AcpConnection::spawn(
            &self.config.command,
            &effective_args,
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

        // Snapshot Arcs under the read lock, then release it before touching
        // any per-connection mutex. Otherwise a long `session_prompt` would
        // hang `cleanup_idle` on the connection mutex while it still held
        // the pool write lock — exactly the starvation per-connection
        // locking is meant to eliminate. `try_lock` skips busy connections:
        // a connection that's in use is by definition not idle.
        let snapshot: Vec<(String, Arc<Mutex<AcpConnection>>)> = {
            let conns = self.connections.read().await;
            conns.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut stale = Vec::new();
        for (key, conn_arc) in &snapshot {
            let Ok(conn) = conn_arc.try_lock() else { continue };
            if conn.last_active < cutoff || !conn.alive() {
                stale.push(key.clone());
            }
        }

        if stale.is_empty() {
            return;
        }

        let mut conns = self.connections.write().await;
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
            let mut cwds = self.thread_cwds.write().await;
            if cwds.remove(thread_id).is_some() {
                self.persist_cwds(&cwds);
            }
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

fn load_state(path: &std::path::Path) -> Option<HashMap<String, String>> {
    let raw = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<HashMap<String, String>>(&raw) {
        Ok(map) => Some(map),
        Err(e) => {
            warn!(error = %e, path = %path.display(), "failed to parse state file");
            None
        }
    }
}

fn save_state(path: &std::path::Path, cwds: &HashMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let json = serde_json::to_string_pretty(cwds)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
