use std::process::Command;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::agent::{normalize_text, MOTION_RETRY_PROMPT};
use crate::config::Backend;
use crate::error::CouncilError;
use crate::http::call_anthropic_api;
use crate::schema::validate_motion_response;
use crate::types::ParsedMotion;

const MAX_TOKENS: u32 = 1024;
const MAX_RETRIES: u32 = 1;

fn call_api(
    system: &str,
    messages: &[(String, String)],
    model: &str,
) -> Result<String, CouncilError> {
    let msg_array: Vec<Value> = messages
        .iter()
        .map(|(role, content)| json!({"role": role, "content": content}))
        .collect();

    let client = Client::new();
    call_anthropic_api(&client, model, system, &msg_array, MAX_TOKENS, None)
}

fn call_sdk(system: &str, prompt: &str, model: &str) -> Result<String, CouncilError> {
    let output = Command::new("claude")
        .arg("--print")
        .arg("--model")
        .arg(model)
        .arg("--system-prompt")
        .arg(system)
        .arg("--")
        .arg(prompt)
        .env_remove("CLAUDECODE")
        .output()
        .map_err(|e| {
            CouncilError::ApiError(format!("Failed to run claude CLI for motion: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CouncilError::ApiError(format!(
            "claude CLI motion call failed: {}",
            stderr
        )));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(normalize_text(text.trim()))
}

fn raw_call(
    system: &str,
    messages: &[(String, String)],
    model: &str,
    backend: &Backend,
) -> Result<String, CouncilError> {
    match backend {
        Backend::Api => call_api(system, messages, model),
        Backend::AgentSdk => {
            // Flatten messages into a single prompt for the SDK
            let prompt = messages
                .iter()
                .map(|(role, content)| {
                    if role == "assistant" {
                        format!("[Your previous response]\n{}", content)
                    } else {
                        content.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            call_sdk(system, &prompt, model)
        }
    }
}

pub fn craft_motion(
    question: &str,
    system_prompt: &str,
    model: &str,
    backend: &Backend,
) -> Result<ParsedMotion, CouncilError> {
    let mut messages = vec![("user".to_string(), question.to_string())];

    for attempt in 0..=MAX_RETRIES {
        let text = raw_call(system_prompt, &messages, model, backend)?;

        if let Some(parsed) = validate_motion_response(&text) {
            return Ok(parsed);
        }

        if attempt < MAX_RETRIES {
            messages.push(("assistant".to_string(), text));
            messages.push(("user".to_string(), MOTION_RETRY_PROMPT.to_string()));
        }
    }

    Err(CouncilError::RetryExhausted(
        "Motion crafter failed to produce valid ---MOTION--- block".into(),
    ))
}
