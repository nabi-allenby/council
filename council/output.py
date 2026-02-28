"""Concise decision output for stdout."""

from __future__ import annotations

from .types import Session


def format_decision_record(session: Session) -> str:
    """Concise executive summary — outcome, motion, votes."""
    outcome = session.outcome
    yays = [v for v in session.votes if v.vote == "yay"]
    nays = [v for v in session.votes if v.vote == "nay"]

    sections = [
        f"# Council Decision: {session.question}",
        f"**Outcome: {outcome.upper()}** ({len(yays)}-{len(nays)})",
    ]

    # Vote breakdown
    vote_lines = []
    for v in session.votes:
        icon = "Y" if v.vote == "yay" else "N"
        vote_lines.append(f"- [{icon}] **{v.agent.title()}**: {v.reason}")
    sections.append("## Votes\n\n" + "\n".join(vote_lines))

    # Key concerns from final round (informational only)
    max_round = max((t.round for t in session.turns), default=0)
    final_turns = [t for t in session.turns if t.round == max_round]
    all_concerns = []
    for turn in final_turns:
        for concern in turn.parsed.concerns:
            all_concerns.append(f"- **{turn.agent.title()}**: {concern}")
    if all_concerns:
        sections.append("## Outstanding Concerns\n\n" + "\n".join(all_concerns))

    return "\n\n".join(sections)


