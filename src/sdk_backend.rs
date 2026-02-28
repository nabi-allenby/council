use std::fs;
use std::path::Path;
use std::process::Command;

use crate::agent::{
    normalize_text, AgentBackend, Message, DISCUSSION_RETRY_PROMPT, VOTE_RETRY_PROMPT,
};
use crate::error::CouncilError;
use crate::schema::{strip_structured_block, validate_discussion_response, validate_vote_response};
use crate::types::{Turn, Vote};

pub struct SdkAgent {
    role_name: String,
    personality: String,
    model: String,
}

impl SdkAgent {
    pub fn new(role: &str, personality_path: &Path, model: &str) -> Result<Self, CouncilError> {
        let personality = fs::read_to_string(personality_path).map_err(|_| {
            CouncilError::FileNotFound(format!(
                "Personality file not found: {}",
                personality_path.display()
            ))
        })?;

        Ok(SdkAgent {
            role_name: role.to_string(),
            personality,
            model: model.to_string(),
        })
    }

    fn query(&self, system: &str, messages: &[Message]) -> Result<String, CouncilError> {
        let prompt = Self::flatten_messages(messages);

        let output = Command::new("claude")
            .arg("--print")
            .arg("--model")
            .arg(&self.model)
            .arg("--system-prompt")
            .arg(system)
            .arg("--")
            .arg(&prompt)
            .env_remove("CLAUDECODE")
            .output()
            .map_err(|e| CouncilError::ApiError(format!("Failed to run claude CLI: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CouncilError::ApiError(format!(
                "claude CLI exited with {}: {}",
                output.status, stderr
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(normalize_text(text.trim()))
    }

    fn flatten_messages(messages: &[Message]) -> String {
        let mut parts = Vec::new();
        for msg in messages {
            if msg.role == "assistant" {
                parts.push(format!("[Your previous response]\n{}", msg.content));
            } else {
                parts.push(msg.content.clone());
            }
        }
        parts.join("\n\n")
    }
}

impl AgentBackend for SdkAgent {
    fn role(&self) -> &str {
        &self.role_name
    }

    fn respond(
        &self,
        round_num: u32,
        system_context: &str,
        messages: &[Message],
        max_retries: u32,
    ) -> Result<Turn, CouncilError> {
        let system = format!("{}\n\n{}", self.personality, system_context);
        let mut msgs = messages.to_vec();

        for attempt in 0..=max_retries {
            let text = self.query(&system, &msgs)?;

            if let Some(parsed) = validate_discussion_response(&text) {
                return Ok(Turn {
                    agent: self.role_name.clone(),
                    round: round_num,
                    content: strip_structured_block(&text),
                    parsed,
                });
            }

            if attempt < max_retries {
                msgs.push(Message {
                    role: "assistant".to_string(),
                    content: text,
                });
                msgs.push(Message {
                    role: "user".to_string(),
                    content: DISCUSSION_RETRY_PROMPT.to_string(),
                });
            }
        }

        Err(CouncilError::RetryExhausted(format!(
            "{} failed to produce valid ---RESPONSE--- block after {} retries",
            self.role_name, max_retries
        )))
    }

    fn cast_vote(
        &self,
        system_context: &str,
        messages: &[Message],
        max_retries: u32,
    ) -> Result<Vote, CouncilError> {
        let system = format!("{}\n\n{}", self.personality, system_context);
        let mut msgs = messages.to_vec();

        for attempt in 0..=max_retries {
            let text = self.query(&system, &msgs)?;

            if let Some(parsed) = validate_vote_response(&text) {
                return Ok(Vote {
                    agent: self.role_name.clone(),
                    vote: parsed.vote,
                    reason: parsed.reason,
                });
            }

            if attempt < max_retries {
                msgs.push(Message {
                    role: "assistant".to_string(),
                    content: text,
                });
                msgs.push(Message {
                    role: "user".to_string(),
                    content: VOTE_RETRY_PROMPT.to_string(),
                });
            }
        }

        Err(CouncilError::RetryExhausted(format!(
            "{} failed to produce valid ---VOTE--- block after {} retries",
            self.role_name, max_retries
        )))
    }
}
