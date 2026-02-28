use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::agent::normalize_text;
use crate::error::CouncilError;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const MAX_HTTP_RETRIES: u32 = 3;

/// Status codes that are safe to retry (transient errors).
pub(crate) fn is_retryable(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
}

/// Determine how long to wait before retrying.
/// For 429, respects the retry-after header (capped at 120s).
/// For other retryable errors, uses exponential backoff starting at 5s.
pub(crate) fn retry_delay(status: u16, retry_after: Option<&str>, attempt: u32) -> Duration {
    if status == 429 {
        if let Some(secs) = retry_after.and_then(|s| s.parse::<u64>().ok()) {
            return Duration::from_secs(secs.min(120));
        }
        // 429 without retry-after: 30s, 60s, 120s
        let multiplier = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
        return Duration::from_secs(30u64.saturating_mul(multiplier).min(120));
    }
    // 5xx / 529: exponential backoff 5s, 10s, 20s, 40s (capped at 120s)
    let multiplier = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
    Duration::from_secs(5u64.saturating_mul(multiplier).min(120))
}

/// Make a request to the Anthropic Messages API with automatic retry on transient errors.
pub fn call_anthropic_api(
    client: &Client,
    model: &str,
    system: &str,
    messages: &[Value],
    max_tokens: u32,
    tools: Option<Value>,
) -> Result<String, CouncilError> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| CouncilError::ApiError("ANTHROPIC_API_KEY not set".into()))?;

    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        "messages": messages,
    });

    if let Some(tools_val) = tools {
        body["tools"] = tools_val;
    }

    let mut last_error = String::new();

    for attempt in 0..=MAX_HTTP_RETRIES {
        let response = client
            .post(API_URL)
            .header("x-api-key", &api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| CouncilError::ApiError(format!("HTTP request failed: {}", e)))?;

        let status = response.status();

        if status.is_success() {
            let resp_json: Value = response.json().map_err(|e| {
                CouncilError::ApiError(format!("Failed to parse API response: {}", e))
            })?;
            return extract_text(&resp_json);
        }

        let status_code = status.as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let resp_body = response.text().unwrap_or_default();

        if is_retryable(status_code) && attempt < MAX_HTTP_RETRIES {
            let delay = retry_delay(status_code, retry_after.as_deref(), attempt);
            eprintln!(
                "  [API returned {} — retrying in {}s ({}/{})]",
                status_code,
                delay.as_secs(),
                attempt + 1,
                MAX_HTTP_RETRIES
            );
            thread::sleep(delay);
            continue;
        }

        last_error = format!("API returned {}: {}", status, resp_body);
    }

    Err(CouncilError::ApiError(last_error))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_codes() {
        assert!(is_retryable(429));
        assert!(is_retryable(500));
        assert!(is_retryable(502));
        assert!(is_retryable(503));
        assert!(is_retryable(504));
        assert!(is_retryable(529));
        assert!(!is_retryable(200));
        assert!(!is_retryable(400));
        assert!(!is_retryable(401));
        assert!(!is_retryable(403));
        assert!(!is_retryable(404));
    }

    #[test]
    fn test_429_with_retry_after_header() {
        let delay = retry_delay(429, Some("10"), 0);
        assert_eq!(delay, Duration::from_secs(10));
    }

    #[test]
    fn test_429_retry_after_capped_at_120s() {
        let delay = retry_delay(429, Some("300"), 0);
        assert_eq!(delay, Duration::from_secs(120));
    }

    #[test]
    fn test_429_without_retry_after_escalates() {
        let d0 = retry_delay(429, None, 0);
        let d1 = retry_delay(429, None, 1);
        let d2 = retry_delay(429, None, 2);
        assert_eq!(d0, Duration::from_secs(30));
        assert_eq!(d1, Duration::from_secs(60));
        assert_eq!(d2, Duration::from_secs(120));
    }

    #[test]
    fn test_5xx_exponential_backoff() {
        let d0 = retry_delay(500, None, 0);
        let d1 = retry_delay(500, None, 1);
        let d2 = retry_delay(500, None, 2);
        assert_eq!(d0, Duration::from_secs(5));
        assert_eq!(d1, Duration::from_secs(10));
        assert_eq!(d2, Duration::from_secs(20));
    }

    #[test]
    fn test_529_uses_same_backoff_as_5xx() {
        let d0 = retry_delay(529, None, 0);
        let d1 = retry_delay(529, None, 1);
        assert_eq!(d0, Duration::from_secs(5));
        assert_eq!(d1, Duration::from_secs(10));
    }

    #[test]
    fn test_5xx_backoff_capped_at_120s() {
        let d = retry_delay(500, None, 10);
        assert_eq!(d, Duration::from_secs(120));
    }

    #[test]
    fn test_429_invalid_retry_after_falls_back() {
        let delay = retry_delay(429, Some("not-a-number"), 0);
        assert_eq!(delay, Duration::from_secs(30));
    }
}
