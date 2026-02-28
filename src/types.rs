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

#[derive(Debug, Clone)]
pub struct ParsedResponse {
    pub position: String,
    pub reasoning: Vec<String>,
    pub concerns: Vec<String>,
    pub updated_by: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedVote {
    pub vote: VoteChoice,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub agent: String,
    pub round: u32,
    pub content: String,
    pub parsed: ParsedResponse,
}

#[derive(Debug, Clone)]
pub struct Vote {
    pub agent: String,
    pub vote: VoteChoice,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub question: String,
    pub turns: Vec<Turn>,
    pub votes: Vec<Vote>,
}

impl Session {
    pub fn new(question: String) -> Self {
        Session {
            question,
            turns: Vec::new(),
            votes: Vec::new(),
        }
    }

    pub fn outcome(&self) -> Outcome {
        let yays = self
            .votes
            .iter()
            .filter(|v| v.vote == VoteChoice::Yay)
            .count();
        let majority = self.votes.len() / 2 + 1;
        if yays >= majority {
            Outcome::Approved
        } else {
            Outcome::Rejected
        }
    }

    pub fn motion(&self) -> &str {
        &self.question
    }

    pub fn rotation(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for turn in &self.turns {
            if !seen.contains(&turn.agent) {
                seen.push(turn.agent.clone());
            }
        }
        seen
    }
}
