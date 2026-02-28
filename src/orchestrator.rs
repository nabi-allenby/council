use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::{AgentBackend, Message, MAX_RETRIES_DEFAULT};
use crate::api_backend::ApiAgent;
use crate::config::{Backend, CouncilConfig};
use crate::error::CouncilError;
use crate::prompt::{discussion_prompt, vote_prompt};
use crate::sdk_backend::SdkAgent;
use crate::types::{Session, Turn, Vote};

pub struct Orchestrator {
    config: CouncilConfig,
    agents: HashMap<String, Box<dyn AgentBackend>>,
    verbose: bool,
    prompts_dir: PathBuf,
}

impl Orchestrator {
    pub fn new(
        config: CouncilConfig,
        agents_dir: &Path,
        prompts_dir: &Path,
        verbose: bool,
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
            prompts_dir: prompts_dir.to_path_buf(),
        })
    }

    pub fn with_agents(
        config: CouncilConfig,
        agents: HashMap<String, Box<dyn AgentBackend>>,
        prompts_dir: &Path,
        verbose: bool,
    ) -> Self {
        Orchestrator {
            config,
            agents,
            verbose,
            prompts_dir: prompts_dir.to_path_buf(),
        }
    }

    pub fn run(&self, question: &str) -> Result<Session, CouncilError> {
        let mut session = Session::new(question.to_string());

        // Discussion rounds (agents within a round run in parallel)
        for round_num in 1..=self.config.rounds {
            self.log_round(&round_num.to_string());
            let system_ctx =
                discussion_prompt(&self.prompts_dir, round_num, self.config.rounds)?;

            // Build shared transcript (contains only turns from previous rounds)
            let base_transcript = Self::build_round_transcript(&session);

            for role in &self.config.rotation {
                self.log_waiting(role, "thinking");
            }

            let turns = std::thread::scope(|s| {
                let handles: Vec<_> = self
                    .config
                    .rotation
                    .iter()
                    .map(|role| {
                        let transcript = format!(
                            "{}\n\n---\n\nYou are **{}** speaking in **Round {}**.",
                            &base_transcript,
                            title_case(role),
                            round_num
                        );
                        let ctx = &system_ctx;
                        s.spawn(move || {
                            let messages = vec![Message {
                                role: "user".to_string(),
                                content: transcript,
                            }];
                            let agent = self.agents.get(role).unwrap();
                            agent.respond(round_num, ctx, &messages, MAX_RETRIES_DEFAULT)
                        })
                    })
                    .collect();

                handles
                    .into_iter()
                    .map(|h| {
                        h.join().unwrap_or_else(|_| {
                            Err(CouncilError::ApiError("Agent thread panicked".into()))
                        })
                    })
                    .collect::<Result<Vec<Turn>, _>>()
            })?;

            for turn in turns {
                self.log_turn(&turn);
                session.turns.push(turn);
            }
        }

        // Vote phase (all votes run in parallel)
        self.log_round("VOTE");
        let vote_ctx = vote_prompt(&self.prompts_dir, question)?;
        let full_transcript = Self::build_full_transcript(&session);

        for role in &self.config.rotation {
            self.log_waiting(role, "voting");
        }

        let votes = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .config
                .rotation
                .iter()
                .map(|role| {
                    let vote_message = format!(
                        "{}\n\n---\n\nYou are {}. Cast your vote on the question above.",
                        &full_transcript,
                        title_case(role)
                    );
                    let ctx = &vote_ctx;
                    s.spawn(move || {
                        let messages = vec![Message {
                            role: "user".to_string(),
                            content: vote_message,
                        }];
                        let agent = self.agents.get(role).unwrap();
                        agent.cast_vote(ctx, &messages, MAX_RETRIES_DEFAULT)
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        Err(CouncilError::ApiError("Agent thread panicked".into()))
                    })
                })
                .collect::<Result<Vec<Vote>, _>>()
        })?;

        for vote in votes {
            self.log_vote(&vote);
            session.votes.push(vote);
        }

        Ok(session)
    }

    fn build_round_transcript(session: &Session) -> String {
        let mut parts = vec![format!("# Question\n\n{}", session.question)];

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

        parts.join("\n\n")
    }

    fn build_full_transcript(session: &Session) -> String {
        let mut parts = vec![
            format!("# Question\n\n{}", session.question),
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
