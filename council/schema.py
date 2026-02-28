import json
import re

from .types import ParsedResponse, ParsedVote


def validate_discussion_response(text: str) -> ParsedResponse | None:
    """Extract and validate the ---RESPONSE--- block from agent output."""
    match = re.search(r"---RESPONSE---\s*\n(.*?)---END---", text, re.DOTALL)
    if not match:
        return None

    try:
        data = json.loads(match.group(1).strip())
    except json.JSONDecodeError:
        return None

    # Required: position
    position = data.get("position")
    if not isinstance(position, str) or not position.strip():
        return None
    if len(position) > 300:
        return None

    # Required: reasoning (1-5 items)
    reasoning = data.get("reasoning")
    if not isinstance(reasoning, list) or not (1 <= len(reasoning) <= 5):
        return None
    if not all(isinstance(r, str) and len(r) <= 300 for r in reasoning):
        return None

    # Optional: concerns (0-5 items)
    concerns = data.get("concerns", [])
    if not isinstance(concerns, list) or len(concerns) > 5:
        return None
    if not all(isinstance(c, str) and len(c) <= 300 for c in concerns):
        return None

    # Optional: updated_by
    updated_by = data.get("updated_by", [])
    if not isinstance(updated_by, list):
        return None
    if not all(isinstance(u, str) for u in updated_by):
        return None

    return ParsedResponse(
        position=position.strip(),
        reasoning=[r.strip() for r in reasoning],
        concerns=[c.strip() for c in concerns],
        updated_by=[u.strip() for u in updated_by],
    )


def validate_vote_response(text: str) -> ParsedVote | None:
    """Extract and validate the ---VOTE--- block from agent output."""
    match = re.search(r"---VOTE---\s*\n(.*?)---END---", text, re.DOTALL)
    if not match:
        return None

    try:
        data = json.loads(match.group(1).strip())
    except json.JSONDecodeError:
        return None

    vote = data.get("vote", "").strip().lower()
    if vote not in ("yay", "nay"):
        return None

    reason = data.get("reason")
    if not isinstance(reason, str) or not reason.strip():
        return None
    if len(reason) > 200:
        return None

    return ParsedVote(vote=vote, reason=reason.strip())


def strip_structured_block(text: str) -> str:
    """Remove ---RESPONSE---...---END--- or ---VOTE---...---END--- from content."""
    text = re.sub(
        r"\n*---RESPONSE---\s*\n.*?---END---\s*", "", text, flags=re.DOTALL
    )
    text = re.sub(
        r"\n*---VOTE---\s*\n.*?---END---\s*", "", text, flags=re.DOTALL
    )
    return text.rstrip()
