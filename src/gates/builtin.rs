use crate::config::{GateAction, GateEntry};
use anyhow::Result;
use regex::Regex;

pub struct BuiltinGate {
    pub name: String,
    pub patterns: Vec<Regex>,
    pub action: GateAction,
}

pub enum BuiltinResult {
    Pass,
    Block { matched: Vec<String> },
    Redact { redacted_text: String, matched: Vec<String> },
}

impl BuiltinGate {
    pub fn new(entry: &GateEntry) -> Result<Self> {
        let patterns = entry
            .patterns
            .iter()
            .map(|p| Regex::new(p).map_err(|e| anyhow::anyhow!("invalid pattern '{}': {}", p, e)))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            name: entry.name.clone(),
            patterns,
            action: entry.action.clone(),
        })
    }

    pub fn evaluate(&self, text: &str) -> BuiltinResult {
        let mut matched = Vec::new();
        for pattern in &self.patterns {
            for m in pattern.find_iter(text) {
                matched.push(m.as_str().to_string());
            }
        }

        if matched.is_empty() {
            return BuiltinResult::Pass;
        }

        match self.action {
            GateAction::Block => BuiltinResult::Block { matched },
            GateAction::Redact => {
                let mut redacted = text.to_string();
                for pattern in &self.patterns {
                    redacted = pattern.replace_all(&redacted, "[REDACTED]").to_string();
                }
                BuiltinResult::Redact {
                    redacted_text: redacted,
                    matched,
                }
            }
        }
    }
}
