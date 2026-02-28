use std::fs;
use std::path::Path;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::agent::{
    normalize_text, AgentBackend, Message, DISCUSSION_RETRY_PROMPT, VOTE_RETRY_PROMPT,
};
use crate::error::CouncilError;
use crate::schema::{strip_structured_block, validate_discussion_response, validate_vote_response};
use crate::types::{Turn, Vote};

const MAX_TOKENS: u32 = 2048;
const MAX_TOKENS_SEARCH: u32 = 4096;
const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

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
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| CouncilError::ApiError("ANTHROPIC_API_KEY not set".into()))?;

        let msg_array: Vec<Value> = messages
            .iter()
            .map(|m| json!({"role": m.role, "content": m.content}))
            .collect();

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": msg_array,
        });

        if use_tools && self.tools.contains(&"web_search".to_string()) {
            body["tools"] = json!([{
                "type": "web_search_20250305",
                "name": "web_search",
                "max_uses": 5
            }]);
        }

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| CouncilError::ApiError(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(CouncilError::ApiError(format!(
                "API returned {}: {}",
                status, body
            )));
        }

        let resp_json: Value = response
            .json()
            .map_err(|e| CouncilError::ApiError(format!("Failed to parse API response: {}", e)))?;

        Self::extract_text(&resp_json)
    }

    fn extract_text(response: &Value) -> Result<String, CouncilError> {
        let content = response
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| CouncilError::ApiError("No content in response".into()))?;

        let parts: Vec<String> = content
            .iter()
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(normalize_text(&parts.join("\n\n")))
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
