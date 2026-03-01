use crate::types::{title_case, Session, VoteChoice};

pub fn format_decision_record(session: &Session) -> String {
    let outcome = session.outcome();
    let yays: Vec<_> = session
        .votes
        .iter()
        .filter(|v| v.choice == VoteChoice::Yay)
        .collect();
    let nays: Vec<_> = session
        .votes
        .iter()
        .filter(|v| v.choice == VoteChoice::Nay)
        .collect();

    let mut sections = vec![format!("# Council Decision: {}", session.question)];
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
            let icon = if v.choice == VoteChoice::Yay {
                "Y"
            } else {
                "N"
            };
            format!(
                "- [{}] **{}**: {}",
                icon,
                title_case(&v.participant),
                v.reason
            )
        })
        .collect();
    sections.push(format!("## Votes\n\n{}", vote_lines.join("\n")));

    // Key concerns from final round
    let max_round = session.turns.iter().map(|t| t.round).max().unwrap_or(0);
    let mut all_concerns = Vec::new();
    for turn in &session.turns {
        if turn.round == max_round {
            for concern in &turn.concerns {
                all_concerns.push(format!(
                    "- **{}**: {}",
                    title_case(&turn.participant),
                    concern
                ));
            }
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
