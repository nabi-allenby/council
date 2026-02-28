"""Shared agent interface and constants for all backends.

Defines the AgentBackend Protocol (maps to a Rust trait) and shared
retry prompt strings / text normalization used by both the direct-API
and Agent SDK backends.
"""

from __future__ import annotations

from typing import Protocol

from .types import Turn, Vote

# ── Retry prompt constants (identical across backends) ──

DISCUSSION_RETRY_PROMPT = (
    "Your response is missing or has an invalid ---RESPONSE--- block. "
    "Please reply with ONLY the corrected block:\n\n"
    "---RESPONSE---\n"
    '{"position": "...", "reasoning": ["..."], '
    '"concerns": [], "updated_by": []}\n'
    "---END---"
)

VOTE_RETRY_PROMPT = (
    "Your response is missing or has an invalid ---VOTE--- block. "
    "The reason MUST be under 500 characters. "
    "Please reply with ONLY the corrected block:\n\n"
    "---VOTE---\n"
    '{"vote": "yay or nay", "reason": "one or two sentences (max 500 chars)"}\n'
    "---END---"
)

MAX_RETRIES_DEFAULT = 2


def normalize_text(text: str) -> str:
    """Collapse runs of 3+ newlines down to 2. Used by all backends."""
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")
    return text


class AgentBackend(Protocol):
    """Structural type for agent backends (maps to a Rust trait).

    Both Agent (direct API) and AgentSDKAgent implement this interface,
    as does MockAgent in tests.
    """

    role: str

    def respond(
        self,
        round_num: int,
        system_context: str,
        messages: list[dict[str, str]],
        max_retries: int = MAX_RETRIES_DEFAULT,
    ) -> Turn: ...

    def cast_vote(
        self,
        system_context: str,
        messages: list[dict[str, str]],
        max_retries: int = MAX_RETRIES_DEFAULT,
    ) -> Vote: ...
