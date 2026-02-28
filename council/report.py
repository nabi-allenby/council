"""Generate structured markdown reports for council sessions.

Reports are saved to logs/ with a clear two-level structure:
  1. Executive summary (outcome, votes, position evolution)
  2. Full detail (complete agent responses, round by round)
"""

from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from .types import Session

LOGS_DIR = Path(__file__).resolve().parent.parent / "logs"


def generate_report(session: Session) -> str:
    """Build a two-level markdown report: summary + detail."""
    outcome = session.outcome
    motion = session.motion
    yays = [v for v in session.votes if v.vote == "yay"]
    nays = [v for v in session.votes if v.vote == "nay"]
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    rotation = session.rotation

    # ── SECTION 1: Executive Summary ──
    lines = [
        "# Council Report",
        f"**Date:** {now}  ",
        f"**Outcome:** {outcome.upper()} ({len(yays)}-{len(nays)})",
        "",
        "## Question",
        "",
        f"> {session.question}",
        "",
        "---",
        "",
    ]

    # Vote table
    lines.extend([
        "## Vote Results",
        "",
        "| Agent | Vote | Reason |",
        "|-------|:----:|--------|",
    ])
    for v in session.votes:
        vote_str = "YAY" if v.vote == "yay" else "NAY"
        lines.append(f"| {v.agent.title()} | **{vote_str}** | {v.reason} |")
    lines.append("")

    # Position evolution table
    max_round = max((t.round for t in session.turns), default=0)
    lines.extend([
        "## Position Evolution",
        "",
    ])
    # Build header
    header = "| Agent |"
    separator = "|-------|"
    for r in range(1, max_round + 1):
        header += f" Round {r} |"
        separator += "---------|"
    lines.extend([header, separator])

    for role in rotation:
        positions: dict[int, str] = {}
        for turn in session.turns:
            if turn.agent == role:
                positions[turn.round] = turn.parsed.position
        row = f"| {role.title()} |"
        for r in range(1, max_round + 1):
            row += f" {_truncate(positions.get(r, '-'), 80)} |"
        lines.append(row)
    lines.append("")

    # Outstanding concerns from final round
    concerns = []
    for turn in session.turns:
        if turn.round == max_round:
            for c in turn.parsed.concerns:
                concerns.append(f"- **{turn.agent.title()}**: {c}")
    if concerns:
        lines.extend(["## Outstanding Concerns", "", *concerns, ""])

    # ── SECTION 2: Full Detail ──
    lines.extend([
        "---",
        "",
        "<details>",
        "<summary><strong>Full Deliberation (click to expand)</strong></summary>",
        "",
    ])

    prev_round = 0
    for i, turn in enumerate(session.turns, 1):
        if turn.round != prev_round:
            prev_round = turn.round
            lines.extend([f"## Round {prev_round}", ""])

        lines.append(f"### Turn {i}: {turn.agent.title()}")
        lines.append(f"**Position:** {turn.parsed.position}")
        if turn.parsed.reasoning:
            lines.append(f"**Reasoning:** {' | '.join(turn.parsed.reasoning)}")
        if turn.parsed.concerns:
            lines.append(f"**Concerns:** {' | '.join(turn.parsed.concerns)}")
        if turn.parsed.updated_by:
            lines.append(
                f"**Influenced by:** "
                f"{', '.join(t.title() for t in turn.parsed.updated_by)}"
            )
        lines.extend(["", turn.content, "", "---", ""])

    # Vote detail
    lines.extend(["## Vote Round", ""])
    for v in session.votes:
        vote_str = "YAY" if v.vote == "yay" else "NAY"
        lines.extend([
            f"### {v.agent.title()}: **{vote_str}**",
            v.reason,
            "",
        ])

    lines.append("</details>")

    return "\n".join(lines)


def save_report(session: Session) -> Path:
    """Generate and save report to logs/ directory. Returns the file path."""
    LOGS_DIR.mkdir(exist_ok=True)

    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    slug = session.question[:40].lower()
    slug = "".join(c if c.isalnum() or c == " " else "" for c in slug)
    slug = slug.strip().replace(" ", "-") or "council"

    filename = f"{timestamp}-{slug}.md"
    path = LOGS_DIR / filename

    report = generate_report(session)
    path.write_text(report)
    return path


def _truncate(text: str, max_len: int) -> str:
    if len(text) > max_len:
        return text[: max_len - 3] + "..."
    return text
