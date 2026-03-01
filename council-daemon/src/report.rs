use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Local, Utc};

use crate::error::DaemonError;
use crate::types::{title_case, Session, VoteChoice};

pub fn generate_report(session: &Session) -> String {
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
    let now = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let rotation = session.rotation();

    let mut lines = vec![
        "# Council Report".to_string(),
        format!("**Date:** {}  ", now),
        format!(
            "**Outcome:** {} ({}-{})",
            outcome.upper(),
            yays.len(),
            nays.len()
        ),
        String::new(),
        "## Question".to_string(),
        String::new(),
        format!("> {}", session.question),
        String::new(),
        "---".to_string(),
        String::new(),
    ];

    // Vote table
    lines.extend([
        "## Vote Results".to_string(),
        String::new(),
        "| Participant | Vote | Reason |".to_string(),
        "|-------------|:----:|--------|".to_string(),
    ]);
    for v in &session.votes {
        let vote_str = if v.choice == VoteChoice::Yay {
            "YAY"
        } else {
            "NAY"
        };
        lines.push(format!(
            "| {} | **{}** | {} |",
            title_case(&v.participant),
            vote_str,
            v.reason
        ));
    }
    lines.push(String::new());

    // Position evolution table
    let max_round = session.turns.iter().map(|t| t.round).max().unwrap_or(0);
    lines.extend(["## Position Evolution".to_string(), String::new()]);

    let mut header = "| Participant |".to_string();
    let mut separator = "|-------------|".to_string();
    for r in 1..=max_round {
        header += &format!(" Round {} |", r);
        separator += "---------|";
    }
    lines.push(header);
    lines.push(separator);

    for name in &rotation {
        let mut positions: HashMap<u32, String> = HashMap::new();
        for turn in &session.turns {
            if turn.participant == *name {
                positions.insert(turn.round, turn.position.clone());
            }
        }
        let mut row = format!("| {} |", title_case(name));
        for r in 1..=max_round {
            let pos = positions.get(&r).map(|s| s.as_str()).unwrap_or("-");
            row += &format!(" {} |", truncate(pos, 80));
        }
        lines.push(row);
    }
    lines.push(String::new());

    // Outstanding concerns from final round
    let mut concerns = Vec::new();
    for turn in &session.turns {
        if turn.round == max_round {
            for c in &turn.concerns {
                concerns.push(format!("- **{}**: {}", title_case(&turn.participant), c));
            }
        }
    }
    if !concerns.is_empty() {
        lines.push("## Outstanding Concerns".to_string());
        lines.push(String::new());
        lines.extend(concerns);
        lines.push(String::new());
    }

    // Full detail section
    lines.extend([
        "---".to_string(),
        String::new(),
        "<details>".to_string(),
        "<summary><strong>Full Deliberation (click to expand)</strong></summary>".to_string(),
        String::new(),
    ]);

    let mut prev_round = 0;
    for (i, turn) in session.turns.iter().enumerate() {
        if turn.round != prev_round {
            prev_round = turn.round;
            lines.push(format!("## Round {}", prev_round));
            lines.push(String::new());
        }

        lines.push(format!(
            "### Turn {}: {}",
            i + 1,
            title_case(&turn.participant)
        ));
        lines.push(format!("**Position:** {}", turn.position));
        if !turn.reasoning.is_empty() {
            lines.push(format!("**Reasoning:** {}", turn.reasoning.join(" | ")));
        }
        if !turn.concerns.is_empty() {
            lines.push(format!("**Concerns:** {}", turn.concerns.join(" | ")));
        }
        lines.push(String::new());
        lines.push("---".to_string());
        lines.push(String::new());
    }

    // Vote detail
    lines.push("## Vote Round".to_string());
    lines.push(String::new());
    for v in &session.votes {
        let vote_str = if v.choice == VoteChoice::Yay {
            "YAY"
        } else {
            "NAY"
        };
        lines.push(format!(
            "### {}: **{}**",
            title_case(&v.participant),
            vote_str
        ));
        lines.push(v.reason.clone());
        lines.push(String::new());
    }

    lines.push("</details>".to_string());

    lines.join("\n")
}

pub fn save_report(session: &Session, logs_dir: &Path) -> Result<PathBuf, DaemonError> {
    fs::create_dir_all(logs_dir)
        .map_err(|e| DaemonError::Io(format!("Cannot create logs directory: {}", e)))?;

    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug: String = session
        .question
        .chars()
        .take(40)
        .collect::<String>()
        .to_lowercase();
    let slug: String = slug
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect();
    let slug = slug.trim().replace(' ', "-");
    let slug = if slug.is_empty() {
        "council".to_string()
    } else {
        slug
    };

    let filename = format!("{}-{}.md", timestamp, slug);
    let path = logs_dir.join(filename);

    let report = generate_report(session);
    fs::write(&path, report).map_err(|e| DaemonError::Io(format!("Cannot write report: {}", e)))?;

    Ok(path)
}

fn truncate(text: &str, max_len: usize) -> String {
    if text.len() > max_len {
        format!("{}...", &text[..max_len - 3])
    } else {
        text.to_string()
    }
}
