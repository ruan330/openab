pub mod agent;
pub mod builtin;

use crate::config::{AgentConfig, GateTrigger, GateType, GatesConfig};
use anyhow::Result;
use tracing::info;

pub use agent::{AgentGate, AgentVerdict};
pub use builtin::BuiltinGate;

pub enum GateResult {
    /// All gates passed
    Pass,
    /// Builtin gate redacted content
    Redacted(String),
    /// Builtin gate blocked
    Blocked(String),
    /// Agent gate blocked with findings for remediation
    BlockedWithFindings { reason: String, findings: String },
}

pub struct GatePipeline {
    builtin_gates: Vec<(GateTrigger, BuiltinGate)>,
    agent_gates: Vec<(GateTrigger, AgentGate)>,
    pub max_rounds: u32,
}

impl GatePipeline {
    pub fn from_config(gates: &GatesConfig, agent_config: &AgentConfig) -> Result<Self> {
        let mut builtin_gates = Vec::new();
        let mut agent_gates = Vec::new();
        let mut max_rounds = 3u32;

        if !gates.enabled {
            return Ok(Self {
                builtin_gates,
                agent_gates,
                max_rounds,
            });
        }

        for entry in &gates.pipeline {
            match entry.gate_type {
                GateType::Builtin => {
                    let gate = BuiltinGate::new(entry)?;
                    info!(name = %entry.name, "loaded builtin gate");
                    builtin_gates.push((entry.trigger.clone(), gate));
                }
                GateType::Agent => {
                    let prompt_file = entry
                        .prompt_file
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("agent gate '{}' missing prompt_file", entry.name))?;
                    let gate = AgentGate::new(
                        entry.name.clone(),
                        prompt_file,
                        entry.max_rounds,
                        agent_config.clone(),
                    )?;
                    if entry.max_rounds > 0 {
                        max_rounds = max_rounds.max(entry.max_rounds);
                    }
                    info!(name = %entry.name, prompt_file, "loaded agent gate");
                    agent_gates.push((entry.trigger.clone(), gate));
                }
            }
        }

        Ok(Self {
            builtin_gates,
            agent_gates,
            max_rounds,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.builtin_gates.is_empty() && self.agent_gates.is_empty()
    }

    /// Run all gates matching the given trigger. Builtin gates first, then agent gates.
    pub async fn evaluate(
        &self,
        text: &str,
        trigger: &GateTrigger,
        working_dir: &str,
    ) -> GateResult {
        let mut current_text = text.to_string();
        let mut was_redacted = false;

        // Builtin gates (fast, synchronous)
        for (gate_trigger, gate) in &self.builtin_gates {
            if gate_trigger != trigger {
                continue;
            }
            match gate.evaluate(&current_text) {
                builtin::BuiltinResult::Pass => {}
                builtin::BuiltinResult::Block { matched } => {
                    let reason = format!(
                        "Gate '{}' blocked: matched patterns {:?}",
                        gate.name, matched
                    );
                    return GateResult::Blocked(reason);
                }
                builtin::BuiltinResult::Redact { redacted_text, matched } => {
                    info!(gate = %gate.name, matched = ?matched, "redacted content");
                    current_text = redacted_text;
                    was_redacted = true;
                }
            }
        }

        // Agent gates (expensive, spawn reviewer)
        for (gate_trigger, gate) in &self.agent_gates {
            if gate_trigger != trigger {
                continue;
            }

            // For on_file_change, check if there are actually changes
            if *trigger == GateTrigger::OnFileChange {
                if !has_file_changes(working_dir).await {
                    continue;
                }
            }

            let git_diff = get_git_diff(working_dir).await;
            if git_diff.is_empty() && *trigger == GateTrigger::OnFileChange {
                continue;
            }

            let timeout = tokio::time::timeout(
                std::time::Duration::from_secs(120),
                gate.evaluate(&git_diff, working_dir),
            )
            .await;

            match timeout {
                Ok(Ok(AgentVerdict::Allow)) => {}
                Ok(Ok(AgentVerdict::Block { findings })) => {
                    let reason = format!("Gate '{}' blocked", gate.name);
                    return GateResult::BlockedWithFindings { reason, findings };
                }
                Ok(Err(e)) => {
                    // Gate error → fail open with warning (don't block user on infra failure)
                    tracing::error!(gate = %gate.name, error = %e, "agent gate failed, passing through");
                }
                Err(_) => {
                    tracing::error!(gate = %gate.name, "agent gate timed out (120s), passing through");
                }
            }
        }

        if was_redacted {
            GateResult::Redacted(current_text)
        } else {
            GateResult::Pass
        }
    }
}

async fn has_file_changes(working_dir: &str) -> bool {
    let output = tokio::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(working_dir)
        .output()
        .await;
    match output {
        Ok(o) => !o.stdout.is_empty(),
        Err(_) => false,
    }
}

async fn get_git_diff(working_dir: &str) -> String {
    let output = tokio::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(working_dir)
        .output()
        .await;
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}
