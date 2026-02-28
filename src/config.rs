use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::CouncilError;

pub const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";

#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    Api,
    AgentSdk,
}

impl Backend {
    pub fn as_str(&self) -> &str {
        match self {
            Backend::Api => "api",
            Backend::AgentSdk => "agent-sdk",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CouncilConfig {
    pub rotation: Vec<String>,
    pub rounds: u32,
    pub model: String,
    pub tools: HashMap<String, Vec<String>>,
    pub backend: Backend,
}

impl Default for CouncilConfig {
    fn default() -> Self {
        CouncilConfig {
            rotation: Vec::new(),
            rounds: 3,
            model: DEFAULT_MODEL.to_string(),
            tools: HashMap::new(),
            backend: Backend::Api,
        }
    }
}

#[derive(Deserialize)]
struct RawConfig {
    rotation: Option<Vec<String>>,
    rounds: Option<u32>,
    model: Option<String>,
    tools: Option<HashMap<String, Vec<String>>>,
    backend: Option<String>,
}

pub fn load_config(agents_dir: &Path) -> Result<CouncilConfig, CouncilError> {
    let config_path = agents_dir.join("council.json");
    if !config_path.exists() {
        return Err(CouncilError::FileNotFound(format!(
            "Config file not found: {}",
            config_path.display()
        )));
    }

    let text = fs::read_to_string(&config_path)
        .map_err(|e| CouncilError::FileNotFound(format!("Cannot read config: {}", e)))?;
    let raw: RawConfig = serde_json::from_str(&text)
        .map_err(|e| CouncilError::Validation(format!("Invalid JSON in config: {}", e)))?;

    let rotation = raw
        .rotation
        .ok_or_else(|| CouncilError::Validation("Config 'rotation' is required".into()))?;
    if rotation.is_empty() {
        return Err(CouncilError::Validation(
            "Config 'rotation' must be a list of at least 1 agent name".into(),
        ));
    }
    if rotation.len() > 7 {
        return Err(CouncilError::Validation(
            "Config 'rotation' must have at most 7 agents".into(),
        ));
    }
    if rotation.len() > 1 && rotation.len() % 2 == 0 {
        return Err(CouncilError::Validation(
            "Config 'rotation' must have an odd number of agents (or exactly 1)".into(),
        ));
    }

    let rounds = raw.rounds.unwrap_or(3);
    if !(1..=3).contains(&rounds) {
        return Err(CouncilError::Validation(
            "Config 'rounds' must be an integer between 1 and 3".into(),
        ));
    }

    let model = raw.model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    if model.trim().is_empty() {
        return Err(CouncilError::Validation(
            "Config 'model' must be a non-empty string".into(),
        ));
    }

    let backend = match raw.backend.as_deref() {
        Some("agent-sdk") => Backend::AgentSdk,
        Some("api") | None => Backend::Api,
        Some(other) => {
            return Err(CouncilError::Validation(format!(
                "Config 'backend' must be one of ('api', 'agent-sdk'), got: '{}'",
                other
            )))
        }
    };

    let tools = raw.tools.unwrap_or_default();

    // Validate agent personality files exist
    for role in &rotation {
        let path = agents_dir.join(format!("{}.md", role));
        if !path.exists() {
            return Err(CouncilError::FileNotFound(format!(
                "Agent personality file not found: {}",
                path.display()
            )));
        }
    }

    // Validate tools keys reference agents in rotation
    for role in tools.keys() {
        if !rotation.contains(role) {
            return Err(CouncilError::Validation(format!(
                "Config 'tools' references unknown agent: {}",
                role
            )));
        }
    }

    Ok(CouncilConfig {
        rotation,
        rounds,
        model,
        tools,
        backend,
    })
}
