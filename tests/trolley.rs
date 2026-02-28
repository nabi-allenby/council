use std::collections::HashMap;
use std::path::PathBuf;

use council::agent::{AgentBackend, Message};
use council::config::CouncilConfig;
use council::error::CouncilError;
use council::orchestrator::{title_case, Orchestrator};
use council::output::format_decision_record;
use council::prompt::{discussion_prompt, motion_prompt, vote_prompt};
use council::report::{generate_report, save_report};
use council::schema::{
    strip_structured_block, validate_discussion_response, validate_motion_response,
    validate_vote_response,
};
use council::types::{ParsedResponse, Session, Turn, Vote, VoteChoice};

const TROLLEY_QUESTION: &str =
    "Should you pull the trolley lever to divert the train and save 5 people at the cost of 1?";

const ROTATION: &[&str] = &["architect", "sentinel", "steward", "mediator", "firebrand"];

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn prompts_dir() -> PathBuf {
    project_root().join("prompts")
}

fn agents_dir() -> PathBuf {
    project_root().join("agents")
}

fn mock_config(rounds: u32) -> CouncilConfig {
    CouncilConfig {
        rotation: ROTATION.iter().map(|s| s.to_string()).collect(),
        rounds,
        tools: HashMap::new(),
        ..CouncilConfig::default()
    }
}

// ── Mock agent ──

struct MockPositions {
    positions: Vec<&'static str>,
}

fn mock_positions() -> HashMap<&'static str, MockPositions> {
    let mut m = HashMap::new();
    m.insert("architect", MockPositions { positions: vec![
        "Reframe: this is not a binary — it is a design question about what kind of moral agent you want to be.",
        "The lever pull is the obvious answer; the interesting question is why we built systems where this choice exists.",
        "Pull the lever. But the elegant move is preventing this scenario entirely — that is the real design challenge.",
    ]});
    m.insert("sentinel", MockPositions { positions: vec![
        "Pull, but the moral injury to the actor is being underweighted — someone has to live with actively causing a death.",
        "The Architect's reframe dodges the immediate stakes. Real people die while we redesign systems.",
        "Pull the lever, carry the cost. But flag: repeated trolley choices erode moral sensitivity over time.",
    ]});
    m.insert("steward", MockPositions { positions: vec![
        "Pull the lever. 5 > 1. But document the reasoning and assign accountability for preventing recurrence.",
        "Architect and Sentinel both have points, but neither has a concrete next step. Here is what we actually do.",
        "Pull. Then: incident review within 48 hours, infrastructure audit within 30 days, assigned owner for each.",
    ]});
    m.insert("mediator", MockPositions { positions: vec![
        "Everyone is converging on pulling — the real disagreement is about what happens after and how we hold the cost.",
        "Architect and Steward are saying the same thing differently: act now, fix systems later. The Sentinel adds the emotional cost.",
        "Pull the lever. The group agrees. Name the cost, resource the follow-through, support the person who acts.",
    ]});
    m.insert("firebrand", MockPositions { positions: vec![
        "Pull the lever. Five lives outweigh one. Stop philosophizing and decide.",
        "The council is overthinking this. The math is clear and the moral case holds. Pull it.",
        "Pull the lever and own the choice. This is the council's clear recommendation.",
    ]});
    m
}

fn vote_reasons() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("architect", "The reframe holds: act now, design the prevention. This is the right shape.");
    m.insert("sentinel", "Pull, but only because the alternative is worse. The cost to the actor is real.");
    m.insert("steward", "Clear, accountable, actionable. Pull the lever with follow-through.");
    m.insert("mediator", "The group converged honestly. Pull the lever, hold the cost together.");
    m.insert("firebrand", "Five lives. One lever. Pull it. This should not have taken three rounds.");
    m
}

struct MockAgent {
    role_name: String,
}

impl MockAgent {
    fn new(role: &str) -> Self {
        MockAgent {
            role_name: role.to_string(),
        }
    }
}

impl AgentBackend for MockAgent {
    fn role(&self) -> &str {
        &self.role_name
    }

    fn respond(
        &self,
        round_num: u32,
        _system_context: &str,
        _messages: &[Message],
        _max_retries: u32,
    ) -> Result<Turn, CouncilError> {
        let positions = mock_positions();
        let role_positions = &positions[self.role_name.as_str()];
        let idx = (round_num as usize - 1).min(role_positions.positions.len() - 1);
        let position = role_positions.positions[idx];

        Ok(Turn {
            agent: self.role_name.clone(),
            round: round_num,
            content: format!(
                "[{} Round {}] {}",
                title_case(&self.role_name),
                round_num,
                position
            ),
            parsed: ParsedResponse {
                position: position.to_string(),
                reasoning: vec![format!(
                    "Reasoning point for {} round {}",
                    self.role_name, round_num
                )],
                concerns: if round_num == 3 {
                    vec![]
                } else {
                    vec![format!("Minor caveat from {}", self.role_name)]
                },
                updated_by: if round_num == 1 {
                    vec![]
                } else {
                    vec!["architect".to_string(), "sentinel".to_string()]
                },
            },
        })
    }

    fn cast_vote(
        &self,
        _system_context: &str,
        _messages: &[Message],
        _max_retries: u32,
    ) -> Result<Vote, CouncilError> {
        let reasons = vote_reasons();
        Ok(Vote {
            agent: self.role_name.clone(),
            vote: VoteChoice::Yay,
            reason: reasons[self.role_name.as_str()].to_string(),
        })
    }
}

fn make_mock_agents() -> HashMap<String, Box<dyn AgentBackend>> {
    let mut agents: HashMap<String, Box<dyn AgentBackend>> = HashMap::new();
    for role in ROTATION {
        agents.insert(role.to_string(), Box::new(MockAgent::new(role)));
    }
    agents
}

// ── Prompt file tests ──

#[test]
fn test_prompt_files_exist() {
    let dir = prompts_dir();
    let required = [
        "engagement.md",
        "brevity.md",
        "response-format.md",
        "vote-format.md",
        "round-1.md",
        "round-2.md",
        "round-3.md",
        "vote.md",
        "motion.md",
    ];
    for name in required {
        let path = dir.join(name);
        assert!(path.exists(), "Missing prompt file: {}", path.display());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.trim().is_empty(), "Empty prompt file: {}", path.display());
    }
}

#[test]
fn test_discussion_prompt_loads() {
    let dir = prompts_dir();
    for round_num in 1..=3u32 {
        let prompt = discussion_prompt(&dir, round_num, 3).unwrap();
        assert!(
            prompt.contains(&format!("Round {} of 3", round_num)),
            "Missing round info in prompt"
        );
        assert!(prompt.contains("engage directly"), "Missing engagement rule");
        assert!(prompt.contains("150 words"), "Missing brevity rule");
        assert!(prompt.contains("---RESPONSE---"), "Missing response format");
    }
}

#[test]
fn test_vote_prompt_loads() {
    let dir = prompts_dir();
    let prompt = vote_prompt(&dir, "Should we pull the lever?").unwrap();
    assert!(prompt.contains("Should we pull the lever?"));
    let lower = prompt.to_lowercase();
    assert!(lower.contains("yay") || lower.contains("nay"));
    assert!(prompt.contains("---VOTE---"));
}

// ── Schema validation tests ──

#[test]
fn test_valid_response_parses() {
    let text = r#"Here is my analysis.

---RESPONSE---
{
  "position": "Pull the lever to save 5 lives",
  "reasoning": ["Net harm reduction", "Inaction is also a choice"],
  "concerns": ["Moral weight of active killing"],
  "updated_by": []
}
---END---"#;
    let parsed = validate_discussion_response(text);
    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert_eq!(parsed.position, "Pull the lever to save 5 lives");
    assert_eq!(parsed.reasoning.len(), 2);
    assert_eq!(parsed.concerns.len(), 1);
}

#[test]
fn test_missing_response_block_returns_none() {
    assert!(validate_discussion_response("Just some text without a block").is_none());
}

#[test]
fn test_invalid_json_returns_none() {
    let text = "---RESPONSE---\n{bad json}\n---END---";
    assert!(validate_discussion_response(text).is_none());
}

#[test]
fn test_valid_vote_parses() {
    let text = r#"I support this.

---VOTE---
{
  "vote": "yay",
  "reason": "Sound reasoning and clear action"
}
---END---"#;
    let parsed = validate_vote_response(text);
    assert!(parsed.is_some());
    assert_eq!(parsed.unwrap().vote, VoteChoice::Yay);
}

#[test]
fn test_invalid_vote_value_returns_none() {
    let text = r#"---VOTE---
{"vote": "maybe", "reason": "unsure"}
---END---"#;
    assert!(validate_vote_response(text).is_none());
}

#[test]
fn test_strip_structured_block() {
    let text = "My analysis here.\n\n---RESPONSE---\n{\"position\": \"x\"}\n---END---";
    let stripped = strip_structured_block(text);
    assert!(!stripped.contains("---RESPONSE---"));
    assert!(stripped.contains("My analysis here."));
}

// ── Pipeline tests (mock, no LLM calls) ──

#[test]
fn test_council_completes() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    assert!(
        session.outcome() == council::types::Outcome::Approved
            || session.outcome() == council::types::Outcome::Rejected
    );
    assert_eq!(session.turns.len(), 15); // 3 rounds x 5 agents
    assert_eq!(session.votes.len(), 5);
}

#[test]
fn test_council_single_round() {
    let config = mock_config(1);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    assert!(
        session.outcome() == council::types::Outcome::Approved
            || session.outcome() == council::types::Outcome::Rejected
    );
    assert_eq!(session.turns.len(), 5); // 1 round x 5 agents
    assert_eq!(session.votes.len(), 5);
}

#[test]
fn test_decision_record_format() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();
    let record = format_decision_record(&session);

    assert!(
        record.contains("Outcome: APPROVED") || record.contains("Outcome: REJECTED"),
        "Missing outcome in decision record"
    );
    for role in ROTATION {
        assert!(
            record.contains(&title_case(role)),
            "Missing role {} in decision record",
            role
        );
    }
}

#[test]
fn test_report_has_position_evolution() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();
    let report = generate_report(&session);

    assert!(report.contains("Position Evolution"));
    assert!(report.contains("Round 1"));
    assert!(report.contains("Round 2"));
    assert!(report.contains("Round 3"));
}

#[test]
fn test_report_saves_to_disk() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    let logs_dir = project_root().join("logs");
    let path = save_report(&session, &logs_dir).unwrap();

    assert!(path.exists());
    assert_eq!(path.extension().and_then(|s| s.to_str()), Some("md"));
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.len() > 100);

    // Cleanup
    std::fs::remove_file(&path).ok();
}

#[test]
fn test_motion_is_original_question() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    assert_eq!(session.motion(), TROLLEY_QUESTION);
}

#[test]
fn test_transcript_builds_incrementally() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    // Round 1
    assert_eq!(session.turns[0].agent, "architect");
    assert_eq!(session.turns[0].round, 1);
    assert_eq!(session.turns[1].agent, "sentinel");
    assert_eq!(session.turns[1].round, 1);
    assert_eq!(session.turns[4].agent, "firebrand");
    assert_eq!(session.turns[4].round, 1);

    // Round 2 starts at index 5
    assert_eq!(session.turns[5].agent, "architect");
    assert_eq!(session.turns[5].round, 2);

    // Round 3 starts at index 10
    assert_eq!(session.turns[10].agent, "architect");
    assert_eq!(session.turns[10].round, 3);
}

#[test]
fn test_concerns_are_informational_only() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    // Mock agents have concerns in rounds 1-2 but not round 3
    let r1_concerns: Vec<_> = session
        .turns
        .iter()
        .filter(|t| t.round == 1)
        .map(|t| &t.parsed.concerns)
        .collect();
    assert!(r1_concerns.iter().any(|c| !c.is_empty()));

    // Outcome is still decisive regardless
    assert!(
        session.outcome() == council::types::Outcome::Approved
            || session.outcome() == council::types::Outcome::Rejected
    );
}

#[test]
fn test_rotation_derived_from_session() {
    let config = mock_config(3);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    let expected: Vec<String> = ROTATION.iter().map(|s| s.to_string()).collect();
    assert_eq!(session.rotation(), expected);
}

// ── Backend config tests ──

#[test]
fn test_config_default_backend_is_api() {
    let config = mock_config(3);
    assert_eq!(config.backend, council::config::Backend::Api);
}

#[test]
fn test_orchestrator_creates_with_api_backend() {
    let mut config = mock_config(3);
    config.backend = council::config::Backend::Api;
    let result = Orchestrator::new(config, &agents_dir(), &prompts_dir(), false, true);
    assert!(result.is_ok(), "Orchestrator should create with api backend");
}

#[test]
fn test_orchestrator_accepts_mock_agents() {
    let config = CouncilConfig {
        rotation: ROTATION.iter().map(|s| s.to_string()).collect(),
        rounds: 1,
        ..CouncilConfig::default()
    };
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    assert!(
        session.outcome() == council::types::Outcome::Approved
            || session.outcome() == council::types::Outcome::Rejected
    );
    assert_eq!(session.turns.len(), 5);
    assert_eq!(session.votes.len(), 5);
}

// ── Motion crafting tests ──

#[test]
fn test_motion_prompt_file_exists() {
    let path = prompts_dir().join("motion.md");
    assert!(path.exists(), "Missing prompt file: motion.md");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(!content.trim().is_empty(), "Empty motion prompt file");
    assert!(
        content.contains("---MOTION---"),
        "Motion prompt must reference ---MOTION--- format"
    );
}

#[test]
fn test_motion_prompt_loads() {
    let dir = prompts_dir();
    let prompt = motion_prompt(&dir).unwrap();
    assert!(
        prompt.contains("---MOTION---"),
        "Motion prompt must reference ---MOTION--- format"
    );
    assert!(
        prompt.contains("binary"),
        "Motion prompt should mention binary framing"
    );
}

#[test]
fn test_valid_motion_proceed_parses() {
    let text = r#"This is a clear binary question.

---MOTION---
{"motion": "Should we proceed with rewriting the backend in Rust?", "rationale": "Already a binary question, cleaned up", "proceed": true}
---END---"#;
    let parsed = validate_motion_response(text);
    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert!(parsed.proceed);
    assert_eq!(
        parsed.motion.unwrap(),
        "Should we proceed with rewriting the backend in Rust?"
    );
    assert!(!parsed.rationale.is_empty());
}

#[test]
fn test_valid_motion_non_binary_parses() {
    let text = r#"This question has infinite answers.

---MOTION---
{"motion": null, "rationale": "Infinite choices, no binary framing possible", "suggestion": "Should we say the sky is blue?", "proceed": false}
---END---"#;
    let parsed = validate_motion_response(text);
    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert!(!parsed.proceed);
    assert!(parsed.motion.is_none());
    assert!(!parsed.rationale.is_empty());
    assert_eq!(
        parsed.suggestion.unwrap(),
        "Should we say the sky is blue?"
    );
}

#[test]
fn test_non_binary_without_suggestion_parses() {
    let text = r#"---MOTION---
{"motion": null, "rationale": "Nonsensical input", "proceed": false}
---END---"#;
    // No suggestion is valid for nonsensical input
    let parsed = validate_motion_response(text);
    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert!(!parsed.proceed);
    assert!(parsed.suggestion.is_none());
}

#[test]
fn test_meta_question_suggestion_rejected() {
    let text = r#"---MOTION---
{"motion": null, "rationale": "Not a question", "suggestion": "Could you rephrase this as a yes/no question?", "proceed": false}
---END---"#;
    let parsed = validate_motion_response(text).unwrap();
    // Meta-questions are filtered out — suggestion should be None
    assert!(parsed.suggestion.is_none());
}

#[test]
fn test_proceed_true_ignores_suggestion() {
    let text = r#"---MOTION---
{"motion": "Should we do X?", "rationale": "test", "suggestion": "ignored", "proceed": true}
---END---"#;
    let parsed = validate_motion_response(text);
    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert!(parsed.proceed);
    assert!(parsed.suggestion.is_none());
}

#[test]
fn test_missing_motion_block_returns_none() {
    assert!(validate_motion_response("Just some text").is_none());
}

#[test]
fn test_invalid_motion_json_returns_none() {
    let text = "---MOTION---\n{bad json}\n---END---";
    assert!(validate_motion_response(text).is_none());
}

#[test]
fn test_motion_empty_text_returns_none() {
    let text = r#"---MOTION---
{"motion": "", "rationale": "test", "proceed": true}
---END---"#;
    assert!(validate_motion_response(text).is_none());
}

#[test]
fn test_motion_too_long_returns_none() {
    let long_motion = "x".repeat(501);
    let text = format!(
        "---MOTION---\n{{\"motion\": \"{}\", \"rationale\": \"test\", \"proceed\": true}}\n---END---",
        long_motion
    );
    assert!(validate_motion_response(&text).is_none());
}

#[test]
fn test_motion_non_binary_empty_rationale_returns_none() {
    let text = r#"---MOTION---
{"motion": null, "rationale": "", "proceed": false}
---END---"#;
    assert!(validate_motion_response(text).is_none());
}

#[test]
fn test_motion_null_motion_with_proceed_true_returns_none() {
    let text = r#"---MOTION---
{"motion": null, "rationale": "test", "proceed": true}
---END---"#;
    assert!(validate_motion_response(text).is_none());
}

#[test]
fn test_motion_missing_proceed_returns_none() {
    let text = r#"---MOTION---
{"motion": "Should we do X?", "rationale": "test"}
---END---"#;
    assert!(validate_motion_response(text).is_none());
}

#[test]
fn test_strip_structured_block_handles_motion() {
    let text = "Analysis here.\n\n---MOTION---\n{\"motion\": \"x\"}\n---END---";
    let stripped = strip_structured_block(text);
    assert!(!stripped.contains("---MOTION---"));
    assert!(stripped.contains("Analysis here."));
}

#[test]
fn test_session_motion_returns_question_when_no_crafted_motion() {
    let session = Session::new("Original question?".to_string());
    assert_eq!(session.motion(), "Original question?");
}

#[test]
fn test_session_motion_returns_crafted_motion() {
    let mut session = Session::new("How should we do auth?".to_string());
    session.crafted_motion = Some("Should we adopt JWT-based authentication?".to_string());
    assert_eq!(session.motion(), "Should we adopt JWT-based authentication?");
}

#[test]
fn test_skip_motion_uses_original_question() {
    let config = mock_config(1);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    assert!(session.crafted_motion.is_none());
    assert_eq!(session.motion(), TROLLEY_QUESTION);
}

#[test]
fn test_run_with_motion_sets_crafted_motion() {
    let config = mock_config(1);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let suggested = "Should we pull the lever to save five?".to_string();
    let session = orchestrator
        .run_with_motion(TROLLEY_QUESTION, suggested.clone())
        .unwrap();

    assert_eq!(session.crafted_motion.as_deref(), Some(suggested.as_str()));
    assert_eq!(session.motion(), suggested);
    assert_eq!(session.question, TROLLEY_QUESTION);
    assert_eq!(session.turns.len(), 5);
    assert_eq!(session.votes.len(), 5);
}

#[test]
fn test_decision_record_shows_motion() {
    let config = mock_config(1);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let mut session = orchestrator.run(TROLLEY_QUESTION).unwrap();
    session.crafted_motion = Some("Should we pull the lever?".to_string());

    let record = format_decision_record(&session);
    assert!(
        record.contains("Should we pull the lever?"),
        "Decision record should show crafted motion"
    );
    assert!(
        record.contains("Original question:"),
        "Decision record should show original question label"
    );
}

#[test]
fn test_report_shows_motion_section() {
    let config = mock_config(1);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let mut session = orchestrator.run(TROLLEY_QUESTION).unwrap();
    session.crafted_motion = Some("Should we pull the lever?".to_string());

    let report = generate_report(&session);
    assert!(
        report.contains("## Motion"),
        "Report should have a Motion section when motion was crafted"
    );
    assert!(
        report.contains("Should we pull the lever?"),
        "Report should contain crafted motion"
    );
}

#[test]
fn test_report_no_motion_section_when_skipped() {
    let config = mock_config(1);
    let orchestrator =
        Orchestrator::with_agents(config, make_mock_agents(), &prompts_dir(), false, true);
    let session = orchestrator.run(TROLLEY_QUESTION).unwrap();

    let report = generate_report(&session);
    assert!(
        !report.contains("## Motion"),
        "Report should not have a Motion section when motion was skipped"
    );
}

// ── E2E tests (real LLM calls, slow) ──

#[test]
fn test_trolley_e2e() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping E2E test: ANTHROPIC_API_KEY not set");
        return;
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_council"))
        .args(["--rounds", "1", "--skip-motion", TROLLEY_QUESTION])
        .current_dir(project_root())
        .output()
        .expect("Failed to run council binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Council crashed:\n{}",
        stderr
    );
    assert!(
        stdout.contains("Outcome: APPROVED") || stdout.contains("Outcome: REJECTED"),
        "Missing outcome in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Full report saved to:"),
        "Missing report path in stdout:\n{}",
        stdout
    );
}

#[test]
fn test_motion_crafting_e2e() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping E2E test: ANTHROPIC_API_KEY not set");
        return;
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_council"))
        .args(["--rounds", "1", TROLLEY_QUESTION])
        .current_dir(project_root())
        .output()
        .expect("Failed to run council binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Council crashed:\n{}",
        stderr
    );
    assert!(
        stdout.contains("Outcome: APPROVED") || stdout.contains("Outcome: REJECTED"),
        "Missing outcome in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Original question:"),
        "Motion crafting should produce an 'Original question:' line when the motion differs:\n{}",
        stdout
    );
}
