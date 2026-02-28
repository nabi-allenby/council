use crate::orchestrator::title_case;
use crate::types::{Session, VoteChoice};

pub fn format_decision_record(session: &Session) -> String {
    let outcome = session.outcome();
    let yays: Vec<_> = session
        .votes
        .iter()
        .filter(|v| v.vote == VoteChoice::Yay)
        .collect();
    let nays: Vec<_> = session
        .votes
        .iter()
        .filter(|v| v.vote == VoteChoice::Nay)
        .collect();

    let mut sections = vec![
        format!("# Council Decision: {}", session.motion()),
    ];
    if session.crafted_motion.is_some() {
        sections.push(format!("*Original question: {}*", session.question));
    }
    sections.push(format!(
        "**Outcome: {}** ({}-{})",
        outcome.upper(),
        yays.len(),
        nays.len()
    ));

    // Vote breakdown
    let vote_lines: Vec<String> = session
        .votes
        .iter()
        .map(|v| {
            let icon = if v.vote == VoteChoice::Yay {
                "Y"
            } else {
                "N"
            };
            format!("- [{}] **{}**: {}", icon, title_case(&v.agent), v.reason)
        })
        .collect();
    sections.push(format!("## Votes\n\n{}", vote_lines.join("\n")));

    // Key concerns from final round
    let max_round = session.turns.iter().map(|t| t.round).max().unwrap_or(0);
    let final_turns: Vec<_> = session
        .turns
        .iter()
        .filter(|t| t.round == max_round)
        .collect();
    let mut all_concerns = Vec::new();
    for turn in final_turns {
        for concern in &turn.parsed.concerns {
            all_concerns.push(format!(
                "- **{}**: {}",
                title_case(&turn.agent),
                concern
            ));
        }
    }
    if !all_concerns.is_empty() {
        sections.push(format!(
            "## Outstanding Concerns\n\n{}",
            all_concerns.join("\n")
        ));
    }

    sections.join("\n\n")
}
