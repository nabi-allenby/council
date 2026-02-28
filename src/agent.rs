use crate::error::CouncilError;
use crate::types::{Turn, Vote};

// Retry prompt constants (identical across backends)

pub const DISCUSSION_RETRY_PROMPT: &str = "\
Your response is missing or has an invalid ---RESPONSE--- block. \
Please reply with ONLY the corrected block:\n\n\
---RESPONSE---\n\
{\"position\": \"...\", \"reasoning\": [\"...\"], \
\"concerns\": [], \"updated_by\": []}\n\
---END---";

pub const VOTE_RETRY_PROMPT: &str = "\
Your response is missing or has an invalid ---VOTE--- block. \
The reason MUST be under 500 characters. \
Please reply with ONLY the corrected block:\n\n\
---VOTE---\n\
{\"vote\": \"yay or nay\", \"reason\": \"one or two sentences (max 500 chars)\"}\n\
---END---";

pub const MAX_RETRIES_DEFAULT: u32 = 2;

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

pub fn normalize_text(text: &str) -> String {
    let mut result = text.to_string();
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result
}

pub trait AgentBackend {
    fn role(&self) -> &str;

    fn respond(
        &self,
        round_num: u32,
        system_context: &str,
        messages: &[Message],
        max_retries: u32,
    ) -> Result<Turn, CouncilError>;

    fn cast_vote(
        &self,
        system_context: &str,
        messages: &[Message],
        max_retries: u32,
    ) -> Result<Vote, CouncilError>;
}
