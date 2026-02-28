use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::{AgentBackend, Message, MAX_RETRIES_DEFAULT};
use crate::api_backend::ApiAgent;
use crate::config::{Backend, CouncilConfig};
use crate::error::CouncilError;
use crate::motion::craft_motion;
use crate::prompt::{discussion_prompt, motion_prompt, vote_prompt};
use crate::sdk_backend::SdkAgent;
use crate::types::{Session, Turn, Vote};

pub struct Orchestrator {
    config: CouncilConfig,
    agents: HashMap<String, Box<dyn AgentBackend>>,
    verbose: bool,
    skip_motion: bool,
    prompts_dir: PathBuf,
}

impl Orchestrator {
    pub fn new(
        config: CouncilConfig,
        agents_dir: &Path,
        prompts_dir: &Path,
        verbose: bool,
        skip_motion: bool,
    ) -> Result<Self, CouncilError> {
        let mut agents: HashMap<String, Box<dyn AgentBackend>> = HashMap::new();

        for role in &config.rotation {
            let path = agents_dir.join(format!("{}.md", role));
            if !path.exists() {
                return Err(CouncilError::FileNotFound(format!(
                    "Agent personality file not found: {}",
                    path.display()
                )));
            }
            let tools = config.tools.get(role).cloned().unwrap_or_default();

            match config.backend {
                Backend::Api => {
                    let agent = ApiAgent::new(role, &path, &config.model, tools)?;
                    agents.insert(role.clone(), Box::new(agent));
                }
                Backend::AgentSdk => {
                    let agent = SdkAgent::new(role, &path, &config.model)?;
                    agents.insert(role.clone(), Box::new(agent));
                }
            }
        }

        Ok(Orchestrator {
            config,
            agents,
            verbose,
            skip_motion,
            prompts_dir: prompts_dir.to_path_buf(),
        })
    }

    pub fn with_agents(
        config: CouncilConfig,
        agents: HashMap<String, Box<dyn AgentBackend>>,
        prompts_dir: &Path,
        verbose: bool,
        skip_motion: bool,
    ) -> Self {
        Orchestrator {
            config,
            agents,
            verbose,
            skip_motion,
            prompts_dir: prompts_dir.to_path_buf(),
        }
    }

    pub fn run(&self, question: &str) -> Result<Session, CouncilError> {
        let mut session = Session::new(question.to_string());

        // Motion crafting stage
        if !self.skip_motion {
            self.log_round("MOTION");
            self.log_waiting("motion crafter", "crafting");
            let system = motion_prompt(&self.prompts_dir)?;
            let parsed = craft_motion(question, &system, &self.config.model, &self.config.backend)?;

            if let Some(motion) = parsed.motion {
                self.log_motion(&motion, &parsed.rationale);
                session.crafted_motion = Some(motion);
            } else {
                return Err(CouncilError::NonBinaryQuestion {
                    rationale: parsed.rationale,
                    suggestion: parsed.suggestion,
                });
            }
        }

        self.run_session(session)
    }

    pub fn run_with_motion(&self, question: &str, motion: String) -> Result<Session, CouncilError> {
        let mut session = Session::new(question.to_string());
        session.crafted_motion = Some(motion);
        self.run_session(session)
    }

    fn run_session(&self, mut session: Session) -> Result<Session, CouncilError> {
        // Discussion rounds
        for round_num in 1..=self.config.rounds {
            self.log_round(&round_num.to_string());
            let system_ctx =
                discussion_prompt(&self.prompts_dir, round_num, self.config.rounds)?;

            for role in &self.config.rotation {
                self.log_waiting(role, "thinking");
                let transcript = Self::build_transcript(&session, round_num, role);
                let messages = vec![Message {
                    role: "user".to_string(),
                    content: transcript,
                }];

                let agent = self.agents.get(role).unwrap();
                let turn = agent.respond(round_num, &system_ctx, &messages, MAX_RETRIES_DEFAULT)?;
                self.log_turn(&turn);
                session.turns.push(turn);
            }
        }

        // Vote phase
        self.log_round("VOTE");
        let vote_ctx = vote_prompt(&self.prompts_dir, session.motion())?;
        let full_transcript = Self::build_full_transcript(&session);

        for role in &self.config.rotation {
            self.log_waiting(role, "voting");
            let vote_message = format!(
                "{}\n\n---\n\nYou are {}. Cast your vote on the question above.",
                full_transcript,
                title_case(role)
            );
            let messages = vec![Message {
                role: "user".to_string(),
                content: vote_message,
            }];

            let agent = self.agents.get(role).unwrap();
            let vote = agent.cast_vote(&vote_ctx, &messages, MAX_RETRIES_DEFAULT)?;
            self.log_vote(&vote);
            session.votes.push(vote);
        }

        Ok(session)
    }

    fn build_transcript(session: &Session, current_round: u32, current_role: &str) -> String {
        let mut parts = vec![format!("# Motion\n\n{}", session.motion())];

        let mut prev_round = 0;
        for turn in &session.turns {
            if turn.round != prev_round {
                prev_round = turn.round;
                parts.push(format!("## Round {}", turn.round));
            }

            let mut entry = format!(
                "### {} (Round {})\n\n{}",
                title_case(&turn.agent),
                turn.round,
                turn.content
            );
            entry += &format!("\n\n**Position:** {}", turn.parsed.position);
            if !turn.parsed.concerns.is_empty() {
                entry += &format!("\n**Concerns:** {}", turn.parsed.concerns.join("; "));
            }

            parts.push(entry);
        }

        parts.push(format!(
            "\n---\n\nYou are **{}** speaking in **Round {}**.",
            title_case(current_role),
            current_round
        ));

        parts.join("\n\n")
    }

    fn build_full_transcript(session: &Session) -> String {
        let mut parts = vec![
            format!("# Motion\n\n{}", session.motion()),
            "# Full Discussion Transcript".to_string(),
        ];

        let mut prev_round = 0;
        for turn in &session.turns {
            if turn.round != prev_round {
                prev_round = turn.round;
                parts.push(format!("## Round {}", prev_round));
            }

            let mut entry = format!(
                "### {}\n\n{}",
                title_case(&turn.agent),
                turn.content
            );
            entry += &format!("\n\n**Position:** {}", turn.parsed.position);
            if !turn.parsed.concerns.is_empty() {
                entry += &format!("\n**Concerns:** {}", turn.parsed.concerns.join("; "));
            }

            parts.push(entry);
        }

        parts.join("\n\n")
    }

    fn log_waiting(&self, role: &str, action: &str) {
        if self.verbose {
            eprintln!("  [{} {}...]", title_case(role), action);
        }
    }

    fn log_round(&self, round_id: &str) {
        if self.verbose {
            eprintln!("\n{}", "=".repeat(60));
            eprintln!("  ROUND: {}", round_id);
            eprintln!("{}\n", "=".repeat(60));
        }
    }

    fn log_turn(&self, turn: &Turn) {
        if self.verbose {
            eprintln!(
                "--- {} (Round {}) ---",
                title_case(&turn.agent),
                turn.round
            );
            eprintln!("Position: {}", turn.parsed.position);
            if !turn.parsed.concerns.is_empty() {
                eprintln!("Concerns: {:?}", turn.parsed.concerns);
            }
            eprintln!("{}\n", turn.content);
        }
    }

    fn log_motion(&self, motion: &str, rationale: &str) {
        if self.verbose {
            eprintln!("--- Motion Crafted ---");
            eprintln!("Motion: {}", motion);
            if !rationale.is_empty() {
                eprintln!("Rationale: {}", rationale);
            }
            eprintln!();
        }
    }

    fn log_vote(&self, vote: &Vote) {
        if self.verbose {
            eprintln!(
                "--- {}: {} ---",
                title_case(&vote.agent),
                vote.vote.to_string().to_uppercase()
            );
            eprintln!("Reason: {}\n", vote.reason);
        }
    }
}

pub fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
