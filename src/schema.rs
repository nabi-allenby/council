use regex::Regex;
use serde_json::Value;

use crate::types::{ParsedMotion, ParsedResponse, ParsedVote, VoteChoice};

pub fn validate_discussion_response(text: &str) -> Option<ParsedResponse> {
    let re = Regex::new(r"(?s)---RESPONSE---\s*(.*?)---END---").unwrap();
    let caps = re.captures(text)?;
    let json_str = caps.get(1)?.as_str().trim();

    let data: Value = serde_json::from_str(json_str).ok()?;

    // Required: position
    let position = data.get("position")?.as_str()?;
    let position = position.trim();
    if position.is_empty() || position.len() > 300 {
        return None;
    }

    // Required: reasoning (1-5 items)
    let reasoning = data.get("reasoning")?.as_array()?;
    if reasoning.is_empty() || reasoning.len() > 5 {
        return None;
    }
    let reasoning: Vec<String> = reasoning
        .iter()
        .map(|r| r.as_str().map(|s| s.trim().to_string()))
        .collect::<Option<Vec<_>>>()?;
    if reasoning.iter().any(|r| r.len() > 300) {
        return None;
    }

    // Optional: concerns (0-5 items)
    let concerns = match data.get("concerns") {
        Some(v) => {
            let arr = v.as_array()?;
            if arr.len() > 5 {
                return None;
            }
            let concerns: Vec<String> = arr
                .iter()
                .map(|c| c.as_str().map(|s| s.trim().to_string()))
                .collect::<Option<Vec<_>>>()?;
            if concerns.iter().any(|c| c.len() > 300) {
                return None;
            }
            concerns
        }
        None => Vec::new(),
    };

    // Optional: updated_by
    let updated_by = match data.get("updated_by") {
        Some(v) => {
            let arr = v.as_array()?;
            arr.iter()
                .map(|u| u.as_str().map(|s| s.trim().to_string()))
                .collect::<Option<Vec<_>>>()?
        }
        None => Vec::new(),
    };

    Some(ParsedResponse {
        position: position.to_string(),
        reasoning,
        concerns,
        updated_by,
    })
}

pub fn validate_vote_response(text: &str) -> Option<ParsedVote> {
    let re = Regex::new(r"(?s)---VOTE---\s*(.*?)---END---").unwrap();
    let caps = re.captures(text)?;
    let json_str = caps.get(1)?.as_str().trim();

    let data: Value = serde_json::from_str(json_str).ok()?;

    let vote_str = data.get("vote")?.as_str()?.trim().to_lowercase();
    let vote = match vote_str.as_str() {
        "yay" => VoteChoice::Yay,
        "nay" => VoteChoice::Nay,
        _ => return None,
    };

    let reason = data.get("reason")?.as_str()?;
    let reason = reason.trim();
    if reason.is_empty() || reason.len() > 500 {
        return None;
    }

    Some(ParsedVote {
        vote,
        reason: reason.to_string(),
    })
}

pub fn validate_motion_response(text: &str) -> Option<ParsedMotion> {
    let re = Regex::new(r"(?s)---MOTION---\s*(.*?)---END---").unwrap();
    let caps = re.captures(text)?;
    let json_str = caps.get(1)?.as_str().trim();

    let data: Value = serde_json::from_str(json_str).ok()?;

    let proceed = data.get("proceed")?.as_bool()?;

    let rationale = data
        .get("rationale")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if proceed {
        let motion = data.get("motion")?.as_str()?.trim().to_string();
        if motion.is_empty() {
            return None;
        }
        Some(ParsedMotion {
            motion: Some(motion),
            rationale,
            proceed: true,
        })
    } else {
        Some(ParsedMotion {
            motion: None,
            rationale,
            proceed: false,
        })
    }
}

pub fn strip_structured_block(text: &str) -> String {
    let re_response = Regex::new(r"(?s)\n*---RESPONSE---\s*.*?---END---\s*").unwrap();
    let text = re_response.replace_all(text, "");
    let re_vote = Regex::new(r"(?s)\n*---VOTE---\s*.*?---END---\s*").unwrap();
    let text = re_vote.replace_all(&text, "");
    let re_motion = Regex::new(r"(?s)\n*---MOTION---\s*.*?---END---\s*").unwrap();
    let text = re_motion.replace_all(&text, "");
    text.trim_end().to_string()
}
