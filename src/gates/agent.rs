use crate::acp::connection::AcpConnection;
use crate::config::AgentConfig;
use anyhow::Result;
use tracing::{info, warn};

pub enum AgentVerdict {
    Allow,
    Block { findings: String },
}

pub struct AgentGate {
    pub name: String,
    pub prompt_template: String,
    pub max_rounds: u32,
    pub agent_config: AgentConfig,
}

impl AgentGate {
    pub fn new(
        name: String,
        prompt_file: &str,
        max_rounds: u32,
        agent_config: AgentConfig,
    ) -> Result<Self> {
        let prompt_template = std::fs::read_to_string(prompt_file)
            .map_err(|e| anyhow::anyhow!("failed to read prompt file '{}': {}", prompt_file, e))?;
        Ok(Self {
            name,
            prompt_template,
            max_rounds,
            agent_config,
        })
    }

    /// Spawn an ephemeral reviewer session, send the review prompt, collect verdict.
    pub async fn evaluate(
        &self,
        git_diff: &str,
        working_dir: &str,
    ) -> Result<AgentVerdict> {
        let prompt = self
            .prompt_template
            .replace("{{GIT_DIFF}}", git_diff);

        info!(gate = %self.name, "spawning reviewer session");

        let mut conn = AcpConnection::spawn(
            &self.agent_config.command,
            &self.agent_config.args,
            working_dir,
            &self.agent_config.env,
        )
        .await?;

        conn.initialize().await?;
        conn.session_new(working_dir).await?;

        let (mut rx, _perm_rx, _id) = conn.session_prompt(&prompt).await?;

        // Collect full response text
        let mut response_text = String::new();
        while let Some(notification) = rx.recv().await {
            if notification.id.is_some() {
                break; // prompt response arrived
            }
            if let Some(params) = &notification.params {
                if let Some(update) = params.get("update") {
                    if update.get("sessionUpdate").and_then(|s| s.as_str()) == Some("agent_message_chunk") {
                        if let Some(text) = update.get("content").and_then(|c| c.get("text")).and_then(|t| t.as_str()) {
                            response_text.push_str(text);
                        }
                    }
                }
            }
        }

        conn.prompt_done().await;
        // conn drops here → kill_on_drop cleans up the process

        // Parse verdict from first line
        let first_line = response_text.lines().next().unwrap_or("").trim();
        if first_line.starts_with("ALLOW") {
            info!(gate = %self.name, "reviewer approved");
            Ok(AgentVerdict::Allow)
        } else if first_line.starts_with("BLOCK") {
            let findings = response_text
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            info!(gate = %self.name, "reviewer blocked");
            Ok(AgentVerdict::Block { findings })
        } else {
            // Cannot parse → fail closed
            warn!(gate = %self.name, first_line, "could not parse reviewer verdict, defaulting to BLOCK");
            Ok(AgentVerdict::Block {
                findings: format!("Reviewer output could not be parsed:\n{response_text}"),
            })
        }
    }
}
