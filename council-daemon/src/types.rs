use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum VoteChoice {
    Yay,
    Nay,
}

impl fmt::Display for VoteChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoteChoice::Yay => write!(f, "yay"),
            VoteChoice::Nay => write!(f, "nay"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Approved,
    Rejected,
}

impl Outcome {
    pub fn upper(&self) -> &str {
        match self {
            Outcome::Approved => "APPROVED",
            Outcome::Rejected => "REJECTED",
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Approved => write!(f, "approved"),
            Outcome::Rejected => write!(f, "rejected"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    LobbyOpen,
    InProgress,
    Voting,
    Completed,
}

#[derive(Debug, Clone)]
pub struct Participant {
    pub name: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub participant: String,
    pub round: u32,
    pub position: String,
    pub reasoning: Vec<String>,
    pub concerns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Vote {
    pub participant: String,
    pub choice: VoteChoice,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub question: String,
    pub status: SessionStatus,
    pub participants: Vec<Participant>,
    pub turns: Vec<Turn>,
    pub votes: Vec<Vote>,
    pub total_rounds: u32,
    pub current_round: u32,
    pub current_speaker_idx: usize,
}

impl Session {
    pub fn new(id: String, question: String, total_rounds: u32) -> Self {
        Session {
            id,
            question,
            status: SessionStatus::LobbyOpen,
            participants: Vec::new(),
            turns: Vec::new(),
            votes: Vec::new(),
            total_rounds,
            current_round: 0,
            current_speaker_idx: 0,
        }
    }

    pub fn outcome(&self) -> Outcome {
        let yays = self
            .votes
            .iter()
            .filter(|v| v.choice == VoteChoice::Yay)
            .count();
        let majority = self.votes.len() / 2 + 1;
        if yays >= majority {
            Outcome::Approved
        } else {
            Outcome::Rejected
        }
    }

    pub fn participant_names(&self) -> Vec<String> {
        self.participants.iter().map(|p| p.name.clone()).collect()
    }

    pub fn current_speaker(&self) -> Option<&str> {
        if self.status == SessionStatus::InProgress {
            self.participants
                .get(self.current_speaker_idx)
                .map(|p| p.name.as_str())
        } else {
            None
        }
    }

    pub fn has_voted(&self, name: &str) -> bool {
        self.votes.iter().any(|v| v.participant == name)
    }

    pub fn all_voted(&self) -> bool {
        self.votes.len() == self.participants.len()
    }

    pub fn start_discussion(&mut self) {
        self.status = SessionStatus::InProgress;
        self.current_round = 1;
        self.current_speaker_idx = 0;
    }

    /// Advance to the next speaker, wrapping to the next round when all have spoken.
    /// Transitions to `Voting` when all rounds are complete.
    pub fn advance_speaker(&mut self) {
        self.current_speaker_idx += 1;
        if self.current_speaker_idx >= self.participants.len() {
            self.current_speaker_idx = 0;
            self.current_round += 1;
            if self.current_round > self.total_rounds {
                self.status = SessionStatus::Voting;
            }
        }
    }

    pub fn build_transcript(&self) -> String {
        let mut parts = vec![format!("# Question\n\n{}", self.question)];

        let mut prev_round = 0;
        for turn in &self.turns {
            if turn.round != prev_round {
                prev_round = turn.round;
                parts.push(format!("## Round {}", turn.round));
            }

            let mut entry = format!(
                "### {} (Round {})",
                title_case(&turn.participant),
                turn.round
            );
            entry += &format!("\n\n**Position:** {}", turn.position);
            if !turn.reasoning.is_empty() {
                for r in &turn.reasoning {
                    entry += &format!("\n- {}", r);
                }
            }
            if !turn.concerns.is_empty() {
                entry += "\n\n**Concerns:**";
                for c in &turn.concerns {
                    entry += &format!("\n- {}", c);
                }
            }
            parts.push(entry);
        }

        parts.join("\n\n")
    }
}

pub fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
