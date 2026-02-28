use std::fs;
use std::path::Path;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::agent::{AgentBackend, Message, DISCUSSION_RETRY_PROMPT, VOTE_RETRY_PROMPT};
use crate::error::CouncilError;
use crate::http::call_anthropic_api;
use crate::schema::{strip_structured_block, validate_discussion_response, validate_vote_response};
use crate::types::{Turn, Vote};

const MAX_TOKENS: u32 = 2048;
const MAX_TOKENS_SEARCH: u32 = 4096;

pub struct ApiAgent {
    role_name: String,
    personality: String,
    model: String,
    tools: Vec<String>,
    client: Client,
}

impl ApiAgent {
    pub fn new(
        role: &str,
        personality_path: &Path,
        model: &str,
        tools: Vec<String>,
    ) -> Result<Self, CouncilError> {
        let personality = fs::read_to_string(personality_path).map_err(|_| {
            CouncilError::FileNotFound(format!(
                "Personality file not found: {}",
                personality_path.display()
            ))
        })?;

        Ok(ApiAgent {
            role_name: role.to_string(),
            personality,
            model: model.to_string(),
            tools,
            client: Client::new(),
        })
    }

    fn call_api(
        &self,
        system: &str,
        messages: &[Message],
        max_tokens: u32,
        use_tools: bool,
    ) -> Result<String, CouncilError> {
        let msg_array: Vec<Value> = messages
            .iter()
            .map(|m| json!({"role": m.role, "content": m.content}))
            .collect();

        let tools = if use_tools && self.tools.contains(&"web_search".to_string()) {
            Some(json!([{
                "type": "web_search_20250305",
                "name": "web_search",
                "max_uses": 5
            }]))
        } else {
            None
        };

        call_anthropic_api(&self.client, &self.model, system, &msg_array, max_tokens, tools)
    }
}

impl AgentBackend for ApiAgent {
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
        let use_search = self.tools.contains(&"web_search".to_string());
        let max_tokens = if use_search {
            MAX_TOKENS_SEARCH
        } else {
            MAX_TOKENS
        };

        let mut msgs = messages.to_vec();

        for attempt in 0..=max_retries {
            let text = self.call_api(&system, &msgs, max_tokens, use_search)?;

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
            let text = self.call_api(&system, &msgs, 512, false)?;

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
